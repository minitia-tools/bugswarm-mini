"""Base provider adapter — shared enterprise logic for all LLM providers.

Features: retry classification, token counting fallback, streaming support,
cost estimation, request tracing, response schema validation.
"""

from __future__ import annotations

import asyncio
import time
from abc import ABC, abstractmethod
from typing import AsyncIterator

import structlog

from ..types import (
    ChatRequest, ChatResponse, ChatMessage,
    ProviderConfig, CostInfo, TokenUsage,
)

logger = structlog.get_logger(__name__)


class RetryableError(Exception):
    """Error that should be retried with backoff."""
    pass


class FatalError(Exception):
    """Error that should NOT be retried (401, 403, invalid request)."""
    pass


class BaseProviderAdapter(ABC):
    """Abstract base for all LLM provider adapters."""

    def __init__(self, config: ProviderConfig):
        self.config = config
        self._request_count = 0
        self._error_count = 0
        self._total_latency_ms = 0.0

    @abstractmethod
    async def chat(self, request: ChatRequest) -> ChatResponse:
        """Send a chat request and return a complete response."""
        ...

    async def chat_stream(self, request: ChatRequest) -> AsyncIterator[str]:
        """Stream chat tokens one at a time. Override for real SSE streaming."""
        response = await self.chat(request)
        # Default: yield full content (backward compat)
        yield response.content

    async def count_tokens(self, messages: list[ChatMessage]) -> int:
        """Count tokens for a message list. Override for provider-specific tokenizers."""
        # Universal fallback: ~4 chars per token
        return sum(max(1, len(m.content) // 4) for m in messages)

    def estimate_cost(self, input_tokens: int, output_tokens: int) -> CostInfo:
        """Estimate cost from token counts and provider pricing."""
        input_cost = (input_tokens / 1_000_000) * self.config.input_cost_per_mtok
        output_cost = (output_tokens / 1_000_000) * self.config.output_cost_per_mtok
        return CostInfo(
            input_cost_usd=input_cost,
            output_cost_usd=output_cost,
            total_cost_usd=input_cost + output_cost,
            model=self.config.default_model,
            provider=self.config.provider.value,
        )

    def classify_error(self, error: Exception) -> type:
        """Classify an error as retryable or fatal."""
        msg = str(error).lower()
        # Fatal errors
        if any(code in msg for code in ["401", "403", "404", "invalid api key", "insufficient_quota"]):
            return FatalError
        # Retryable errors
        if any(code in msg for code in ["429", "500", "502", "503", "504",
                                         "rate limit", "timeout", "connection",
                                         "server error", "overloaded"]):
            return RetryableError
        # Unknown — retry once
        return RetryableError

    def _track_request(self, latency_ms: float) -> None:
        self._request_count += 1
        self._total_latency_ms += latency_ms

    def _track_error(self) -> None:
        self._error_count += 1

    @property
    def avg_latency_ms(self) -> float:
        return self._total_latency_ms / max(self._request_count, 1)

    @property
    def error_rate(self) -> float:
        return self._error_count / max(self._request_count + self._error_count, 1)

    @property
    def stats(self) -> dict:
        return {
            "provider": self.config.provider.value,
            "model": self.config.default_model,
            "requests": self._request_count,
            "errors": self._error_count,
            "error_rate": round(self.error_rate, 4),
            "avg_latency_ms": round(self.avg_latency_ms, 1),
        }
