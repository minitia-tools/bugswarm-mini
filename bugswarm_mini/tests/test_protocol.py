from __future__ import annotations

import pytest

from bugswarm_mini.gateway.protocol import (
    create_adapter,
    PROVIDER_PROTOCOLS,
    ModelCapabilities,
    ModelInfo,
    ModelPricing,
    ChatRequest,
    ChatResponse,
    OpenAICompatibleAdapter,
    AnthropicAdapter,
    GoogleAdapter,
    OllamaAdapter,
)


class TestModelCapabilities:
    def test_empty_capabilities(self):
        caps = ModelCapabilities()
        assert caps.is_empty() is True

    def test_non_empty_capabilities(self):
        caps = ModelCapabilities(supports_tools=True)
        assert caps.is_empty() is False


class TestModelInfo:
    def test_pricing_known(self):
        info = ModelInfo(
            id="test-model",
            provider_label="Test",
            protocol="openai-compatible",
            pricing=ModelPricing(input_cost_per_mtok=1.0, output_cost_per_mtok=2.0),
        )
        assert info.is_pricing_known is True
        assert info.has_full_capabilities is False

    def test_pricing_unknown(self):
        info = ModelInfo(
            id="test-model",
            provider_label="Test",
            protocol="openai-compatible",
        )
        assert info.is_pricing_known is False

    def test_full_capabilities(self):
        info = ModelInfo(
            id="test-model",
            provider_label="Test",
            protocol="openai-compatible",
            capabilities=ModelCapabilities(supports_tools=True, supports_json_mode=True),
        )
        assert info.has_full_capabilities is True


class TestChatRequest:
    def test_to_dict_basic(self):
        req = ChatRequest(
            model="test-model",
            messages=[{"role": "user", "content": "Hello"}],
        )
        d = req.to_dict()
        assert d["model"] == "test-model"
        assert d["messages"] == [{"role": "user", "content": "Hello"}]
        assert d["temperature"] == 0.7
        assert "tools" not in d

    def test_to_dict_with_tools(self):
        req = ChatRequest(
            model="test-model",
            messages=[{"role": "user", "content": "Hi"}],
            tools=[{"type": "function", "function": {"name": "test"}}],
        )
        d = req.to_dict()
        assert d["tools"] == [{"type": "function", "function": {"name": "test"}}]

    def test_to_dict_json_mode(self):
        req = ChatRequest(
            model="test-model",
            messages=[{"role": "user", "content": "Hi"}],
            json_mode=True,
        )
        d = req.to_dict()
        assert d["response_format"] == {"type": "json_object"}

    def test_to_anthropic_dict_system_separate(self):
        req = ChatRequest(
            model="claude-3",
            messages=[
                {"role": "system", "content": "You are a helpful assistant"},
                {"role": "user", "content": "Hello"},
            ],
        )
        d = req.to_anthropic_dict()
        assert d["system"] == "You are a helpful assistant"
        assert d["messages"] == [{"role": "user", "content": "Hello"}]

    def test_to_anthropic_dict_tools(self):
        req = ChatRequest(
            model="claude-3",
            messages=[{"role": "user", "content": "Hi"}],
            tools=[
                {
                    "type": "function",
                    "function": {"name": "test", "description": "A test", "parameters": {"type": "object"}},
                }
            ],
        )
        d = req.to_anthropic_dict()
        assert d["tools"] == [{"name": "test", "description": "A test", "input_schema": {"type": "object"}}]


class TestChatResponse:
    def test_total_tokens(self):
        resp = ChatResponse(
            content="Hello",
            model="test",
            usage_input_tokens=10,
            usage_output_tokens=20,
            duration_ms=100.0,
        )
        assert resp.total_tokens == 30

    def test_zero_tokens(self):
        resp = ChatResponse(
            content="",
            model="test",
            usage_input_tokens=0,
            usage_output_tokens=0,
            duration_ms=0.0,
        )
        assert resp.total_tokens == 0


class TestProviderProtocols:
    def test_all_providers_have_protocols(self):
        assert len(PROVIDER_PROTOCOLS) >= 10
        assert "OpenAI" in PROVIDER_PROTOCOLS
        assert "DeepSeek" in PROVIDER_PROTOCOLS
        assert "Anthropic" in PROVIDER_PROTOCOLS
        assert "Google" in PROVIDER_PROTOCOLS
        assert "OpenRouter" in PROVIDER_PROTOCOLS
        assert "Ollama" in PROVIDER_PROTOCOLS

    def test_openai_protocol(self):
        proto, url = PROVIDER_PROTOCOLS["OpenAI"]
        assert proto == "openai-compatible"
        assert "openai.com" in url

    def test_anthropic_protocol(self):
        proto, url = PROVIDER_PROTOCOLS["Anthropic"]
        assert proto == "anthropic"
        assert "anthropic.com" in url

    def test_ollama_protocol(self):
        proto, url = PROVIDER_PROTOCOLS["Ollama"]
        assert proto == "ollama"
        assert "localhost" in url


class TestCreateAdapter:
    def test_create_openai(self):
        adapter = create_adapter("OpenAI", "sk-test-key")
        assert isinstance(adapter, OpenAICompatibleAdapter)
        assert adapter.base_url == "https://api.openai.com/v1"

    def test_create_deepseek(self):
        adapter = create_adapter("DeepSeek", "sk-test-key")
        assert isinstance(adapter, OpenAICompatibleAdapter)
        assert adapter.base_url == "https://api.deepseek.com/v1"

    def test_create_anthropic(self):
        adapter = create_adapter("Anthropic", "sk-ant-test-key")
        assert isinstance(adapter, AnthropicAdapter)

    def test_create_google(self):
        adapter = create_adapter("Google", "test-key")
        assert isinstance(adapter, GoogleAdapter)

    def test_create_ollama(self):
        adapter = create_adapter("Ollama")
        assert isinstance(adapter, OllamaAdapter)

    def test_create_openrouter(self):
        adapter = create_adapter("OpenRouter", "sk-test-key")
        assert isinstance(adapter, OpenAICompatibleAdapter)
        assert "openrouter.ai" in adapter.base_url

    def test_create_unknown_provider(self):
        with pytest.raises(ValueError, match="Unknown provider"):
            create_adapter("NonExistent", "key")

    def test_create_with_base_url_override(self):
        adapter = create_adapter("OpenAI", "sk-test", base_url_override="https://custom.url/v1")
        assert adapter.base_url == "https://custom.url/v1"


class TestProviderAliases:
    def test_openai_compatible_aliases(self):
        adapter = OpenAICompatibleAdapter("key", "https://api.deepseek.com/v1")
        assert adapter.PROVIDER_ALIASES["https://api.deepseek.com/v1"] == "DeepSeek"
        assert adapter.PROVIDER_ALIASES["https://api.openai.com/v1"] == "OpenAI"
        assert len(adapter.PROVIDER_ALIASES) >= 8


class TestOpenAICompatibleAdapter:
    @pytest.mark.asyncio
    async def test_get_headers(self):
        adapter = OpenAICompatibleAdapter("sk-test-key")
        headers = adapter.get_headers()
        assert headers["Authorization"] == "Bearer sk-test-key"
        assert headers["Content-Type"] == "application/json"

    @pytest.mark.asyncio
    async def test_get_headers_no_key(self):
        adapter = OpenAICompatibleAdapter()
        headers = adapter.get_headers()
        assert headers["Content-Type"] == "application/json"
        assert "Authorization" not in headers


class TestAnthropicAdapter:
    @pytest.mark.asyncio
    async def test_get_headers(self):
        adapter = AnthropicAdapter("sk-ant-test-key")
        headers = adapter.get_headers()
        assert headers["x-api-key"] == "sk-ant-test-key"
        assert headers["anthropic-version"] == "2023-06-01"


class TestGoogleAdapter:
    @pytest.mark.asyncio
    async def test_get_headers(self):
        adapter = GoogleAdapter("test-key")
        headers = adapter.get_headers()
        assert headers["Content-Type"] == "application/json"
        assert "Authorization" not in headers


class TestOllamaAdapter:
    @pytest.mark.asyncio
    async def test_get_headers(self):
        adapter = OllamaAdapter()
        headers = adapter.get_headers()
        assert headers["Content-Type"] == "application/json"

    def test_default_base_url(self):
        adapter = OllamaAdapter()
        assert adapter.base_url == "http://localhost:11434"
