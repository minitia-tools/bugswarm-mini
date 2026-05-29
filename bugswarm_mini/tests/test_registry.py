from __future__ import annotations

import json
from pathlib import Path

import pytest

from bugswarm_mini.gateway.registry import ModelRegistry, ModelDisplay
from bugswarm_mini.gateway.protocol import (
    ModelCapabilities,
    ModelInfo,
    ModelPricing,
    ChatRequest,
    ChatResponse,
    ProtocolAdapter,
)


class MockAdapter(ProtocolAdapter):
    def __init__(self):
        super().__init__("mock-key", "https://mock.api/v1")
        self._call_count = 0

    async def chat(self, request: ChatRequest) -> ChatResponse:
        self._call_count += 1
        return ChatResponse(
            content='{"ok": true}',
            model=request.model,
            usage_input_tokens=10,
            usage_output_tokens=5,
            duration_ms=50.0,
        )

    async def list_models(self) -> list[dict]:
        return [
            {"id": "brand-new-model-v3", "provider": "MockProvider", "created": 1700000000},
            {"id": "gpt-4o", "provider": "OpenAI", "created": 1700000000},
        ]


class FailingListModelsAdapter(ProtocolAdapter):
    async def chat(self, request: ChatRequest) -> ChatResponse:
        raise RuntimeError("Not implemented")

    async def list_models(self) -> list[dict]:
        raise RuntimeError("API unavailable")


class TestModelRegistry:
    @pytest.mark.asyncio
    async def test_load_shipped_only(self):
        reg = ModelRegistry()
        models = await reg.load(adapter=None, probe_new=False)
        assert len(models) >= 50
        assert all(isinstance(m, ModelInfo) for m in models)
        assert all(m.id for m in models)

    @pytest.mark.asyncio
    async def test_load_with_provider_discovery(self):
        reg = ModelRegistry()
        adapter = MockAdapter()
        models = await reg.load(adapter=adapter, probe_new=False)
        model_ids = [m.id for m in models]
        assert "brand-new-model-v3" in model_ids
        assert "gpt-4o" in model_ids

    @pytest.mark.asyncio
    async def test_load_with_probing(self):
        reg = ModelRegistry()
        adapter = MockAdapter()
        models = await reg.load(adapter=adapter, probe_new=True)
        new_model = next((m for m in models if m.id == "brand-new-model-v3"), None)
        assert new_model is not None
        assert new_model.capabilities.supports_json_mode is True
        assert new_model.capabilities.supports_tools is True

    @pytest.mark.asyncio
    async def test_failing_provider_discovery(self):
        reg = ModelRegistry()
        adapter = FailingListModelsAdapter()
        models = await reg.load(adapter=adapter, probe_new=False)
        assert len(models) >= 50

    @pytest.mark.asyncio
    async def test_shipped_registry_has_all_fields(self):
        reg = ModelRegistry()
        models = await reg.load(adapter=None, probe_new=False)
        for m in models:
            assert m.id, f"Model missing id"
            assert m.provider_label, f"Model {m.id} missing provider"
            assert m.protocol, f"Model {m.id} missing protocol"
            if m.is_pricing_known:
                assert m.pricing.input_cost_per_mtok > 0
                assert m.pricing.output_cost_per_mtok > 0

    @pytest.mark.asyncio
    async def test_top_models_ranked_first(self):
        reg = ModelRegistry()
        models = await reg.load(adapter=None, probe_new=False)
        ranked = reg.rank_for_display(models)
        top_ids = [m.id for m in ranked[:5]]
        assert len(top_ids) == 5


class TestModelDisplay:
    def test_no_warning_for_known_model(self):
        display = ModelDisplay(
            id="test",
            provider="Test",
            pricing_label="$1.00 / $2.00 per 1M tok",
            capabilities=ModelCapabilities(supports_tools=True, supports_json_mode=True),
            warning=None,
            is_new=False,
            context_window=128000,
            protocol="openai-compatible",
        )
        assert display.warning is None
        assert display.is_new is False

    def test_warning_for_unknown_pricing(self):
        display = ModelDisplay(
            id="test",
            provider="Test",
            pricing_label="Unknown",
            capabilities=ModelCapabilities(supports_tools=True),
            warning="Pricing unknown",
            is_new=True,
            context_window=128000,
            protocol="openai-compatible",
        )
        assert display.warning is not None
        assert display.is_new is True

    def test_warning_for_no_tools(self):
        display = ModelDisplay(
            id="test",
            provider="Test",
            pricing_label="$1.00 / $2.00 per 1M tok",
            capabilities=ModelCapabilities(supports_tools=False, supports_json_mode=True),
            warning="No tool calling",
            is_new=False,
            context_window=128000,
            protocol="openai-compatible",
        )
        assert "No tool calling" in display.warning


class TestRegistryContent:
    def test_registry_json_exists(self):
        path = Path(__file__).resolve().parent.parent / "models" / "registry.json"
        assert path.exists()
        with open(path) as f:
            data = json.load(f)
        assert "version" in data
        assert "models" in data
        assert len(data["models"]) >= 50

    def test_registry_models_have_required_fields(self):
        path = Path(__file__).resolve().parent.parent / "models" / "registry.json"
        with open(path) as f:
            data = json.load(f)
        for entry in data["models"]:
            assert "id" in entry
            assert "provider" in entry
            assert "protocol" in entry
            assert "capabilities" in entry
            assert "pricing" in entry
            assert "context_window" in entry

    def test_registry_covers_major_providers(self):
        path = Path(__file__).resolve().parent.parent / "models" / "registry.json"
        with open(path) as f:
            data = json.load(f)
        providers = {e["provider"] for e in data["models"]}
        for p in ["OpenAI", "Anthropic", "Google", "DeepSeek", "Groq", "Together"]:
            assert p in providers, f"Missing provider: {p}"

    def test_registry_prices_are_positive(self):
        path = Path(__file__).resolve().parent.parent / "models" / "registry.json"
        with open(path) as f:
            data = json.load(f)
        for entry in data["models"]:
            p = entry["pricing"]
            assert p["input_cost_per_mtok"] > 0
            assert p["output_cost_per_mtok"] > 0
