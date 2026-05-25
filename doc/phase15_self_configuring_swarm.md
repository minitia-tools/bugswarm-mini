# Phase 15: Self-Configuring Swarm — Enterprise Architecture Plan

## Overview

The swarm is not configured by the user. The user provides two API keys (one for workers, one for judges). The system discovers available models, assesses the target repository's difficulty, and dynamically allocates agents, models, temperatures, personas, and judge configurations. Nothing is hardcoded. Tomorrow a provider releases a new model — it appears in the ranking automatically.

---

## Architecture: 5 New Modules

```
swarm/
├── discovery.py      # Provider Discovery — queries model lists, ranks by capability
├── assessment.py     # Difficulty Assessment — scout agent analyzes repo difficulty
├── allocation.py     # Allocation Algorithm — difficulty × model ranks → SwarmAllocation
├── judge_cfg.py      # Judge Configuration — difficulty → judge count + model selection
└── swarm_config.py   # SwarmConfig v2 — dynamic allocation replaces hardcoded model/provider
```

---

## MODULE 1: `discovery.py` — Provider Discovery Engine

### Purpose
Query the provider's API for available models. Rank them by a composite capability score. Cache the result for the session.

### Provider Adapter Extension

Each provider adapter gains a `list_models()` method:

```python
class ModelInfo:
    id: str                           # "deepseek-v4-pro"
    provider: str                     # "deepseek"
    context_window: int               # 1_000_000
    max_output_tokens: int            # 384_000
    supports_json_mode: bool          # True
    supports_tool_calls: bool         # True
    supports_thinking: bool           # True (R1)
    supports_streaming: bool          # True
    input_cost_per_mtok: float        # 1.74
    output_cost_per_mtok: float       # 3.48
    reasoning_benchmark_score: float  # 9.2 (from provider's published benchmarks or estimated)
    raw_metadata: dict                # Full provider response for audit

class ProviderDiscovery:
    def __init__(self, gateway: LLMClient):
        self.gateway = gateway
        self.worker_models: list[ModelInfo] = []
        self.judge_models: list[ModelInfo] = []
        self.discovery_timestamp: float = 0.0

    async def discover_workers(self) -> list[ModelInfo]:
        """Query worker provider. For DeepSeek: GET https://api.deepseek.com/models"""
        ...

    async def discover_judges(self) -> list[ModelInfo]:
        """Query judge provider. For Anthropic: no public /models endpoint — use known list."""
        ...

    def rank(self, models: list[ModelInfo]) -> list[ModelInfo]:
        """Rank by composite capability score. Rank 1 = highest capability."""
        ...
```

### Capability Score Formula

```
capability_score = (
    (context_window / 1_000_000) * 0.15 +            # Context size
    (max_output_tokens / 384_000) * 0.10 +            # Output capacity
    (reasoning_benchmark_score / 10.0) * 0.40 +       # Reasoning quality (dominant)
    (1.0 if supports_thinking else 0.0) * 0.20 +      # Thinking mode bonus
    (1.0 if supports_tool_calls else 0.0) * 0.10 +    # Tool calling bonus
    (1.0 if supports_json_mode else 0.0) * 0.05       # Structured output bonus
)
```

Models are sorted descending by `capability_score`. Rank 1 = highest score = most capable.

### Provider-Specific Discovery

| Provider | Discovery Method |
|----------|-----------------|
| DeepSeek | `GET https://api.deepseek.com/models` — returns JSON array |
| OpenAI | `client.models.list()` — returns paginated list |
| Anthropic | Hardcoded known list (no public /models endpoint): Claude Opus, Sonnet, Haiku |
| Google | `client.models.list()` — returns available Gemini models |
| Ollama | `GET http://localhost:11434/api/tags` — returns local models |

### Hard Metrics
- Discovery timeout: 10s per provider
- Cache TTL: session lifetime (models don't change mid-run)
- Fallback: if discovery fails, use last-known-good cached list from `~/.bugswarm/model_cache.yaml`
- Audit log: `discovery_result` written to persistence with timestamp and full model list

---

## MODULE 2: `assessment.py` — Difficulty Assessment Engine

### Purpose
One scout agent, using the single most capable model available (Rank 1), analyzes the repository and returns a Difficulty Level from 1 to 10.

### Scout Agent Configuration

```python
class ScoutConfig:
    model: str                # Rank 1 model (most capable)
    provider: str             # Worker provider
    max_rounds: int = 1       # Single round
    max_turns: int = 5        # Enough turns to explore
    persona: str = "causal"   # Neutral, analytical persona
    temperature: float = 0.3  # Low — deterministic assessment needed
```

### Assessment Metrics (8-factor model)

The scout agent executes these tool calls:

| # | Tool | Metric | Weight | How Measured |
|---|------|--------|--------|-------------|
| 1 | `list_dir` | File count | 0.15 | Count of .py, .js, .go, .rs, .java, .c, .cpp files recursively |
| 2 | `query_cpg` | Taint density | 0.25 | CPG `sources / total_functions` ratio |
| 3 | `read_file` (sampled) | Cyclomatic complexity | 0.10 | Average `if/for/while/except` per function across sampled files |
| 4 | `query_cpg` | Function count | 0.05 | Total functions in CPG |
| 5 | `read_file` (deepest) | Max nesting depth | 0.05 | Deepest AST nesting across all files |
| 6 | Pattern scan | CVE similarity | 0.20 | Substring matches against known CVE patterns in codebase |
| 7 | `list_dir` | Language diversity | 0.05 | Count of distinct file extensions |
| 8 | `query_cpg` | Cross-file call density | 0.15 | `total_edges / total_nodes` ratio |

### Difficulty Calculation

Each metric produces a normalized score 0.0–1.0. The weighted sum produces a raw score 0.0–1.0, then mapped to 1–10:

```
raw_score = sum(metric_i * weight_i for i in 1..8)
difficulty = max(1, min(10, round(raw_score * 10)))
```

### Difficulty Level Reference

| Level | Description | Example Repos | File Count | Taint Density |
|-------|-------------|---------------|------------|---------------|
| 1-2 | Trivial | Single-file script, < 200 lines | < 5 | < 0.1 |
| 3-4 | Simple | Flask API, basic CRUD, < 2K lines | 5-20 | 0.1-0.3 |
| 5-6 | Moderate | Django monolith, Express API, 2K-20K lines | 20-100 | 0.3-0.5 |
| 7-8 | Complex | Microservices, authentication systems, 20K-100K lines | 100-500 | 0.5-0.7 |
| 9-10 | Critical | Smart contracts, crypto libraries, medical firmware, >100K lines | 500+ | > 0.7 |

### Hard Metrics
- Scout run budget: max 5 turns, 1 round (never exceeds this)
- Scout model: ALWAYS Rank 1 most capable model
- Assessment output: JSON blob persisted to `assessment.json` in run directory
- Audit: scout's full conversation log retained for review

---

## MODULE 3: `allocation.py` — Dynamic Allocation Algorithm

### Purpose
Consumes (1) Difficulty Level, (2) Ranked Model List, (3) Provider Capabilities, and (4) Budget Constraints to produce a complete `SwarmAllocation`.

### Data Structures

```python
@dataclass
class ModelAllocation:
    model_id: str              # "deepseek-v4-pro"
    rank: int                  # 1 (highest capability)
    agent_count: int           # How many agents use this model
    temperature_min: float     # 0.7
    temperature_max: float     # 1.0
    persona_distribution: dict[str, float]  # {"adversarial": 0.40, ...}
    priority: str              # "precision" | "volume" | "verification"

@dataclass
class SwarmAllocation:
    difficulty_level: int                    # 1-10
    total_agents: int                        # Computed, not user-set
    worker_provider: str                     # "deepseek"
    judge_provider: str                      # "anthropic"
    models: list[ModelAllocation]            # Per-model allocation
    judge_count: int                         # 1, 3, or 5
    judge_models: list[str]                  # Which ranks for judges
    max_rounds: int                          # Computed from difficulty
    max_turns_per_round: int                 # Computed from difficulty
    estimated_cost: float                    # Pre-run estimate
    batch_size: int                          # Semaphore size
```

### The Core Algorithm

```
function allocate(difficulty: int, ranked_models: list[ModelInfo], budget: BudgetConfig) -> SwarmAllocation:

    # Step 1: Determine how many models to use
    if difficulty <= 3:
        usable_models = ranked_models[-1:]     # Only cheapest (Rank N)
        total_agents = 6
    elif difficulty <= 6:
        usable_models = ranked_models[-3:]     # Bottom 3 ranks
        total_agents = min(25, 12 + difficulty * 2)
    else:
        usable_models = ranked_models[-5:]     # All available (max 5)
        total_agents = min(50, 25 + difficulty * 3)

    # Step 2: Distribute agents across models
    # Inverse cost weighting: cheaper models get more agents
    total_cost = sum(1.0 / m.input_cost_per_mtok for m in usable_models)
    for model in usable_models:
        weight = (1.0 / model.input_cost_per_mtok) / total_cost
        model.agent_count = max(1, round(total_agents * weight))

    # Step 3: Assign temperature ranges per model
    # Expensive models get lower temperature (precision focus)
    # Cheap models get higher temperature (exploration focus)
    for model in usable_models:
        cost_ratio = model.input_cost_per_mtok / ranked_models[0].input_cost_per_mtok
        model.temperature_min = 0.3 + (1.0 - cost_ratio) * 0.2   # 0.3–0.5
        model.temperature_max = 0.6 + (1.0 - cost_ratio) * 0.4   # 0.6–1.0

    # Step 4: Assign persona distributions per model
    # Cheap models: equal distribution (broad exploration)
    # Expensive models: adversarial-heavy (targeted attack)
    for model in usable_models:
        adv_weight = 0.25 + (1.0 - cost_ratio) * 0.15
        model.persona_distribution = {
            "adversarial": adv_weight,
            "causal": (1.0 - adv_weight) / 3,
            "defensive": (1.0 - adv_weight) / 3,
            "semantic": (1.0 - adv_weight) / 3,
        }

    # Step 5: Compute rounds and turns
    max_rounds = min(15, max(3, difficulty * 1.5))
    max_turns = min(10, max(3, difficulty))

    # Step 6: Compute estimated cost
    estimated_cost = sum(
        m.agent_count * max_rounds * max_turns * 2000 *   # ~2000 tokens per turn
        (m.input_cost_per_mtok + m.output_cost_per_mtok) / 1_000_000
        for m in usable_models
    )

    return SwarmAllocation(...)
```

### Agent Distribution Table (Reference)

| Difficulty | Total Agents | Model Split | Batch | Rounds | Turns |
|------------|-------------|-------------|-------|--------|-------|
| 1 | 6 | 100% cheapest | 6 | 3 | 3 |
| 2 | 8 | 100% cheapest | 8 | 3 | 3 |
| 3 | 10 | 100% cheapest | 10 | 4 | 4 |
| 4 | 14 | 70% cheap, 30% mid | 12 | 4 | 4 |
| 5 | 18 | 60% cheap, 40% mid | 12 | 5 | 5 |
| 6 | 25 | 50% cheap, 35% mid, 15% pro | 12 | 5 | 5 |
| 7 | 31 | 40% cheap, 30% mid, 20% pro, 10% best | 12 | 6 | 6 |
| 8 | 38 | 35% cheap, 30% mid, 20% pro, 15% best | 12 | 6 | 7 |
| 9 | 44 | 30% cheap, 30% mid, 20% pro, 20% best | 12 | 7 | 8 |
| 10 | 50 | 25% cheap, 25% mid, 25% pro, 25% best | 12 | 8 | 10 |

---

## MODULE 4: `judge_cfg.py` — Judge Configuration

### Purpose
Maps Difficulty Level → number of judges and which models from the judge provider.

### Algorithm

```python
def configure_judges(difficulty: int, ranked_judge_models: list[ModelInfo]) -> JudgeAllocation:

    if difficulty <= 3:
        # 1 judge, cheapest available model
        usable = ranked_judge_models[-1:]
        judge_count = 1
    elif difficulty <= 6:
        # 3 judges, bottom 3 models
        usable = ranked_judge_models[-3:]
        judge_count = 3
    else:
        # 5 judges, bottom 5 models (all available)
        usable = ranked_judge_models[-5:]
        judge_count = 5

    # Specialty assignment: round-robin across judges
    specialties = ["security", "logic", "concurrency", "architecture", "data_integrity"]
    for i, model in enumerate(usable):
        model.specialty = specialties[i % len(specialties)]

    return JudgeAllocation(
        judge_count=judge_count,
        models=usable,
        tiebreaker_model=ranked_judge_models[0],  # Always Rank 1 for tiebreaker
    )
```

### Judge Count Mapping

| Difficulty | Judges | Models Used | Specialty Handling |
|------------|--------|-------------|--------------------|
| 1-3 | 1 | Cheapest | All specialties handled by 1 judge |
| 4-6 | 3 | Mid-range | 2 specialties per judge (round-robin) |
| 7-10 | 5 | All available | 1 dedicated specialty per judge |

### Hard Metrics
- Judge calls are real LLM API calls — no simulation
- Each judge receives the structured case brief (not raw debate transcript)
- Judge verdict schema includes: `bug_verified`, `severity`, `evidence_quality`, `trace_citation`
- Tiebreaker uses Rank 1 model regardless of difficulty level

---

## MODULE 5: `swarm_config.py` — SwarmConfig v2

### Purpose
Replaces the hardcoded `SwarmConfig(model="deepseek-v4-flash")` with a `SwarmAllocation` object produced by the self-configuring pipeline.

### Before (Current)

```python
@dataclass
class SwarmConfig:
    model: str = "deepseek-v4-flash"        # HARDCODED
    provider: ProviderType = ProviderType.DEEPSEEK  # HARDCODED
    num_agents: int = 12                    # User-set
    batch_size: int = 6
    max_rounds: int = 5
    max_turns_per_round: int = 8
```

### After (Enterprise)

```python
@dataclass
class SwarmConfig:
    # ── Set by user ──
    repo_path: Path
    worker_api_key: str          # DEEPSEEK_API_KEY
    judge_api_key: str           # ANTHROPIC_API_KEY or OPENAI_API_KEY

    # ── Discovered automatically ──
    worker_provider: str = ""    # "deepseek" (detected from key prefix)
    judge_provider: str = ""     # "anthropic" (detected from key prefix)
    ranked_worker_models: list[ModelInfo] = field(default_factory=list)
    ranked_judge_models: list[ModelInfo] = field(default_factory=list)

    # ── Computed by scout agent ──
    difficulty_level: int = 0    # 1-10

    # ── Computed by allocation algorithm ──
    allocation: SwarmAllocation | None = None

    # ── Budget override (user can cap) ──
    max_total_cost: float = 50.0
    max_total_tokens: int = 10_000_000
    max_wall_time_minutes: int = 2400
```

---

## Full Execution Flow

```
bugswarm run ./repo
│
├─ [TUI] User pastes worker key + judge key
│
├─ Phase 0: PROVIDER DISCOVERY (~2s)
│   ├─ Query worker provider → 4 DeepSeek models found
│   ├─ Query judge provider → 2 Anthropic models found
│   ├─ Rank both lists by capability_score
│   └─ TUI updates: "DeepSeek · 4 models · Rank 1: v4-pro"
│
├─ Phase 1: DIFFICULTY ASSESSMENT (~15s)
│   ├─ Scout agent spawned with Rank 1 model
│   ├─ 8-factor analysis of repo
│   ├─ Output: Difficulty 7/10
│   └─ TUI updates: "Difficulty 7/10 · Complex microservice detected"
│
├─ Phase 2: ALLOCATION (~1ms)
│   ├─ Difficulty 7 → 31 agents, 4 models, 12 batch, 6 rounds
│   ├─ Model split: 40% flash, 30% reasoner, 20% pro, 10% best
│   ├─ Temperature ranges computed
│   ├─ Persona distributions computed
│   ├─ Judge config: 5 judges, all Anthropic models
│   └─ TUI updates: "31 agents · $8.40 estimated · [Start Swarm]"
│
├─ Phase 3: SWARM EXECUTION
│   ├─ 31 IEPEngine instances spawned per allocation
│   ├─ Each agent uses its assigned model + temperature + persona
│   ├─ Agents contribute to evidence graph
│   └─ Budget enforced per SwarmAllocation
│
├─ Phase 4: BENCH ADJUDICATION
│   ├─ 5 judges (real LLM calls) evaluate findings
│   ├─ Structured verdicts with trace citations
│   └─ Consensus + tiebreaker protocol
│
└─ Phase 5: REPORT
    ├─ SARIF export
    ├─ Cost breakdown by model
    └─ TUI summary
```

---

## Implementation Order

| Step | Module | Effort | Depends On |
|------|--------|--------|------------|
| 1 | `discovery.py` — Provider model listing + ranking | 3h | Gateway adapter `list_models()` |
| 2 | `assessment.py` — Scout agent difficulty assessment | 4h | IEPEngine, tools, CPG |
| 3 | `allocation.py` — Core allocation algorithm | 3h | discovery output |
| 4 | `judge_cfg.py` — Judge count + model mapping | 2h | discovery output + allocation |
| 5 | `swarm_config.py` — SwarmConfig v2 wiring | 2h | All above |
| 6 | `orchestrator.py` — Update to consume SwarmAllocation | 3h | swarm_config |
| 7 | `bench.py` — Replace _simulate_verdict with real LLM calls | 4h | judge_cfg |
| 8 | `tui.py` — Provider status panel + model ranking display | 4h | discovery |
| 9 | End-to-end gate test | 3h | All above |

**Total: ~28 hours. This is Phase 15 of the build order.**

---

## Hard Invariants

1. **No model name is ever hardcoded.** All model IDs come from provider discovery. A new model release requires zero code changes.

2. **User only provides API keys.** Everything else is automatic. The user never types a model name.

3. **Difficulty assessment is reproducible.** Same repo → same difficulty level. The scout agent's conversation log is retained for audit.

4. **Allocation is a pure function.** `allocate(difficulty, ranked_models, budget)` always produces the same output for the same inputs. Testable, deterministic.

5. **Judges are always real LLM calls.** The `_simulate_verdict` method is deleted. Every verdict comes from a frontier model API call.

6. **Provider discovery failure is not fatal.** If the /models endpoint is down, use the cached model list from the last successful discovery. Warn the user but proceed.

7. **Cost estimate is shown before execution.** The TUI displays estimated cost based on the allocation. User confirms before any API call is made.

8. **Allocation respects budget.** If the estimated cost exceeds the user's budget cap, the allocation scales down (fewer agents, fewer rounds, cheaper models) until it fits.
