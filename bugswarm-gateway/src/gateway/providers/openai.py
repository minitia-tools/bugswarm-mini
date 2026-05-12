"""OpenAI provider adapter — enterprise grade with error classification and streaming."""

from __future__ import annotations

import time
from typing import AsyncIterator

import structlog
from openai import AsyncOpenAI, APIError, APITimeoutError, RateLimitError, AuthenticationError

from ..types import (
    ChatRequest, ChatResponse, ChatMessage, MessageRole,
    ProviderConfig, ProviderType, TokenUsage,
)
from .base import BaseProviderAdapter, RetryableError, FatalError

logger = structlog.get_logger(__name__)


class OpenAIAdapter(BaseProviderAdapter):
    """Adapter for OpenAI API (GPT-4o, GPT-4o-mini, etc.).

    Also used for DeepSeek (OpenAI-compatible API) via base_url override.
    """

    def __init__(self, config: ProviderConfig):
        super().__init__(config)
        self.client = AsyncOpenAI(
            api_key=config.api_key,
            base_url=config.base_url,
            timeout=float(config.timeout_secs),
            max_retries=0,  # Gateway handles retries
        )

    async def chat(self, request: ChatRequest) -> ChatResponse:
        messages = self._convert_messages(request.messages)
        model = request.model or self.config.default_model

        kwargs: dict = {
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

        t0 = time.monotonic()
        try:
            response = await self.client.chat.completions.create(**kwargs)
        except (RateLimitError, APITimeoutError) as e:
            self._track_error()
            raise RetryableError(str(e)) from e
        except AuthenticationError as e:
            self._track_error()
            raise FatalError(str(e)) from e
        except APIError as e:
            self._track_error()
            if e.status_code and 500 <= e.status_code < 600:
                raise RetryableError(str(e)) from e
            raise FatalError(str(e)) from e

        elapsed = (time.monotonic() - t0) * 1000
        self._track_request(elapsed)

        choice = response.choices[0]
        content = choice.message.content or ""

        usage = TokenUsage(
            input_tokens=response.usage.prompt_tokens if response.usage else 0,
            output_tokens=response.usage.completion_tokens if response.usage else 0,
            total_tokens=response.usage.total_tokens if response.usage else 0,
        )

        cost = self.estimate_cost(usage.input_tokens, usage.output_tokens)

        return ChatResponse(
            content=content, model=response.model, provider=ProviderType.OPENAI,
            usage=usage, cost=cost, finish_reason=choice.finish_reason or "stop",
            request_id=response.id, latency_ms=elapsed,
        )

    async def chat_stream(self, request: ChatRequest) -> AsyncIterator[str]:
        messages = self._convert_messages(request.messages)
        model = request.model or self.config.default_model

        kwargs: dict = {
            "model": model, "messages": messages,
            "temperature": request.temperature, "max_tokens": request.max_tokens,
            "top_p": request.top_p, "stream": True,
        }
        if request.seed is not None:
            kwargs["seed"] = request.seed

        try:
            stream = await self.client.chat.completions.create(**kwargs)
            async for chunk in stream:
                if chunk.choices and chunk.choices[0].delta.content:
                    yield chunk.choices[0].delta.content
        except Exception as e:
            logger.error("openai_stream_error", error=str(e)[:200])
            raise

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
        """Use tiktoken for accurate OpenAI token counting."""
        try:
            import tiktoken
            enc = tiktoken.encoding_for_model(self.config.default_model)
            total = 0
            for msg in messages:
                total += len(enc.encode(msg.content))
                total += 4  # Message framing overhead
            total += 2  # Reply priming
            return total
        except Exception:
            return await super().count_tokens(messages)
