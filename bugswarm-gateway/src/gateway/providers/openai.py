"""OpenAI provider adapter."""

from __future__ import annotations

import structlog
from openai import AsyncOpenAI

from ..types import (
    ChatRequest, ChatResponse, ChatMessage, MessageRole,
    ProviderConfig, ProviderType, TokenUsage, CostInfo,
)
from .base import BaseProviderAdapter

logger = structlog.get_logger(__name__)


class OpenAIAdapter(BaseProviderAdapter):
    """Adapter for OpenAI API (GPT-4o, GPT-4o-mini, etc.)."""

    def __init__(self, config: ProviderConfig):
        super().__init__(config)
        self.client = AsyncOpenAI(
            api_key=config.api_key,
            base_url=config.base_url,
            timeout=config.timeout_secs,
            max_retries=0,  # We handle retries at the gateway level
        )

    async def chat(self, request: ChatRequest) -> ChatResponse:
        messages = self._convert_messages(request.messages)
        model = request.model or self.config.default_model

        kwargs = {
            "model": model,
            "messages": messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "top_p": request.top_p,
        }
        if request.seed is not None:
            kwargs["seed"] = request.seed
        if request.stop:
            kwargs["stop"] = request.stop
        if request.response_format:
            kwargs["response_format"] = request.response_format

        response = await self.client.chat.completions.create(**kwargs)

        choice = response.choices[0]
        content = choice.message.content or ""

        usage = TokenUsage(
            input_tokens=response.usage.prompt_tokens if response.usage else 0,
            output_tokens=response.usage.completion_tokens if response.usage else 0,
            total_tokens=response.usage.total_tokens if response.usage else 0,
            cached_input_tokens=getattr(response.usage, 'prompt_tokens_details', None) and
                getattr(response.usage.prompt_tokens_details, 'cached_tokens', 0) or 0,
        )

        cost = self.estimate_cost(usage.input_tokens, usage.output_tokens)

        return ChatResponse(
            content=content,
            model=response.model,
            provider=ProviderType.OPENAI,
            usage=usage,
            cost=cost,
            finish_reason=choice.finish_reason or "stop",
            request_id=response.id,
        )

    def _convert_messages(self, messages: list[ChatMessage]) -> list[dict]:
        converted = []
        for msg in messages:
            entry: dict = {"role": msg.role.value, "content": msg.content}
            if msg.name:
                entry["name"] = msg.name
            if msg.tool_calls:
                entry["tool_calls"] = msg.tool_calls
            if msg.tool_call_id:
                entry["tool_call_id"] = msg.tool_call_id
            converted.append(entry)
        return converted

    async def count_tokens(self, messages: list[ChatMessage]) -> int:
        try:
            import tiktoken
            enc = tiktoken.encoding_for_model(self.config.default_model)
            total = 0
            for msg in messages:
                total += len(enc.encode(msg.content))
                total += 4  # Message overhead
            total += 2  # Reply priming
            return total
        except Exception:
            # Fallback: ~4 chars per token
            return sum(len(m.content) // 4 for m in messages)
