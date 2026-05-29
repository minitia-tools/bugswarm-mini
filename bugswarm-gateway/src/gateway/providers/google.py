"""Google Gemini provider adapter — enterprise grade."""

from __future__ import annotations

import asyncio
import time

import structlog

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


class GoogleAdapter(BaseProviderAdapter):
    """Adapter for Google Gemini API (Gemini 1.5 Flash, Gemini 1.5 Pro, etc.)."""

    def __init__(self, config: ProviderConfig):
        super().__init__(config)
        self._client = None

    def _get_client(self):
        if self._client is None:
            from google import genai

            self._client = genai.Client(
                api_key=self.config.api_key,
                http_options={"timeout": int(self.config.timeout_secs * 1000)},
            )
        return self._client

    async def chat(self, request: ChatRequest) -> ChatResponse:
        client = self._get_client()
        model = request.model or self.config.default_model

        contents = self._convert_messages(request.messages)
        system_instruction = None
        filtered_contents = []
        for c in contents:
            if c.get("role") == "system":
                system_instruction = c["parts"][0]["text"] if c.get("parts") else None
            else:
                filtered_contents.append(c)

        try:
            from google.genai.types import GenerateContentConfig

            config = GenerateContentConfig(
                temperature=request.temperature,
                top_p=request.top_p,
                max_output_tokens=request.max_tokens,
            )
            if system_instruction:
                config.system_instruction = system_instruction
        except ImportError:
            config = None

        t0 = time.monotonic()
        try:
            response = await asyncio.to_thread(
                lambda: client.models.generate_content(
                    model=model,
                    contents=filtered_contents,
                    config=config,
                )
            )
        except Exception as e:
            self._track_error()
            error_type = self.classify_error(e)
            if error_type is FatalError:
                raise FatalError(str(e)) from e
            raise RetryableError(str(e)) from e

        elapsed = (time.monotonic() - t0) * 1000
        self._track_request(elapsed)

        content = response.text or ""

        usage = TokenUsage(
            input_tokens=response.usage_metadata.prompt_token_count if response.usage_metadata else 0,
            output_tokens=response.usage_metadata.candidates_token_count if response.usage_metadata else 0,
            total_tokens=response.usage_metadata.total_token_count if response.usage_metadata else 0,
        )

        cost = self.estimate_cost(usage.input_tokens, usage.output_tokens)

        return ChatResponse(
            content=content,
            model=model,
            provider=ProviderType.GOOGLE,
            usage=usage,
            cost=cost,
            finish_reason="stop",
            latency_ms=elapsed,
        )

    def _convert_messages(self, messages: list[ChatMessage]) -> list[dict]:
        contents = []
        for msg in messages:
            role = "user"
            if msg.role == MessageRole.SYSTEM:
                role = "system"
            elif msg.role == MessageRole.ASSISTANT:
                role = "model"
            contents.append({"role": role, "parts": [{"text": msg.content}]})
        return contents

    async def count_tokens(self, messages: list[ChatMessage]) -> int:
        """Use Gemini's native token counting when available."""
        try:
            client = self._get_client()
            total = 0
            for msg in messages:
                result = await asyncio.to_thread(
                    lambda m=msg: client.models.count_tokens(
                        model=self.config.default_model,
                        contents={"role": "user", "parts": [{"text": m.content}]},
                    )
                )
                total += result.total_tokens if result else 0
            return total
        except Exception:
            return await super().count_tokens(messages)
