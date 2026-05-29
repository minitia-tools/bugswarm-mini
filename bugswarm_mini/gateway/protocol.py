from __future__ import annotations

import time
from abc import ABC, abstractmethod
from dataclasses import dataclass, field

import httpx

DEFAULT_TIMEOUT = 30.0


@dataclass
class ModelCapabilities:
    supports_tools: bool = False
    supports_json_mode: bool = False
    supports_thinking: bool = False
    supports_streaming: bool = False
    max_input_tokens: int = 128_000
    max_output_tokens: int = 4_096

    def is_empty(self) -> bool:
        return not any(
            [
                self.supports_tools,
                self.supports_json_mode,
                self.supports_thinking,
                self.supports_streaming,
            ]
        )


@dataclass
class ModelPricing:
    input_cost_per_mtok: float
    output_cost_per_mtok: float

    def as_tuple(self) -> tuple[float, float]:
        return (self.input_cost_per_mtok, self.output_cost_per_mtok)


@dataclass
class ModelInfo:
    id: str
    provider_label: str
    protocol: str
    capabilities: ModelCapabilities = field(default_factory=ModelCapabilities)
    pricing: ModelPricing | None = None
    context_window: int = 128_000
    knowledge_cutoff: str = ""

    @property
    def is_pricing_known(self) -> bool:
        return self.pricing is not None

    @property
    def has_full_capabilities(self) -> bool:
        return not self.capabilities.is_empty()


@dataclass
class ChatRequest:
    model: str
    messages: list[dict]
    temperature: float = 0.7
    max_tokens: int = 4_096
    tools: list[dict] | None = None
    json_mode: bool = False

    def to_dict(self) -> dict:
        body: dict = {
            "model": self.model,
            "messages": self.messages,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
        }
        if self.tools:
            body["tools"] = self.tools
        if self.json_mode:
            body["response_format"] = {"type": "json_object"}
        return body

    def to_anthropic_dict(self) -> dict:
        system = None
        messages = []
        for msg in self.messages:
            if msg["role"] == "system":
                system = msg["content"]
            else:
                messages.append(msg)
        body: dict = {
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
        }
        if system:
            body["system"] = system
        if self.tools:
            anthropic_tools = []
            for t in self.tools:
                if t.get("type") == "function":
                    fn = t.get("function", {})
                    anthropic_tools.append(
                        {
                            "name": fn.get("name", ""),
                            "description": fn.get("description", ""),
                            "input_schema": fn.get("parameters", {}),
                        }
                    )
            body["tools"] = anthropic_tools
        if self.json_mode:
            body["extra_headers"] = {"anthropic-beta": "max-tokens-3-5-sonnet-2024-07-15"}
        return body

    def to_gemini_dict(self) -> dict:
        contents = []
        for msg in self.messages:
            role = "model" if msg["role"] == "assistant" else "user"
            contents.append(
                {
                    "role": role,
                    "parts": [{"text": msg["content"]}],
                }
            )
        body: dict = {
            "contents": contents,
            "generationConfig": {
                "temperature": self.temperature,
                "maxOutputTokens": self.max_tokens,
            },
        }
        if self.tools:
            body["tools"] = [
                {
                    "functionDeclarations": [
                        {
                            "name": t.get("function", {}).get("name", ""),
                            "description": t.get("function", {}).get("description", ""),
                            "parameters": t.get("function", {}).get("parameters", {}),
                        }
                        for t in self.tools
                        if t.get("type") == "function"
                    ]
                }
            ]
        return body

    def to_ollama_dict(self) -> dict:
        body: dict = {
            "model": self.model,
            "messages": self.messages,
            "temperature": self.temperature,
            "options": {"num_predict": self.max_tokens},
        }
        if self.tools:
            body["tools"] = [
                {
                    "type": "function",
                    "function": {
                        "name": t.get("function", {}).get("name", ""),
                        "description": t.get("function", {}).get("description", ""),
                        "parameters": t.get("function", {}).get("parameters", {}),
                    },
                }
                for t in self.tools
                if t.get("type") == "function"
            ]
        if self.json_mode:
            body["format"] = "json"
        return body


@dataclass
class ChatResponse:
    content: str
    model: str
    usage_input_tokens: int
    usage_output_tokens: int
    duration_ms: float

    @property
    def total_tokens(self) -> int:
        return self.usage_input_tokens + self.usage_output_tokens


class ProtocolAdapter(ABC):
    def __init__(self, api_key: str = "", base_url: str | None = None):
        self.api_key = api_key
        self.base_url = base_url
        self._client: httpx.AsyncClient | None = None

    async def _get_client(self) -> httpx.AsyncClient:
        if self._client is None:
            self._client = httpx.AsyncClient(timeout=DEFAULT_TIMEOUT)
        return self._client

    async def close(self) -> None:
        if self._client:
            await self._client.aclose()
            self._client = None

    @abstractmethod
    async def chat(self, request: ChatRequest) -> ChatResponse: ...

    async def chat_with_retry(self, request: ChatRequest, max_retries: int = 3) -> ChatResponse:
        last_error: Exception | None = None
        for attempt in range(max_retries):
            try:
                return await self.chat(request)
            except httpx.HTTPStatusError as e:
                if e.response.status_code in (429, 502, 503, 504) and attempt < max_retries - 1:
                    wait = 2**attempt
                    await asyncio.sleep(wait)
                    last_error = e
                    continue
                raise
            except Exception as e:
                last_error = e
                if attempt < max_retries - 1:
                    wait = 2**attempt
                    await asyncio.sleep(wait)
                    continue
                raise
        raise RuntimeError(f"Max retries ({max_retries}) exceeded") from last_error

    async def list_models(self) -> list[dict]:
        return []

    async def health_check(self) -> bool:
        try:
            await self.list_models()
            return True
        except Exception:
            return False

    def get_headers(self) -> dict:
        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        return headers


import asyncio


class OpenAICompatibleAdapter(ProtocolAdapter):
    PROVIDER_ALIASES: dict[str, str] = {
        "https://api.openai.com/v1": "OpenAI",
        "https://api.deepseek.com/v1": "DeepSeek",
        "https://api.deepseek.com": "DeepSeek",
        "https://api.groq.com/openai/v1": "Groq",
        "https://api.together.xyz/v1": "Together",
        "https://api.fireworks.ai/inference/v1": "Fireworks",
        "https://api.perplexity.ai": "Perplexity",
        "https://models.inference.ai.azure.com": "GitHub Models",
        "https://openrouter.ai/api/v1": "OpenRouter",
        "https://api.mistral.ai/v1": "Mistral",
        "https://api.x.ai/v1": "xAI",
    }

    async def chat(self, request: ChatRequest) -> ChatResponse:
        client = await self._get_client()
        start = time.monotonic()
        resp = await client.post(
            f"{self.base_url}/chat/completions",
            headers=self.get_headers(),
            json=request.to_dict(),
        )
        resp.raise_for_status()
        duration = (time.monotonic() - start) * 1000
        data = resp.json()
        choice = data["choices"][0]
        content = choice.get("message", {}).get("content", "") or ""
        usage = data.get("usage", {})
        return ChatResponse(
            content=content,
            model=data.get("model", request.model),
            usage_input_tokens=usage.get("prompt_tokens", 0),
            usage_output_tokens=usage.get("completion_tokens", 0),
            duration_ms=duration,
        )

    async def list_models(self) -> list[dict]:
        client = await self._get_client()
        resp = await client.get(
            f"{self.base_url}/models",
            headers={"Authorization": f"Bearer {self.api_key}"},
        )
        resp.raise_for_status()
        return resp.json().get("data", [])


class AnthropicAdapter(ProtocolAdapter):
    def __init__(self, api_key: str = "", base_url: str | None = None):
        super().__init__(api_key, base_url or "https://api.anthropic.com/v1")

    def get_headers(self) -> dict:
        return {
            "Content-Type": "application/json",
            "x-api-key": self.api_key,
            "anthropic-version": "2023-06-01",
        }

    async def chat(self, request: ChatRequest) -> ChatResponse:
        client = await self._get_client()
        start = time.monotonic()

        system = None
        messages = []
        for msg in request.messages:
            if msg["role"] == "system":
                system = msg["content"]
            else:
                messages.append(msg)

        body: dict = {
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        }
        if system:
            body["system"] = system
        if request.tools:
            anthropic_tools = []
            for t in request.tools:
                if t.get("type") == "function":
                    fn = t.get("function", {})
                    anthropic_tools.append(
                        {
                            "name": fn.get("name", ""),
                            "description": fn.get("description", ""),
                            "input_schema": fn.get("parameters", {}),
                        }
                    )
            body["tools"] = anthropic_tools
        if request.json_mode:
            body["extra_headers"] = {"anthropic-beta": "max-tokens-3-5-sonnet-2024-07-15"}

        resp = await client.post(
            f"{self.base_url}/messages",
            headers=self.get_headers(),
            json=body,
        )
        resp.raise_for_status()
        duration = (time.monotonic() - start) * 1000
        data = resp.json()
        content = "".join(b.get("text", "") for b in data.get("content", []) if b.get("type") == "text")
        usage = data.get("usage", {})
        return ChatResponse(
            content=content,
            model=data.get("model", request.model),
            usage_input_tokens=usage.get("input_tokens", 0),
            usage_output_tokens=usage.get("output_tokens", 0),
            duration_ms=duration,
        )


class GoogleAdapter(ProtocolAdapter):
    def __init__(self, api_key: str = "", base_url: str | None = None):
        super().__init__(api_key, base_url or "https://generativelanguage.googleapis.com/v1beta")

    def get_headers(self) -> dict:
        return {"Content-Type": "application/json"}

    async def chat(self, request: ChatRequest) -> ChatResponse:
        client = await self._get_client()
        start = time.monotonic()

        contents = []
        for msg in request.messages:
            role = "model" if msg["role"] == "assistant" else "user"
            contents.append(
                {
                    "role": role,
                    "parts": [{"text": msg["content"]}],
                }
            )

        body: dict = {
            "contents": contents,
            "generationConfig": {
                "temperature": request.temperature,
                "maxOutputTokens": request.max_tokens,
            },
        }
        if request.tools:
            body["tools"] = [
                {
                    "functionDeclarations": [
                        {
                            "name": t.get("function", {}).get("name", ""),
                            "description": t.get("function", {}).get("description", ""),
                            "parameters": t.get("function", {}).get("parameters", {}),
                        }
                        for t in request.tools
                        if t.get("type") == "function"
                    ]
                }
            ]

        url = f"{self.base_url}/models/{request.model}:generateContent?key={self.api_key}"
        resp = await client.post(url, headers=self.get_headers(), json=body)
        resp.raise_for_status()
        duration = (time.monotonic() - start) * 1000
        data = resp.json()
        candidate = data.get("candidates", [{}])[0]
        content = "".join(p.get("text", "") for p in candidate.get("content", {}).get("parts", []))
        usage = data.get("usageMetadata", {})
        return ChatResponse(
            content=content,
            model=f"models/{request.model}",
            usage_input_tokens=usage.get("promptTokenCount", 0),
            usage_output_tokens=usage.get("candidatesTokenCount", 0),
            duration_ms=duration,
        )

    async def list_models(self) -> list[dict]:
        client = await self._get_client()
        url = f"{self.base_url}/models?key={self.api_key}"
        resp = await client.get(url, headers=self.get_headers())
        resp.raise_for_status()
        return [
            m for m in resp.json().get("models", []) if "generateContent" in m.get("supportedGenerationMethods", [])
        ]


class OllamaAdapter(ProtocolAdapter):
    def __init__(self, api_key: str = "", base_url: str | None = None):
        super().__init__(api_key, base_url or "http://localhost:11434")

    def get_headers(self) -> dict:
        return {"Content-Type": "application/json"}

    async def chat(self, request: ChatRequest) -> ChatResponse:
        client = await self._get_client()
        start = time.monotonic()

        body: dict = {
            "model": request.model,
            "messages": request.messages,
            "temperature": request.temperature,
            "options": {"num_predict": request.max_tokens},
        }
        if request.tools:
            body["tools"] = [
                {
                    "type": "function",
                    "function": {
                        "name": t.get("function", {}).get("name", ""),
                        "description": t.get("function", {}).get("description", ""),
                        "parameters": t.get("function", {}).get("parameters", {}),
                    },
                }
                for t in request.tools
                if t.get("type") == "function"
            ]
        if request.json_mode:
            body["format"] = "json"

        resp = await client.post(
            f"{self.base_url}/api/chat",
            headers=self.get_headers(),
            json=body,
        )
        resp.raise_for_status()
        duration = (time.monotonic() - start) * 1000
        data = resp.json()
        return ChatResponse(
            content=data.get("message", {}).get("content", ""),
            model=data.get("model", request.model),
            usage_input_tokens=data.get("prompt_eval_count", 0),
            usage_output_tokens=data.get("eval_count", 0),
            duration_ms=duration,
        )

    async def list_models(self) -> list[dict]:
        client = await self._get_client()
        resp = await client.get(f"{self.base_url}/api/tags")
        resp.raise_for_status()
        return [{"id": m["name"], "provider": "Ollama"} for m in resp.json().get("models", [])]


PROVIDER_PROTOCOLS: dict[str, tuple[str, str]] = {
    "OpenAI": ("openai-compatible", "https://api.openai.com/v1"),
    "DeepSeek": ("openai-compatible", "https://api.deepseek.com/v1"),
    "Anthropic": ("anthropic", "https://api.anthropic.com/v1"),
    "Google": ("google", "https://generativelanguage.googleapis.com/v1beta"),
    "OpenRouter": ("openai-compatible", "https://openrouter.ai/api/v1"),
    "Groq": ("openai-compatible", "https://api.groq.com/openai/v1"),
    "Together": ("openai-compatible", "https://api.together.xyz/v1"),
    "Fireworks": ("openai-compatible", "https://api.fireworks.ai/inference/v1"),
    "Perplexity": ("openai-compatible", "https://api.perplexity.ai"),
    "GitHub Models": ("openai-compatible", "https://models.inference.ai.azure.com"),
    "Mistral": ("openai-compatible", "https://api.mistral.ai/v1"),
    "xAI": ("openai-compatible", "https://api.x.ai/v1"),
    "Ollama": ("ollama", "http://localhost:11434"),
}


def create_adapter(
    provider: str,
    api_key: str = "",
    base_url_override: str | None = None,
) -> ProtocolAdapter:
    if provider not in PROVIDER_PROTOCOLS:
        raise ValueError(f"Unknown provider '{provider}'. Available: {', '.join(sorted(PROVIDER_PROTOCOLS.keys()))}")
    proto, url = PROVIDER_PROTOCOLS[provider]
    base_url = base_url_override or url

    match proto:
        case "openai-compatible":
            adapter = OpenAICompatibleAdapter(api_key, base_url)
            adapter.PROVIDER_ALIASES[base_url] = provider
            return adapter
        case "anthropic":
            return AnthropicAdapter(api_key, base_url)
        case "google":
            return GoogleAdapter(api_key, base_url)
        case "ollama":
            return OllamaAdapter("", base_url)
        case _:
            raise ValueError(f"Unknown protocol '{proto}' for provider '{provider}'")
