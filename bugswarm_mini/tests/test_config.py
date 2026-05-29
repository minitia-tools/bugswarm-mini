from __future__ import annotations

import json
from pathlib import Path

import pytest

from bugswarm_mini.gateway.config import (
    BugSwarmConfig,
    load_config,
    save_config,
    delete_config,
    detect_api_key_provider,
    load_model_cache,
    save_model_cache,
)


class TestBugSwarmConfig:
    def test_default_config(self):
        config = BugSwarmConfig()
        assert config.provider == "DeepSeek"
        assert config.api_key == ""
        assert config.model == ""
        assert config.budget_tokens == 10_000_000
        assert config.is_configured is False

    def test_configured(self):
        config = BugSwarmConfig(provider="OpenAI", api_key="sk-test", model="gpt-4o")
        assert config.is_configured is True

    def test_resolve_protocol_known(self):
        config = BugSwarmConfig(provider="Anthropic")
        assert config.resolve_protocol() == "anthropic"

    def test_resolve_protocol_unknown(self):
        config = BugSwarmConfig(provider="UnknownProvider")
        assert config.resolve_protocol() == "openai-compatible"

    def test_resolve_base_url_known(self):
        config = BugSwarmConfig(provider="DeepSeek")
        assert "deepseek.com" in config.resolve_base_url()

    def test_resolve_base_url_override(self):
        config = BugSwarmConfig(provider="OpenAI", base_url="https://custom.url/v1")
        assert config.resolve_base_url() == "https://custom.url/v1"

    def test_api_key_display_short(self):
        config = BugSwarmConfig(api_key="short")
        assert config.api_key_display == "***"

    def test_api_key_display_normal(self):
        config = BugSwarmConfig(api_key="sk-abcdefghijklmnop")
        assert config.api_key_display == "sk-a...mnop"

    def test_resolve_base_url_unconfigured(self):
        config = BugSwarmConfig(provider="NonExistent")
        assert config.resolve_base_url() == ""


class TestConfigPersistence:
    def test_save_and_load(self, tmp_path):
        original = BugSwarmConfig(
            provider="Anthropic",
            api_key="sk-ant-test-key-12345",
            model="claude-sonnet-4",
            budget_tokens=5_000_000,
        )
        save_config(original)

        loaded = load_config()
        assert loaded.provider == "Anthropic"
        assert loaded.api_key == "sk-ant-test-key-12345"
        assert loaded.model == "claude-sonnet-4"
        assert loaded.budget_tokens == 5_000_000

        delete_config()

    def test_load_default_when_no_config(self):
        delete_config()
        config = load_config()
        assert config.provider == "DeepSeek"
        assert config.api_key == ""

    def test_load_corrupted_config(self):
        config_path = Path.home() / ".config" / "bugswarm" / "config.json"
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text("not valid json{")

        config = load_config()
        assert config.provider == "DeepSeek"

        delete_config()

    def test_config_file_permissions(self):
        config = BugSwarmConfig(
            provider="OpenAI",
            api_key="sk-secret-key-12345",
            model="gpt-4o",
        )
        save_config(config)

        config_path = Path.home() / ".config" / "bugswarm" / "config.json"
        assert config_path.exists()

        with open(config_path) as f:
            data = json.load(f)
        assert data["api_key"] == "sk-secret-key-12345"

        delete_config()


class TestDetectApiKeyProvider:
    def test_anthropic_key(self):
        assert detect_api_key_provider("sk-ant-test123") == "Anthropic"

    def test_openai_project_key(self):
        assert detect_api_key_provider("sk-proj-test-key") == "OpenAI"

    def test_groq_key(self):
        assert detect_api_key_provider("gsk_test_key_123") == "Groq"

    def test_perplexity_key(self):
        assert detect_api_key_provider("pplx-test-key") == "Perplexity"

    def test_unknown_key(self):
        assert detect_api_key_provider("arbitrary-string") is None

    def test_empty_key(self):
        assert detect_api_key_provider("") is None


class TestModelCache:
    def test_no_cache_returns_empty(self):
        from bugswarm_mini.gateway.config import CONFIG_DIR

        cache_path = CONFIG_DIR / "model_cache.json"
        if cache_path.exists():
            cache_path.unlink()
        cache = load_model_cache()
        assert cache == {}

    def test_save_and_load_cache(self):
        from bugswarm_mini.gateway.config import CONFIG_DIR

        cache_path = CONFIG_DIR / "model_cache.json"
        if cache_path.exists():
            cache_path.unlink()
        cache = {"test-model": {"capabilities": {"supports_tools": True}, "probed_at": 1000}}
        save_model_cache(cache)
        loaded = load_model_cache()
        assert loaded["test-model"]["capabilities"]["supports_tools"] is True
        cache_path.unlink()

    def test_overwrite_cache(self):
        from bugswarm_mini.gateway.config import CONFIG_DIR

        cache_path = CONFIG_DIR / "model_cache.json"
        if cache_path.exists():
            cache_path.unlink()
        save_model_cache({"a": {"capabilities": {"supports_tools": False}, "probed_at": 0}})
        save_model_cache({"b": {"capabilities": {"supports_tools": True}, "probed_at": 1}})
        loaded = load_model_cache()
        assert "a" not in loaded
        assert loaded["b"]["capabilities"]["supports_tools"] is True
        cache_path.unlink()
