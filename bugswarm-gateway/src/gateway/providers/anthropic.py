"""Anthropic (Claude) provider adapter — enterprise grade."""

from __future__ import annotations

import time
from collections.abc import AsyncIterator

import structlog
from anthropic import AsyncAnthropic

from ..types import (
    ChatMessage,
    ChatRequest,
    ChatResponse,
    MessageRole,
    ProviderConfig,
    ProviderType,
    TokenUsage,
)
from .base import BaseProviderAdapter, FatalError, RetryableError

logger = structlog.get_logger(__name__)


class AnthropicAdapter(BaseProviderAdapter):
    """Adapter for Anthropic API (Claude 3.5 Sonnet, Claude 3 Opus, etc.)."""

    def __init__(self, config: ProviderConfig):
        super().__init__(config)
        self.client = AsyncAnthropic(
            api_key=config.api_key,
            base_url=config.base_url,
            timeout=float(config.timeout_secs),
            max_retries=0,
        )

    async def chat(self, request: ChatRequest) -> ChatResponse:
        system_prompt, user_messages = self._split_messages(request.messages)
        model = request.model or self.config.default_model

        kwargs: dict = {
            "model": model,
            "messages": user_messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "top_p": request.top_p,
        }
        if system_prompt:
            kwargs["system"] = system_prompt

        t0 = time.monotonic()
        try:
            response = await self.client.messages.create(**kwargs)
        except Exception as e:
            self._track_error()
            error_type = self.classify_error(e)
            if error_type is FatalError:
                raise FatalError(str(e)) from e
            raise RetryableError(str(e)) from e

        elapsed = (time.monotonic() - t0) * 1000
        self._track_request(elapsed)

        content = "".join(block.text for block in response.content if block.type == "text")

        usage = TokenUsage(
            input_tokens=response.usage.input_tokens if response.usage else 0,
            output_tokens=response.usage.output_tokens if response.usage else 0,
            total_tokens=((response.usage.input_tokens or 0) + (response.usage.output_tokens or 0))
            if response.usage
            else 0,
        )

        cost = self.estimate_cost(usage.input_tokens, usage.output_tokens)

        return ChatResponse(
            content=content,
            model=response.model,
            provider=ProviderType.ANTHROPIC,
            usage=usage,
            cost=cost,
            finish_reason=response.stop_reason or "stop",
            request_id=response.id,
            latency_ms=elapsed,
        )

    async def chat_stream(self, request: ChatRequest) -> AsyncIterator[str]:
        system_prompt, user_messages = self._split_messages(request.messages)
        model = request.model or self.config.default_model

        kwargs: dict = {
            "model": model,
            "messages": user_messages,
            "max_tokens": request.max_tokens,
            "stream": True,
        }
        if system_prompt:
            kwargs["system"] = system_prompt

        try:
            async with self.client.messages.stream(**kwargs) as stream:
                async for event in stream:
                    if event.type == "content_block_delta" and event.delta.type == "text_delta":
                        yield event.delta.text
        except Exception as e:
            logger.error("anthropic_stream_error", error=str(e)[:200])
            raise

    def _split_messages(self, messages: list[ChatMessage]) -> tuple[str | None, list[dict]]:
        """Anthropic uses a separate system parameter, not a message role."""
        system_prompt = None
        user_messages = []
        for msg in messages:
            if msg.role == MessageRole.SYSTEM:
                system_prompt = msg.content
            else:
                role = "assistant" if msg.role == MessageRole.ASSISTANT else "user"
                user_messages.append({"role": role, "content": msg.content})
        return system_prompt, user_messages

    async def count_tokens(self, messages: list[ChatMessage]) -> int:
        """Approximate Claude token count using cl100k_base."""
        try:
            import tiktoken

            enc = tiktoken.get_encoding("cl100k_base")
            return sum(len(enc.encode(m.content)) for m in messages)
        except Exception:
            return await super().count_tokens(messages)
