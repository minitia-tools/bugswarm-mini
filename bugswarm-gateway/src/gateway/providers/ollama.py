"""Ollama (local) provider adapter — enterprise grade."""

from __future__ import annotations

import time
from typing import AsyncIterator

import httpx
import structlog

from ..types import (
    ChatRequest, ChatResponse, ChatMessage, MessageRole,
    ProviderConfig, ProviderType, TokenUsage,
)
from .base import BaseProviderAdapter, RetryableError

logger = structlog.get_logger(__name__)


class OllamaAdapter(BaseProviderAdapter):
    """Adapter for local Ollama models (Llama 3, Mistral, etc.)."""

    def __init__(self, config: ProviderConfig):
        super().__init__(config)
        self._http = httpx.AsyncClient(
            base_url=config.base_url or "http://localhost:11434",
            timeout=float(config.timeout_secs),
        )

    async def chat(self, request: ChatRequest) -> ChatResponse:
        model = request.model or self.config.default_model
        messages = self._convert_messages(request.messages)

        payload: dict = {
            "model": model, "messages": messages, "stream": False,
            "options": {
                "temperature": request.temperature,
                "top_p": request.top_p,
                "num_predict": request.max_tokens,
            },
        }
        if request.seed is not None:
            payload["options"]["seed"] = request.seed

        t0 = time.monotonic()
        try:
            resp = await self._http.post("/api/chat", json=payload)
            resp.raise_for_status()
        except httpx.HTTPStatusError as e:
            self._track_error()
            if e.response.status_code >= 500:
                raise RetryableError(str(e)) from e
            raise
        except (httpx.TimeoutException, httpx.ConnectError) as e:
            self._track_error()
            raise RetryableError(str(e)) from e

        elapsed = (time.monotonic() - t0) * 1000
        self._track_request(elapsed)

        data = resp.json()
        content = data.get("message", {}).get("content", "")

        input_tokens = data.get("prompt_eval_count", 0)
        output_tokens = data.get("eval_count", 0)

        usage = TokenUsage(
            input_tokens=input_tokens, output_tokens=output_tokens,
            total_tokens=input_tokens + output_tokens,
        )

        cost = self.estimate_cost(usage.input_tokens, usage.output_tokens)

        return ChatResponse(
            content=content, model=data.get("model", model),
            provider=ProviderType.OLLAMA, usage=usage, cost=cost,
            finish_reason=data.get("done_reason", "stop"), latency_ms=elapsed,
        )

    async def chat_stream(self, request: ChatRequest) -> AsyncIterator[str]:
        model = request.model or self.config.default_model
        messages = self._convert_messages(request.messages)

        payload: dict = {
            "model": model, "messages": messages, "stream": True,
            "options": {"temperature": request.temperature, "num_predict": request.max_tokens},
        }

        async with self._http.stream("POST", "/api/chat", json=payload) as resp:
            resp.raise_for_status()
            async for line in resp.aiter_lines():
                if line.strip():
                    try:
                        import json
                        chunk = json.loads(line)
                        if chunk.get("message", {}).get("content"):
                            yield chunk["message"]["content"]
                    except Exception:
                        continue

    def _convert_messages(self, messages: list[ChatMessage]) -> list[dict]:
        return [{"role": msg.role.value, "content": msg.content} for msg in messages]

    async def close(self) -> None:
        await self._http.aclose()
