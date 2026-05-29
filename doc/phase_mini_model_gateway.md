# Phase Mini: Model Gateway & Cost Control Plane — Architecture Plan

**Status**: Architecture Specification
**Integration Target**: Mini BugSwarm (`bugswarm-mini/`)
**Replaces**: `bugswarm-gateway/` (provider-per-class pattern)

---

## Overview

Three design decisions that everyone else gets wrong:

1. **Providers don't matter. Protocols do.** There are exactly 4 wire protocols in existence. Everything else is a re-skin. Stop modeling providers, model protocols.
2. **Tokens are truth. Dollars are decoration.** If prices change, execution must not break. Dollar estimates are stamped at run start and explicitly labeled as estimates.
3. **The model list is content, not code.** It ships as a JSON registry, not a match/case statement. Users edit it, we update it remotely, and it never requires a code release.

The user flow is:

```
bugswarm configure
  → "Select a provider: [OpenAI-compatible] [Anthropic] [Google] [Ollama] [OpenRouter]"
  → "Paste your API key: __________"
  → System calls provider's GET /models → fetches ALL available models (not just our list)
  → Merges with shipped registry (pricing, capabilities) + probes unknown models in parallel
  → Shows ranked top 15 with prices and capability warnings
  → User selects model → confirm → done
```

The execution flow is:

```
bugswarm run /target
  → Load config (model, key, budget tokens)
  → Fetch live prices from remote registry (timeout 2s, non-blocking)
  → Stamp price snapshot on run record
  → Execute swarm with token budget
  → On completion: usage page shows tokens + price_at_run_start
```

---

## Architecture: 5 Components

```
bugswarm-mini/
├── gateway/
│   ├── __init__.py
│   ├── protocol.py          # ProtocolAdapter base + 4 protocol implementations
│   ├── config.py            # Config loading, env vars, key detection
│   ├── registry.py          # Model registry loader (local + remote refresh)
│   └── cost_tracker.py     # Token tracking, price stamping, usage page
├── models/
│   └── registry.json        # Shipped model registry (~60 entries)
├── cli/
│   ├── configure.py         # Setup wizard
│   └── usage.py             # Billing & usage page
└── data/
    └── usage.db             # SQLite run log for usage history
```

---

## MODULE 1: `protocol.py` — Protocol Adapters (4 total)

### The Insight

There are exactly 4 wire protocols for LLM APIs. Every provider implements one of them:

| Protocol | Implemented By | Wire Format |
|----------|---------------|-------------|
| **OpenAI-compatible** | OpenAI, DeepSeek, OpenRouter, Groq, Together, Fireworks, Perplexity, GitHub Models, Azure, Mistral API, Cohere, xAI, 50+ more | `POST /chat/completions` |
| **Anthropic** | Anthropic | `POST /messages` |
| **Google** | Google Gemini | `POST /models/generateContent` |
| **Ollama** | Ollama (local) | `POST /api/chat` |

No other protocol matters. Every "new" provider is an OpenAI-compatible wrapper. The match/case in the current codebase at `client.py:153-172` creates a new case for each provider — this is replaced entirely.

### ProtocolAdapter Base Class

```python
@dataclass
class ModelCapabilities:
    supports_tools: bool = False
    supports_json_mode: bool = False
    supports_thinking: bool = False
    supports_streaming: bool = False
    max_input_tokens: int = 128_000
    max_output_tokens: int = 4_096

@dataclass
class ModelInfo:
    id: str
    provider_label: str        # "OpenAI", "Anthropic", etc. — display only
    protocol: str              # "openai-compatible", "anthropic", "google", "ollama"
    capabilities: ModelCapabilities
    input_cost_per_mtok: float
    output_cost_per_mtok: float
    context_window: int
    knowledge_cutoff: str = ""

@dataclass
class ChatRequest:
    model: str
    messages: list[dict]
    temperature: float = 0.7
    max_tokens: int = 4_096
    tools: list[dict] | None = None

@dataclass
class ChatResponse:
    content: str
    model: str
    usage_input_tokens: int
    usage_output_tokens: int
    duration_ms: int

class ProtocolAdapter(ABC):
    def __init__(self, api_key: str, base_url: str | None = None):
        self.api_key = api_key
        self.base_url = base_url

    @abstractmethod
    async def chat(self, request: ChatRequest) -> ChatResponse:
        ...

    @abstractmethod
    async def list_models(self) -> list[dict]:
        """GET /models or equivalent. Returns raw list for registry merge."""
        ...
```

### Four Implementations

#### 1. OpenAICompatibleAdapter

```python
class OpenAICompatibleAdapter(ProtocolAdapter):
    """One adapter for ~50+ providers. base_url is the only variable."""

    PROVIDER_ALIASES = {
        "https://api.openai.com/v1": "OpenAI",
        "https://api.deepseek.com/v1": "DeepSeek",
        "https://api.groq.com/openai/v1": "Groq",
        "https://api.together.xyz/v1": "Together",
        "https://api.fireworks.ai/inference/v1": "Fireworks",
        "https://api.perplexity.ai": "Perplexity",
        "https://models.inference.ai.azure.com": "GitHub Models",
        "https://openrouter.ai/api/v1": "OpenRouter",
    }

    async def chat(self, request: ChatRequest) -> ChatResponse:
        async with httpx.AsyncClient(timeout=30.0) as client:
            resp = await client.post(
                f"{self.base_url}/chat/completions",
                headers={"Authorization": f"Bearer {self.api_key}"},
                json=request.to_dict(),
            )
            resp.raise_for_status()
            data = resp.json()
            return ChatResponse(
                content=data["choices"][0]["message"]["content"],
                model=data["model"],
                usage_input_tokens=data["usage"]["prompt_tokens"],
                usage_output_tokens=data["usage"]["completion_tokens"],
                duration_ms=resp.elapsed.total_seconds() * 1000,
            )

    async def list_models(self) -> list[dict]:
        async with httpx.AsyncClient(timeout=10.0) as client:
            resp = await client.get(
                f"{self.base_url}/models",
                headers={"Authorization": f"Bearer {self.api_key}"},
            )
            resp.raise_for_status()
            return resp.json()["data"]
```

#### 2. AnthropicAdapter

```python
class AnthropicAdapter(ProtocolAdapter):
    base_url = "https://api.anthropic.com/v1"

    async def chat(self, request: ChatRequest) -> ChatResponse:
        # Anthropic messages format — system as separate field
        ...
```

#### 3. GoogleAdapter

```python
class GoogleAdapter(ProtocolAdapter):
    base_url = "https://generativelanguage.googleapis.com/v1beta"

    async def chat(self, request: ChatRequest) -> ChatResponse:
        # Gemini format — contents array
        ...
```

#### 4. OllamaAdapter

```python
class OllamaAdapter(ProtocolAdapter):
    base_url = "http://localhost:11434"

    async def chat(self, request: ChatRequest) -> ChatResponse:
        # No API key needed
        ...
```

### Protocol Auto-Detection

The config wizard doesn't ask for base_url. It asks for the provider name, which maps to a known base_url:

```python
PROVIDER_PROTOCOLS = {
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
    "Ollama": ("ollama", "http://localhost:11434"),
}
```

New provider not listed? User selects "OpenAI-compatible" and pastes the base_url manually. Power-user escape hatch, not the common path.

### Construction

```python
def create_adapter(provider: str, api_key: str, base_url_override: str | None = None) -> ProtocolAdapter:
    proto, url = PROVIDER_PROTOCOLS[provider]
    base_url = base_url_override or url
    match proto:
        case "openai-compatible": return OpenAICompatibleAdapter(api_key, base_url)
        case "anthropic":         return AnthropicAdapter(api_key)
        case "google":            return GoogleAdapter(api_key)
        case "ollama":            return OllamaAdapter()
```

---

## MODULE 2: `registry.py` — Model Registry

### Shipped Registry (`models/registry.json`)

A curated JSON file of ~60 models that covers 95% of real-world usage:

```json
{
  "version": 1,
  "updated": "2026-05-28",
  "models": [
    {
      "id": "claude-sonnet-4-20250514",
      "provider": "Anthropic",
      "protocol": "anthropic",
      "capabilities": {
        "supports_tools": true,
        "supports_json_mode": true,
        "supports_thinking": false,
        "supports_streaming": true,
        "max_input_tokens": 200000,
        "max_output_tokens": 8192
      },
      "pricing": {
        "input_cost_per_mtok": 3.00,
        "output_cost_per_mtok": 15.00
      },
      "context_window": 200000,
      "knowledge_cutoff": "2025-04-01"
    },
    {
      "id": "deepseek-v4-flash",
      "provider": "DeepSeek",
      "protocol": "openai-compatible",
      "capabilities": {
        "supports_tools": true,
        "supports_json_mode": true,
        "supports_thinking": false,
        "supports_streaming": true,
        "max_input_tokens": 1000000,
        "max_output_tokens": 384000
      },
      "pricing": {
        "input_cost_per_mtok": 0.14,
        "output_cost_per_mtok": 0.28
      },
      "context_window": 1000000,
      "knowledge_cutoff": "2025-05-01"
    }
  ]
}
```

### Provider Model Discovery

On every `bugswarm configure`, after the user enters API key, the system calls the provider's own model listing endpoint:

```python
class ModelRegistry:
    def __init__(self):
        self.local_path = Path("~/.config/bugswarm/registry.json")
        self.remote_url = "https://models.bugswarm.ai/v1/registry.json"
        self.models: dict[str, ModelInfo] = {}
        self.last_refreshed: float = 0.0
        self.dynamic_entries: dict[str, DynamicModelEntry] = {}  # Probed at configure time

    async def load(self, adapter: ProtocolAdapter | None = None) -> list[ModelInfo]:
        """Load registry with remote refresh and dynamic provider discovery.

        Three sources, merged:
        1. Shipped registry.json (always available)
        2. Remote registry fetch (non-blocking, 2s timeout)
        3. Provider's live model list via adapter.list_models()
        """
        # Step 1: Load shipped + remote (existing logic)
        local = await self._load_shipped()
        remote_success = await self._try_fetch_remote()
        merged = self._merge(local, remote) if remote_success else local

        # Step 2: Fetch provider's live model list
        if adapter:
            provider_models = await self._fetch_provider_models(adapter)

            # Step 3: Merge — every model from provider is included
            # Known models get pricing + capabilities from registry
            # Unknown models get probed dynamically
            for pm in provider_models:
                mid = pm["id"]
                existing = merged.get(mid)
                if existing:
                    # Known model — registry data wins, but update from provider
                    existing.provider_label = pm.get("provider", existing.provider_label)
                else:
                    # Unknown model — will be probed
                    merged[mid] = ModelInfo(
                        id=mid,
                        provider_label=pm.get("provider", "Unknown"),
                        protocol="dynamic",
                        capabilities=ModelCapabilities(),  # Empty until probed
                        pricing=None,  # Unknown
                    )

        return list(merged.values())

    async def _fetch_provider_models(self, adapter: ProtocolAdapter) -> list[dict]:
        """Fetch ALL models from the provider. Timeout at 5s."""
        try:
            models = await adapter.list_models()
            return sorted(models, key=lambda m: m.get("created", 0), reverse=True)
        except Exception as e:
            logger.warning(f"Provider model discovery failed: {e}")
            return []

    async def probe_unknown_models(
        self, adapter: ProtocolAdapter, models: list[ModelInfo], max_probe: int = 15
    ) -> dict[str, ModelInfo]:
        """Probe unknown models for capabilities. Parallel, max 2s per model."""
        to_probe = [m for m in models if m.pricing is None and m.capabilities.is_empty()]

        async def probe_one(model: ModelInfo) -> tuple[str, ModelInfo]:
            caps = await self._probe_capabilities(adapter, model.id)
            model.capabilities = caps
            return (model.id, model)

        results = await asyncio.gather(
            *[probe_one(m) for m in to_probe[:max_probe]],
            return_exceptions=True,
        )
        probed = {}
        for r in results:
            if isinstance(r, tuple):
                mid, info = r
                probed[mid] = info
        return probed


### Capability Probe (Lightweight, ~1-2s)

The probe sends 2 tiny requests to detect tool calling and JSON mode support:

```python
async def _probe_capabilities(self, adapter: ProtocolAdapter, model_id: str) -> ModelCapabilities:
    """Detect model capabilities with 2 lightweight probes.

    Total cost: ~200 tokens per probe model (~$0.00003 for DeepSeek V4 Flash).
    Acceptable for a one-time configure action.
    """
    caps = ModelCapabilities()

    # Probe 1: JSON mode
    try:
        req = ChatRequest(
            model=model_id,
            messages=[{"role": "user", "content": "Say hello in JSON: {\"greeting\": \"...\"}"}],
            max_tokens=20,
        )
        resp = await asyncio.wait_for(adapter.chat(req), timeout=5.0)
        caps.supports_json_mode = True
        caps.max_output_tokens = resp.usage_output_tokens  # Approximate
    except Exception:
        caps.supports_json_mode = False

    # Probe 2: Tool calling
    try:
        req = ChatRequest(
            model=model_id,
            messages=[{"role": "user", "content": "What is 2+2?"}],
            tools=[{
                "type": "function",
                "function": {
                    "name": "calculate",
                    "description": "Calculate a math expression",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "expr": {"type": "string"}
                        },
                        "required": ["expr"]
                    }
                }
            }],
            max_tokens=20,
        )
        resp = await asyncio.wait_for(adapter.chat(req), timeout=5.0)
        caps.supports_tools = True
    except Exception:
        caps.supports_tools = False

    return caps
```

### Merge Priority

| Source | Pricing | Capabilities | Priority |
|--------|---------|-------------|----------|
| Shipped `registry.json` | Known (hardcoded) | Known (tested) | **Highest** — always authoritative |
| Remote `models.bugswarm.ai` | Known (updated) | Known (updated) | Overrides shipped |
| Provider `GET /models` only | Unknown | Unknown (probed) | Included but marked as dynamic |

### Display to User

```python
def rank_for_display(models: list[ModelInfo]) -> list[ModelDisplay]:
    """Rank: known with pricing > probed (known caps) > unprobed.

    Show top 15. User can expand to see all.
    """
    def sort_key(m: ModelInfo) -> tuple:
        # 0 = known + priced, 1 = known no pricing, 2 = probed, 3 = unprobed
        has_pricing = 0 if m.pricing else 1
        has_caps = 0 if not m.capabilities.is_empty() else 1
        return (has_pricing, has_caps, m.id)

    sorted_models = sorted(models, key=sort_key)

    result = []
    for m in sorted_models[:15]:
        warning = None
        if m.pricing is None:
            warning = "Pricing unknown — budget mode uses token ceiling only. Dollar estimates unavailable."
        elif not m.capabilities.supports_tools:
            warning = "No tool calling — BugSwarm will not use CPG or sandbox. Text analysis only."

        result.append(ModelDisplay(
            id=m.id,
            pricing=f"${m.input_cost_per_mtok:.2f}/${m.output_cost_per_mtok:.2f} per 1M tokens"
                    if m.pricing else "Pricing unknown",
            capabilities=m.capabilities,
            warning=warning,
            is_new=m.pricing is None,
        ))
    return result
```

### Provider Edge Cases

| Provider | GET /models | Behavior |
|----------|------------|----------|
| OpenAI | `client.models.list()` | Returns all GPT models. Filter to chat models only. |
| DeepSeek | `GET /models` | Returns all models. Probe resolves capabilities. |
| OpenRouter | `GET /models` (public, no key needed) | Returns 300+ models. **Show top 15 by usage rank.** Allow search/filter. |
| Groq | `GET /models` | Returns ~20 models. All known. |
| Anthropic | **No public endpoint** | Fall back to shipped `registry.json` + note: *"Anthropic does not expose a model list. Models shown are known Claude releases."* |
| Google | `client.models.list()` | Returns Gemini models. |
| Ollama | `GET /api/tags` | Returns locally pulled models. No pricing (free). |

### Remote Registry Refresh

On `bugswarm configure` and `bugswarm run`, a non-blocking fetch from our remote:

```python
async def _try_fetch_remote(self) -> bool:
    """Fetch latest registry from models.bugswarm.ai. 2s timeout. Non-blocking."""
    try:
        async with httpx.AsyncClient(timeout=2.0) as client:
            resp = await client.get(self.remote_url)
            resp.raise_for_status()
            self._remote_cache = resp.json()
            with open(self.local_path, "w") as f:
                json.dump(self._remote_cache, f)
            self.last_refreshed = time.time()
            return True
    except (httpx.TimeoutException, httpx.HTTPError):
        return False
```

### Price Stamping

### Price Stamping

When a run starts, the current prices are retrieved from the registry and **stamped** on the run record:

```python
@dataclass
class RunCostSnapshot:
    model: str
    input_cost_per_mtok: float
    output_cost_per_mtok: float
    registry_version: int
    registry_refreshed_at: str   # ISO timestamp
    warning: str | None = None   # "Prices may be stale — last refreshed 14 days ago"
```

This snapshot is written to the run's metadata. It never changes. The usage page always shows the price as it was when the run started.

---

## MODULE 3: `cost_tracker.py` — Token Budget & Cost Tracking

### The Token-First Rule

```python
@dataclass
class CostTracker:
    budget_tokens: int = 10_000_000        # Default: 10M tokens
    budget_dollars: float | None = None     # Optional: dollar cap

    tokens_consumed: int = 0
    run_cost_snapshot: RunCostSnapshot | None = None

    def record_usage(self, input_tokens: int, output_tokens: int) -> None:
        self.tokens_consumed += input_tokens + output_tokens

    def is_budget_exhausted(self) -> bool:
        """Token budget is the AUTHORITATIVE gate. Dollar budget is secondary."""
        if self.tokens_consumed >= self.budget_tokens:
            return True
        if self.budget_dollars and self.run_cost_snapshot:
            est_cost = self._estimated_cost()
            if est_cost >= self.budget_dollars:
                return True
        return False

    def estimated_cost_dollars(self) -> float:
        """Explicitly labeled as estimate. Based on prices at run start."""
        if not self.run_cost_snapshot:
            return 0.0
        s = self.run_cost_snapshot
        # Simple estimate: assume output = 30% of total tokens
        total_mtok = self.tokens_consumed / 1_000_000
        return total_mtok * (s.input_cost_per_mtok * 0.7 + s.output_cost_per_mtok * 0.3)

    def stale_warning(self) -> str | None:
        if not self.run_cost_snapshot:
            return None
        days_stale = (time.time() - self.run_cost_snapshot.refreshed_at) / 86400
        if days_stale > 30:
            return f"Price estimate may be inaccurate — model prices last refreshed {int(days_stale)} days ago"
        if days_stale > 7:
            return f"Price estimate last refreshed {int(days_stale)} days ago"
        return None
```

### How Budget Works in Practice

| User Says | What Happens |
|-----------|-------------|
| `bugswarm run /target` | Uses default 10M token budget. Dollar estimate shown with `(est.)` |
| `bugswarm run /target --budget 5M` | 5M token ceiling. Hard stop at 5M. |
| `bugswarm run /target --budget $2` | Converts $2 to tokens at run-start prices. Hard stop at tokens. |
| `bugswarm run /target --unlimited` | No budget ceiling. Brave. |

**Critical invariant:** `--budget $2` is converted to tokens ONCE at run start. If prices change mid-run, the token budget is unaffected. The dollar cap has already been converted. No mid-run price sensitivity.

---

## MODULE 4: `configure.py` — Setup Wizard

### User Flow

```
$ bugswarm configure

  ┌──────────────────────────────────────────────┐
  │                                              │
  │   Welcome to BugSwarm!                       │
  │                                              │
  │   First, let's connect your model provider.  │
  │   You can change this later with             │
  │   'bugswarm configure --edit'.               │
  │                                              │
  └──────────────────────────────────────────────┘

  Select your provider:
    1)  OpenAI
    2)  DeepSeek
    3)  Anthropic
    4)  Google (Gemini)
    5)  OpenRouter
    6)  Groq
    7)  Together
    8)  Fireworks
    9)  Perplexity
   10)  GitHub Models
   11)  Ollama (local — no API key needed)
   12)  Other (OpenAI-compatible — enter base_url manually)
  > 2

  Paste your API key (will be saved to ~/.config/bugswarm/config.yaml):
  > sk-abc123def456...

  [Contacting DeepSeek API...]
  [Fetched 37 available models...]
  [Probing 3 new models for capabilities...]

  Available models for DeepSeek (showing top 15 of 37):
    1)  deepseek-v4-flash   $0.14/1M in   $0.28/1M out   [1M ctx]  tools, json  ★ RECOMMENDED
    2)  deepseek-v4-pro     $1.74/1M in   $3.48/1M out   [1M ctx]  tools, json
    3)  deepseek-r2         $0.55/1M in   $2.19/1M out   [128K ctx] thinking
    4)  deepseek-v4-turbo   Pricing unknown — token budget only               ⚠  NEW
        ⚠  No cost tracking available. Budget mode uses token ceiling only.
            Probed: tools ✓, json ✓, thinking ✓

  Select model [1] (or 'a' to see all 37):
  > 1

  ⚠  Warning: DeepSeek V4 Flash supports tool calling and JSON mode.
     Full BugSwarm capability available.

  Configuration saved to ~/.config/bugswarm/config.yaml

  To start hunting bugs:  bugswarm run /path/to/repo
  To view usage stats:    bugswarm usage
```

If the user selects a model with unknown pricing, the budget warning clarifies:

```
  ⚠  You selected 'deepseek-v4-turbo' — a model not in our price registry.
     Dollar budget estimates will not be available.
     Token budget ($bugswarm run --budget 5M) works normally.
     Run 'bugswarm configure --refresh' to update pricing if available.
```

### Capability Warning Logic

```python
def show_model_warning(model: ModelInfo) -> str | None:
    if not model.capabilities.supports_tools and not model.capabilities.supports_json_mode:
        return ("⚠  This model does not support tool calling or JSON mode. "
                "BugSwarm will have severely limited bug-finding ability. "
                "Recommend a model that supports both.")
    if not model.capabilities.supports_tools:
        return ("⚠  This model does not support tool calling. "
                "BugSwarm will not be able to use CPG analysis or sandbox execution. "
                "Model will only analyze source code textually.")
    if not model.capabilities.supports_json_mode:
        return ("⚠  This model does not support JSON output mode. "
                "BugSwarm may have difficulty parsing structured findings. "
                "Recommend choosing a model with JSON support.")
    return None
```

### Config File (`~/.config/bugswarm/config.yaml`)

```yaml
provider: DeepSeek
api_key: sk-abc123def456...     # Stored with 0600 permissions
protocol: openai-compatible
base_url: https://api.deepseek.com/v1
model: deepseek-v4-flash

budget:
  default_tokens: 10000000       # 10M tokens

usage:
  track: true                    # Log all runs to usage.db
```

---

## MODULE 5: `usage.py` — Billing & Usage Page

### Data Model (`data/usage.db` — SQLite)

```sql
CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    repo TEXT NOT NULL,
    model TEXT NOT NULL,
    provider TEXT NOT NULL,
    started_at TEXT NOT NULL,         -- ISO 8601
    duration_secs INTEGER,
    tokens_consumed INTEGER,
    budget_tokens INTEGER,
    price_snapshot TEXT NOT NULL,     -- JSON of RunCostSnapshot
    estimated_cost_dollars REAL,
    findings_count INTEGER,
    verified_findings INTEGER,
    stale_warning TEXT
);

CREATE INDEX idx_runs_started ON runs(started_at DESC);
```

### CLI Command

```
$ bugswarm usage

  ┌────────── Usage Summary ───────────┐
  │                                    │
  │  Total runs:           47          │
  │  Total tokens:      12.4M          │
  │  Est. cost:          $3.82 (est.)  │
  │  Models used:        4             │
  │                                    │
  │  Active budget:     10M tokens     │
  │  Budget consumed:    8.2M / 10M    │
  │                                    │
  └────────────────────────────────────┘

  Recent runs:
  ┌──────┬────────────┬──────────┬──────────┬──────────┬─────────┐
  │ Date │ Repo       │ Model    │ Tokens   │ Est. $   │ Found   │
  ├──────┼────────────┼──────────┼──────────┼──────────┼─────────┤
  │ May28│ my-app     │ dsv4-flsh│  892,340 │   $0.125 │ 3 bugs  │
  │ May27│ api-server │ dsv4-flsh│  341,200 │   $0.048 │ 1 bug   │
  │ May26│ lib-core   │ dsv4-pro │ 2,102,400│   $5.674 │ 7 bugs  │
  │ May25│ my-app     │ dsv4-flsh│  450,100 │   $0.063 │ 2 bugs  │
  └──────┴────────────┴──────────┴──────────┴──────────┴─────────┘

  Models used:
  ┌──────────────────┬────────┬──────────┬──────────┬──────────┐
  │ Model            │ Runs   │ Tokens   │ Est. $   │ Price    │
  ├──────────────────┼────────┼──────────┼──────────┼──────────┤
  │ deepseek-v4-flash│ 38     │ 10.1M    │   $1.41  │$0.14/$0.28│
  │ deepseek-v4-pro  │ 6      │  2.1M    │   $5.67  │$1.74/$3.48│
  │ claude-sonnet-4  │ 2      │  0.2M    │   $0.60  │$3.00/$15  │
  │ llama-3-70b      │ 1      │  0.01M   │   $0.00  │local      │
  └──────────────────┴────────┴──────────┴──────────┴──────────┘

  ⚠  Price estimate for deepseek-v4-flash may be inaccurate —
     last refreshed 14 days ago. Run 'bugswarm configure --refresh' to update.
```

### Staleness Warning System

| Time Since Refresh | Display |
|-------------------|---------|
| < 7 days | No warning |
| 7–30 days | "Price estimate last refreshed N days ago" |
| > 30 days | "Price estimate may be inaccurate — model prices last refreshed N days ago" |
| Registry unreachable at run start | "Prices loaded from local cache — remote registry unreachable" |
| No registry ever loaded | "Price estimates use default prices — run 'bugswarm configure' to update" |

---

## Summary: Key Design Decisions

| Decision | Current (`bugswarm-gateway/`) | Mini (`bugswarm-mini/gateway/`) |
|----------|------------------------------|--------------------------------|
| Provider model | Per-provider class with match/case | 4 protocol adapters, config-driven |
| New provider | Write a new class, add to match/case | Edit registry.json or select "OpenAI-compatible" |
| Model list | Hardcoded in adapter | **Dynamic discovery from provider** + shipped registry + remote refresh |
| Unknown model | Cannot be used | Probed for capabilities (tools, JSON) at configure time |
| Pricing for unknown model | N/A | Marked "Pricing unknown — token budget only" |
| Cost tracking | None | Token budget = truth, dollar = stamped estimate |
| Budget gate | None | Token ceiling (dollar converted at run start) |
| Price freshness | Never checked | Non-blocking fetch at configure + run, clear staleness warnings |
| Setup UX | Manual env vars | **Interactive wizard with live provider model list + probe** |
| Usage history | None | SQLite run log with per-run price snapshots |

### What This Enables

1. **User picks a model, not a provider.** The registry handles routing. OpenRouter models just work.
2. **Prices change? Execution never breaks.** Dollar is decoration. Token budget is the gate.
3. **New model tomorrow?** It appears automatically. Either from our registry update (remote push) or because `GET /models` returned it and we probed it.
4. **Unknown model still works.** No pricing? Probing tells us if it supports tools/JSON. User can select it with a clear warning.
5. **Offline user?** Shipped registry + local cache. Warning shown. Execution unaffected.
6. **Cost transparency.** Every run has a price snapshot. The usage page shows exactly what was paid, at the prices that existed when the run started.

### What's NOT Implemented (Future)

- **Provider fallback**: if primary provider fails, try next. (Needs multi-key config.)
- **Model A/B comparison**: run same target with 2 models, compare results. (Needs parallel run tracking.)
- **Budget per round**: spend X tokens per round, not just globally. (Needs round-level cost tracker.)
- **Auto-budget calculation**: estimate token needs from repo size before running. (Needs scout agent.)
