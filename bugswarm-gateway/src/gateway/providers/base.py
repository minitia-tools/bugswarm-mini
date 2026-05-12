"""Base provider adapter with shared logic."""

from __future__ import annotations

from ..types import (
    ChatRequest, ChatResponse, ChatMessage,
    ProviderConfig, CostInfo,
)


class BaseProviderAdapter:
    """Base class for all provider adapters."""

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
