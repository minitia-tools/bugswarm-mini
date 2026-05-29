"""Core types for the LLM Gateway."""

from __future__ import annotations

import hashlib
import hmac
import os
import time
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any

from pydantic import BaseModel, Field


def secrets_compare(a: str, b: str) -> bool:
    """Constant-time comparison of two secret strings.

    Uses hmac.compare_digest to prevent timing side-channel attacks
    when comparing API keys, tokens, or other sensitive values.
    """
    return hmac.compare_digest(a.encode("utf-8"), b.encode("utf-8"))


def _load_api_key(key_name: str) -> str:
    """Load API key from env var or Docker secret file."""
    provider = os.getenv("BGSWARM_SECRETS_PROVIDER", "env")
    if provider == "file":
        path = os.getenv(f"{key_name}_FILE", f"/run/secrets/{key_name.lower()}")
        try:
            return Path(path).read_text().strip()
        except Exception:
            return ""
    return os.getenv(key_name, "")


class ProviderType(str, Enum):
    OPENAI = "openai"
    ANTHROPIC = "anthropic"
    GOOGLE = "google"
    OLLAMA = "ollama"
    DEEPSEEK = "deepseek"


class MessageRole(str, Enum):
    SYSTEM = "system"
    USER = "user"
    ASSISTANT = "assistant"
    TOOL = "tool"


class ChatMessage(BaseModel):
    role: MessageRole
    content: str
    name: str | None = None
    tool_call_id: str | None = None
    tool_calls: list[dict[str, Any]] | None = None


class ChatRequest(BaseModel):
    messages: list[ChatMessage]
    model: str | None = None  # If None, provider's default is used
    temperature: float = Field(default=0.7, ge=0.0, le=2.0)
    max_tokens: int = Field(default=4096, ge=1, le=128000)
    top_p: float = Field(default=1.0, ge=0.0, le=1.0)
    seed: int | None = None
    stop: list[str] | None = None
    response_format: dict[str, Any] | None = None  # For JSON mode
    extra: dict[str, Any] = Field(default_factory=dict)


class TokenUsage(BaseModel):
    input_tokens: int = 0
    output_tokens: int = 0
    total_tokens: int = 0
    cached_input_tokens: int = 0  # For provider caching (Claude prompt caching, etc.)


class CostInfo(BaseModel):
    input_cost_usd: float = 0.0
    output_cost_usd: float = 0.0
    total_cost_usd: float = 0.0
    model: str = ""
    provider: str = ""


class ChatResponse(BaseModel):
    content: str
    model: str
    provider: ProviderType
    usage: TokenUsage
    cost: CostInfo
    finish_reason: str = "stop"
    latency_ms: float = 0.0
    request_id: str | None = None
    raw_response: dict[str, Any] | None = None


@dataclass
class ProviderConfig:
    """Configuration for a single provider."""
    provider: ProviderType
    api_key: str = ""
    base_url: str | None = None
    default_model: str = ""
    max_retries: int = 3
    timeout_secs: float = 120.0
    # Cost per million tokens
    input_cost_per_mtok: float = 0.0
    output_cost_per_mtok: float = 0.0
    # Model-specific overrides
    extra: dict[str, Any] = field(default_factory=dict)


@dataclass
class GatewayConfig:
    """Top-level gateway configuration."""
    providers: dict[ProviderType, ProviderConfig] = field(default_factory=dict)
    default_provider: ProviderType = ProviderType.OPENAI
    fallback_providers: list[ProviderType] = field(default_factory=list)
    # Token budget
    token_budget: int = 5_000_000
    cost_budget_usd: float = 50.0
    # Retry config
    max_total_retries: int = 5
    retry_base_delay_secs: float = 1.0
    retry_max_delay_secs: float = 60.0
    retryable_statuses: list[int] = field(default_factory=lambda: [429, 502, 503, 504])

    def get_provider_config(self, provider: ProviderType) -> ProviderConfig:
        if provider not in self.providers:
            raise ValueError(f"Provider {provider} not configured")
        return self.providers[provider]

    @property
    def configured_providers(self) -> dict[ProviderType, ProviderConfig]:
        return {p: c for p, c in self.providers.items() if c.api_key.strip()}

    @classmethod
    def from_env(cls, validate: bool = True) -> "GatewayConfig":
        """Build config from environment variables."""
        import os
        providers = {}

        # OpenAI
        if _load_api_key("OPENAI_API_KEY"):
            providers[ProviderType.OPENAI] = ProviderConfig(
                provider=ProviderType.OPENAI,
                api_key=_load_api_key("OPENAI_API_KEY"),
                default_model=os.getenv("OPENAI_MODEL", "gpt-4o-mini"),
                input_cost_per_mtok=0.15,
                output_cost_per_mtok=0.60,
            )

        # Anthropic
        if _load_api_key("ANTHROPIC_API_KEY"):
            providers[ProviderType.ANTHROPIC] = ProviderConfig(
                provider=ProviderType.ANTHROPIC,
                api_key=_load_api_key("ANTHROPIC_API_KEY"),
                default_model=os.getenv("ANTHROPIC_MODEL", "claude-3-5-sonnet-20241022"),
                input_cost_per_mtok=3.0,
                output_cost_per_mtok=15.0,
            )

        # Google
        if os.getenv("GOOGLE_API_KEY"):
            providers[ProviderType.GOOGLE] = ProviderConfig(
                provider=ProviderType.GOOGLE,
                api_key=os.getenv("GOOGLE_API_KEY", ""),
                default_model=os.getenv("GOOGLE_MODEL", "gemini-1.5-flash"),
                input_cost_per_mtok=0.075,
                output_cost_per_mtok=0.30,
            )

        # Ollama
        providers[ProviderType.OLLAMA] = ProviderConfig(
            provider=ProviderType.OLLAMA,
            base_url=os.getenv("OLLAMA_BASE_URL", "http://localhost:11434"),
            default_model=os.getenv("OLLAMA_MODEL", "llama3:8b"),
            input_cost_per_mtok=0.0,
            output_cost_per_mtok=0.0,
        )

        # DeepSeek (OpenAI-compatible API)
        if _load_api_key("DEEPSEEK_API_KEY"):
            providers[ProviderType.DEEPSEEK] = ProviderConfig(
                provider=ProviderType.DEEPSEEK,
                api_key=_load_api_key("DEEPSEEK_API_KEY"),
                base_url="https://api.deepseek.com",
                default_model=os.getenv("DEEPSEEK_MODEL", "deepseek-v4-flash"),
                input_cost_per_mtok=0.14,   # $0.14/1M input tokens
                output_cost_per_mtok=0.28,  # $0.28/1M output tokens
            )

        default = ProviderType(os.getenv("GATEWAY_DEFAULT_PROVIDER", "openai"))

        config = cls(
            providers=providers,
            default_provider=default,
            fallback_providers=[p for p in ProviderType if p != default and p in providers],
            token_budget=int(os.getenv("GATEWAY_TOKEN_BUDGET", "5000000")),
            cost_budget_usd=float(os.getenv("GATEWAY_COST_BUDGET", "50.0")),
        )

        if validate and not config.configured_providers:
            raise ValueError("No API providers configured. Set DEEPSEEK_API_KEY or OPENAI_API_KEY.")

        return config


def request_fingerprint(request: ChatRequest) -> str:
    """Generate a deterministic fingerprint for a chat request."""
    parts = [
        request.model or "",
        str(request.temperature),
        str(request.max_tokens),
        str(request.top_p),
        str(request.seed or ""),
    ]
    for m in request.messages:
        parts.append(f"{m.role}:{m.content[:100]}")
    data = "|".join(parts)
    return hashlib.sha256(data.encode()).hexdigest()[:16]
