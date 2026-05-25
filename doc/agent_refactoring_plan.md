# Agent Refactoring — Enterprise-Grade Architecture Plan

## Current State
`agent/core.py` — 714-line God object containing: system prompt, tool dispatch, PII scanner, SQLite WAL, finding parser, IEP loop, tool execution. Everything hardcoded. Everything in one file.

## Target State: 8 Files

```
agent/
├── prompts.py         # Prompt templates, versioning, persona registry, A/B testing
├── tools.py           # ToolRegistry with plugin system, retry, timeout, circuit breaker
├── cpg_client.py      # CPG service client (gRPC/socket, not subprocess)
├── sandbox_client.py  # Sandbox service client (gRPC/socket, not subprocess)
├── scanner.py         # Unified PII + injection scanner, shared with Rust patterns
├── persistence.py     # SQLite with migration framework, connection pool, WAL
├── parser.py          # Structured output parser with JSON Schema validation
├── loop.py            # IEP loop with hypothesis ranking, context management, planning
└── cli.py             # Entry point (thin, just wires dependencies)
```

---

## FILE 1: `prompts.py` — Prompt Engineering System

### Problems to fix
- `SYSTEM_PROMPT` is a module-level string constant — no versioning, no A/B testing, no audit trail
- Persona additions are a hardcoded dict with one-liner descriptions
- No prompt registry for different models (DeepSeek vs GPT-4o vs Claude need different formats)
- No i18n or localization support
- No prompt optimization feedback loop

### Enterprise Architecture

```python
@dataclass
class PromptTemplate:
    id: str                          # "v2.1.0-adversarial"
    version: str                     # Semver
    content: str                     # Jinja2 template
    model_family: str                # "openai", "anthropic", "deepseek"
    created_at: datetime
    performance_score: float         # Updated from A/B test results
    active: bool

class PromptRegistry:
    """Version-controlled prompt store with A/B testing."""
    - load(template_id) → PromptTemplate
    - register(template) → None
    - get_active(persona, model_family) → PromptTemplate
    - record_outcome(template_id, bugs_found, false_positives) → update performance_score
    - export_all() → JSON for audit
    - diff(template_a, template_b) → text diff

class PersonaEngine:
    """Generates persona-specific prompt augmentations."""
    - build(persona: Persona, domain: DomainProfile) → str
    - Adversarial: injects "assume malice" framing + specific attack vectors
    - Causal: injects "trace error propagation" + call chain analysis directives
    - Defensive: injects "assume instability" + error handling focus
    - Semantic: injects "check contracts" + type safety + API boundary analysis
    - Each persona gets NOT a one-liner but a 500-token expansion with:
        * Cognitive framing (how to think)
        * Priority heuristics (what to look for first)
        * Anti-patterns to flag
        * Tool preferences (which tools to favor)
```

### Hard Metrics
- `PROMPT_VERSION` stored in every agent state row
- `prompt_performance` table tracks: `(template_id, run_id, bugs_found, verified, false_positives, avg_severity, tokens_per_finding)`
- Automated A/B: randomize prompt variant per agent, compare outcomes after 100 runs
- Prompt change requires PR + benchmark against golden dataset

---

## FILE 2: `tools.py` — Tool Registry with Plugin System

### Problems to fix
- Tool dispatch is an `if/elif` chain — adding a tool means editing the dispatcher
- No retry on individual tool failures within a turn
- No timeout per-tool (relies on subprocess timeouts)
- No circuit breaker — a failing tool poisons every subsequent call
- No tool output caching — identical `read_file("bugs.py", 1, 50)` called by 3 agents
- No tool rate limiting

### Enterprise Architecture

```python
@dataclass
class ToolDefinition:
    name: str
    description: str              # For LLM function calling schema
    parameters: dict              # JSON Schema for parameters
    handler: Callable             # Async function
    timeout_secs: float = 30.0
    max_retries: int = 2
    cache_ttl_secs: float = 60.0  # Cache identical calls
    required_permissions: list[str]  # Future: RBAC

class ToolRegistry:
    """Plugin-based tool dispatch with retry, cache, circuit breaker."""
    - register(tool: ToolDefinition) → None
    - execute(name, args, context) → ToolResult
    - get_schema_for_llm() → list[dict]  # For OpenAI function calling
    - enable_circuit_breaker(name) → None
    - stats() → {tool: {calls, successes, failures, avg_latency}}

class ToolCache:
    """LRU cache for tool results, keyed by (tool_name, args_hash)."""
    - get(tool_name, args) → ToolResult | None
    - set(tool_name, args, result) → None
    - invalidate(tool_name) → None  # On round boundary
    - hit_rate → float

class RetryPolicy:
    """Configurable retry with exponential backoff + jitter."""
    - max_retries: int
    - base_delay: float
    - max_delay: float
    - retryable_exceptions: list[type]
    - execute(fn, *args) → Result  # Wraps any async call
```

### Hard Metrics
- Tool latency p50/p95/p99 tracked per tool
- Circuit breaker opens after 5 consecutive failures, half-opens after 30s
- Cache hit rate must stay >60% for `read_file` and >80% for `query_cpg`
- Retry budget: max 3 retries total across all tools per turn

---

## FILE 3: `cpg_client.py` — CPG Service Client

### Problems to fix
- Hardcoded path `/root/a/bugswarm-cpg/target/release/bugswarm-cpg`
- Spawns a new subprocess for every query — no persistent connection
- Blocks event loop with `proc.communicate()` for up to 30s
- No connection pooling, no health checks, no graceful degradation
- Output parsing is ad-hoc string matching and grep

### Enterprise Architecture

```python
class CPGClient:
    """Client for the CPG service (Unix socket or gRPC)."""
    - connect(endpoint: str) → None
    - query_functions(name: str, kind: str) → list[FunctionInfo]
    - query_taint_paths(source: str, sink: str) → list[TaintPath]
    - query_call_graph(function: str, radius: int) → CallGraph
    - health_check() → bool
    - close() → None

class CPGConnectionPool:
    """Pool of persistent connections to CPG daemon."""
    - acquire() → CPGClient
    - release(client) → None
    - max_connections: int = 4
    - idle_timeout: float = 300.0
```

### Hard Metrics
- Connection pool size: 4 (one per concurrent agent batch)
- Health check interval: 30s
- Reconnect on failure: 3 attempts with 1s backoff
- Query timeout: 15s for stats, 30s for taint, 5s for function lookup

---

## FILE 4: `sandbox_client.py` — Sandbox Service Client

### Problems to fix
- Hardcoded path `/root/a/bugswarm-sandbox/target/release/bugswarm-sandbox`
- Spawns subprocess for every PoC — no persistent daemon connection
- Blocks event loop for up to 130s
- PoC written to temp file, executed, temp file deleted — no artifact preservation
- Receipt parsing is fragile — JSON decode can fail silently

### Enterprise Architecture

```python
class SandboxClient:
    """Client for the Sandbox daemon (Unix socket or gRPC)."""
    - connect(endpoint: str) → None
    - execute(poc_code: str, env: dict, flaky: bool) → ExecutionReceipt
    - execute_statistical(poc_code: str, count: int) → StatisticalResult
    - execute_causal(poc_code: str, intervention: CausalIntervention) → CausalResult
    - independent_reexecute(receipt_id: str) → ExecutionReceipt
    - health_check() → bool

class SandboxReceiptCache:
    """Caches execution receipts to avoid redundant re-execution."""
    - get(poc_hash: str) → ExecutionReceipt | None
    - set(poc_hash: str, receipt: ExecutionReceipt) → None
    - TTL: 300s (same PoC with same hash within 5 min → cache hit)
```

### Hard Metrics
- Execution timeout: 120s wall-clock, 60s CPU
- Receipt cache hit rate target: >30% (agents often submit similar PoCs)
- Artifact retention: keep PoC files and receipts for 7 days
- Connection pool: 12 persistent connections (matches batch size)

---

## FILE 5: `scanner.py` — Unified Security Scanner

### Problems to fix
- PII patterns duplicated from Rust sandbox — two sources of truth
- Regex patterns compiled on every scan call (not cached)
- Escape detection is substring matching, not AST-aware
- Injection detection is substring matching, not ML-based
- No entropy-based secret detection (present in Rust, missing in Python)
- No configurable patterns via YAML/file

### Enterprise Architecture

```python
class UnifiedScanner:
    """
    Single source of truth for all content scanning.
    Syncs patterns with Rust sandbox via shared config.
    """
    PII_SCAN = "pii"
    ESCAPE_SCAN = "escape"
    INJECTION_SCAN = "injection"
    ENTROPY_SCAN = "entropy"

    - __init__(config_path: str) → loads patterns from YAML
    - scan(content: str, scan_types: list[str]) → ScanReport
    - redact(content: str) → tuple[str, int]  # Redacted text + count
    - detect_escape(content: str) → list[str]  # Matched patterns
    - detect_injection(content: str) → float   # 0.0-1.0 confidence
    - detect_high_entropy(content: str) → list[str]  # High-entropy tokens

@dataclass
class ScanReport:
    pii_count: int
    pii_types: dict[str, int]      # {"email": 3, "aws_key": 1}
    escape_matches: list[str]
    injection_confidence: float
    high_entropy_tokens: list[str]
    redacted_content: str
    scan_duration_ms: float
```

### Hard Metrics
- PII false positive rate target: <5% (too aggressive redaction breaks context)
- PII false negative rate target: 0% (must never leak)
- Scan latency: <10ms for 10KB content, <50ms for 100KB
- Pattern config: loaded from `~/.bugswarm/scanner.yaml`, hot-reloadable
- Shared pattern source: single YAML file consumed by both Rust sandbox and Python agent

---

## FILE 6: `persistence.py` — Enterprise Persistence Layer

### Problems to fix
- Default `:memory:` loses everything on crash
- Raw SQLite with no connection pooling, no migration framework
- No async I/O — `sqlite3` module is synchronous, blocks event loop
- No backup/restore, no WAL checkpoint management
- No data retention policies
- `get_stats` reads column names from `LIMIT 0` query — fragile

### Enterprise Architecture

```python
class PersistenceManager:
    """Async-safe persistence with connection pooling and migrations."""
    - __init__(db_path: str, pool_size: int = 4)
    - migrate() → apply pending migrations
    - checkpoint() → force WAL checkpoint
    - backup(target_path: str) → None
    - vacuum() → reclaim disk space
    - close() → graceful shutdown

class RunRepository:
    """CRUD for agent runs."""
    - create(config: AgentConfig) → RunID
    - get(run_id: str) → RunState
    - update(run_id: str, state: RunState) → None
    - list_recent(limit: int = 10) → list[RunID]

class MessageRepository:
    """CRUD for conversation messages."""
    - log(run_id, round, turn, role, content, tokens) → None
    - get_history(run_id, last_n: int = 50) → list[Message]

class FindingRepository:
    """CRUD for bug findings."""
    - log(run_id, finding: Finding) → None
    - get_verified(run_id) → list[Finding]
    - get_all(run_id) → list[Finding]
    - update_verification(finding_id, receipt: ExecutionReceipt) → None

class CheckpointRepository:
    """CRUD for run checkpoints."""
    - save(run_id, round, state: dict) → None
    - load_latest(run_id) → dict | None
    - restore(run_id) → rebuild agent state from checkpoint
```

### Migration Framework
```sql
-- V001: Initial schema
-- V002: Add prompt_version to agent_state
-- V003: Add sandbox_receipt_json to findings
-- V004: Add indexes on (run_id, round) for messages
-- V005: Add cost_tracking table
```

### Hard Metrics
- Write latency: <5ms per message (batched commits every 50 messages)
- WAL checkpoint: every 1000 messages or 5 minutes, whichever comes first
- Connection pool: 4 connections, acquire timeout 1s
- Backup: daily full backup, hourly WAL archive to S3/MinIO
- Retention: messages 30 days, findings 1 year, checkpoints 7 days

---

## FILE 7: `parser.py` — Structured Output Parser

### Problems to fix
- Finding extraction uses `content.index('{"type"')` and manual brace counting — breaks on nested JSON, code blocks, edge cases
- Freeform confirmation parsing is keyword-based shotgun approach — catches tool call JSON as findings
- No JSON Schema validation — any dict with `type: "finding"` is accepted
- No support for native function calling (OpenAI tools, Anthropic tool_use)
- Severity is hardcoded to 7 for freeform findings
- Location regex only matches `bugs.py` patterns

### Enterprise Architecture

```python
class OutputParser:
    """Multi-strategy parser with fallback chain."""
    - parse(content: str) → ParsedOutput

class ParsedOutput:
    type: OutputType  # FINDING, TOOL_CALL, POC, UNKNOWN
    finding: Finding | None
    tool_call: ToolCall | None
    poc: PoCCode | None
    raw: str  # Original content for audit

class FindingParser:
    """JSON Schema-validated finding extraction."""
    FINDING_SCHEMA = {
        "type": "object",
        "required": ["type", "claim", "location", "mechanism", "severity_estimate"],
        "properties": {
            "type": {"const": "finding"},
            "claim": {"type": "string", "minLength": 10, "maxLength": 500},
            "location": {"type": "string", "pattern": "^[^:]+:\\d+$"},
            "mechanism": {"type": "string", "minLength": 10},
            "prediction": {"type": "string"},
            "severity_estimate": {"type": "integer", "minimum": 1, "maximum": 10},
            "verified": {"type": "boolean"},
        }
    }
    - parse_json_block(content) → Finding | None  # From ```json fences
    - parse_inline_json(content) → Finding | None  # From raw {"type":"finding"...}
    - parse_native_tool_call(tool_call_obj) → Finding | None  # From OpenAI function calling
    - parse_freeform(content) → Finding | None  # Last resort fallback
    - validate(finding_dict) → Finding | list[ValidationError]  # JSON Schema

class ToolCallParser:
    """Parses tool requests from LLM output."""
    - parse_json_block(content) → ToolCall | None
    - parse_native_function_call(fn_call_obj) → ToolCall | None

class PoCParser:
    """Parses PoC code blocks."""
    - parse_json_block(content) → PoCCode | None
    - extract_code_block(content, language="python") → str | None
```

### Parse Strategy Chain (tried in order, first success wins)
1. **Native function calling** (OpenAI `tool_calls`, Anthropic `tool_use` blocks) — highest fidelity
2. **JSON-in-code-fence** (\`\`\`json {...} \`\`\`) — high fidelity
3. **Inline JSON** (`{"type":"finding",...}` in freeform text) — medium fidelity
4. **Freeform heuristics** (keyword matching for bug confirmations) — low fidelity, last resort

### Hard Metrics
- Parse success rate: >95% for strategy 1+2 (JSON blocks)
- False positive rate for freeform parser: <10% (currently catches tool JSON as findings)
- Schema validation: 100% of accepted findings must pass JSON Schema
- Location regex: must handle `path/to/file.py:123`, `file.py:456`, absolute paths

---

## FILE 8: `loop.py` — Enterprise IEP Engine

### Problems to fix
- `_execute_turn` mixes LLM call, parsing, tool execution, and finding logging
- `_should_stop` has only 2 conditions (verified >= 3, budget exhausted)
- No hypothesis ranking — agent investigates randomly
- No context window management — messages grow unbounded
- No planning phase — agent doesn't prioritize what to investigate
- No self-reflection — agent doesn't learn from failed PoCs within a run
- `print()` statements for logging instead of structured logging
- `run()` returns a plain dict, not a typed result

### Enterprise Architecture

```python
@dataclass
class IEPState:
    """Full agent state at any point in the loop."""
    run_id: str
    round: int
    turn: int
    messages: list[ChatMessage]
    hypotheses: list[Hypothesis]       # Ranked list of things to investigate
    findings: list[VerifiedFinding]    # Confirmed bugs
    context_window_usage: float         # 0.0-1.0
    budget_remaining: BudgetState
    last_tool_results: list[ToolResult]

@dataclass
class Hypothesis:
    """A candidate bug to investigate, ranked by priority."""
    id: str
    claim: str
    location: str
    priority_score: float     # Computed from: taint_path_exists * 0.4 + code_complexity * 0.2 + reachability * 0.3 + novelty * 0.1
    source: str               # "cpg_taint", "agent_generated", "pattern_match"
    investigated: bool
    investigation_result: str  # "confirmed", "refuted", "inconclusive"

class HypothesisRanker:
    """Ranks what to investigate next."""
    - rank(hypotheses: list[Hypothesis], cpg: CPGClient) → list[Hypothesis]
    - Factors:
        * Taint path confidence from CPG (weight: 0.40)
        * Code complexity (cyclomatic complexity, nesting depth) (weight: 0.20)
        * Reachability from external input (weight: 0.30)
        * Novelty — not already investigated by another agent (weight: 0.10)

class ContextManager:
    """Manages agent context window to prevent bloat."""
    - add_message(msg: ChatMessage) → None
    - should_compress() → bool        # At 80% of context window
    - compress() → str                # Returns compressed summary
    - get_active_context() → list[ChatMessage]  # What the LLM actually sees
    - estimate_tokens() → int
    - max_tokens: int = 128000

class IEPEngine:
    """Core IEP (Issue → Evidence → Proof) execution loop."""
    - __init__(config: AgentConfig, tools: ToolRegistry, parser: OutputParser, gateway: LLMClient, persistence: PersistenceManager)
    - async run() → IEPReport
    - async _planning_phase() → list[Hypothesis]      # ISSUE: explore CPG, rank hypotheses
    - async _evidence_phase(hypothesis) → ToolResult   # EVIDENCE: read code, trace deps
    - async _proof_phase(hypothesis, evidence) → VerifiedFinding | None  # PROOF: sandbox PoC
    - async _reflect(finding: VerifiedFinding) → None   # Learn from result, adjust ranking
    - _should_continue(state: IEPState) → bool

@dataclass
class IEPReport:
    run_id: str
    total_rounds: int
    total_turns: int
    hypotheses_generated: int
    hypotheses_investigated: int
    findings: list[VerifiedFinding]
    verified_count: int
    tokens_consumed: int
    cost_usd: float
    duration_secs: float
    persona: str
    prompt_version: str
    tool_stats: dict[str, dict]
```

### Hard Metrics
- **Termination conditions** (at least 3 of these must be true):
  1. 3+ verified findings
  2. Budget exhausted (tokens, cost, or time)
  3. All top-10 hypotheses investigated without new findings
  4. Novelty rate <5% for last 5 investigations
  5. Context window compressed 3+ times without new evidence

- **Hypothesis ranking**: recomputed after every 2 turns
- **Context compression**: triggered at 80% window or every 12 interactions
- **Self-reflection**: after each sandbox result, adjust hypothesis confidence by ±0.2
- **Planning phase budget**: max 2 turns per round for exploration
- **Evidence phase budget**: max 3 turns per hypothesis
- **Proof phase budget**: max 2 sandbox executions per hypothesis

---

## FILE 9: `cli.py` — Thin Entry Point

```python
async def main():
    config = load_config()
    persistence = PersistenceManager(config.db_path)
    scanner = UnifiedScanner(config.scanner_config_path)
    gateway = LLMClient(GatewayConfig.from_env())
    gateway.register_default_adapters()

    tools = ToolRegistry()
    tools.register(ToolDefinition("read_file", ..., CPGClient.read_file))
    tools.register(ToolDefinition("query_cpg", ..., CPGClient.query))
    tools.register(ToolDefinition("exec_sandbox", ..., SandboxClient.execute))
    # ... more tools

    parser = OutputParser()
    engine = IEPEngine(config.agent, tools, parser, gateway, persistence)
    report = await engine.run()
    print(report.model_dump_json(indent=2))
```

---

## Implementation Order

| Step | Files | Effort | Depends On |
|------|-------|--------|------------|
| 1 | `scanner.py` | 2h | Nothing — pure extraction from core.py |
| 2 | `persistence.py` | 3h | Nothing — standalone with migration framework |
| 3 | `cpg_client.py` + `sandbox_client.py` | 3h | Daemon mode in Rust (deferred — use subprocess with proper abstraction for now) |
| 4 | `tools.py` | 3h | scanner, cpg_client, sandbox_client |
| 5 | `prompts.py` | 2h | Nothing — standalone |
| 6 | `parser.py` | 3h | Nothing — standalone JSON Schema |
| 7 | `loop.py` | 4h | All of the above |
| 8 | `cli.py` | 1h | All of the above |

**Total: 21 hours of focused refactoring.**

## Post-Split Verification

After each file is extracted:
1. Existing 12 Phase-4 gate tests must still pass
2. Run DeepSeek live test against enterprise bugs — must find >= 1 verified bug
3. `import agent.core` must still work (backward compat shim)
4. All hardcoded paths replaced with config
5. Zero `print()` statements — all structured logging
6. All Pydantic models have `model_validate` tests
