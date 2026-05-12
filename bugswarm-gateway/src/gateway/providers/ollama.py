"""Ollama (local) provider adapter."""

from __future__ import annotations

import structlog
import httpx

from ..types import (
    ChatRequest, ChatResponse, ChatMessage, MessageRole,
    ProviderConfig, ProviderType, TokenUsage,
)
from .base import BaseProviderAdapter

logger = structlog.get_logger(__name__)


class OllamaAdapter(BaseProviderAdapter):
    """Adapter for local Ollama models (Llama 3, etc.)."""

    def __init__(self, config: ProviderConfig):
        super().__init__(config)
        self._http = httpx.AsyncClient(
            base_url=config.base_url or "http://localhost:11434",
            timeout=config.timeout_secs,
        )

    async def chat(self, request: ChatRequest) -> ChatResponse:
        model = request.model or self.config.default_model
        messages = self._convert_messages(request.messages)

        payload = {
            "model": model,
            "messages": messages,
            "stream": False,
            "options": {
                "temperature": request.temperature,
                "top_p": request.top_p,
                "num_predict": request.max_tokens,
            },
        }
        if request.seed is not None:
            payload["options"]["seed"] = request.seed

        resp = await self._http.post("/api/chat", json=payload)
        resp.raise_for_status()
        data = resp.json()

        content = data.get("message", {}).get("content", "")

        # Ollama reports eval_count and prompt_eval_count
        input_tokens = data.get("prompt_eval_count", 0)
        output_tokens = data.get("eval_count", 0)

        usage = TokenUsage(
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            total_tokens=input_tokens + output_tokens,
        )

        cost = self.estimate_cost(usage.input_tokens, usage.output_tokens)

        return ChatResponse(
            content=content,
            model=data.get("model", model),
            provider=ProviderType.OLLAMA,
            usage=usage,
            cost=cost,
            finish_reason=data.get("done_reason", "stop"),
        )

    def _convert_messages(self, messages: list[ChatMessage]) -> list[dict]:
        return [
            {"role": msg.role.value, "content": msg.content}
            for msg in messages
        ]

    async def count_tokens(self, messages: list[ChatMessage]) -> int:
        # For local models, use character-based estimation
        return sum(len(m.content) // 4 for m in messages)

    async def close(self) -> None:
        await self._http.aclose()
