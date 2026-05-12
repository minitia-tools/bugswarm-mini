# Bug Swarm CLI — Technical Requirements

---

## Build Order & Dependency Map

The architecture is not built top-to-bottom by category. It is built **inside-out** — each phase depends on the one before it and produces a testable artifact. The reference sections (§1–§15) are organized by topic; this section organizes them by **when you build them**.

### Phase 1: The Sandbox Oracle (6 weeks)
**Why first**: Every other component depends on the sandbox for ground truth. Without it, agents hallucinate and judges are rubber-stamping rhetoric. Build this until it can execute arbitrary PoCs in isolated containers and return machine-verifiable results.

| What | Reference Sections |
|------|--------------------|
| Rust sandbox daemon (Linux namespaces, cgroups v2, seccomp) | §3 Container Runtime, §7 Security & Isolation |
| Docker container pool with pre-built language images | §3 Docker Engine, §4 Sandbox Target Runtimes |
| Cryptographic execution receipts (SHA256 hashes, exit codes, stdout/stderr) | §7 Security & Isolation |
| Timeout enforcement (wall-clock + CPU-time), OOM detection, deadlock diagnostics | §3 Container Runtime |
| Statistical re-execution: run PoC N times, return distribution, not boolean | — |
| Causal intervention: surgically modify code, re-run, check if crash disappears | — |
| `rr` (Record and Replay) integration for deterministic replay | §4 (system packages) |
| **Deliverable**: A Rust binary that takes a PoC script + codebase path, returns a signed execution receipt within 120s. Usable standalone as `sandboxd`. |

### Phase 2: The Code Property Graph (6 weeks)
**Why second**: Agents can't discover bugs in code they can't see. The CPG transforms the codebase into a queryable graph with call edges, dataflow edges, and taint paths. This directs agents to high-signal regions before they spend tokens.

| What | Reference Sections |
|------|--------------------|
| tree-sitter parsers for all target languages | §4 (tree-sitter CLI), §9 (Rust crates) |
| AST → CFG → DFG → type graph transformer | — |
| Taint tracking engine (source → sanitizer → sink) | — |
| Graph database (Neo4j or Kuzu) with subgraph isomorphism queries | §5 Graph Database |
| CVE pattern corpus import + graph edit distance computation | — |
| Dynamic edge injection from sandbox runtime traces | — |
| **Deliverable**: A Rust binary that ingests a repo, outputs a CPG in the graph DB. Agents query it via `trace_dependency()`, `get_tainted_paths()`, `find_similar_to_cve()`. |

### Phase 3: LLM Gateway + Provider Adapters (2 weeks)
**Why third**: Needed before any agent can call an LLM. Must be provider-agnostic from day one — never hardcode OpenAI assumptions.

| What | Reference Sections |
|------|--------------------|
| Abstract `LLMClient` interface with provider adapters (OpenAI, Anthropic, Google, Ollama) | §6 LLM Providers & Models |
| Canonical Jinja2 prompt templates with provider-specific rendering | §6 |
| Normalized token counting across providers | §6 |
| Auto-retry with exponential backoff (429, 502) | §9 (tenacity) |
| Cost-per-token tracking from API response headers | §6 |
| Hot-swap provider mid-session via config reload (SIGUSR1) | §6 |
| **Deliverable**: Python package `bugswarm.llm`. `llm.chat(messages, model="auto")` routes to cheapest available provider. |

### Phase 4: Single Agent + IEP Loop (4 weeks)
**Why fourth**: Prove the core hypothesis before scaling. One agent, one repo, one round at a time. The IEP cycle (Issue → Evidence → Proof) must produce real bugs.

| What | Reference Sections |
|------|--------------------|
| Pydantic models: `AgentState`, `ArgumentPayload`, `ExecutionTrace` | §9 (Python deps) |
| Agent system prompt with adversarial directive, anti-sycophancy guard, anti-deference bias | — |
| Tool dispatch: `query_cpg`, `trace_dependency`, `exec_sandbox`, `expand_tool_result`, `recall_raw_context` | — |
| SQLite WAL for agent state persistence (append-only, crash recovery) | §5 SQLite |
| PII/Secrets scanner on ALL tool outputs before they enter LLM context | §7 PII Scanner |
| Prompt injection scanner on ALL code chunks before they enter LLM context | §7 Prompt Injection Scanner |
| Single-writer database queue (no race conditions even with 1 agent, but design for N) | §5 PostgreSQL |
| **Deliverable**: `bugswarm run ./repo --single-agent`. Finds real bugs in real codebases >1 confirmed bug per 10K tokens. If it can't, stop — scaling won't fix it. |

### Phase 5: Evidence Graph (4 weeks)
**Why fifth**: Replace debate with structured evidence discovery. Agents don't reply to each other. They contribute to an immutable hypergraph. Claims without sandbox-verified predictions are invisible to judges.

| What | Reference Sections |
|------|--------------------|
| Directed hypergraph: nodes = claims, predictions, sandbox runs, code locations; edges = supports, contradicts, confirms, refines | — |
| Graph query API for agents: "find weakest-supported claims," "find unexplored code regions," "find contradictory evidence" | — |
| Cryptographic hashing of sandbox run nodes (immutable evidence) | — |
| Agent scoring: `novelty_score`, `verification_score`, `efficiency_score` | — |
| **Deliverable**: The data structure that will eventually hold 50 agents' worth of evidence. Agents read/write to it; judges consume from it. |

### Phase 6: Multi-Agent Swarm (4 weeks)
**Why sixth**: Scale from 1 to 12 agents. The architecture decisions here determine whether the system works at 50.

| What | Reference Sections |
|------|--------------------|
| asyncio cooperative multitasking with `Semaphore(12)` — NOT 50 OS threads | §4 (Python), §9 (asyncio) |
| Brokered P2P via orchestrator queue — agents never directly message each other | — |
| Round-1 isolation: agents generate hypotheses independently | — |
| MMR-based critique routing (Hungarian algorithm) for Round 2 pairings | — |
| Persona seeding via stratified sampling (25% each: Defensive, Causal, Semantic, Adversarial) | — |
| Agent performance monitoring: hallucination rate, contribution score, loop count | — |
| Agent ejection + soft recovery (re-instate if ejection was wrong) | — |
| Spare agent pool for replacements | — |
| **Deliverable**: `bugswarm run ./repo --agents 12 --rounds 4`. 12 agents competing to contribute verified evidence to the graph. |

### Phase 7: Context Compression & Loop Detection (3 weeks)
**Why seventh**: 12 agents generate enough tokens to blow context windows and loop endlessly. These must be in place before scaling to 50.

| What | Reference Sections |
|------|--------------------|
| Context compression trigger: 80% of context window OR every 12 interactions | — |
| Map-reduce summarization (never single-pass over long transcripts) | — |
| Tiered summarization: local Llama 3 8B → GPT-4o-mini → GPT-4o (fidelity-gated) | §6 Summarization Models |
| Summary fidelity check: round-trip reconstruction + BERTScore > 0.85 | — |
| Semantic loop detection: sliding window of 5 messages, cosine > 0.92 for 3 windows | — |
| Triadic echo-chamber detection: Tarjan's SCC on agreement graph | — |
| Pattern-break prompt injection on loop detection | — |
| Per-agent rate limiter on `recall_raw_context` (max 3/round, cooldown 5 turns) | — |
| Tool output relevance scorer (heuristic, not LLM — zero token cost) | — |
| Vector DB query cache with embedding-based deduplication | §5 Vector DB |
| **Deliverable**: Swarm runs at 12 agents for 10+ rounds without context bloat or infinite loops. |

### Phase 8: Financial Control Plane (2 weeks)
**Why eighth**: Before scaling to 50 agents or letting this run for 40 hours, you need hard budget enforcement. This is not a feature — it's a circuit breaker.

| What | Reference Sections |
|------|--------------------|
| Triple budget: token cap, cost cap ($), wall-clock deadline | — |
| Budget check before every round and every batch | — |
| Severity-gated budget override (severity 8+ auto-proposes override, Security Judge votes) | — |
| Agent-level budget anomaly detection (Z-score > 3σ = capped) | — |
| Tiered spending: severity 1–3 (10%), 4–7 (40%), 8–10 (uncapped) | — |
| Information-theoretic stopping: stop when marginal information gain per token < threshold | — |
| Cost attribution per agent/per batch/per judge | — |
| **Deliverable**: `--token-budget 5M --cost-budget 50 --time-budget 40h`. Swarm self-terminates on any budget exhaustion. |

### Phase 9: Scale to 50 Agents (2 weeks)
**Why ninth**: The architecture was designed for this. If phases 1–8 are solid, scaling is a config change. If it breaks here, the problem is in an earlier phase.

| What | Reference Sections |
|------|--------------------|
| Increase batch semaphore from 12 to 50 | — |
| Load-test DB writer queue back-pressure (throttle → pause → Redis overflow) | §5 Redis |
| Load-test Vector DB concurrent queries (cache hit rate must stay >70%) | §5 Vector DB |
| Load-test LLM API rate limits across providers | §6 |
| Diversity Threshold computation at 50 agents (must stay <100ms CPU) | — |
| GPU memory budgeting if local LLM is co-located (reserve VRAM for sandbox) | §1 GPU |
| **Deliverable**: `bugswarm run ./repo --agents 50 --rounds 15`. Stable for 40-hour runs. |

### Phase 10: The Bench — 5 Judges (4 weeks)
**Why tenth**: Judges are expensive (frontier models). They only fire after the swarm has accumulated enough evidence. Build them last because they depend on every earlier phase producing clean, structured data.

| What | Reference Sections |
|------|--------------------|
| 5 Judge models with specialty prompts (Security, Logic, Concurrency, Architecture, Data Integrity) | §6 Judge Models |
| Structured brief format (not raw 20K-token transcripts): executive summary top, evidence receipts bottom | — |
| Judge verdict schema: `{bug_verified, severity, evidence_quality, causal_chain_valid, false_positive_risk, trace_citation}` | — |
| Pre-deployment calibration on 200 golden bug reports | — |
| Score normalization per model bias (`severity_bias`, `false_positive_rate`) | — |
| Tiebreaker protocol: weighted re-vote → evidence-only vote → specialist escalation → Meta-Judge | — |
| Judge queries: budget of 3 per case, can request swarm re-investigation | — |
| Judge query recursion limit: case re-opened at most once | — |
| Inter-Judge Synthesis Debate for unresolvable disagreement | — |
| Conservative principle: "when in doubt, escalate severity" | — |
| **Deliverable**: Judges produce structured, evidence-cited verdicts. No vote-based consensus — threshold on evidence quality metrics. |

### Phase 11: TUI (4 weeks)
**Why eleventh**: The TUI is the human interface. It can be built in parallel with phases 6–10 (it reads from the same orchestrator state), but its final form requires the Bench and Financial Control Plane to be functional for those views to render real data.

| What | Reference Sections |
|------|--------------------|
| Textual framework setup, 7 named screens | §12 TUI |
| Unix socket + MessagePack protocol between TUI process and orchestrator | §12 TUI |
| Swarm Overview, Agent Detail, Sandbox Monitor, Bench Panel, Evidence Graph, Finance, Raw Logs | §12 TUI |
| Human override modal with governance controls (justification required, dual-auth for severity 9+) | §12 TUI |
| Command mode (`:override`, `:eject`, `:budget`, `:provider`) | §12 TUI |
| Theming (Dark Matrix, Light, Monochrome + custom `.tcss`) | §12 TUI |
| Export: JSON, SARIF, HTML, CSV, PNG screenshot | §12 TUI |
| Non-interactive modes: `--watch`, `--headless`, `--json`, `-q` | §12 TUI |
| Responsive layout (120+ cols → 60 cols → single-panel) | §12 TUI |
| **Deliverable**: `bugswarm run ./repo` opens full TUI. `bugswarm run ./repo --json` streams to stdout for Minitia. |

### Phase 12: Minitia Integration (2 weeks)
**Why last**: Minitia is the meta-tool that installs and orchestrates Bug Swarm alongside future engines. It depends on Bug Swarm being a stable, standalone binary with a machine-readable interface. Build it last because it's the thinnest layer.

| What | Reference Sections |
|------|--------------------|
| Engine contract: `<engine> run | status | stop | report` | §15 Minitia Integration |
| `minitia install bugswarm` — downloader + checksum verifier + symlink | §15 |
| `minitia run bugswarm ./repo --budget 50` — subprocess runner with `--json` flag | §15 |
| Minitia dashboard TUI showing all running engines | §15 |
| `[D]etach TUI` — launches engine's native TUI in new terminal | §15 |
| Engine registry (`registry.minitia.ai/engines.json`) | §15 |
| **Deliverable**: `brew install minitia && minitia install bugswarm && minitia run bugswarm ./repo`. Users never install Bug Swarm directly unless they want the standalone experience. |

### Phase 13: Observability, Alerts, CI/CD (2 weeks)
**Why last**: Metrics and alerting depend on the system being stable. Don't build dashboards for a system that doesn't work yet.

| What | Reference Sections |
|------|--------------------|
| Prometheus metrics (all `bugswarm_*` gauges/counters) | §11 Observability |
| Structured JSON logging to stdout/stderr | §11 |
| OpenTelemetry tracing (span per agent turn, sandbox execution, judge deliberation) | §11 |
| Alerting rules (budget >80%, sandbox escape, OOM, judge deadlock) | §11 |
| CI/CD integration: `bugswarm ci ./src --diff origin/main...HEAD` | §13 Build & Deployment |
| SARIF export for GitHub Code Scanning | §13 |
| **Deliverable**: Grafana dashboard, PagerDuty alerts, GitHub Actions integration. |

### Phase 14: Hardening & Feedback Loops (ongoing)
Never truly "done." These improve the system with every run and every production escape.

| What | Reference Sections |
|------|--------------------|
| Production bug ingestion: `bugswarm learn --incident-id PROD-1234` | — |
| Retrospective analysis: did CPG miss it? Sandbox fail? Agent overlook? Judge dismiss? | — |
| Auto-tuning: prompt weights, persona distribution, CPG parameters, judge calibration | — |
| CVE-to-CPG pipeline: new CVEs → graph patterns → vulnerability corpus | — |
| Regression firewall for patches: property-based tests, differential tests, fuzzing | — |
| WAL replication to S3 with CRC32 (survive kernel panic) | §5 Object Storage |
| Override governance audit trail | §14 Compliance |
| Domain profiles: `web_app`, `smart_contract`, `medical_device`, `crypto_library` | — |
| **Deliverable**: The system gets better with every run. False negatives become permanent system improvements. |

---

### Dependency Graph (What Blocks What)

```
Phase 1: Sandbox ─────────────────────────────┐
Phase 2: CPG ─────────────────────────────────┤
Phase 3: LLM Gateway ─────────────────────────┤
                                               ├──▶ Phase 4: Single Agent
                                               │         │
                                               │         ▼
                                               │    Phase 5: Evidence Graph
                                               │         │
                                               │         ▼
                                               │    Phase 6: Multi-Agent (12)
                                               │         │
                          ┌────────────────────┤         │
                          ▼                    │         ▼
                   Phase 7: Compression        │    Phase 8: Finance
                   Loop Detection              │         │
                          │                    │         │
                          └────────┬───────────┘         │
                                   ▼                     │
                            Phase 9: Scale to 50         │
                                   │                     │
                                   ▼                     │
                            Phase 10: The Bench          │
                                   │                     │
                    ┌──────────────┼──────────────┐      │
                    ▼              ▼              ▼      │
             Phase 11: TUI  Phase 12: Minitia  Phase 13:│
                                               Observability
                                                    │
                                                    ▼
                                             Phase 14: Harden
```

Phases 7+8, 11+12 can be developed in parallel once Phase 6 is stable.

---

## 1. Compute & Hardware

### Development / Single-Repository Runs
| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | 8 cores (x86_64) | 16+ cores |
| RAM | 32 GB | 64 GB |
| Disk | 100 GB SSD (NVMe) | 500 GB NVMe |
| GPU | None (cloud-only LLM) | 1× NVIDIA A10 (24 GB VRAM) for local Llama 3 8B inference |
| Network | 100 Mbps | 1 Gbps |

### Production / Multi-Tenant / Concurrent Swarm Runs
| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | 32 cores (x86_64 or ARM64) | 64 cores |
| RAM | 128 GB | 256 GB ECC |
| Disk | 1 TB NVMe RAID-1 | 4 TB NVMe RAID-10 |
| GPU | 2× NVIDIA A100 (80 GB) or 4× A10 | 4× A100 or 8× A10 with MIG partitioning |
| Network | 10 Gbps | 25 Gbps |

#### GPU Justification
- **Local LLM inference**: Llama 3 8B Q4_K_M at batch-size=12 needs ~6 GB VRAM. A single A10 handles 12 concurrent agent requests. For 50 agents, shard across 4 GPUs.
- **CUDA sandbox execution**: PoC validation for GPU-dependent code needs ephemeral GPU access. MIG partitioning on A100 allows 7 isolated GPU instances per card, one per sandbox container.
- **Embedding generation**: `all-MiniLM-L6-v2` runs on CPU (384-dim, <100ms per embedding at batch-50). No GPU needed.

---

## 2. Operating System

- **Primary**: Linux (Ubuntu 22.04 LTS or Rocky Linux 9)
- **Kernel**: 5.15+ (required for cgroups v2, eBPF, seccomp, user namespace remapping, `binfmt_misc`)
- **Kernel modules**: `overlay`, `nf_nat` (disabled), `br_netfilter` (disabled), `nvidia` / `nvidia_uvm` (if GPU present)
- **Filesystem**: ext4 or XFS with `reflink` support (for copy-on-write sandbox clones). Btrfs acceptable.
- **Mandatory Access Control**: AppArmor (Ubuntu) or SELinux (Rocky) — enforcing mode, not permissive

### Kernel Boot Parameters
```
cgroup_no_v1=all systemd.unified_cgroup_hierarchy=1
```
This forces cgroups v2 exclusively, required for the orchestrator's container resource limits.

---

## 3. Container Runtime

### Docker Engine
- **Version**: 24.0+ (BuildKit enabled)
- **Storage driver**: `overlay2` with `xfs` backing filesystem
- **User namespace remapping**: `userns-remap` enabled, mapping container root → host UID 100000–165536
- **Default seccomp profile**: Custom, whitelisting <50 syscalls (see §7)
- **Default capabilities**: `CAP_DROP_ALL`
- **cgroup driver**: `systemd` (cgroups v2)
- **Runtime**: `runc` (default) or `crun` for lower memory footprint

### NVIDIA Container Toolkit (GPU-required deployments only)
- `nvidia-container-toolkit` 1.14+
- `nvidia-container-runtime` configured as Docker runtime
- CUDA driver 535+ with MIG support enabled on A100

### Firecracker (optional, for highest-security deployments)
- Firecracker 1.5+ microVM manager
- Replaces Docker for sandbox isolation when `--sandbox-engine firecracker` is set
- Requires `/dev/kvm` access on host

---

## 4. Language Runtimes & Compilers

### Core System (Orchestrator, Agents, Gateway)
| Component | Language | Version | Rationale |
|-----------|----------|---------|-----------|
| Orchestrator | Rust | 1.75+ | Deterministic, no GC pauses, direct syscall access for cgroups/seccomp |
| Agents (LLM loop) | Python | 3.11+ | asyncio ecosystem, Pydantic models, rich LLM library support |
| Sandbox daemon | Rust | 1.75+ | Minimal attack surface, direct Linux namespace management |
| CLI | Python | 3.11+ | Typer + Rich integration with agent/orchestrator SDK |
| CPG builder | Rust | 1.75+ | tree-sitter bindings, graph construction, performance-critical parsing |

### Sandbox Target Runtimes (pre-installed in sandbox images)
| Language | Min Version | Tooling |
|----------|-------------|---------|
| Python | 3.8, 3.9, 3.10, 3.11, 3.12 | pip, venv, Hypothesis, pytest, coverage.py, faulthandler |
| Node.js | 16, 18, 20, 22 LTS | npm, yarn, pnpm, Stryker, Playwright |
| Go | 1.19, 1.20, 1.21, 1.22 | pprof, race detector (`-race`), `rr` replay |
| Java | 11, 17, 21 LTS | Maven, Gradle, jmap, jstack, JFR |
| Ruby | 3.1, 3.2, 3.3 | Bundler, RSpec |
| Rust | 1.70+ | cargo, `rr` replay |
| C/C++ | GCC 12+, Clang 16+ | AddressSanitizer, UBSan, ThreadSanitizer, `rr` replay, AFL++ |

---

## 5. Databases & Storage

### Primary: PostgreSQL 16+
- **Purpose**: Relational state — swarms, batches, agent_runs, debate_logs, judge_verdicts, token_ledger, calibration_data, override_audit_log
- **Extensions required**: `pg_stat_statements`, `pg_prewarm`, `pg_cron`
- **Extensions recommended**: TimescaleDB (for time-series token/cost data), `pgvector` (for hybrid queries)
- **Configuration**:
  ```
  max_connections: 200
  shared_buffers: 25% of system RAM
  wal_level: replica (for WAL replication)
  wal_keep_size: 1GB
  synchronous_commit: remote_write (if replica exists)
  ```
- **Partitioning**: `agent_runs` and `debate_logs` tables partitioned by `swarm_id` (list partitioning)
- **Materialized views**: `mv_swarm_summary`, `mv_agent_performance`, `mv_judge_calibration`

### SQLite 3.42+ (WAL Journal)
- **Purpose**: Local append-only WAL for crash recovery, per-agent state snapshots
- **Mode**: WAL journal mode, `synchronous=NORMAL`, `cache_size=-64000` (64 MB)
- **Concurrent access**: Single-writer, multiple-reader (orchestrator is sole writer)

### Vector Database: ChromaDB 0.5+ or Milvus 2.4+
- **ChromaDB** (development / single-node):
  - Embedding model: `all-MiniLM-L6-v2` (384-dim, local, no API cost)
  - Distance metric: cosine
  - HNSW index with fixed seed (`hnsw:space=cosine`, `hnsw:construction_ef=200`, `ef_search=256`)
  - Post-query exact re-ranking enabled
- **Milvus** (production / multi-node):
  - Distributed index across 3+ nodes
  - IVF_FLAT or HNSW index depending on corpus size
  - Consistency level: strong (for deterministic results)

### Graph Database: Neo4j 5+ or Kuzu 0.3+
- **Purpose**: Code Property Graph storage — AST nodes, CFG/DFG edges, taint paths, vulnerability patterns
- **Neo4j** (production): APOC and GDS libraries required for subgraph isomorphism and graph edit distance
- **Kuzu** (embedded): For single-machine deployments, zero-config embedded graph DB
- **Schema**: Node types: `Function`, `CallSite`, `Variable`, `Parameter`, `Return`, `Sink`, `Source`, `Sanitizer`. Edge types: `CALLS`, `CONTROLS`, `DATA_FLOW`, `TAINT`, `PATCHES`, `SIMILAR_TO`.

### Redis 7+ (optional, for high-throughput deployments)
- **Purpose**: Message buffer for DB writer overflow (QD7), pub/sub for orchestrator-to-agent signals
- **Data structures**: Streams (for debate log buffer), Pub/Sub (for FREEZE/PAUSE signals)
- **Persistence**: AOF + RDB (for recovery)
- **Not required** for single-swarm runs; the asyncio.Queue handles back-pressure in-process.

### Object Storage: S3-compatible (MinIO or AWS S3)
- **Purpose**: WAL replication target (QD6), sandbox execution artifacts, large stack traces offloaded from context windows
- **Bucket structure**:
  - `bugswarm-wal/{swarm_id}/` — WAL block snapshots + CRC32 checksums
  - `bugswarm-sandbox/{swarm_id}/{run_id}/` — stack traces, PoC scripts, execution receipts
  - `bugswarm-models/` — calibration datasets, prompt templates, persona definitions

---

## 6. LLM Providers & Models

### Agent Models (Swarm Tier — Cheap, Fast)
| Model | Provider | Context Window | Cost (per 1M tokens) | Role |
|-------|----------|----------------|----------------------|------|
| Llama 3 8B Q4_K_M | Local (Ollama) | 8K | $0 (hardware amortized) | Primary agent model (60% of calls) |
| Gemini 1.5 Flash | Google | 1M (uses ~128K) | $0.075 input / $0.30 output | Secondary agent (25% of calls) |
| GPT-4o-mini | OpenAI | 128K | $0.15 input / $0.60 output | High-complexity agent fallback (15% of calls) |

### Judge Models (Bench Tier — Frontier, Accurate)
| Model | Provider | Context Window | Cost (per 1M tokens) | Role |
|-------|----------|----------------|----------------------|------|
| GPT-4o | OpenAI | 128K | $5 input / $15 output | Security Judge, Logic Judge |
| Claude 3.5 Sonnet | Anthropic | 200K | $3 input / $15 output | Architecture Judge, Data Integrity Judge |
| Gemini 1.5 Pro | Google | 2M (uses ~128K) | $3.50 input / $10.50 output | Concurrency Judge |
| Claude 3 Opus | Anthropic | 200K | $15 input / $75 output | Meta-Judge (tiebreaker only, rare) |

### Summarization Models (Compression Tier — Cheapest Possible)
| Model | Provider | Cost (per 1M tokens) | Role |
|-------|----------|----------------------|------|
| Llama 3 8B Q4_K_M | Local (Ollama) | $0 | Tier 1 summarization (80% of cases) |
| GPT-4o-mini | OpenAI | $0.15 input | Tier 2 summarization (low fidelity fallback) |
| GPT-4o | OpenAI | $5 input | Tier 3 summarization (judge-requested detailed recap) |

### Embedding Model
| Model | Dimension | Deployment | Cost |
|-------|-----------|------------|------|
| `all-MiniLM-L6-v2` | 384 | Local (sentence-transformers) | $0 |
| `text-embedding-3-small` | 1536 | OpenAI API | $0.02/1M tokens (fallback) |

### Local Model Server
- **Ollama** 0.1.30+ or **llama.cpp** server (b2827)
- Loaded with `--gpu-memory-fraction 0.6` (reserve 40% VRAM for sandbox)
- `OLLAMA_NUM_PARALLEL=12` (match batch size)
- `OLLAMA_MAX_LOADED_MODELS=2` (Llama 3 8B + embedding model)
- Deterministic decoding: `seed=42`, `top_k=1`, `temperature=0` for summarization

---

## 7. Security & Isolation Requirements

### Sandbox Container Profile
```json
{
  "capabilities": ["CAP_DROP_ALL"],
  "security_opt": ["no-new-privileges", "apparmor=docker-sandbox"],
  "read_only": true,
  "network_mode": "none",
  "pid_mode": "container:sandbox-pid-ns",
  "tmpfs": {
    "/tmp": "noexec,nosuid,nodev,size=100M",
    "/run": "noexec,nosuid,nodev,size=10M"
  },
  "cpus": 1,
  "memory": "512m",
  "memory-swap": "512m",
  "pids-limit": 100,
  "storage_opt": ["size=1G"],
  "ulimit": {
    "nproc": 50,
    "nofile": 256,
    "cpu": 60
  },
  "userns_mode": "host-uid-100000"
}
```

### Allowed Seccomp Syscalls (Whitelist)
```
read, write, openat, close, fstat, lseek, mmap, mprotect, munmap,
brk, rt_sigaction, rt_sigprocmask, rt_sigreturn, ioctl, pread64,
pwrite64, readv, writev, access, pipe2, dup, dup2, dup3, fcntl,
clock_gettime, exit, exit_group, getpid, gettid, getuid, geteuid,
getgid, getegid, arch_prctl, futex, set_tid_address, set_robust_list,
rseq, madvise, sched_yield, sched_getaffinity, nanosleep, epoll_create1,
epoll_ctl, epoll_wait, eventfd2, prlimit64, getrandom, tgkill
```
**Total: ~45 syscalls.** Explicitly blocked: `ptrace`, `mount`, `kexec_load`, `bpf`, `perf_event_open`, `create_module`, `init_module`, `delete_module`, `clone` (except CLONE_VM|CLONE_VFORK via `fork`), `unshare`, `setns`, `personality`, `chroot`, `pivot_root`, `acct`, `add_key`, `request_key`, `keyctl`, `iopl`, `ioperm`, `kexec_file_load`, `finit_module`, `nfsservctl`, `_sysctl`, `process_vm_readv`, `process_vm_writev`, `uselib`, `syslog`, `vhangup`, `swapoff`, `swapon`, `reboot`.

### PII / Secrets Scanner Pipeline
Applied to ALL tool outputs before they enter any LLM context window:
- **Regex patterns**: Email (`[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}`), phone, SSN, credit card (Luhn validation), IPv4, IPv6
- **Entropy-based detection**: Shannon entropy > 4.5 on base64-like strings → potential API key/token
- **Path-based**: Files matching `*.env`, `*.pem`, `*.key`, `credentials.*`, `id_rsa*`, `*.pfx`, `*.p12`, `secrets.*`
- **Variable-name-based**: `password`, `secret`, `token`, `key`, `credential`, `private_key`, `api_key`, `auth_token`
- **User-configurable**: `.bugswarm_secrets.yaml` in repo root
- **Redaction format**: `[REDACTED: type=<category>]`
- **Encryption at rest**: AES-256-GCM, per-session key, stored in memory only, zeroized on session termination

### Prompt Injection Scanner
Runs on ALL code chunks before they enter agent context:
- Model: Fine-tuned `deberta-v3-base` classifier (binary: safe / injection)
- Patterns detected: "Ignore previous instructions", "You are now DAN", "SYSTEM:", "[INST]", "### Human:", role-switching tokens, override directives
- Action: Chunks classified as injection are wrapped in `<SANITIZED_CODE_CONTEXT>` tags or blocked entirely
- Local execution only (no external API call for scanning)

### Network Segmentation
- The orchestrator process communicates with LLM providers over HTTPS only (TLS 1.3)
- The sandbox daemon communicates with the orchestrator over Unix domain socket (`/var/run/bugswarm/sandbox.sock`, permission `0600`)
- Sandbox containers have NO network access (`--network=none`)
- No inbound ports exposed. The CLI connects to the orchestrator via the same Unix socket or a local TCP socket bound to `127.0.0.1` only.

---

## 8. External Services & APIs

### Required
| Service | Purpose | Auth Method |
|---------|---------|-------------|
| OpenAI API | GPT-4o, GPT-4o-mini (judges, fallback agents, summarization) | API key (`OPENAI_API_KEY`) |
| Anthropic API | Claude 3.5 Sonnet, Claude 3 Opus (judges) | API key (`ANTHROPIC_API_KEY`) |
| Google AI API | Gemini 1.5 Flash, Gemini 1.5 Pro (agents, concurrency judge) | API key (`GOOGLE_API_KEY`) |

### Optional
| Service | Purpose | Auth Method |
|---------|---------|-------------|
| AWS S3 / MinIO | WAL replication, artifact storage | IAM role or access key |
| PostgreSQL (managed) | RDS / Cloud SQL for production deployments | IAM or password |
| Neo4j Aura (managed) | Cloud-hosted graph DB for CPG | Username/password or API key |
| Redis Cloud | Message buffer for high-throughput | Password or IAM |
| etcd / Consul | Service discovery for multi-node deployments | mTLS |
| SMTP Server | Override review notifications (QD48) | SMTP credentials |
| Slack/Discord Webhook | Alerting (sandbox escape attempts, budget overruns, judge deadlocks) | Webhook URL |

---

## 9. Build & Dependency Toolchain

### Python (Core)
```
python >= 3.11
pip >= 23.0
```
**Required packages:**
```
typer >= 0.12.0
rich >= 13.7.0
pydantic >= 2.6.0
httpx >= 0.27.0
openai >= 1.14.0
anthropic >= 0.25.0
google-generativeai >= 0.6.0
chromadb >= 0.5.0
sentence-transformers >= 2.6.0
scikit-learn >= 1.4.0
sqlalchemy >= 2.0.25
asyncpg >= 0.29.0 (PostgreSQL)
aiosqlite >= 0.20.0 (SQLite)
redis >= 5.0.0 (optional)
pyyaml >= 6.0
cryptography >= 42.0.0
jinja2 >= 3.1.3
tenacity >= 8.2.0 (retry logic)
```

### Rust (Oracle / Sandbox daemon / CPG builder)
```
rustc >= 1.75.0
cargo >= 1.75.0
```
**Required crates:**
```
tokio = { version = "1.36", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bollard = "0.16" (Docker API)
nix = "0.28" (Linux namespaces, cgroups)
caps = "0.5" (capability management)
seccompiler = "0.4" (seccomp BPF generation)
tree-sitter = "0.22"
tree-sitter-python = "0.21"
tree-sitter-javascript = "0.21"
tree-sitter-typescript = "0.21"
tree-sitter-go = "0.21"
tree-sitter-java = "0.21"
tree-sitter-ruby = "0.21"
tree-sitter-rust = "0.21"
tree-sitter-c = "0.21"
tree-sitter-cpp = "0.21"
neo4rs = "0.7" (Neo4j driver) or kuzu = "0.3" (embedded graph DB)
sha2 = "0.10"
blake3 = "1.5"
ring = "0.17" (cryptography)
rustls = "0.23" (TLS for LLM API calls)
libc = "0.2"
```

### Go (optional sandbox daemon alternative)
```
go >= 1.22
```
Required for the minimal sandbox daemon mentioned in QD36 (<5,000 lines). If using the Rust daemon, Go is not needed.

### System Packages (Ubuntu 22.04)
```bash
apt-get install -y \
  build-essential pkg-config \
  libssl-dev libseccomp-dev \
  docker.io docker-buildx \
  nvidia-container-toolkit (if GPU) \
  postgresql-client-16 \
  redis-tools \
  rr (Mozilla Record and Replay, v5.6+) \
  bpfcc-tools linux-headers-$(uname -r) (eBPF tooling) \
  apparmor-profiles apparmor-utils \
  jq yq (JSON/YAML CLI tools) \
  tree-sitter-cli
```

---

## 10. Filesystem Layout

```
/etc/bugswarm/
├── config.yaml                    # Global configuration
├── profiles/
│   ├── web_app.yaml               # Domain profiles (Q49)
│   ├── smart_contract.yaml
│   ├── medical_device.yaml
│   └── crypto_library.yaml
├── personas/
│   ├── defensive.txt              # System prompt templates
│   ├── causal.txt
│   ├── semantic.txt
│   └── adversarial.txt
├── judges/
│   ├── security.txt
│   ├── logic.txt
│   ├── concurrency.txt
│   ├── architecture.txt
│   └── data_integrity.txt
├── seccomp/
│   └── sandbox.json               # Custom seccomp profile
├── calibration/
│   └── golden_dataset.json        # 200 pre-adjudicated bug reports
└── providers.yaml                 # LLM provider registry + API keys (0600)

/var/lib/bugswarm/
├── wal/                           # SQLite WAL files, per-swarm
├── sandbox_outputs/               # Stack traces, PoC logs, artifacts
├── sandbox_images/                # Built Docker images
├── checkpoints/                   # Agent state snapshots (every 5 min)
├── vector_db/                     # ChromaDB persistent storage
├── graph_db/                      # Kuzu embedded DB (if not using Neo4j)
└── cache/
    ├── query_cache/               # Vector DB query cache
    └── model_cache/               # Ollama model storage

~/.config/bugswarm/
├── credentials.yaml               # User API keys (0600)
├── history.db                     # CLI command history + run logs
└── per-repo/
    └── <repo_hash>/
        ├── index_cache/           # Cached CPG data
        └── run_history/           # Previous run results
```

---

## 11. Observability & Monitoring

### Metrics (Prometheus + Grafana)
| Metric | Type | Labels |
|--------|------|--------|
| `bugswarm_agents_active` | Gauge | swarm_id |
| `bugswarm_tokens_consumed` | Counter | swarm_id, agent_id, model, input/output |
| `bugswarm_cost_usd` | Counter | swarm_id, component (agent/judge/summarization/sandbox) |
| `bugswarm_bugs_found` | Counter | swarm_id, severity, verified (bool) |
| `bugswarm_sandbox_executions` | Counter | swarm_id, result (pass/fail/timeout/oom/tainted) |
| `bugswarm_sandbox_execution_seconds` | Histogram | swarm_id |
| `bugswarm_judge_verdicts` | Counter | judge_id, verdict (confirmed/dismissed) |
| `bugswarm_debate_rounds` | Gauge | swarm_id |
| `bugswarm_semantic_loops_detected` | Counter | swarm_id |
| `bugswarm_vector_db_cache_hit_ratio` | Gauge | swarm_id |
| `bugswarm_compression_fidelity_score` | Gauge | swarm_id |
| `bugswarm_queue_depth` | Gauge | queue_name |
| `bugswarm_host_resource_usage` | Gauge | resource (cpu/mem/disk/gpu) |

### Logging
- **Structured JSON logs** to stdout/stderr (12-factor app)
- **Log levels**: TRACE (full agent conversation), DEBUG (tool calls, sandbox runs), INFO (round boundaries, verdicts), WARN (timeouts, loops, budget warnings), ERROR (crashes, API failures), CRITICAL (sandbox escape attempt, data corruption)
- **OpenTelemetry traces**: Span per agent turn, sandbox execution, judge deliberation. Trace context propagated across asyncio tasks.
- **Audit log**: Immutable append-only log of all human overrides, budget overrides, sandbox escape attempts, and judge recusals. Cryptographically chained (SHA256 chain).

### Alerting Rules
| Condition | Severity | Channel |
|-----------|----------|---------|
| `bugswarm_cost_usd > budget * 0.8` | WARN | Slack, CLI bell |
| `bugswarm_cost_usd > budget` | CRITICAL | PagerDuty |
| `bugswarm_sandbox_executions{result="tainted"} > 0` | WARN | Slack |
| `CRITICAL:SANDBOX_ESCAPE_ATTEMPT` log event | CRITICAL | PagerDuty + email security lead |
| `bugswarm_host_resource_usage{resource="mem"} > 0.8` | WARN | Slack |
| `bugswarm_judge_verdicts{verdict="dismissed"} / total > 0.9` for 3 consecutive swarms | WARN | Slack (possible swarm degradation) |
| `bugswarm_compression_fidelity_score < 0.7` | WARN | Slack |
| PostgreSQL replication lag > 5s | WARN | PagerDuty |
| Orchestrator heartbeat missing > 30s | CRITICAL | PagerDuty |

---

## 12. Terminal UI & Visualization Engine

The TUI is not a dashboard bolted onto the orchestrator. It is the primary human interface — where operators watch the swarm reason, overrule judges, inspect evidence, and control budgets. It must render 50 concurrent agents, live sandbox results, judge deliberations, and financial data at sub-500ms refresh rates without terminal flicker or dropped frames.

### Architecture

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Rendering | **Textual** 1.0+ | Terminal framework with CSS-like layout, reactive data binding, async widget model. Replaces raw `rich.live` — handles 60fps terminal rendering with diff-based screen updates. |
| Styling | Textual CSS (TCSS) | Themeable via external `.tcss` files. Dark mode default (green-on-black matrix aesthetic). Light mode for accessibility. |
| Data pipe | Unix domain socket + MessagePack | Orchestrator pushes state deltas (not full snapshots) to the TUI process over `/var/run/bugswarm/tui.sock`. Binary MessagePack encoding — zero allocation overhead vs JSON. |
| State model | Textual `reactive` attributes | Each widget binds to reactive attributes. When orchestrator pushes a delta, the attribute updates; Textual diffs and re-renders only changed screen regions. |
| Widget library | Custom `bugswarm_tui` package | Reusable widgets for agent cards, batch timelines, judge panels, cost bars, sandbox logs. Separated from orchestrator code. |

### Views (Screens)

The TUI has 7 named screens, navigable via keyboard shortcuts.

#### 1. Swarm Overview (`F1` — default)

The primary view. Replaces the spec's "matrix-style logs" with a structured grid.

```
┌─ Bug Swarm · acme/webapp · RUNNING · 02:14:38 elapsed ───────────────────────────────────────────────────────┐
│                                                                                                                │
│  ╔══════════════════════════════════╗  ┌── Agents Live ──────────────────────────────────────────────────────┐ │
│  ║         BUDGET BURN             ║  │ ID  Persona     Status    Tokens  Bugs  Health                        │ │
│  ║  ██████████░░░░░░░ $23.14/$50   ║  │ A1  Causal      ▶ R4     12,431    2   🟢 OK                         │ │
│  ║  ██████░░░░░░░░░░░ 2.1M/5M tok  ║  │ A2  Adversarial ▶ R4     11,892    3   🟢 OK                         │ │
│  ║  ████████████████░ 36h/40h      ║  │ A3  Semantic    ⏳ R4     10,234    1   🟡 Slow (Z=2.1)               │ │
│  ╚══════════════════════════════════╝  │ A4  Defensive   🔁 R4    14,567    0   🔴 Looping                    │ │
│                                        │ A5  Causal      ▶ R4      9,876    2   🟢 OK                         │ │
│  ┌── Round Summary ──────────────────┐ │ A6  Adversarial ▶ R4     11,003    1   🟢 OK                         │ │
│  │ Round 4/15 · Batch 3/5            │ │ A7  Semantic    🗑️  —        —      —   ⛔ Ejected (halluc. 62%)     │ │
│  │                                   │ │ A8  Defensive   ▶ R4      8,921    0   🟢 OK                         │ │
│  │  Evidence Graph:                  │ │ A9  Causal      ▶ R4     10,445    2   🟢 OK                         │ │
│  │  Nodes: 247  Edges: 1,203         │ │ A10 Adversarial ⏸  R4    9,234    1   🟡 Wait (DB queue 78%)         │ │
│  │  Verified: 18  Contradicted: 5    │ │ A11 Semantic    ▶ R4     11,678    2   🟢 OK                         │ │
│  │  Unexplored: 34 code regions      │ │ A12 Defensive   ▶ R4     10,001    1   🟢 OK                         │ │
│  │                                   │ └──────────────────────────────────────────────────────────────────────┘ │
│  │  Diversity Score: 0.62 ████░░ OK  │                                                                        │
│  │  Loop Detections: 2 this round    │ ┌── Recent Evidence ──────────────────────────────────────────────────┐ │
│  │  Agent Ejections: 1               │ │ ✓ A2: NPE confirmed at auth.py:42       [run #8472]  Severity 7 ▲  │ │
│  └───────────────────────────────────┘ │ ✗ A6: Race condition claim refuted      [run #8501]  False positive│ │
│                                        │ ✓ A1: SQL injection verified at db.py:89  [run #8493]  Severity 9 ▲▲│ │
│  ┌── Cost Breakdown ─────────────────┐ │ ⧖ A3: Deadlock suspected at lock.c:156   [run #8512]  Investigating│ │
│  │ Agents:   ████████  $13.45  58%   │ │ ? A8: Memory leak claim unverifiable     [run #8530]  Needs PoC    │ │
│  │ Judges:   ████      $5.20   22%   │ └──────────────────────────────────────────────────────────────────────┘ │
│  │ Compress: ██        $2.10    9%   │                                                                        │
│  │ Sandbox:  ██        $1.89    8%   │ ┌── Log ──────────────────────────────────────────────────────────────┐ │
│  │ GPU Amort:█         $0.50    2%   │ │ 14:32:01  Batch 3 started · 12 agents dispatched                    │ │
│  └───────────────────────────────────┘ │ 14:32:04  A2 submitted PoC → sandbox #8472 queued                    │ │
│                                        │ 14:32:07  Sandbox #8472: PASS · NPE at auth.py:42                   │ │
│  Tokens/min (last hour)               │ 14:32:09  A6 submitted PoC → sandbox #8501 queued                    │ │
│  ▁▂▃▅▃▂▁▂▃▄▃▂▁▂▃▄▅▄▃▂▁▂▃▄▅▆▅▄▃▂▁   │ 14:32:12  Sandbox #8501: FAIL · claim refuted                      │ │
│                                        │ 14:32:15  A2 finding escalated to Bench (severity 7)                │ │
│                                        └──────────────────────────────────────────────────────────────────────┘ │
│  F1 Swarm  F2 Agents  F3 Sandbox  F4 Bench  F5 Graph  F6 Finance  F7 Logs  Tab:Focus  q:Quit  /:Search         │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Live elements:**
- Agent table rows update in real-time (status icons animate: `▶` active, `⏳` waiting, `🔁` looping, `⏸` paused, `🗑️` ejected)
- Budget burn bars redraw on every cost delta from orchestrator
- Evidence feed scrolls with newest entries at top, color-coded by verdict
- Tokens/min sparkline updates every 5 seconds
- Diversity score bar shifts color: green (>0.6), yellow (0.4–0.6), red (<0.4)
- Bottom log scrolls like `tail -f`, with ANSI color codes for severity

#### 2. Agent Detail (`F2`)

Drill into any agent (selected via arrow keys or `/` search by ID):

```
┌─ Agent A2 · Adversarial · Round 4 · Trust Score: 0.87 ───────────────────────────────────────────────────────┐
│                                                                                                                │
│  ┌── Current Hypothesis ───────────────────────────┐  ┌── Tool Call History ─────────────────────────────────┐ │
│  │ Bug: NullPointerException in auth.py:42          │  │ #1  query_cpg("auth.py authenticate")  → 3 nodes    │ │
│  │ Severity estimate: 7                             │  │ #2  trace_dependency("login", radius=2)  → 47 lines │ │
│  │ Prediction: input `null` username → crash        │  │ #3  exec_sandbox(poc_8472.py)            → PASS ✓   │ │
│  │ Sandbox: ✓ CONFIRMED (run #8472)                 │  │ #4  expand_tool_result(#2, frames 3-7)   → 12 lines │ │
│  │ Evidence quality: 0.92                           │  │ #5  recall_raw_context(turns 4-6)        → 1,203 tok│ │
│  │                                                  │  │ #6  query_vector_db("auth null check")   → cache hit│ │
│  │  ┌── Causal Chain ──────────────────────────────┐│  └────────────────────────────────────────────────────┘ │
│  │  │ login() → validate_user() → user.name.trim() ││                                                         │
│  │  │         ↑ null passed, trim() on null → NPE  ││  ┌── Context Window Usage ────────────────────────────┐ │
│  │  └──────────────────────────────────────────────┘│  │ ████████████████░░░░ 82,400 / 128,000 tokens (64%)  │ │
│  │                                                  │  │ Compressed: 2x this round                            │ │
│  └──────────────────────────────────────────────────┘  │ Last compression: 14:28:01 · fidelity: 0.91          │ │
│                                                        └──────────────────────────────────────────────────────┘ │
│  ┌── Performance ─────────────────────────────────────┐  ┌── Persona Profile ─────────────────────────────────┐ │
│  │ Tokens consumed: 11,892     Cost: $0.87            │  │ Adversarial: "Assume this code was written by a     │ │
│  │ Verified findings: 3        Unverifiable: 1        │  │ junior developer. Every line is suspect."           │ │
│  │ Sandbox executions: 14      Timeouts: 1            │  │ Reasoning: Focus on error propagation chains.       │ │
│  │ Avg response time: 2.3s     Loops detected: 0      │  │ Tools preferred: trace_dependency, exec_sandbox     │ │
│  │ Hallucination rate: 8%      Recall rate: 1.2/rnd   │  │ Rotation: Next round → Semantic                     │ │
│  └────────────────────────────────────────────────────┘  └──────────────────────────────────────────────────────┘ │
│                                                                                                                │
│  ←Back  t:Tools  c:Context  p:PoC History  s:Sandbox Runs  j:JSON Export  Esc:Close                            │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

#### 3. Sandbox Monitor (`F3`)

Real-time view of all sandbox executions across the swarm:

```
┌─ Sandbox · 47 executions · 12 active · 3 failed · 2 timed out ───────────────────────────────────────────────┐
│                                                                                                                │
│  ┌── Active Executions ──────────────────────────────────────────────────────────────────────────────────────┐ │
│  │ Run ID    Agent  Status    Duration  Memory   CPU   Exit  Classification                                    │ │
│  │ #8513     A1     Running   00:01.2   87 MB   22%    —     —                                                 │ │
│  │ #8514     A5     Running   00:00.8   112 MB  45%    —     —                                                 │ │
│  │ #8515     A3     Running   00:03.1   204 MB  98%    —     (CPU-bound, possible infinite loop)               │ │
│  │ #8516     A11    Running   00:00.3   56 MB   12%    —     —                                                 │ │
│  │ #8512     A3     Done      00:04.7   189 MB  35%    0     PASS · Deadlock detected (pre-kill diagnostic)    │ │
│  │ #8510     A12    Done      00:01.2   45 MB   18%    1     FAIL · IndexError at api.py:203                  │ │
│  │ #8509     A2     Done      00:00.9   92 MB   40%    137   OOMKilled · MEMORY_EXHAUSTION_CONFIRMED           │ │
│  │ #8508     A9     Timeout   02:00.0   512 MB  100%   -     TIMEOUT · Probable infinite loop                  │ │
│  │ #8507     A6     Done      00:00.4   34 MB   8%     0     PASS · No bug found                               │ │
│  └────────────────────────────────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                                                │
│  ┌── Container Pool ───────────────────────────────────┐  ┌── Execution Stats ────────────────────────────────┐ │
│  │ Total containers: 50 (12 active, 38 idle/warm)      │  │ Total: 47 · Pass: 32 · Fail: 8 · Timeout: 2        │ │
│  │ Image builds cached: python3.11, node20, go1.22     │  │ OOM: 2 · Tainted: 0 · Architectural mismatch: 3    │ │
│  │ Avg container start: 0.3s (warm) / 2.1s (cold)     │  │ Flaky detection runs: 4 active (100 iterations ea)  │ │
│  │ GPU sandboxes: 0 active (no GPU code detected)     │  │ Avg execution: 2.3s · Avg PoC size: 47 lines       │ │
│  └──────────────────────────────────────────────────────┘  │ PII redactions: 12 (emails: 8, tokens: 4)          │ │
│                                                            └───────────────────────────────────────────────────┘ │
│  Enter:Inspect  r:Rerun  k:Kill  f:Follow Logs  l:Less (full trace)  p:Pause All  Esc:Back                      │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Sandbox inspect modal** (pressing Enter on a run):
- Full execution receipt (Docker image hash, command, exit code, timing, memory profile)
- If OOM: memory growth graph (ASCII line chart over time)
- If deadlock: thread dump with highlighted wait cycles
- If flaky: histogram of 100-run results
- Raw stdout/stderr with ANSI color preserved (scrollable via vim keys)
- Cryptographic receipt validation status (✓ valid / ✗ tainted)
- `r` re-runs the exact same PoC; `R` re-runs with different seed

#### 4. Bench / Judge Panel (`F4`)

Judge deliberations and verdict queue:

```
┌─ Bench · 5 Judges · 3 cases pending · 2 resolved ────────────────────────────────────────────────────────────┐
│                                                                                                                │
│  ┌── Active Deliberations ───────────────────────────────────────────────────────────────────────────────────┐ │
│  │ Case   Claim                          Judges Voted        Verdict                Confidence  Queries      │ │
│  │ #7     SQLi at db.py:89               ████████████████     BUG CONFIRMED · 9.2    HIGH        2/3         │ │
│  │        (A1)                           4-1 (Architecture ✗)                                                  │ │
│  │ #8     NPE at auth.py:42              ████████░░░░░░░░     DELIBERATING           —            1/3         │ │
│  │        (A2)                           2-2-1 (tied)                                                          │ │
│  │ #9     Race condition at lock.c:156   ░░░░░░░░░░░░░░░░     QUEUED                 —            0/3         │ │
│  │        (A3, A6)                       —                                                                    │ │
│  └────────────────────────────────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                                                │
│  ┌── Judge A (Security · GPT-4o) ─────────────────────┐  ┌── Judge B (Logic · GPT-4o) ───────────────────────┐ │
│  │ Case #7: SQLi at db.py:89                           │  │ Case #7: SQLi at db.py:89                          │ │
│  │ Verdict: ✓ CONFIRMED                                │  │ Verdict: ✓ CONFIRMED                               │ │
│  │ Severity: 9 · Evidence quality: 0.95                │  │ Severity: 8 · Evidence quality: 0.89                │ │
│  │ "Clear SQL injection. User input reaches raw        │  │ "Confirm injection vector. Severity 8 because      │ │
│  │  query via `f\"SELECT * FROM users WHERE            │  │  the query is read-only — no DROP/INSERT path.      │ │
│  │  name='{username}'\"`. Sandbox confirmed with        │  │  Still critical for data exfiltration."             │ │
│  │  `username=' OR 1=1 --`. No sanitizer found.        │  │ Citation: sandbox #8493, frames 3-7                 │ │
│  │  Citation: sandbox #8493, line 4                    │  │                                                     │ │
│  │ Query 1/3: "Verify if WAF present in infra"         │  │                                                     │ │
│  └─────────────────────────────────────────────────────┘  └────────────────────────────────────────────────────┘ │
│                                                                                                                │
│  ┌── Case #8: Tiebreaker in Progress ────────────────────────────────────────────────────────────────────────┐ │
│  │ Weighted re-vote: 2.7-2.3 (still tied)                                                                     │ │
│  │ Evidence-only re-vote: IN PROGRESS · sending sandbox #8472 trace to all 5 judges                           │ │
│  │ If still tied → Concurrency Judge breaks (bug is a race condition)                                         │ │
│  └────────────────────────────────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                                                │
│  o:Override  r:Re-vote  q:Query Swarm  v:View Evidence  j:JSON Export  Tab:Next Case  Esc:Back                  │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

#### 5. Evidence Graph (`F5`)

Visual representation of the evidence hypergraph. Since Textual cannot render full graph layouts, this view uses an ASCII-art force-directed layout refreshed every 2 seconds:

```
┌─ Evidence Graph · 247 nodes · 1,203 edges · Filter: Verified Only ───────────────────────────────────────────┐
│                                                                                                                │
│                              ┌──────────┐                                                                      │
│                              │ A2: NPE  │───[CONFIRMED]───▶ SB #8472 (PASS)                                   │
│                              │ auth:42  │                                                                      │
│                              └────┬─────┘                                                                      │
│                                   │ [PREDICTS]                                                                 │
│                                   ▼                                                                            │
│                        ┌──────────────────┐                                                                    │
│                        │ login() →        │                                                                    │
│                        │ validate_user()  │◀───[TAINTED_PATH]─── CPG: param #3                                 │
│                        └────────┬─────────┘                                                                    │
│                                 │ [CALLS]                                                                      │
│                                 ▼                                                                             │
│                        ┌──────────────────┐       ┌──────────┐                                                │
│                        │ user.name.trim() │──────▶│ NPE sink │                                                │
│                        └──────────────────┘       └──────────┘                                                │
│                                                                                                                │
│  ┌── Legend ─────────────────────────────────────────────────────────────────────────────────────────────────┐ │
│  │ ● Claim (Agent)    ■ Sandbox Run     ▲ CPG Node     ── Evidence Edge     ══ Contradiction Edge             │ │
│  │ 🟢 Verified    🟡 Investigating    🔴 Contradicted    ⚪ Unexplored                                        │ │
│  └────────────────────────────────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                                                │
│  f:Filter  c:Collapse  e:Expand  n:Next Node  p:Prev Node  s:Search Node  Tab:Focus Next  Esc:Back             │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Interactions:**
- `f` opens filter modal: toggle verified/unverified/contradicted/unexplored nodes
- `s` fuzzy-search for code location, agent ID, or claim keyword
- `c` collapses a subgraph (hides all descendants of selected node)
- `e` expands a collapsed subgraph
- `Enter` on any node opens its detail panel (claim text, evidence, related sandbox runs)
- `Tab` cycles focus between connected nodes
- Arrow keys navigate between nodes following edge direction

#### 6. Financial Control Panel (`F6`)

```
┌─ Finance · $23.14 spent · $26.86 remaining · Run rate: $0.17/min · Est. completion: 2.6h ────────────────────┐
│                                                                                                                │
│  ┌── Budget Status ───────────────────────────────────┐  ┌── Spending by Component (last hour) ───────────────┐ │
│  │                                                     │  │                                                    │ │
│  │  Token Budget:  ██████░░░░░░  2.1M / 5.0M (42%)   │  │  Agents (GPT-4o-mini):  ████████      $8.23 (48%)  │ │
│  │  Cost Budget:   ████████░░░░  $23.14 / $50 (46%)   │  │  Agents (Llama 3):      ██            $0.00 (0%)   │ │
│  │  Time Budget:   ████████████  36h / 40h (90%)      │  │  Agents (Gemini Flash): ████          $4.12 (24%)  │ │
│  │                                                     │  │  Judges (GPT-4o):       ███           $3.45 (20%)  │ │
│  │  ⚠ Time budget at 90% — early convergence pending  │  │  Summarization:         █             $1.10 (6%)   │ │
│  │                                                     │  │  Sandbox (infra):       ░             $0.24 (2%)   │ │
│  └─────────────────────────────────────────────────────┘  └────────────────────────────────────────────────────┘ │
│                                                                                                                │
│  ┌── Cost Per Bug Found ───────────────────────────────────────────┐  ┌── Provider Spend ────────────────────┐ │
│  │                                                                  │  │                                    │ │
│  │  Bug #1 (Severity 9 · SQLi):            $4.23                    │  │  OpenAI:       $14.50  (63%)        │ │
│  │  Bug #2 (Severity 7 · NPE):             $6.81                    │  │  Anthropic:    $5.20   (22%)        │ │
│  │  Bug #3 (Severity 8 · Buffer overflow): $3.90                    │  │  Google:       $3.44   (15%)        │ │
│  │  Bug #4 (Severity 6 · Race condition):  $8.20                    │  │  Local:        $0.00   (0%)         │ │
│  │                                                                  │  │                                    │ │
│  │  Avg cost/confirmed bug: $5.79                                   │  │  Total:        $23.14               │ │
│  │  Avg cost/severity-point: $0.77                                  │  └────────────────────────────────────┘ │
│  │  ROI vs QA engineer ($75/hr): 103x                              │                                        │
│  └──────────────────────────────────────────────────────────────────┘  ┌── Budget Override History ──────────┐ │
│                                                                        │ #1 14:12  Severity 9 finding ·      │ │
│  ┌── Spending Trend (hourly) ─────────────────────────────────────┐   │    Auto-approved $3.50 overrun      │ │
│  │ $5.00 ┤                                                         │   │    by Security Judge                │ │
│  │ $4.00 ┤     ██                                                  │   └────────────────────────────────────┘ │
│  │ $3.00 ┤  ██ ██ ██                                               │                                        │
│  │ $2.00 ┤  ██ ██ ██ ██ ██                                         │                                        │
│  │ $1.00 ┤  ██ ██ ██ ██ ██ ██                                      │                                        │
│  │       └──┴──┴──┴──┴──┴──┴──┴──┴──┴──┴──                         │                                        │
│  │        H1 H2 H3 H4 H5 H6 H7 H8 H9 H10 H11                       │                                        │
│  └──────────────────────────────────────────────────────────────────┘                                        │
│                                                                                                                │
│  o:Override Budget  p:Pause on Budget  r:Redistribute  s:Simulate Remaining  e:Export CSV  Esc:Back            │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

#### 7. Raw Logs (`F7`)

Full structured JSON log tail with filtering:

```
┌─ Logs · Filter: level>=INFO · component=agent,sandbox · 14,231 entries ──────────────────────────────────────┐
│                                                                                                                │
│  14:32:01.234  INFO   orchestrator  Batch 3 started · 12 agents dispatched · round=4 batch=3                   │
│  14:32:01.456  DEBUG  agent:A2      Hypothesis generated: "NPE at auth.py:42" · tokens=234                    │
│  14:32:02.891  INFO   sandbox       Execution #8472 queued · agent=A2 · PoC=poc_8472.py · size=47 lines       │
│  14:32:03.102  DEBUG  sandbox       Container 3f7a2b started · image=python3.11 · cpu=1 mem=512m               │
│  14:32:07.445  INFO   sandbox       Execution #8472 PASS · exit=1 · NPE at auth.py:42 · duration=4.3s         │
│  14:32:07.501  INFO   agent:A2      Sandbox result received · run=#8472 · verdict=PASS · confirmed            │
│  14:32:07.623  DEBUG  gateway       LLM call: agent=A2 · model=gpt-4o-mini · tokens_in=1245 tokens_out=89     │
│  14:32:08.012  WARN   orchestrator  Queue depth at 78% · throttling batch semaphore 12→8                      │
│  14:32:12.340  INFO   sandbox       Execution #8501 FAIL · claim refuted · agent=A6                           │
│  14:32:15.001  INFO   bench         Case #8 escalated · 2-2-1 split detected · triggering tiebreaker          │
│  14:32:18.567  INFO   agent:A4      Semantic loop detected · mean_cosine=0.94 · injecting pattern-break       │
│  14:32:20.123  WARN   orchestrator  Agent A4 marked LOOPING · token cap imposed (max_tokens=500)              │
│  14:32:25.890  INFO   bench         Judge query #2 · Security Judge asks: "Re-run PoC with input null+UTF8"   │
│  14:32:30.001  CRIT   security      SANDBOX_ESCAPE_ATTEMPT · agent=A13 · PoC contained nsenter pattern        │
│  14:32:30.002  CRIT   security      Agent A13 permanently banned · swarm paused for review                    │
│                                                                                                                │
│  j:JSON  f:Filter  c:Clear  s:Search  /:Regex  p:Pause  F:Follow  g:Top  G:Bottom  Esc:Back                   │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Interactive Controls

#### Global Keyboard Map

| Key | Action |
|-----|--------|
| `F1`–`F7` | Switch view |
| `Tab` | Cycle focus between panels/windows within a view |
| `Shift+Tab` | Reverse cycle focus |
| `↑` `↓` `←` `→` | Navigate within focused panel (rows, tree nodes, graph nodes) |
| `Enter` | Select / expand / drill-down on focused item |
| `Esc` | Go back / close modal / return to parent view |
| `q` | Quit (with confirmation dialog if swarm is running) |
| `Ctrl+C` | Force quit (SIGINT → orchestrator issues FREEZE before exit) |
| `/` | Search within current view (fuzzy, updates as you type) |
| `Space` | Toggle checkbox / expand collapsed section |
| `p` | Pause/resume TUI refresh (orchestrator keeps running) |
| `:` | Command mode (`:override case=7 severity=5`, `:eject agent=A4`, `:budget set cost=75`) |
| `Ctrl+L` | Force full redraw |
| `?` | Show help overlay with all keybindings for current view |

#### Command Mode (`:`)

A vim-style command bar at the bottom of the screen. Supports:

```
:override case=7 severity=5 reason="WAF present in prod"
:pause-swarm
:resume-swarm
:eject agent=A4
:reinstate agent=A7
:budget set cost=75
:budget extend token=1M
:judge-query case=8 judge=security "Re-run with different seed"
:provider swap gpt-4o→claude-3.5-sonnet
:mode headless         # Disable all interactive prompts
:export json           # Dump current state to /tmp/bugswarm-state.json
:export sarif          # Export findings as SARIF for GitHub Code Scanning
:theme dark            # Switch to dark theme
:theme light           # Switch to light theme
:theme matrix          # Classic green-on-black
```

### Human Override Workflow (Q48)

When a verdict requires human interaction in `--interactive` mode, a modal overlay appears:

```
┌─────────────────────────────────────────── OVERRIDE REQUIRED ──────────────────────────────────────────────────┐
│                                                                                                                │
│  Case #7 · SQL Injection in login() at auth.py:89                                                              │
│                                                                                                                │
│  ┌── Verdict Summary ────────────────────────────────────────────────────────────────────────────────────────┐ │
│  │ Bug Verified: YES · Severity: 8.2 · Evidence Quality: 0.92                                                │ │
│  │ Judge Votes: Security ✓ · Logic ✓ · Concurrency ✓ · Architecture ✗ · Data Integrity ✓ (4-1)              │ │
│  │ Architecture Judge dissent: "SQLi is real but sandbox used mocked DB. Prod DB uses parameterized queries   │ │
│  │   at the ORM layer. The injection vector exists only in a deprecated code path."                           │ │
│  │ Verified PoC: sandbox #8493 (PASS) · Independent re-execution: sandbox #8499 (PASS)                       │ │
│  └────────────────────────────────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                                                │
│  ┌── Evidence ───────────────────────────────────────────────────────────────────────────────────────────────┐ │
│  │ Code: auth.py:86-92                                                                                        │ │
│  │  86 │ def login(username, password):                                                                       │ │
│  │  87 │     query = f"SELECT * FROM users WHERE name='{username}'"                                          │ │
│  │  88 │     result = db.execute(query)                                                                       │ │
│  │  89 │     if result is None:                                                                               │ │
│  │  90 │         raise AuthenticationError("User not found")                                                 │ │
│  │                                                                                                            │ │
│  │ Sandbox trace (run #8493):                                                                                 │ │
│  │   Input: username="' OR 1=1 --"                                                                            │ │
│  │   Query executed: SELECT * FROM users WHERE name='' OR 1=1 --'                                            │ │
│  │   Result: ALL USERS RETURNED (authentication bypass confirmed)                                             │ │
│  └────────────────────────────────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                                                │
│  [Y] Accept Verdict    [N] Reject Verdict    [E] Edit Severity (current: 8.2)                                  │
│  [M] Request More Info (judge query)    [R] Reassign to Judge (select judge)                                   │
│  [V] View Full Debate History    [S] View Sandbox Receipt                                                      │
│                                                                                                                │
│  Your choice: _                                                                                                │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Override governance (QD48) is enforced here:
- If operator selects `[E]` to downgrade severity-8+ → a justification text input appears (cannot be empty)
- If downgrading severity-9+ → after entering justification, a second confirmation prompt appears if `--dual-auth` is configured
- Every override is logged with `$USER`, timestamp, before/after values, and justification
- Override log entry is appended to the immutable hash chain

### Responsive Layout

The TUI adapts to terminal size using Textual's responsive CSS:

| Terminal Size | Behavior |
|---------------|----------|
| ≥ 120×40 | Full 7-panel grid layout (Swarm Overview default) |
| 80–119 cols | Condensed layout: agent table reduced to 6 columns, cost breakdown collapsed into a single line, evidence feed truncated to 3 items |
| 60–79 cols | Single-panel mode: one view at a time, switchable via F1–F7. Agent table becomes a list (one agent per line with inline status). |
| < 60 cols | Minimal mode: status bar only + log tail. Full view available on demand. Warning displayed: "Terminal too narrow — press F1 for compact view" |
| < 24 rows | Warning: "Terminal too short — some panels may be truncated. Recommend ≥ 24 rows." |

### Theming

The TUI ships with 3 built-in themes and supports custom themes via `.tcss` files.

**Default (Dark Matrix):**
```tcss
Screen { background: #0a0a0a; color: #00ff00; }
Header { background: #1a1a1a; color: #00ff00; border-bottom: solid #00ff00; }
Button { background: #1a3a1a; color: #00ff00; }
Button:hover { background: #2a5a2a; }
.success { color: #00ff00; }
.warning { color: #ffff00; }
.error { color: #ff3333; }
.critical { color: #ff0000; background: #330000; }
```

**Light (Accessibility):**
```tcss
Screen { background: #ffffff; color: #1a1a1a; }
.success { color: #006600; }
.warning { color: #996600; }
.error { color: #cc0000; }
```

**Minimal (Monochrome):**
```tcss
Screen { background: #000000; color: #cccccc; }
.success { color: #ffffff; text-style: bold; }
.error { color: #ffffff; text-style: bold reverse; }
```

Custom theme: `bugswarm --theme ~/.config/bugswarm/theme.tcss`

### Export & Reporting

| Format | Command | Output |
|--------|---------|--------|
| JSON (full state) | `:export json` or `--json` flag | Complete orchestrator state as JSON to stdout/stderr |
| SARIF | `:export sarif` | Static Analysis Results Interchange Format for GitHub Code Scanning |
| HTML Report | `bugswarm report --html` | Standalone HTML with collapsible findings, evidence, and judge reasoning |
| CSV | `:export csv` (from Finance view) | Token/cost ledger as CSV |
| PNG Screenshot | `Ctrl+P` | Saves current TUI screen as PNG via `textual-screenshot` (requires `termtosvg` backend) |
| Stream log | `--watch` mode + `2> bugswarm.log` | Machine-readable JSON stream for external monitoring |

### Non-Interactive Modes

| Mode | Flag | Behavior |
|------|------|----------|
| Watch | `--watch` | TUI with no interactive prompts (auto-accepts 4-1+ verdicts, pauses on 3-2) |
| Headless | `--headless` | No TUI. Orchestrator runs as daemon. Progress via `bugswarm status` CLI command or REST API. |
| JSON stream | `--json` | No TUI. JSON lines to stdout. Compatible with `jq`, Grafana Loki, Datadog. |
| Quiet | `-q` | No TUI. Only final report to stdout on completion. |

### TUI Dependencies (Python)

```
textual >= 1.0.0
textual-dev >= 1.3.0 (dev tools: screenshot, CSS live reload)
msgpack >= 1.0.0 (binary protocol for orchestrator <-> TUI socket)
pyperclip >= 1.8.0 (copy to clipboard from TUI)
```

### TUI-to-Orchestrator Protocol

The TUI is a separate process from the orchestrator. Communication is over a Unix domain socket using MessagePack-encoded frames:

```python
# Frame types (orchestrator → TUI)
{
    "type": "state_delta",        # Partial state update
    "type": "override_required",  # Human override modal trigger
    "type": "alert",              # High-priority notification
    "type": "log_entry",          # Single log line
}

# Frame types (TUI → orchestrator)
{
    "type": "command",            # :command execution
    "type": "override_response",  # User's choice in override modal
    "type": "subscribe",          # Request state delta stream
    "type": "unsubscribe",
}
```

State deltas are compact: only changed fields since last push. Default push interval is 500ms, but the orchestrator can push immediately on critical events (budget exhausted, sandbox escape, judge deadlock).

---

## 13. Build & Deployment

### Build Pipeline
```
make build          # Compile Rust binaries (orchestrator, sandbox-daemon, cpg-builder)
make build-python   # Package Python wheel (pip install dist/bugswarm-*.whl)
make test           # Unit tests (pytest + cargo test)
make test-integration  # Integration tests (real LLM calls, Docker sandbox)
make lint           # ruff (Python), clippy (Rust)
make docker         # Build sandbox base images for each language
make docs           # Generate CLI reference from Typer docstrings
```

### Deployment Models

**Single Machine (CLI):**
```bash
pip install bugswarm
bugswarm run ./my-repo --budget 50 --time 40h
```

**Docker Compose (Team Server):**
```yaml
services:
  orchestrator:
    image: bugswarm/orchestrator:latest
  sandbox-daemon:
    image: bugswarm/sandbox:latest
    privileged: true  # Required for cgroups + seccomp
  postgres:
    image: postgres:16
  chromadb:
    image: chromadb/chroma:latest
  neo4j:  # or kuzu for embedded
    image: neo4j:5-enterprise
  redis:  # optional
    image: redis:7-alpine
  grafana:
    image: grafana/grafana:latest
```

**Kubernetes (Enterprise Multi-Tenant):**
```yaml
# Key considerations:
# - orchestrator: Deployment with 1 replica (single-writer guarantee)
# - sandbox-daemon: DaemonSet (one per GPU node), privileged securityContext
# - postgres: StatefulSet with 3 replicas (1 primary, 2 standby)
# - prometheus: Operator with ServiceMonitor for bugswarm metrics
# - GPU nodes: NVIDIA device plugin + MIG configuration
# - NetworkPolicy: sandbox pods → no egress, no ingress (isolated)
# - ResourceQuota: Limit total concurrent swarms per namespace
```

### CI/CD Integration
```bash
# GitHub Actions / GitLab CI example
bugswarm ci ./src --diff origin/main...HEAD --severity-min 7 --timeout 30m
# Returns exit code 0 if no bugs found, 1 if severity-7+ bugs found
# Output: SARIF report for GitHub Code Scanning integration
```

---

## 14. Compliance & Governance

### Data Retention
| Data | Retention | Justification |
|------|-----------|---------------|
| Sandbox outputs (stack traces, PoCs) | Duration of swarm + 7 days | Debugging, false-negative analysis |
| Agent debate logs | 30 days | Retrospective analysis (QD45) |
| Judge verdicts + overrides | 1 year + current | Audit trail (QD48) |
| PII redaction logs | Deleted on session termination | Privacy |
| Sandbox execution receipts | Duration of swarm + 30 days | Bug verification trail |
| Token/cost ledger | 1 year | Billing, cost analysis |
| Calibration data | Indefinite (anonymized) | Model improvement |
| WAL files | 7 days (S3 replication) | Crash recovery |

### Encrypted at Rest
- PII redaction logs → AES-256-GCM, key in memory only
- `.bugswarm_secrets.yaml` → never written to disk in plaintext (read from env vars or secret manager)
- API keys in config → file permissions `0600`, or sourced from `$OPENAI_API_KEY` / vault

### Licensing Considerations
- Bug Swarm itself: Apache 2.0 or BUSL (Business Source License) for enterprise features
- tree-sitter grammars: MIT (distributed with attribution)
- Docker images: Respect base image licenses. Sandbox images built from `ubuntu:22.04` (Canonical IP policy compliant)
- LLM model outputs: Agents do not ingest or reproduce GPL code verbatim in outputs. Patches are structured `diff` objects, not derivative works (Q52).
- Code Property Graph: Derivative representation for analysis purposes; covered by fair use / analysis exception in most jurisdictions. Not redistributed.

---

## Summary Matrix

| System Component | Language | Primary Dependency | Deployment |
|-----------------|----------|-------------------|------------|
| Orchestrator | Rust | tokio, bollard, nix | Single binary |
| Sandbox Daemon | Rust | bollard, seccompiler, nix | Single binary (privileged) |
| CPG Builder | Rust | tree-sitter, neo4rs/kuzu | Single binary |
| Agent Runtime | Python | asyncio, httpx, openai/anthropic/google | Python package |
| CLI + TUI | Python | typer, rich, textual, msgpack | Python package |
| TUI (Textual) | Python | textual >= 1.0, textual-dev | Separate process (Unix socket to orchestrator) |
| Judge Panel | Python | Same as Agent Runtime | Python package (stateless, API calls only) |
| PII Scanner | Python | regex, cryptography | Embedded in Agent Runtime |
| Prompt Injection Scanner | Python | transformers (deberta-v3) | Embedded, local model |
| State DB | PostgreSQL 16 | — | External service |
| Vector DB | ChromaDB / Milvus | — | External service |
| Graph DB | Neo4j / Kuzu | — | External / Embedded |
| Message Queue | Redis 7 (optional) | — | External service |
| Object Storage | S3 / MinIO | — | External service |
| LLM Providers | External APIs | OpenAI, Anthropic, Google | Cloud |
| Local LLM | Ollama / llama.cpp | — | Co-located or GPU node |
| Minitia CLI | Python | typer, rich | Python package (installs engines) |
| Minitia TUI | Python | textual >= 1.0 | Separate process (dashboard for all engines) |

---

## 15. Minitia Integration

Bug Swarm is an engine under Minitia, not a feature inside it. Minitia is the meta-tool users install; Bug Swarm is one engine it can run. This section defines the integration contract.

### Architecture

```
┌──────────────────────────────────────────────────┐
│                    minitia                       │
│  ┌────────────┐ ┌──────────┐ ┌───────────────┐  │
│  │ bugswarm   │ │  audit   │ │  perf / etc   │  │
│  │  engine    │ │  engine  │ │   engines     │  │
│  └────────────┘ └──────────┘ └───────────────┘  │
│       ▲                ▲              ▲          │
│       │                │              │          │
│  ~/.minitia/engines/bugswarm-v2.1.0-x86_64      │
│  ~/.minitia/engines/audit-v1.4.0-x86_64         │
│  ~/.minitia/engines/perf-v0.9.1-x86_64          │
└──────────────────────────────────────────────────┘
```

Each engine is a standalone binary. Minitia downloads, verifies, and calls them. No code coupling — just a process contract.

### Engine Contract

Every engine under Minitia must implement four verbs. These are subcommands on the engine's CLI:

```bash
<engine> run <target> [--flags]     # Execute against a target, return exit code 0 on clean, 1 if issues found
<engine> status [--json]            # Print live progress as JSON to stdout
<engine> stop                       # Graceful shutdown (SIGTERM, with FREEZE before exit)
<engine> report <run-id> [--format] # Export results (json, sarif, html, csv)
```

Minitia calls these as subprocesses. The `--json` flag on `run` suppresses the engine's native TUI and outputs machine-readable JSON lines to stdout, which Minitia consumes for its dashboard.

### Bug Swarm's Implementation of the Contract

```bash
# Run (with TUI):
bugswarm run ./repo --budget 50 --time 40h
# → Launches full Textual TUI

# Run (for Minitia, no TUI):
bugswarm run ./repo --budget 50 --json
# → Outputs JSON lines to stdout:
#    {"type":"state","round":4,"agents_active":12,"bugs_found":3,"cost":23.14,...}
#    {"type":"finding","severity":9,"location":"auth.py:89","verdict":"confirmed",...}
#    {"type":"budget_warning","budget":"cost","used_pct":80,...}
#    {"type":"complete","exit_code":0,"bugs_total":5,...}

# Status:
bugswarm status --json
# → {"running":true,"run_id":"acme-webapp-001","elapsed":"02:14:38","round":4,"agents":12,...}

# Stop:
bugswarm stop
# → Orchestrator issues FREEZE → checkpoints all agents → exits. SIGTERM to process.

# Report:
bugswarm report acme-webapp-001 --format sarif
# → Writes SARIF to stdout for GitHub Code Scanning integration.
```

### Installation Flow

```bash
# User installs Minitia once:
brew install minitia
# or: curl -fsSL https://get.minitia.ai | bash

# Minitia installs engines on demand:
minitia install bugswarm
# Under the hood:
# 1. Fetch https://registry.minitia.ai/engines.json
# 2. Find "bugswarm" entry → download URL for current OS/arch
# 3. Download binary to ~/.minitia/engines/bugswarm-v2.1.0-x86_64-unknown-linux-gnu
# 4. Verify SHA256 checksum against registry
# 5. chmod +x
# 6. Symlink ~/.minitia/engines/bugswarm → versioned binary

minitia run bugswarm ./repo --budget 50
# Under the hood:
# 1. Resolve ~/.minitia/engines/bugswarm → versioned binary
# 2. Execute: ~/.minitia/engines/bugswarm run ./repo --budget 50 --json
# 3. Pipe stdout JSON stream into Minitia's dashboard
```

### Engine Registry

A single JSON file hosted at `https://registry.minitia.ai/engines.json`:

```json
{
  "version": 1,
  "engines": {
    "bugswarm": {
      "name": "Bug Swarm",
      "description": "Multi-agent bug detection via swarm intelligence",
      "latest": "2.1.0",
      "homepage": "https://bugswarm.ai",
      "repository": "https://github.com/minitia/bugswarm",
      "license": "Apache-2.0",
      "platforms": {
        "x86_64-unknown-linux-gnu": {
          "url": "https://releases.bugswarm.ai/v2.1.0/bugswarm-v2.1.0-x86_64-unknown-linux-gnu.tar.gz",
          "sha256": "abc123...",
          "size_bytes": 48234496
        },
        "aarch64-unknown-linux-gnu": {
          "url": "https://releases.bugswarm.ai/v2.1.0/bugswarm-v2.1.0-aarch64-unknown-linux-gnu.tar.gz",
          "sha256": "def456...",
          "size_bytes": 45128704
        },
        "x86_64-apple-darwin": {
          "url": "https://releases.bugswarm.ai/v2.1.0/bugswarm-v2.1.0-x86_64-apple-darwin.tar.gz",
          "sha256": "ghi789...",
          "size_bytes": 50123456
        }
      },
      "minitia_version": ">=1.0.0",
      "requires": ["docker", "postgresql-client-16"],
      "optional": ["nvidia-container-toolkit", "rr"]
    }
  }
}
```

### Minitia's TUI (Multi-Engine Dashboard)

```
┌─ Minitia · acme/webapp · 3 engines ───────────────────────────────────────────────────────────────────────────┐
│                                                                                                                │
│  ┌── Bug Swarm ───────────────────────────────┐  ┌── Security Audit ─────────────────────────────────────────┐ │
│  │ Status: RUNNING · 02:14:38                  │  │ Status: COMPLETED · 345 checks                            │ │
│  │ Round 4/15 · 12 agents active               │  │ Passed: 338 · Failed: 7 · Critical: 2                     │ │
│  │ Bugs found: 3 (1 critical, 2 high)          │  │ Report: audit-report-2026-05-11.json                      │ │
│  │ Budget: $23.14 / $50 · Est. 2.1h remaining  │  │                                                           │ │
│  │                                              │  │ [R]erun  [V]iew Report  [F]ix All                         │ │
│  │ [D]etach TUI  [S]top  [V]iew Findings       │  └───────────────────────────────────────────────────────────┘ │
│  └──────────────────────────────────────────────┘                                                               │
│                                                                                                                │
│  ┌── Perf Profiler ────────────────────────────┐  ┌── System ────────────────────────────────────────────────┐ │
│  │ Status: RUNNING · benchmark 3/12             │  │ CPU: 34% · RAM: 42% · GPU: 0% (idle)                    │ │
│  │ Current: auth microservice p99 latency       │  │ Disk: 0.5 TB free · Net: 12 Mbps out                     │ │
│  │ Regression detected: +15% vs baseline       │  │ Engines installed: bugswarm, audit, perf                  │ │
│  │                                              │  │                                                           │ │
│  │ [D]etach TUI  [S]top  [V]iew Flamegraph     │  │ [i]nstall engine  [u]pdate all  [U]ninstall              │ │
│  └──────────────────────────────────────────────┘  └───────────────────────────────────────────────────────────┘ │
│                                                                                                                │
│  F1 Dashboard  F2 Bug Swarm  F3 Audit  F4 Perf  i:Install  u:Update  q:Quit                                    │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Key behaviors:
- `F2`–`F4` switch the right panel to show an engine's full detail view inline within Minitia
- `[D]etach TUI` launches the engine's own native Textual TUI in a new terminal window (`tmux split-window` or `$TERMINAL -e`)
- Minitia shows a summarized status card; the engine's own TUI shows everything
- `i` opens the engine registry browser (searchable list of all available engines)
- `u` checks for updates across all installed engines

### Filesystem Layout

```
~/.minitia/
├── engines/
│   ├── bugswarm         → bugswarm-v2.1.0-x86_64-unknown-linux-gnu   (symlink)
│   ├── bugswarm-v2.1.0-x86_64-unknown-linux-gnu                       (binary)
│   ├── bugswarm-v1.9.0-x86_64-unknown-linux-gnu                       (previous, kept for rollback)
│   ├── audit             → audit-v1.4.0-x86_64-unknown-linux-gnu
│   └── perf              → perf-v0.9.1-x86_64-unknown-linux-gnu
├── registry.yaml          # Cached copy of registry.minitia.ai/engines.json
├── config.yaml            # Global config (API keys, default budgets, theme)
├── runs/                  # Run history across all engines
│   └── 2026-05-11/
│       ├── bugswarm-acme-webapp-001/
│       │   ├── state.json
│       │   └── findings.sarif
│       └── audit-acme-webapp-002/
│           └── report.json
└── credentials.enc        # Encrypted API keys (AES-256-GCM, key in OS keyring)
```

### Package Manager Shims

Bug Swarm is distributed as ONE self-contained binary per platform. Thin shim packages in each registry download and install it:

| Registry | Package Name | Shim Contents |
|----------|-------------|---------------|
| Homebrew | `minitia/tap/bugswarm` | Ruby formula: downloads binary, verifies SHA256, symlinks to `/usr/local/bin/bugswarm` |
| PyPI | `bugswarm-cli` | `setup.py` with `entry_points` that calls a 20-line Python script: detect OS/arch, download binary, place in `~/.local/bin/bugswarm` |
| npm | `bugswarm-cli` | `package.json` with `bin` pointing to a JS script that does the same |
| crates.io | `bugswarm` | Cargo.toml with a `[package.metadata.binstall]` config pointing to the prebuilt binary |
| APT | `bugswarm` | `.deb` with the actual binary + man page + systemd unit for the sandbox daemon |
| AUR | `bugswarm-bin` | PKGBUILD that downloads the binary |

All shims point to the same binary at `https://releases.bugswarm.ai/v2.1.0/`. One build, N delivery channels.

Minitia itself is distributed the same way — one binary, N shims. `minitia install bugswarm` is the primary path. Standalone `brew install bugswarm` is the secondary path for CI/CD and users who only need one engine.
