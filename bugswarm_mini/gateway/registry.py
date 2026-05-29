from __future__ import annotations

import asyncio
import json
import time
from dataclasses import dataclass
from pathlib import Path

import structlog

from .config import load_model_cache, save_model_cache
from .protocol import (
    ChatRequest,
    ModelCapabilities,
    ModelInfo,
    ModelPricing,
    ProtocolAdapter,
)

logger = structlog.get_logger(__name__)

SHIPPED_REGISTRY = Path(__file__).resolve().parent.parent / "models" / "registry.json"
REMOTE_REGISTRY_URL = "https://models.bugswarm.ai/v1/registry.json"
LOCAL_CACHE_PATH = Path.home() / ".config" / "bugswarm" / "registry_cache.json"


@dataclass
class ModelDisplay:
    id: str
    provider: str
    pricing_label: str
    capabilities: ModelCapabilities
    warning: str | None
    is_new: bool
    context_window: int
    protocol: str


class ModelRegistry:
    def __init__(self):
        self._shipped: dict[str, ModelInfo] = {}
        self._remote: dict[str, ModelInfo] = {}
        self._dynamic: dict[str, ModelInfo] = {}
        self.last_refreshed: float = 0.0
        self._loaded = False

    async def load(
        self,
        adapter: ProtocolAdapter | None = None,
        probe_new: bool = True,
    ) -> list[ModelInfo]:
        self._shipped = self._load_shipped()
        remote_success = await self._try_fetch_remote()
        if remote_success:
            self._remote = self._parse_remote(self._remote_cache)
            self.last_refreshed = time.time()

        merged = dict(self._shipped)
        for mid, info in self._remote.items():
            merged[mid] = info

        if adapter:
            provider_models = await self._fetch_provider_models(adapter)
            for pm in provider_models:
                mid = pm["id"]
                if mid not in merged:
                    cached = load_model_cache().get(mid)
                    if cached:
                        merged[mid] = self._dict_to_model_info(mid, cached)
                    else:
                        merged[mid] = ModelInfo(
                            id=mid,
                            provider_label=pm.get("provider", "Unknown"),
                            protocol="dynamic",
                        )
                else:
                    merged[mid].provider_label = pm.get("provider", merged[mid].provider_label)

            if probe_new:
                unknowns = [m for m in merged.values() if not m.has_full_capabilities and not m.is_pricing_known]
                if unknowns:
                    probed = await self._probe_unknown_models(adapter, unknowns)
                    for mid, info in probed.items():
                        merged[mid] = info
                        self._dynamic[mid] = info
                        cache_entry = {
                            "capabilities": {
                                "supports_tools": info.capabilities.supports_tools,
                                "supports_json_mode": info.capabilities.supports_json_mode,
                                "supports_thinking": info.capabilities.supports_thinking,
                                "max_input_tokens": info.capabilities.max_input_tokens,
                                "max_output_tokens": info.capabilities.max_output_tokens,
                            },
                            "probed_at": time.time(),
                        }
                        existing_cache = load_model_cache()
                        existing_cache[mid] = cache_entry
                        save_model_cache(existing_cache)

        self._loaded = True
        return list(merged.values())

    def _load_shipped(self) -> dict[str, ModelInfo]:
        with open(SHIPPED_REGISTRY) as f:
            data = json.load(f)
        result = {}
        for entry in data.get("models", []):
            caps = ModelCapabilities(**entry.get("capabilities", {}))
            pricing_data = entry.get("pricing")
            pricing = ModelPricing(**pricing_data) if pricing_data else None
            result[entry["id"]] = ModelInfo(
                id=entry["id"],
                provider_label=entry["provider"],
                protocol=entry["protocol"],
                capabilities=caps,
                pricing=pricing,
                context_window=entry.get("context_window", 128_000),
                knowledge_cutoff=entry.get("knowledge_cutoff", ""),
            )
        return result

    _remote_cache: dict | None = None

    async def _try_fetch_remote(self) -> bool:
        try:
            import httpx

            async with httpx.AsyncClient(timeout=2.0) as client:
                resp = await client.get(REMOTE_REGISTRY_URL)
                resp.raise_for_status()
                self._remote_cache = resp.json()
                LOCAL_CACHE_PATH.parent.mkdir(parents=True, exist_ok=True)
                with open(LOCAL_CACHE_PATH, "w") as f:
                    json.dump(self._remote_cache, f)
                return True
        except Exception as e:
            logger.debug(f"Remote registry fetch failed: {e}")
            if LOCAL_CACHE_PATH.exists():
                try:
                    with open(LOCAL_CACHE_PATH) as f:
                        self._remote_cache = json.load(f)
                    logger.info("Loaded remote registry from local cache")
                    return True
                except Exception:
                    pass
            return False

    def _parse_remote(self, data: dict) -> dict[str, ModelInfo]:
        result = {}
        for entry in data.get("models", []):
            caps = ModelCapabilities(**entry.get("capabilities", {}))
            pricing_data = entry.get("pricing")
            pricing = ModelPricing(**pricing_data) if pricing_data else None
            result[entry["id"]] = ModelInfo(
                id=entry["id"],
                provider_label=entry["provider"],
                protocol=entry["protocol"],
                capabilities=caps,
                pricing=pricing,
                context_window=entry.get("context_window", 128_000),
                knowledge_cutoff=entry.get("knowledge_cutoff", ""),
            )
        return result

    async def _fetch_provider_models(self, adapter: ProtocolAdapter) -> list[dict]:
        try:
            models = await asyncio.wait_for(adapter.list_models(), timeout=10.0)
            return sorted(models, key=lambda m: m.get("created", 0), reverse=True)
        except Exception as e:
            logger.warning(f"Provider model discovery failed: {e}")
            return []

    async def _probe_unknown_models(
        self,
        adapter: ProtocolAdapter,
        models: list[ModelInfo],
        max_probe: int = 10,
    ) -> dict[str, ModelInfo]:
        to_probe = [m for m in models if not m.has_full_capabilities][:max_probe]
        if not to_probe:
            return {}

        async def probe_one(model: ModelInfo) -> tuple[str, ModelInfo]:
            caps = await self._probe_capabilities(adapter, model.id)
            model.capabilities = caps
            return (model.id, model)

        results = await asyncio.gather(
            *[probe_one(m) for m in to_probe],
            return_exceptions=True,
        )
        probed = {}
        for r in results:
            if isinstance(r, tuple):
                mid, info = r
                probed[mid] = info
        return probed

    async def _probe_capabilities(self, adapter: ProtocolAdapter, model_id: str) -> ModelCapabilities:
        caps = ModelCapabilities()

        try:
            req = ChatRequest(
                model=model_id,
                messages=[{"role": "user", "content": 'Return {"ok": true}'}],
                max_tokens=20,
                json_mode=True,
            )
            resp = await asyncio.wait_for(adapter.chat(req), timeout=8.0)
            caps.supports_json_mode = True
            caps.max_output_tokens = resp.usage_output_tokens
        except Exception:
            caps.supports_json_mode = False

        try:
            req = ChatRequest(
                model=model_id,
                messages=[{"role": "user", "content": "What is 2+2?"}],
                tools=[
                    {
                        "type": "function",
                        "function": {
                            "name": "calculate",
                            "description": "Calculate a math expression",
                            "parameters": {
                                "type": "object",
                                "properties": {"expr": {"type": "string"}},
                                "required": ["expr"],
                            },
                        },
                    }
                ],
                max_tokens=20,
            )
            resp = await asyncio.wait_for(adapter.chat(req), timeout=8.0)
            caps.supports_tools = True
        except Exception:
            caps.supports_tools = False

        caps.max_input_tokens = await self._probe_context_window(adapter, model_id)
        return caps

    async def _probe_context_window(self, adapter: ProtocolAdapter, model_id: str) -> int:
        try:
            req = ChatRequest(
                model=model_id,
                messages=[
                    {"role": "user", "content": "Hello"},
                    {"role": "assistant", "content": "Hi"},
                    {"role": "user", "content": "A" * 100_000},
                ],
                max_tokens=5,
            )
            resp = await asyncio.wait_for(adapter.chat(req), timeout=15.0)
            return resp.usage_input_tokens
        except Exception:
            return 128_000

    def rank_for_display(self, models: list[ModelInfo]) -> list[ModelDisplay]:
        def sort_key(m: ModelInfo) -> tuple:
            has_pricing = 0 if m.is_pricing_known else 1
            has_caps = 0 if m.has_full_capabilities else 1
            return (has_pricing, has_caps, m.id)

        sorted_models = sorted(models, key=sort_key)

        result = []
        for m in sorted_models:
            warning = None
            if not m.is_pricing_known:
                warning = "Pricing unknown — budget mode uses token ceiling only. Dollar estimates unavailable."
            elif not m.capabilities.supports_tools:
                warning = "No tool calling — BugSwarm will not use CPG or sandbox. Text analysis only."

            pricing_label = (
                f"${m.pricing.input_cost_per_mtok:.2f} in / ${m.pricing.output_cost_per_mtok:.2f} out per 1M tok"
                if m.pricing
                else "Unknown"
            )

            result.append(
                ModelDisplay(
                    id=m.id,
                    provider=m.provider_label,
                    pricing_label=pricing_label,
                    capabilities=m.capabilities,
                    warning=warning,
                    is_new=not m.is_pricing_known,
                    context_window=m.context_window,
                    protocol=m.protocol,
                )
            )
        return result

    def _dict_to_model_info(self, model_id: str, data: dict) -> ModelInfo:
        caps_data = data.get("capabilities", {})
        caps = ModelCapabilities(**caps_data) if caps_data else ModelCapabilities()
        return ModelInfo(
            id=model_id,
            provider_label="Unknown",
            protocol="dynamic",
            capabilities=caps,
        )
