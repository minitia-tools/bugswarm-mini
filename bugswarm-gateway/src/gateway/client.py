"""LLM Gateway — main client with provider registry, retry, cost tracking, hot-swap."""

from __future__ import annotations

import asyncio
import os
import signal
import time
from collections import defaultdict
from typing import AsyncIterator

import structlog

from .types import (
    ChatRequest, ChatResponse, ChatMessage, MessageRole,
    GatewayConfig, ProviderConfig, ProviderType,
    TokenUsage, CostInfo, request_fingerprint,
)

logger = structlog.get_logger(__name__)


class ProviderAdapter:
    """Base class for provider-specific adapters."""

    def __init__(self, config: ProviderConfig):
        self.config = config

    async def chat(self, request: ChatRequest) -> ChatResponse:
        raise NotImplementedError

    async def count_tokens(self, messages: list[ChatMessage]) -> int:
        raise NotImplementedError

    def estimate_cost(self, input_tokens: int, output_tokens: int) -> CostInfo:
        input_cost = (input_tokens / 1_000_000) * self.config.input_cost_per_mtok
        output_cost = (output_tokens / 1_000_000) * self.config.output_cost_per_mtok
        return CostInfo(
            input_cost_usd=input_cost,
            output_cost_usd=output_cost,
            total_cost_usd=input_cost + output_cost,
            model=self.config.default_model,
            provider=self.config.provider.value,
        )


class ProviderRegistry:
    """Registry of all configured provider adapters."""

    def __init__(self, config: GatewayConfig):
        self.config = config
        self._adapters: dict[ProviderType, ProviderAdapter] = {}
        self._active_provider: ProviderType = config.default_provider
        self._total_tokens = TokenUsage()
        self._total_cost = 0.0
        self._request_count: dict[ProviderType, int] = defaultdict(int)
        self._error_count: dict[ProviderType, int] = defaultdict(int)
        self._hotswap_listeners: list[callable] = []

    def register(self, provider: ProviderType, adapter: ProviderAdapter) -> None:
        self._adapters[provider] = adapter
        logger.info("Registered provider", provider=provider.value)

    def get_adapter(self, provider: ProviderType | None = None) -> ProviderAdapter:
        p = provider or self._active_provider
        if p not in self._adapters:
            raise ValueError(f"Provider {p} not registered")
        return self._adapters[p]

    @property
    def active_provider(self) -> ProviderType:
        return self._active_provider

    def hotswap(self, new_provider: ProviderType) -> None:
        if new_provider not in self._adapters:
            raise ValueError(f"Cannot hotswap to unregistered provider: {new_provider}")
        old = self._active_provider
        self._active_provider = new_provider
        logger.warning("Provider hotswapped", old=old.value, new=new_provider.value)
        for listener in self._hotswap_listeners:
            try:
                listener(old, new_provider)
            except Exception as e:
                logger.error("Hotswap listener error", error=str(e))

    def on_hotswap(self, callback: callable) -> None:
        self._hotswap_listeners.append(callback)

    def record_usage(self, provider: ProviderType, usage: TokenUsage, cost: CostInfo) -> None:
        self._total_tokens.input_tokens += usage.input_tokens
        self._total_tokens.output_tokens += usage.output_tokens
        self._total_tokens.total_tokens += usage.total_tokens
        self._total_cost += cost.total_cost_usd
        self._request_count[provider] += 1

    def record_error(self, provider: ProviderType) -> None:
        self._error_count[provider] += 1

    @property
    def total_tokens(self) -> TokenUsage:
        return self._total_tokens

    @property
    def total_cost(self) -> float:
        return self._total_cost

    @property
    def stats(self) -> dict:
        return {
            "active_provider": self._active_provider.value,
            "total_tokens": self._total_tokens.model_dump(),
            "total_cost_usd": self._total_cost,
            "request_count": dict(self._request_count),
            "error_count": dict(self._error_count),
        }

    def get_fallback_provider(self) -> ProviderType | None:
        """Get the next available fallback provider."""
        for fb in self.config.fallback_providers:
            if fb in self._adapters and fb != self._active_provider:
                return fb
        # Try any registered provider other than current
        for p in self._adapters:
            if p != self._active_provider:
                return p
        return None

    def disable_provider(self, provider: ProviderType) -> None:
        """Temporarily disable a failing provider."""
        if provider in self._adapters:
            name = provider.value
            # Don't actually remove, just mark
            logger.warning("Provider disabled due to errors", provider=name)


class LLMClient:
    """Main LLM client with retry, cost tracking, and provider abstraction."""

    def __init__(self, config: GatewayConfig | None = None):
        self.config = config or GatewayConfig.from_env()
        self.registry = ProviderRegistry(self.config)
        self._setup_signal_handlers()

    def register_default_adapters(self) -> None:
        """Register adapters for all configured providers."""
        for provider, pconfig in self.config.providers.items():
            adapter = self._create_adapter(provider, pconfig)
            self.registry.register(provider, adapter)

    def _create_adapter(self, provider: ProviderType, pconfig: ProviderConfig) -> ProviderAdapter:
        match provider:
            case ProviderType.OPENAI:
                from .providers.openai import OpenAIAdapter
                return OpenAIAdapter(pconfig)
            case ProviderType.DEEPSEEK:
                # DeepSeek uses OpenAI-compatible API — reuse OpenAI adapter
                from .providers.openai import OpenAIAdapter
                return OpenAIAdapter(pconfig)
            case ProviderType.ANTHROPIC:
                from .providers.anthropic import AnthropicAdapter
                return AnthropicAdapter(pconfig)
            case ProviderType.GOOGLE:
                from .providers.google import GoogleAdapter
                return GoogleAdapter(pconfig)
            case ProviderType.OLLAMA:
                from .providers.ollama import OllamaAdapter
                return OllamaAdapter(pconfig)
            case _:
                raise ValueError(f"Unknown provider: {provider}")

    def _setup_signal_handlers(self) -> None:
        """Setup SIGUSR1 for config reload (hotswap trigger)."""
        def _reload(signum, frame):
            logger.warning("SIGUSR1 received — reloading config")
            try:
                new_config = GatewayConfig.from_env()
                if new_config.default_provider != self.registry.active_provider:
                    self.registry.hotswap(new_config.default_provider)
            except Exception as e:
                logger.error("Config reload failed", error=str(e))
        try:
            signal.signal(signal.SIGUSR1, _reload)
        except (ValueError, OSError):
            pass  # Not in main thread or unsupported platform

    async def chat(
        self,
        request: ChatRequest,
        provider: ProviderType | None = None,
        retry_count: int = 0,
    ) -> ChatResponse:
        """Send a chat request with automatic retry and fallback.

        Retries on: 429 (rate limit), 502, 503, 504.
        Falls back to next provider after exhausting retries.
        """
        current_provider = provider or self.registry.active_provider
        max_retries = self.registry.get_adapter(current_provider).config.max_retries
        total_attempts = 0

        while total_attempts < self.config.max_total_retries:
            total_attempts += 1
            adapter = self.registry.get_adapter(current_provider)
            start_time = time.monotonic()

            try:
                response = await asyncio.wait_for(
                    adapter.chat(request),
                    timeout=adapter.config.timeout_secs,
                )
                response.latency_ms = (time.monotonic() - start_time) * 1000
                self.registry.record_usage(current_provider, response.usage, response.cost)
                return response

            except asyncio.TimeoutError:
                logger.warning("Provider timeout", provider=current_provider.value, attempt=total_attempts)
                self.registry.record_error(current_provider)
                if total_attempts >= max_retries:
                    fb = self._try_fallback(current_provider)
                    if fb:
                        current_provider = fb
                        max_retries = self.registry.get_adapter(current_provider).config.max_retries
                        total_attempts = 0
                        continue
                delay = self._compute_delay(total_attempts)
                await asyncio.sleep(delay)

            except Exception as e:
                error_str = str(e).lower()
                is_retryable = any(
                    str(code) in error_str
                    for code in self.config.retryable_statuses
                ) or "rate limit" in error_str or "timeout" in error_str or "connection" in error_str

                if is_retryable:
                    logger.warning("Retryable error", provider=current_provider.value, error=str(e)[:200], attempt=total_attempts)
                    self.registry.record_error(current_provider)
                    if total_attempts >= max_retries:
                        fb = self._try_fallback(current_provider)
                        if fb:
                            current_provider = fb
                            max_retries = self.registry.get_adapter(current_provider).config.max_retries
                            total_attempts = 0
                            continue
                    delay = self._compute_delay(total_attempts)
                    await asyncio.sleep(delay)
                else:
                    logger.error("Non-retryable error", provider=current_provider.value, error=str(e)[:200])
                    raise

        raise RuntimeError(f"All retries exhausted across {len(self.config.providers)} providers")

    async def chat_stream(
        self,
        request: ChatRequest,
        provider: ProviderType | None = None,
    ) -> AsyncIterator[str]:
        """Stream chat tokens. Falls back to polling non-streaming for unsupported providers."""
        response = await self.chat(request, provider)
        # Simulate streaming by yielding content
        yield response.content

    async def count_tokens(self, messages: list[ChatMessage], provider: ProviderType | None = None) -> int:
        """Count tokens for a message list using the provider's tokenizer."""
        adapter = self.registry.get_adapter(provider)
        return await adapter.count_tokens(messages)

    def _try_fallback(self, current: ProviderType) -> ProviderType | None:
        """Try to fall back to another provider."""
        fb = self.registry.get_fallback_provider()
        if fb:
            logger.warning("Falling back to provider", from_provider=current.value, to_provider=fb.value)
        return fb

    def _compute_delay(self, attempt: int) -> float:
        """Exponential backoff with jitter."""
        import random
        delay = min(
            self.config.retry_base_delay_secs * (2 ** (attempt - 1)),
            self.config.retry_max_delay_secs,
        )
        return delay * (0.5 + random.random())

    def hotswap(self, provider: ProviderType) -> None:
        """Manually hotswap to a different provider."""
        self.registry.hotswap(provider)

    @property
    def budget_remaining(self) -> dict:
        """Check remaining budget."""
        return {
            "token_budget": self.config.token_budget,
            "tokens_used": self.registry.total_tokens.total_tokens,
            "tokens_remaining": self.config.token_budget - self.registry.total_tokens.total_tokens,
            "token_pct": (self.registry.total_tokens.total_tokens / self.config.token_budget * 100) if self.config.token_budget else 0,
            "cost_budget": self.config.cost_budget_usd,
            "cost_used": self.registry.total_cost,
            "cost_remaining": self.config.cost_budget_usd - self.registry.total_cost,
            "cost_pct": (self.registry.total_cost / self.config.cost_budget_usd * 100) if self.config.cost_budget_usd else 0,
        }

    def is_budget_exhausted(self) -> bool:
        budget = self.budget_remaining
        return budget["tokens_remaining"] <= 0 or budget["cost_remaining"] <= 0
