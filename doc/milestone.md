# BugSwarm Implementation Milestones — CLI-First Architecture

**Generated**: 2026-05-29
**Architecture**: CLI-first, no SaaS/web platform, protocol-based adapters,
max-temp-always, token-budget-gated, protocol-based adapters.

**Removed from original plan**: 100 milestones eliminated — web platform
(FastAPI/React, GitHub App, VS Code, Slack/Discord), fine-tuning pipeline,
exploit generation, Terraform/K8s, GitHub Marketplace, config migration tools.

**Source documents retained**:
- `production_audit.md`, `phase30b_high_fix_plan.md`,
  `phase30c_critical_fix_plan.md`, `ARIA.md`,
  `phase15_self_configuring_swarm.md`, `invincible_phases.md`,
  `bugswarm_tool_superpowers.md`, `bugswarm_mythos_gap_analysis.md`,
  `chain_universal_expansion.md`, `q1_all_bug_coverage.md`,
  `q2_tool_calling_architecture.md`, `q4_100_languages.md`,
  `q7_multi_provider_pluggable.md`, `phase_mini_model_gateway.md`,
  plus all phase docs (phase16–phase30) and fix-plan docs.

---

## Source Files Key

| Prefix | Directory |
|--------|-----------|
| `*` (no prefix) | bugswarm-sandbox/ |
| `cpg/*` | bugswarm-cpg/ |
| `ev/*` | bugswarm-evidence/ |
| `sym/*` | bugswarm-symbolic/ |
| `agent/*` | bugswarm-agent/src/agent/ |
| `gateway/*` | bugswarm-gateway/src/gateway/ |
| `swarm/*` | bugswarm-swarm/src/swarm/ |
| `mini/*` | bugswarm_mini/ |
| `tests/*` | tests/ |

---

## Already Complete (5 milestones — not counted in batches)

### M001: Shell injection via heredoc delimiter (H11) ✅ DONE
- **What**: Replace heredoc-based PoC execution with bind-mount; add
  execution_id sanitization (alphanumeric + dash + underscore only)
- **Files**: `src/container.rs:274-276`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 0)
- **Lines**: ~80
- **Depends on**: none
- **Tests**: `tests/fuzz/test_shell_injection.py`,
  `tests/integration/test_sandbox_execution.py::test_bind_mount_heredoc_free`
- **Blocks**: M003

### M002: Path traversal in agent read_file (H12) ✅ DONE
- **What**: 5-layer defense — empty path / null byte / Windows drive letter /
  absolute path / segment-based `..` detection + resolve prefix check +
  double-resolve symlink check
- **Files**: `agent/core.py:240`, `agent/cli/wiring.py`, `agent/tools.py`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 0)
- **Lines**: ~80
- **Depends on**: none
- **Tests**: `tests/security/test_security_path_traversal.py` (118 cases)
- **Blocks**: M003, R001

### M093: Container removal errors propagated (L16) ✅ DONE
- **What**: Container remove failures propagated instead of silently swallowed
- **Files**: `src/container.rs`
- **Plan**: `production_audit.md` (L16), Phase 30a
- **Lines**: ~10
- **Depends on**: none
- **Tests**: Verified in Phase 30a
- **Blocks**: none

---

## Batch 1: Flaw Fixes (12 milestones)

### M003: Tool capability gating — validate all agent tool calls (H17) ✅
- **What**: Security flaw — LLM can call any tool with any arguments. Add
  validation middleware: path validation, arg type checks, rate limiting,
  sandbox boundary enforcement
- **Files**: `agent/tools.py`
- **Lines**: ~300 (10 new validators + arg type check + permission gating)
- **Depends on**: M001, M002
- **Tests**: `tests/unit/test_tool_gating.py` — 99 tests including 10 new
  validator suites + arg type validation, all passing
- **Blocks**: M029-M035
- **Details**: Added 10 missing validators (query_cpg/list_dir/trace_dependency/
  grep/glob/write_file/edit_file/web_fetch/todo_write/kill_shell), arg type
  validation against JSON Schema, required_permissions gate in execute()
  pipeline. 23 total tools validated.

### M022: Graceful shutdown — tokio::select! on SIGTERM (H4) ✅
- **What**: Daemon loop has no graceful shutdown — SIGTERM kills process,
  leaving Docker containers dangling. Add `tokio::select!` accepting SIGTERM;
  5s drain timeout; cleanup all active containers
- **Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-cpg/src/daemon.rs`,
  `bugswarm-evidence/src/daemon.rs`, `bugswarm-sandbox/src/container.rs`
- **Lines**: ~160
- **Depends on**: none
- **Blocks**: M008
- **Details**: All 3 daemons use `JoinSet` to track spawned connection handlers.
  On SIGTERM: stop accepting → wait for active connections (5s timeout) →
  cleanup. Sandbox drains all `bugswarm-sandbox-*` Docker containers via
  bollard API. Evidence daemon: removed dual `ctrl_c()` (was calling exit(0)
  in a spawned task), consolidated shutdown with graph save before drain.

### M025: Remove all .unwrap() from production paths (H9) ✅
- **What**: 15+ non-test `unwrap()`/`expect()` calls cause daemon panics on
  routine errors (e.g., `serde_json::to_string_pretty(&stats).unwrap()` in
  main.rs). Replace with proper error handling; add clippy lint denial
- **Files**: All crate .rs files (audit every `.unwrap()` and `.expect()`)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2)
- **Lines**: ~100
- **Depends on**: M017
- **Tests**: `cargo clippy --workspace -- -D clippy::unwrap_used` passes
- **Blocks**: none

### M027: Evidence graph persistence — save/load to disk (C12) ✅
- **What**: Evidence graph is entirely in-memory. Process restart = total data
  loss. Add SQLite save/load with WAL journaling, migration framework, autosave
  every N seconds
- **Files**: `ev/src/graph.rs`, `ev/src/daemon.rs`
- **Plan**: `production_audit.md` (C12)
- **Lines**: ~150
- **Depends on**: M015 (unified config for persistence path)
- **Tests**: `tests/integration/test_evidence_persistence.py` — restart daemon,
  graph content preserved
- **Blocks**: M081, M082

### M039: diff_execute — fix client→daemon protocol mismatch (C10) ✅
- **What**: Python client sends `--output-a`/`--output-b` but daemon handler
  reads `input`/`reference` keys. Broken CLI command that doesn't exist.
  Harmonize to `input`/`reference`
- **Files**: `src/daemon.rs`, `agent/cli/diff.py`
- **Plan**: `production_audit.md` (C10)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: `tests/integration/test_diff_execute.py` — client sends request,
  daemon processes and returns correct diff
- **Blocks**: none

### M045: exec_causal_intervention — proper shell escaping (H32) ✅
- **What**: Replaces inadequate string escaping (only `'` → `\'`) with proper
  shell quoting using shlex.quote or temp file approach
- **Files**: `src/container.rs`
- **Plan**: `production_audit.md` (H32)
- **Lines**: ~30
- **Depends on**: M001
- **Tests**: `tests/unit/test_causal_intervention.py::test_proper_shell_escaping`
- **Blocks**: none

### M046: 16 unsafe block safety documentation (H15) ✅
- **What**: 16 undocumented unsafe blocks in danger_map.rs. Every unsafe block
  needs `// SAFETY:` comment documenting preconditions and invariants; add
  debug_assert! guards; CI enforces
- **Files**: `src/danger_map.rs` (16 unsafe blocks)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~60
- **Depends on**: none
- **Tests**: CI script `ci/check_safety_comments.sh` — every unsafe block has
  safety comment
- **Blocks**: none

### M047: shm_map error type — String → anyhow::Error (H41) ✅
- **What**: `shm_map` returns `Result<*mut u8, String>` — inconsistent and
  loses error context. Change to `Result<*mut u8, anyhow::Error>`; update
  callers
- **Files**: `src/danger_map.rs`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: `tests/unit/test_danger_map.rs::test_shm_map_error_is_anyhow`
- **Blocks**: none

### M072: API keys — secrets manager with env+file providers (H13) ✅
- **What**: API keys loaded from env vars only with no secrets management.
  Add `BGSWARM_SECRETS_PROVIDER` env var with `env` and `file` (Docker secrets)
  backends; constant-time comparison
- **Files**: `gateway/types.py`, `mini/gateway/protocol.py` (ENHANCE)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2)
- **Lines**: ~60
- **Depends on**: M016 (unified config)
- **Tests**: `tests/unit/test_secrets.py` — env source, file source, missing
  file raises
- **Blocks**: none

### M073: Seccomp profile — explicit deny for fork/clone (H16) ✅
- **What**: `fork`, `clone`, `clone3`, `vfork`, `execveat`, `kexec_load` not
  explicitly listed in seccomp profile. Add explicit DENY block; verify
  `prlimit64`; add documentation comments
- **Files**: `seccomp/default.json`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2), CRIT-PRE4
- **Lines**: ~40
- **Depends on**: none
- **Tests**: `tests/integration/test_seccomp.py::test_fork_blocked`,
  `test_python_startup_under_seccomp`
- **Blocks**: none

### M087: core.py Exception handling — specific error types (M27) ✅ DONE
- **What**: Exception hierarchy in `agent/exceptions.py` with 25+ typed
  exception classes across 4 categories (Infrastructure, Security, Config).
  All 9 `except Exception` blocks in core.py replaced with specific types.
  Security events (PII, escape, injection, path traversal) logged at WARN.
  Infrastructure errors logged at ERROR. All `print()` replaced with structlog.
- **Files**: `agent/exceptions.py` (new), `agent/core.py`
- **Plan**: `production_audit.md` (M27)
- **Lines**: ~280 (new module + refactor)
- **Depends on**: none
- **Tests**: Security errors logged at WARN, infra errors at ERROR
- **Blocks**: none

### M092: Token counting — replace len//4 fallback (L13) ✅ DONE
- **What**: Centralized `gateway/tokenizer.py` with `count_tokens()`,
  `count_message_tokens()`, `count_tool_output()` — tiktoken-backed with
  LRU cache, model→encoding mapping for 15+ models, graceful heuristic
  fallback. All 6 `len//4` call sites replaced (agent/loop.py,
  swarm/{compression,relevance,compressor}.py). All 3 provider adapters
  (base, openai, anthropic) delegate to the centralized module.
  tiktoken dependency added to agent and swarm pyproject.toml.
- **Files**: `gateway/tokenizer.py` (new), `agent/loop.py`,
  `swarm/compression.py`, `swarm/relevance.py`, `swarm/compressor.py`,
  `gateway/providers/base.py`, `gateway/providers/openai.py`,
  `gateway/providers/anthropic.py`
- **Plan**: `production_audit.md` (L13)
- **Lines**: ~140 (new module + 8 call-site updates)
- **Depends on**: none
- **Tests**: Token count within 5% of actual tiktoken count
- **Blocks**: none

---

## Batch 2: Incomplete Features (9 milestones)

### M028: Fuzzer crash artifact collection — wire results back (C8) ✅ DONE
- **What**: Crash collection loop rewritten to use `FuzzController.record_crash()`
  for proper dedup + stats tracking. Each unique crash is converted to an
  `ExecutionReceipt` via `FuzzCrash::to_execution_receipt()` with
  `finding_source: "fuzzer"`. Shared `Arc<Mutex<HashMap<CampaignId, FuzzCampaignState>>>`
  on `ContainerManager` enables thread-safe crash collection. New daemon methods
  `fuzz_crashes` and `fuzz_campaigns` expose collected receipts for evidence
  graph ingestion. Two unused functions (`classify_crash`, `signal_name`) removed.
  73 existing tests pass; 0 clippy warnings.
- **Files**: `src/fuzzer.rs` (+140 lines), `src/container.rs` (+80/−70 lines),
  `src/daemon.rs` (+40 lines)
- **Plan**: `production_audit.md` (C8)
- **Lines**: ~330
- **Depends on**: M001
- **Tests**: `tests/integration/test_fuzzer_results.py` — fuzz campaign →
  crashes appear in evidence graph (73 existing unit tests pass)
- **Blocks**: M036

### M036: mine_invariants — real sandbox execution (C7 / Stub 4) ❌
- **What**: `mine_invariants` handler generates fabricated traces with hardcoded
  values. Replace with real sandbox execution: generate PoC, execute via
  manager.execute(), parse ExecutionReceipt, build real ExecutionTrace
- **Files**: `src/invariant.rs`, `src/daemon.rs`
- **Plan**: `production_audit.md` (C7), CRIT-PRE5
- **Lines**: ~200
- **Depends on**: M001, M028
- **Tests**: `tests/integration/test_invariant_mining.py::test_real_execution`
  — real traces with actual output values
- **Blocks**: M038

### M038: solve_reachability — wire Z3 SMT solver (C11 / Stub 6) ❌
- **What**: `solve_reachability` uses heuristic solver (`val+1` on ints).
  Z3 from bugswarm-symbolic crate never called. Wire real Z3 calls with
  bitvector, integer, string, array theories
- **Files**: `sym/src/engine.rs`, `src/daemon.rs`
- **Plan**: `production_audit.md` (C11)
- **Lines**: ~100
- **Depends on**: M006 (symbolic Dockerfile with Z3), M036
- **Tests**: `tests/unit/test_symbolic_solver.py::test_z3_solves_reachability`
  — constraint solving produces real values
- **Blocks**: M067

### M067: Chain detector — universal bug category expansion ❌
- **What**: EffectType/PreconditionType enums have only 7 variants each (all
  security-focused). Expand to 15 variants: add Crash, ResourceExhaustion,
  DataCorruption, Deadlock, PerformanceDegradation, Timeout, Incompatibility,
  Regression, Inconsistency, Bloat. Add ChainType enum; category-aware severity
- **Files**: `ev/src/chain.rs`
- **Plan**: `chain_universal_expansion.md`
- **Lines**: ~200
- **Depends on**: M038 (real reachability for chain construction)
- **Tests**: 10+ new cross-category chain tests; all existing tests still pass
- **Blocks**: none

### M068: Taint model — Python full source/sink/sanitizer definitions ❌
- **What**: No Python taint model. Define 30+ sources (socket, urllib,
  requests, flask, django, fastapi, pickle, yaml, json, subprocess,
  multiprocessing, redis, kafka, boto3, websocket), 20+ sinks, 15+ sanitizers,
  propagation rules
- **Files**: `cpg/src/parsers/python/taint.rs` (NEW)
- **Plan**: `q4_100_languages.md` (Section 3.4.1)
- **Lines**: ~250
- **Depends on**: M064 (Python CPG parser)
- **Tests**: Taint paths detected in known-vulnerable Python codebases
- **Blocks**: R001

### M069: Taint model — JavaScript full source/sink/sanitizer definitions ❌
- **What**: No JavaScript taint model. Define 30+ browser/Node.js sources,
  20+ sinks, propagation rules; Express/React/Vue framework support
- **Files**: `cpg/src/parsers/javascript/taint.rs` (NEW)
- **Plan**: `q4_100_languages.md` (Section 3.4.2)
- **Lines**: ~250
- **Depends on**: M064 (JS/TS CPG parser)
- **Tests**: Taint paths detected in known-vulnerable JS codebases
- **Blocks**: R001

### M081: NodeKind::Agent, CodeLocation creation in daemon handlers (M14) ❌
- **What**: Evidence graph node types `NodeKind::Agent` and `CodeLocation` are
  defined in the graph schema but never created by daemon handlers. Node
  creation exists only in test code
- **Files**: `ev/src/daemon.rs`, `ev/src/graph.rs`
- **Plan**: `production_audit.md` (M14)
- **Lines**: ~60
- **Depends on**: M027 (evidence graph persistence)
- **Tests**: Agent nodes appear in evidence graph after agent scan
- **Blocks**: R002

### M082: EdgeKind::Authored, Enables, ConditionEquivalent usage (M15) ❌
- **What**: Edge types `Authored`, `Enables`, `ConditionEquivalent` are defined
  but never created. Wire into chain detection, fix impact prediction, and
  agent trace tracking
- **Files**: `ev/src/graph.rs`
- **Plan**: `production_audit.md` (M15)
- **Lines**: ~50
- **Depends on**: M027
- **Tests**: Edges created and traversable in evidence graph queries
- **Blocks**: M067

### M160: Cargo.lock consolidation — workspace lock (M17) 🔶 PARTIAL
- **What**: Per-crate Cargo.lock files may diverge with different transitive
  dep versions. Consolidate into single workspace-level lock file
- **Files**: Root `Cargo.toml`, all crate `Cargo.toml`
- **Plan**: `production_audit.md` (M17)
- **Lines**: ~10
- **Depends on**: none
- **Tests**: `cargo build --workspace` produces single Cargo.lock
- **Blocks**: none

---

## Batch 3: Testing for Hardcoded Output (2 milestones)

### M018: CI/CD pipeline — GitHub Actions (H5) ✅
- **What**: No automated test infrastructure. Create `.github/workflows/ci.yml`
  with build, test, clippy, cargo-audit, docker-build. Every PR runs the full
  test suite to catch hardcoded/stub outputs
- **Files**: `.github/workflows/ci.yml` (NEW)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~80
- **Depends on**: M004, M005, M006, M007, M008
- **Tests**: CI passes on every PR; badge in README shows green
- **Blocks**: none

### M159: Cross-crate Rust integration tests (L20) ❌
- **What**: No tests exercise multiple crates together. Build sandbox→CPG→
  evidence roundtrip tests; full scan workflow; fuzzer→evidence collection.
  These tests verify that real data flows between components (not stubs)
- **Files**: `tests/integration_cross/` (NEW)
- **Plan**: `production_audit.md` (L20)
- **Lines**: ~300
- **Depends on**: M008 (docker-compose for daemon orchestration)
- **Tests**: This IS the test suite
- **Blocks**: none

---

## Batch 4: New Features (18 milestones)

### M004-M007: Dockerfiles for all 4 Rust daemons (H1, H2) ✅
- **What**: Create/enhance 4 Dockerfiles: CPG daemon (debian:bookworm-slim),
  Evidence daemon (debian:bookworm-slim), Symbolic daemon (debian:bookworm-slim
  + libz3), sandbox-python-asan (WORKDIR + LABEL metadata for execution base)
- **Files**: `cpg/Dockerfile` (NEW), `ev/Dockerfile` (NEW),
  `sym/Dockerfile` (ENHANCE), `docker/sandbox-python-asan.Dockerfile` (ENHANCE)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1), CRIT-PRE2, CRIT-PRE3
- **Lines**: ~65 total
- **Depends on**: none
- **Tests**: `docker build` succeeds for all 4; `docker inspect` shows correct
  Cmd/Entrypoint for each
- **Blocks**: M008

### M010: HTTP health endpoint on CPG daemon (C3a) ✅
- **What**: Add axum-based HTTP health check endpoint on CPG daemon; share
  axum server with metrics
- **Files**: `cpg/src/daemon.rs`, `cpg/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 2)
- **Lines**: ~80
- **Depends on**: M015 (unified config for port)
- **Tests**: `curl http://localhost:8080/health` returns 200
- **Blocks**: M008

### M011: HTTP health endpoint on Evidence daemon (C3b) ✅
- **What**: Add axum-based HTTP health check endpoint on Evidence daemon
- **Files**: `ev/src/daemon.rs`, `ev/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 2)
- **Lines**: ~80
- **Depends on**: M015 (unified config for port)
- **Tests**: `curl http://localhost:8081/health` returns 200
- **Blocks**: M008

### M012: /metrics endpoint on Sandbox daemon (C4a) ✅
- **What**: Expose Prometheus metrics on sandbox daemon via axum HTTP server;
  wire existing counters (executions, escapes, errors)
- **Files**: `src/daemon.rs`, `src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 3)
- **Lines**: ~100
- **Depends on**: M015, M017
- **Tests**: `curl http://localhost:9090/metrics` returns Prometheus format
- **Blocks**: M008

### M013: /metrics endpoint on CPG daemon (C4b) ✅
- **What**: Expose Prometheus metrics on CPG daemon via shared axum server
- **Files**: `cpg/src/daemon.rs`, `cpg/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 3)
- **Lines**: ~80
- **Depends on**: M015, M017
- **Tests**: `curl http://localhost:9091/metrics` returns Prometheus format
- **Blocks**: M008

### M014: /metrics endpoint on Evidence daemon (C4c) ✅
- **What**: Expose Prometheus metrics on Evidence daemon via shared axum server
- **Files**: `ev/src/daemon.rs`, `ev/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 3)
- **Lines**: ~80
- **Depends on**: M015, M017
- **Tests**: `curl http://localhost:9092/metrics` returns Prometheus format
- **Blocks**: M008

### M029: Grep — enterprise codebase search (Gap 4 / Tool E1) ❌
- **What**: Grep tool in ToolRegistry — regex search across codebase files
  with path filtering, line numbers, context lines, ripgrep backend
- **Files**: `agent/tools/grep.rs` (NEW)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 4),
  `bugswarm_tool_superpowers.md` (Tool E1)
- **Lines**: ~80
- **Depends on**: M003 (tool gating)
- **Tests**: `tests/unit/test_grep_tool.py` — pattern matching, path filtering,
  binary file skipping
- **Blocks**: none

### M030: Glob — recursive file discovery (Gap 4 / Tool E4) ❌
- **What**: Glob tool — recursive `**` file pattern matching, directory
  filtering, relevance ranking, file type detection
- **Files**: `agent/tools/glob.rs` (NEW)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 4),
  `bugswarm_tool_superpowers.md` (Tool E4)
- **Lines**: ~50
- **Depends on**: M003
- **Tests**: `tests/unit/test_glob_tool.py` — simple glob, recursive, exclude
- **Blocks**: none

### M031: Write file — secure file writer (Gap 4 / Tool E2) ❌
- **What**: Write_file tool — content within repo sandbox; path validation;
  file size limits; atomic writes; version history; rollback
- **Files**: `agent/tools/write_file.rs` (NEW)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 4),
  `bugswarm_tool_superpowers.md` (Tool E2)
- **Lines**: ~60
- **Depends on**: M003
- **Tests**: `tests/unit/test_write_tool.py` — creates file, path traversal
  blocked, overwrite detection
- **Blocks**: none

### M032: Edit — surgical code modifier (Gap 4 / Tool E3) ❌
- **What**: Edit tool — find-and-replace with old_string/new_string; verify
  uniqueness of match; AST-aware via tree-sitter; preview diffs
- **Files**: `agent/tools/edit.rs` (NEW)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 4),
  `bugswarm_tool_superpowers.md` (Tool E3)
- **Lines**: ~70
- **Depends on**: M003
- **Tests**: `tests/unit/test_edit_tool.py` — exact match replacement,
  multi-match rejection, path validation
- **Blocks**: none

### M033: WebFetch — secure web client (Gap 4 / Tool E5) ❌
- **What**: Web_fetch tool — fetch URL content, convert to text/markdown;
  URL allowlisting; timeout; size limits; content sanitization
- **Files**: `agent/tools/web_fetch.rs` (NEW)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 4),
  `bugswarm_tool_superpowers.md` (Tool E5)
- **Lines**: ~80
- **Depends on**: M003
- **Tests**: `tests/unit/test_web_fetch.py` — valid URL fetch, SSRF attempt
  blocked, timeout
- **Blocks**: none

### M034: TodoWrite — investigation state manager (Gap 4 / Tool E6) ❌
- **What**: Todo_write tool — manage investigation task list; track hypotheses,
  waypoints; auto-status updates; dependency tracking; merge semantics
- **Files**: `agent/tools/todo_write.rs` (NEW)
- **Plan**: `bugswarm_tool_superpowers.md` (Tool E6)
- **Lines**: ~50
- **Depends on**: M003
- **Tests**: `tests/unit/test_todo_write.py` — task tracking, merge semantics
- **Blocks**: M126 (ARIA ICO)

### M035: KillShell — process manager (Gap 4 / Tool E7) ❌
- **What**: Kill_shell tool — terminate background process; return PID, exit
  code, stdout/stderr; graceful shutdown; orphan detection
- **Files**: `agent/tools/kill_shell.rs` (NEW)
- **Plan**: `bugswarm_tool_superpowers.md` (Tool E7)
- **Lines**: ~40
- **Depends on**: M003
- **Tests**: `tests/unit/test_kill_shell.py`
- **Blocks**: none

### M053: bugswarm-scanner crate — trait + registry + orchestrator ❌
- **What**: New crate with `BugCategoryScanner` trait, `ScannerRegistry`
  (dynamic module loading), `ScanOrchestrator` (parallel execution),
  `Finding/Proof/Severity` types
- **Files**: `bugswarm-scanner/Cargo.toml` (NEW), `bugswarm-scanner/src/` (NEW)
- **Plan**: `q1_all_bug_coverage.md` (Section 3)
- **Lines**: ~500
- **Depends on**: none
- **Tests**: Scanner registry loads modules, orchestrator runs parallel scans
- **Blocks**: R001

### M054: CWE taxonomy database — TOML rule table ❌
- **What**: CWE taxonomy in TOML — 104+ CWE entries with detection method,
  proof type, FPR/TTD targets; versioned loader
- **Files**: `bugswarm-scanner/src/taxonomy/cwe_table.toml` (NEW),
  `bugswarm-scanner/src/taxonomy/cwe.rs` (NEW)
- **Plan**: `q1_all_bug_coverage.md` (Section 2.3)
- **Lines**: ~400
- **Depends on**: M053
- **Tests**: All 104 CWEs load without error; CWE-to-scanner mapping verified
- **Blocks**: R001

---

## Batch 5: Wiring All (16 milestones)

### M008: docker-compose.yml — 8-service orchestration (C1) ✅
- **What**: Single docker-compose.yml orchestrating all 8 components with
  healthchecks, port assignments, volume mounts, env-var ports, correct
  dependency order
- **Files**: `docker-compose.yml` (NEW)
- **Plan**: `phase30c_critical_fix_plan.md` (Step 4b)
- **Lines**: ~200
- **Depends on**: M004-M007 (Dockerfiles), M010-M014 (health/metrics), M015
- **Tests**: `docker compose up` succeeds, all 8 services report healthy
- **Blocks**: M159 (integration tests need composed services)

### M015: Unified config — YAML schema + Rust daemon readers (C6a) ❌
- **What**: `/etc/bugswarm/config.yaml` schema with env-var overrides; all 3
  Rust daemons read from unified YAML via `--config` flag. Replaces per-crate
  ad-hoc config
- **Files**: `/etc/bugswarm/config.yaml` (NEW), `cpg/src/config.rs` (NEW),
  `ev/src/config.rs` (NEW), `src/config.rs` (ENHANCE)
- **Plan**: `phase30c_critical_fix_plan.md` (Step 1)
- **Lines**: ~350
- **Depends on**: none
- **Tests**: `tests/integration/test_unified_config.py` — all daemons read
  from same config, env overrides work
- **Blocks**: M010-M014, M016

### M016: Unified config — Python components (C6b) ❌
- **What**: Agent, Gateway, Swarm, and bugswarm_mini Python components read
  from unified YAML config via `_load_config()` helpers
- **Files**: `agent/config.py` (NEW), `gateway/config_loader.py` (NEW),
  `mini/config.py` (ENHANCE)
- **Plan**: `phase30c_critical_fix_plan.md` (Step 1)
- **Lines**: ~150
- **Depends on**: M015
- **Tests**: All Python components start with defaults when config missing,
  exit with error on invalid config
- **Blocks**: all batches using Python components

### M017: Refactor daemon handlers — extract match arms (Step 0) ❌
- **What**: Sandbox/CPG/Evidence daemon handler match arms extracted into
  standalone functions; metrics refs passed as parameters. Pure refactor —
  zero behavioral change — but unlocks health/metrics wiring
- **Files**: `src/daemon.rs`, `cpg/src/daemon.rs`, `ev/src/daemon.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 0)
- **Lines**: ~400
- **Depends on**: none
- **Tests**: All existing 1000+ tests pass unchanged
- **Blocks**: M012, M013, M014 (metrics endpoints need clean handlers)

### R002: bugswarm_mini cost ↔ evidence graph bridge ❌
- **What**: Wire model usage stats and cost data from bugswarm_mini cost
  tracker into the evidence graph. Every model call during investigation
  creates a cost record linked to the investigating agent
- **Files**: `mini/gateway/cost_tracker.py`, `ev/src/daemon.rs`, `agent/core.py`
- **Plan**: New milestone
- **Lines**: ~100
- **Depends on**: bugswarm_mini (already built), M081
- **Tests**: Model usage records appear in evidence graph after investigation
- **Blocks**: none

### R003: discovery.py — Provider Discovery Engine ❌
- **What**: Query provider APIs for available models; rank by composite
  capability score. Uses bugswarm_mini protocol adapters as backend.
  Capability score: context (15%) + output (10%) + reasoning (40%) +
  thinking (20%) + tool calls (10%) + JSON mode (5%)
- **Files**: `swarm/discovery.py` (NEW)
- **Plan**: `phase15_self_configuring_swarm.md` (Module 1)
- **Lines**: ~200
- **Depends on**: bugswarm_mini (protocol adapters + GET /models)
- **Tests**: Provider discovery returns ranked model list; new models appear
  without code changes
- **Blocks**: R005

### R004: assessment.py — Difficulty Assessment Engine ❌
- **What**: Scout agent analyzes repo difficulty: file count, languages,
  dependency depth, test coverage, CVE count, code churn, AST complexity.
  Outputs `DifficultyLevel { LOW, MEDIUM, HIGH, EXTREME }`
- **Files**: `swarm/assessment.py` (NEW)
- **Plan**: `phase15_self_configuring_swarm.md` (Module 2)
- **Lines**: ~250
- **Depends on**: M064 (CPG parsers for basic repo analysis)
- **Tests**: Difficulty agrees with human rating on 20 test repos; confidence
  calibration within 10%
- **Blocks**: R005

### R005: allocation.py — Allocation Algorithm ❌
- **What**: Difficulty × model ranks → SwarmAllocation. Maps difficulty to
  agent count, model tier, temperature (always max), judge count, budget
- **Files**: `swarm/allocation.py` (NEW)
- **Plan**: `phase15_self_configuring_swarm.md` (Module 3)
- **Lines**: ~150
- **Depends on**: R003, R004
- **Tests**: LOW → 1 agent + weak model; EXTREME → 12 agents + frontier
- **Blocks**: R007

### R006: judge_cfg.py — Judge Configuration ❌
- **What**: Difficulty → judge count + model selection. LOW: 1 judge (worker).
  MEDIUM: 3 judges (1 strong, 2 worker). HIGH: 5 judges (2 frontier, 3 strong).
  EXTREME: 7 judges (3 frontier, 4 strong). Always max temperature
- **Files**: `swarm/judge_cfg.py` (NEW)
- **Plan**: `phase15_self_configuring_swarm.md` (Module 4)
- **Lines**: ~80
- **Depends on**: R003, R005
- **Tests**: Judge config matches expected count/model tier per difficulty
- **Blocks**: R007

### R007: swarm_config.py — SwarmConfig v2 ❌
- **What**: Dynamic allocation replaces hardcoded model/provider config. Reads
  from discovery + assessment + allocation + judge_cfg. Falls back to
  hardcoded defaults if discovery/assessment unavailable
- **Files**: `swarm/swarm_config.py` (NEW)
- **Plan**: `phase15_self_configuring_swarm.md` (Module 5)
- **Lines**: ~100
- **Depends on**: R003, R004, R005, R006
- **Tests**: Full config from end-to-end pipeline; fallback when discovery
  fails
- **Blocks**: M133 (ARIA runtime)

### M126: ARIA — Investigation Context Object (ICO) ❌
- **What**: Central data structure with TargetProfile, static facts, normalized
  tool outputs, pattern annotations, waypoints, investigation state, capability
  grammar. Feeds prompt builder every cycle
- **Files**: `agent/aria/ico.py` (NEW)
- **Plan**: `ARIA.md` (Sections 3, 8)
- **Lines**: ~400
- **Depends on**: M034 (TodoWrite tool for waypoint tracking)
- **Tests**: ICO serialization round-trip; Layer 0/1/2 data correctly
  partitioned
- **Blocks**: M127, M130, M131

### M127: ARIA — Pre-Computation Engine ❌
- **What**: CPG indexing → pattern pre-run → CVE similarity → profile
  classification → waypoint generation → vector index build. ~6-10s before
  model sees prompt. All deterministic, no LLM call
- **Files**: `agent/aria/pre_computer.py` (NEW)
- **Plan**: `ARIA.md` (Section 11)
- **Lines**: ~300
- **Depends on**: M126, M064
- **Tests**: Pre-computation produces filled ICO with all layers
- **Blocks**: M133

### M130: ARIA — Layered Prompt Builder ❌
- **What**: Build universal prompt: Layer 0 (facts), Layer 1 (patterns),
  Layer 2 (waypoints), Capability Grammar, Open Decision. Identical structure
  for all models — only model ID differs
- **Files**: `agent/aria/prompt_builder.py` (NEW)
- **Plan**: `ARIA.md` (Section 6)
- **Lines**: ~250
- **Depends on**: M126
- **Tests**: Prompt includes all 3 layers; within context budget
- **Blocks**: M133

### M131: ARIA — 3-Layer Intent Parser ❌
- **What**: Layer A (structured): `category.name(args)`, HYPOTHESIS:,
  QUESTION:, CONCLUDE:. Layer B (semantic): free text → intent classification.
  Layer C (constrained): STUCK fallback with multiple-choice recovery
- **Files**: `agent/aria/intent_parser.py` (NEW)
- **Plan**: `ARIA.md` (Section 7)
- **Lines**: ~250
- **Depends on**: M126
- **Tests**: Structured intents parsed; free text classified; STUCK triggers
  fallback
- **Blocks**: M133

### M133: ARIA — Runtime Main Loop (replaces IEP engine) ❌
- **What**: Phase 0: Pre-computation → Phase 1: Build prompt (via bugswarm_mini
  gateway) → Phase 2: Model call → Phase 3: Parse intent → Phase 4: Dispatch
  → Phase 5: Enrich ICO → repeat until completion conditions met. Replaces
  existing `agent/loop.py` (IEP engine)
- **Files**: `agent/aria/runtime.py` (NEW), `agent/loop.py` (deprecate)
- **Plan**: `ARIA.md` (Section 8)
- **Lines**: ~300
- **Depends on**: M127, M130, M131, R007, bugswarm_mini
- **Tests**: Full investigation cycle; completion triggers; waypoints
  regenerated after each cycle
- **Blocks**: none (this is the final agent loop)

### M149: Tool Calling Architecture — provider-agnostic schema mapping ❌
- **What**: ToolProtocol handler: BugSwarm ToolDefinition → provider tool
  schema (OpenAI/Anthropic/DeepSeek/Gemini); parse tool_calls from provider
  responses; manage lifecycle; parallel execution. Extends bugswarm_mini
  protocol adapters
- **Files**: `gateway/tool_protocol.rs` (NEW),
  `mini/gateway/protocol.py` (ENHANCE)
- **Plan**: `q2_tool_calling_architecture.md` (Sections 2, 3)
- **Lines**: ~600
- **Depends on**: bugswarm_mini (already built), M029-M035 (agent tools)
- **Tests**: Tool formats converted for all 4 protocols; tool calls parsed
  from each format
- **Blocks**: M133 (ARIA needs tool calling)

---

## Batch 6: Enterprise Standard / Production Ready (4 milestones)

### M019: Version to 1.0.0 + git tag (H6) ❌
- **What**: Set version to 1.0.0 in all Cargo.toml; add `--version` flag to
  all daemons; create signed git tag v1.0.0
- **Files**: All Cargo.toml, all main.rs files
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: `cargo pkgid | head -1 | grep -q "1.0.0$"`,
  `git describe --tags --exact-match HEAD | grep -q "v1.0.0"`
- **Blocks**: none

### M075: OpenTelemetry distributed tracing (H18) ❌
- **What**: OpenTelemetry spans across all daemon boundaries; trace IDs pass
  through request headers; export to OTLP collector; sampling configurable
  (100% dev, 10% production)
- **Files**: All daemon main.rs, `src/daemon.rs`, `cpg/src/daemon.rs`,
  `ev/src/daemon.rs`
- **Plan**: `production_audit.md` (H18), `phase30b_high_fix_plan.md` (Batch 4)
- **Lines**: ~200
- **Depends on**: M017
- **Tests**: `tests/integration/test_tracing.py` — trace spans in collector;
  correlation IDs match across daemons
- **Blocks**: none

### M077: Daemon stdout → structured file logging + rotation (M3) ❌
- **What**: Daemons currently log to stdout only. Add file logging with
  rotation (tracing-appender), syslog option, configurable log level
- **Files**: All 3 daemon main.rs
- **Plan**: `production_audit.md` (M1, M3)
- **Lines**: ~60
- **Depends on**: M015 (unified config for log paths)
- **Tests**: Log file created, rotated at size limit, old files gzipped
- **Blocks**: none

### M091: Python package lock files (L9, L18) 🔶 PARTIAL
- **What**: Add lock files for Python transitive deps; pin versions in
  pyproject.toml; `pip install --require-hashes` must succeed on all packages
- **Files**: `agent/pyproject.toml`, `mini/pyproject.toml`,
  `swarm/pyproject.toml`
- **Plan**: `production_audit.md` (L9, L18)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: `pip install --require-hashes` succeeds on all Python packages
- **Blocks**: none

---

## Batch 7: Final Testing — Real-Life Validation (5 milestones)

### R001: Top-10 CWE scanners — validated against Juliet test suite ❌
- **What**: 10 focused scanners for highest-impact CWEs: SQL injection (89),
  XSS (79), path traversal (22), buffer overflow (120), OS command injection
  (78), XXE (611), SSRF (918), deserialization (502), auth bypass (287), weak
  crypto (326). Each scanner must achieve >90% detection on Juliet test suite
- **Files**: `bugswarm-scanner/src/scanners/` (10 NEW files)
- **Plan**: New milestone
- **Lines**: ~400
- **Depends on**: M053, M054, M064 (CPG), M068, M069 (taint models)
- **Tests**: Juliet test suite top-10 categories; >90% detection rate; <10%
  false positive rate
- **Blocks**: none

### M064: CPG Tier 1 — 4 language parsers (Python + JS/TS + C/C++ + Rust) ❌
- **What**: Per-language parsers with AST, CFG, call graph, taint, SSA, types,
  mutation operators. Validate by parsing 50+ real-world open-source files per
  language; extract functions, classes, call edges, control flow, taint paths
- **Files**: `cpg/src/parsers/{python,javascript,typescript,c,cpp,rust}/`
  (30 NEW files)
- **Plan**: New milestone
- **Lines**: ~1500
- **Depends on**: none
- **Tests**: Parse real open-source codebases; extract taint paths from
  known-vulnerable files
- **Blocks**: R001, M068, M069

### R008: Phase gate stress tests (14 adversarial gates) ❌
- **What**: Implement all 14 phase gate stress tests from `invincible_phases.md`
  as runnable test suites. Each gate is an adversarial test designed to break
  the implementation. Phase passes only when its gate is green at 100%
- **Files**: `tests/gates/` (14 NEW files)
- **Plan**: `invincible_phases.md` (all 14 phases)
- **Lines**: ~2000
- **Depends on**: M008 (docker-compose for daemon orchestration)
- **Tests**: Phase 1 (Sandbox: 18 attack classes), Phase 2 (CPG: 20 obfuscation
  patterns), Phase 3 (Gateway: 10 failure modes), Phase 4 (Agent: 8 test
  repos), Phase 5 (Evidence: 8 attacks), Phase 6 (Swarm: 10 scenarios),
  Phase 7 (Compression: 8 contexts), Phase 8 (Budget: 8 drills), Phase 9
  (Scale: 10 metrics), Phase 10 (Judges: 10 scenarios), Phase 11 (TUI: 13
  conditions), Phase 12 (Minitia: 10 scenarios), Phase 13 (Observability: 11
  alerts), Phase 14 (Learning: 8 regressions)
- **Blocks**: none

### R009: Security penetration test suite ❌
- **What**: Dedicated security test suite covering all attack surfaces —
  sandbox escape (18 vectors from Phase 1 gate), path traversal (38 vectors),
  shell injection (8 variants), prompt injection (10 patterns), seccomp
  bypass (5 approaches), SSRF (3 variants), API key leakage (4 scenarios)
- **Files**: `tests/security/` (NEW)
- **Plan**: New milestone
- **Lines**: ~500
- **Depends on**: M001, M002, M045, M072, M073, M003
- **Tests**: This IS the test suite — zero security vulnerabilities found
- **Blocks**: none

### R010: Real-world benchmark runs ❌
- **What**: Run BugSwarm against real-world targets: 10 open-source repos with
  known CVEs (severity 7+), Juliet test suite (100K test cases), 1M+ LOC
  monorepo. Measure: bugs found, time-to-first-bug, false positive rate, cost
  per bug, detection rate vs ground truth
- **Files**: `tests/benchmarks/` (NEW)
- **Plan**: New milestone
- **Lines**: ~500
- **Depends on**: M133 (ARIA runtime), R008, R009
- **Tests**: This IS the test suite — metrics: >80% detection, <20% FPR,
  <$50 per confirmed bug
- **Blocks**: none

---

## Summary

| Batch | Name | Milestones |
|-------|------|-----------|
| — | Already Complete | 3 |
| 1 | Flaw Fixes | 12 |
| 2 | Incomplete Features | 9 |
| 3 | Testing for Hardcoded Output | 2 |
| 4 | New Features | 18 |
| 5 | Wiring All | 16 |
| 6 | Enterprise / Production Ready | 4 |
| 7 | Final Testing — Real-Life Validation | 5 |
| **Total** | | **66** |

### Status Breakdown
- ✅ DONE: 5 (M001, M002, M003, M022, M093)
- 🔶 PARTIAL: 2 (M091, M160)
- ❌ NOT_STARTED: 61

### Key Architecture Decisions
- **CLI-first**: No web API server, no React frontend, no GitHub App, no VS
  Code extension, no Slack/Discord bots. Everything runs from the terminal.
- **Protocol-based adapters**: 4 protocols (OpenAI-compatible, Anthropic,
  Google, Ollama) cover all providers. New providers need only base_url, not
  new adapter code.
- **bugswarm_mini is the model gateway**: The Mini BugSwarm package at
  `/root/b/bugswarm_mini/` handles all model communication. Main BugSwarm
  never calls provider APIs directly.
- **Always max temperature**: Temperature is always 1.0+ for all roles
  (investigator, judge, critic). No low-temperature inference.
- **Token budget is the hard gate**: Dollar cost estimates are decorative.
  The token budget is authoritative and enforced before execution begins.
- **Model discovery via GET /models**: Provider API is queried for available
  models at configure time. Unknown models are probed for capabilities.
- **ARIA replaces IEP engine**: The old IEP agent loop (`loop.py`) is replaced
  by the ARIA runtime which uses layered prompts, intent parsing, and the
  Investigation Context Object.
- **Self-configuring swarm (Phase 15)**: The swarm dynamically allocates agents
  and models based on repo difficulty assessment. Nothing is hardcoded.
