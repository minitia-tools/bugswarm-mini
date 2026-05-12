"""Anthropic (Claude) provider adapter."""

from __future__ import annotations

import structlog
from anthropic import AsyncAnthropic

from ..types import (
    ChatRequest, ChatResponse, ChatMessage, MessageRole,
    ProviderConfig, ProviderType, TokenUsage,
)
from .base import BaseProviderAdapter

logger = structlog.get_logger(__name__)


class AnthropicAdapter(BaseProviderAdapter):
    """Adapter for Anthropic API (Claude 3.5 Sonnet, Claude 3 Opus, etc.)."""

    def __init__(self, config: ProviderConfig):
        super().__init__(config)
        self.client = AsyncAnthropic(
            api_key=config.api_key,
            base_url=config.base_url,
            timeout=config.timeout_secs,
            max_retries=0,
        )

    async def chat(self, request: ChatRequest) -> ChatResponse:
        system_prompt, user_messages = self._split_messages(request.messages)
        model = request.model or self.config.default_model

        kwargs = {
            "model": model,
            "messages": user_messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "top_p": request.top_p,
        }
        if system_prompt:
            kwargs["system"] = system_prompt

        response = await self.client.messages.create(**kwargs)

        content = ""
        for block in response.content:
            if block.type == "text":
                content += block.text

        usage = TokenUsage(
            input_tokens=response.usage.input_tokens if response.usage else 0,
            output_tokens=response.usage.output_tokens if response.usage else 0,
            total_tokens=(
                (response.usage.input_tokens or 0) + (response.usage.output_tokens or 0)
                if response.usage else 0
            ),
            cached_input_tokens=getattr(response.usage, 'cache_read_input_tokens', 0) if response.usage else 0,
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
        )

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
        try:
            import tiktoken
            enc = tiktoken.get_encoding("cl100k_base")
            total = 0
            for msg in messages:
                total += len(enc.encode(msg.content))
            return total
        except Exception:
            return sum(len(m.content) // 4 for m in messages)
