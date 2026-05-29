from __future__ import annotations

import json
import os
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any

from .protocol import PROVIDER_PROTOCOLS


CONFIG_DIR = Path.home() / ".config" / "bugswarm"
CONFIG_FILE = CONFIG_DIR / "config.json"
MODEL_CACHE_FILE = CONFIG_DIR / "model_cache.yaml"


@dataclass
class BugSwarmConfig:
    provider: str = "DeepSeek"
    api_key: str = ""
    base_url: str | None = None
    model: str = ""
    budget_tokens: int = 10_000_000
    budget_dollars: float | None = None
    track_usage: bool = True

    def resolve_protocol(self) -> str:
        if self.provider in PROVIDER_PROTOCOLS:
            return PROVIDER_PROTOCOLS[self.provider][0]
        return "openai-compatible"

    def resolve_base_url(self) -> str:
        if self.base_url:
            return self.base_url
        if self.provider in PROVIDER_PROTOCOLS:
            return PROVIDER_PROTOCOLS[self.provider][1]
        return ""

    @property
    def is_configured(self) -> bool:
        return bool(self.api_key and self.model)

    @property
    def api_key_display(self) -> str:
        if len(self.api_key) <= 8:
            return "***"
        return self.api_key[:4] + "..." + self.api_key[-4:]


def get_config_path() -> Path:
    return CONFIG_FILE


def load_config() -> BugSwarmConfig:
    path = get_config_path()
    if not path.exists():
        return BugSwarmConfig()
    try:
        with open(path) as f:
            data = json.load(f)
        return BugSwarmConfig(**data)
    except (json.JSONDecodeError, TypeError, KeyError):
        return BugSwarmConfig()


def save_config(config: BugSwarmConfig) -> None:
    path = get_config_path()
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        json.dump(asdict(config), f, indent=2)
    os.chmod(path, 0o600)


def delete_config() -> None:
    path = get_config_path()
    if path.exists():
        path.unlink()


def load_model_cache() -> dict[str, dict]:
    path = CONFIG_DIR / "model_cache.json"
    if not path.exists():
        return {}
    try:
        with open(path) as f:
            return json.load(f)
    except (json.JSONDecodeError, FileNotFoundError):
        return {}


def save_model_cache(cache: dict[str, dict]) -> None:
    path = CONFIG_DIR / "model_cache.json"
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        json.dump(cache, f, indent=2)


def detect_api_key_provider(api_key: str) -> str | None:
    if api_key.startswith("sk-ant-"):
        return "Anthropic"
    if api_key.startswith("sk-proj-"):
        return "OpenAI"
    if api_key.startswith("sk-") and len(api_key) == 44:
        return "OpenAI"
    if api_key.startswith("gsk_"):
        return "Groq"
    if api_key.startswith("pplx-"):
        return "Perplexity"
    return None
