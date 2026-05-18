# BugSwarm Implementation Milestones

**Generated**: 2026-05-18  
**Sources**: production_audit.md (105 findings), phase30b_high_fix_plan.md (41 HIGH), phase30c_critical_fix_plan.md (8 CRITICAL), q1_all_bug_coverage.md (104 CWE), q2_tool_calling_architecture.md, q3_finetuning_pipeline.md, q4_100_languages.md, q5_zero_cli_platform.md, q6_github_integration.md, q7_multi_provider_pluggable.md, ARIA.md, bugswarm_tool_superpowers.md, bugswarm_mythos_gap_analysis.md, chain_universal_expansion.md, phase24.md, phase25.md, phase30.md  

---

## Batch 1: Critical Fixes — Deployment & Security Blockers (28 milestones, est. 4 weeks)

### M001: Shell injection via heredoc delimiter (H11) ❌ NOT_STARTED
- **What**: Replace heredoc-based PoC execution with bind-mount to prevent BUGSWARM_EOF escape
- **Files**: `bugswarm-sandbox/src/container.rs:274-276`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 0)
- **Lines**: ~80
- **Depends on**: none
- **Tests**: `tests/fuzz/test_shell_injection.py`, `tests/integration/test_sandbox_execution.py::test_bind_mount_heredoc_free`
- **Blocks**: M003

### M002: Path traversal in agent read_file (H12) ❌ NOT_STARTED
- **What**: Reject absolute paths and `../` patterns in agent read_file BEFORE filesystem resolution; validate sandbox boundary
- **Files**: `bugswarm-agent/src/agent/core.py:240`, `bugswarm-agent/src/agent/cli/wiring.py`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 0)
- **Lines**: ~80
- **Depends on**: none
- **Tests**: `tests/unit/test_read_file_path_validation.py` (12 test cases)
- **Blocks**: M003, M008

### M003: Tool capability gating — validate all agent tool calls (H17) ❌ NOT_STARTED
- **What**: Add validation middleware for all agent tool dispatches — path validation, arg type checks, rate limiting, sandbox boundary enforcement
- **Files**: `bugswarm-agent/src/agent/tool_executor.rs`, `bugswarm-agent/src/agent/validation.rs`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2, combined G6 fix), `production_audit.md` (H17)
- **Lines**: ~200
- **Depends on**: M002 (H12 path validation)
- **Tests**: `tests/unit/test_tool_gating.py` — oversized PoC rejected, absolute paths rejected, rate limit enforced
- **Blocks**: M005, M010

### M004: Dockerfile for CPG daemon (H1a) ❌ NOT_STARTED
- **What**: Create multi-stage Dockerfile for CPG daemon using debian:bookworm-slim
- **Files**: `bugswarm-cpg/Dockerfile` (NEW)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1), CRIT-PRE2
- **Lines**: ~15
- **Depends on**: none
- **Tests**: `docker build bugswarm-cpg/ && docker run --rm bugswarm-cpg --version`
- **Blocks**: M018

### M005: Dockerfile for Evidence daemon (H1b) ❌ NOT_STARTED
- **What**: Create multi-stage Dockerfile for Evidence daemon using debian:bookworm-slim
- **Files**: `bugswarm-evidence/Dockerfile` (NEW)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1), CRIT-PRE2
- **Lines**: ~15
- **Depends on**: none
- **Tests**: `docker build bugswarm-evidence/ && docker run --rm bugswarm-evidence --version`
- **Blocks**: M018

### M006: Dockerfile for Symbolic daemon (H1c) ❌ NOT_STARTED
- **What**: Enhance Dockerfile for Symbolic daemon with debian:bookworm-slim, libz3 runtime
- **Files**: `bugswarm-symbolic/Dockerfile` (ENHANCE)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1), CRIT-PRE2
- **Lines**: ~20
- **Depends on**: none
- **Tests**: `docker build bugswarm-symbolic/ && docker run --rm bugswarm-symbolic --version`
- **Blocks**: M018

### M007: sandbox-python-asan Dockerfile — execution base image (H2) ❌ NOT_STARTED
- **What**: Add WORKDIR, LABEL metadata to sandbox-python-asan Dockerfile (execution base image, NOT service)
- **Files**: `bugswarm-sandbox/docker/sandbox-python-asan.Dockerfile`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1), CRIT-PRE3
- **Lines**: ~15
- **Depends on**: none
- **Tests**: `docker inspect bugswarm/sandbox-python-asan:latest | jq '.[0].Config.Cmd'` is null
- **Blocks**: M018

### M008: docker-compose.yml — 8-service orchestration (C1) ❌ NOT_STARTED
- **What**: Single docker-compose.yml starting all 8 components in correct dependency order with healthchecks, port assignments, volume mounts, env-var ports
- **Files**: `docker-compose.yml` (NEW)
- **Plan**: `phase30c_critical_fix_plan.md` (Step 4b)
- **Lines**: ~200
- **Depends on**: M004, M005, M006, M007, M010, M011, M012, M013, M014, M015, M003
- **Tests**: `docker compose up` succeeds, all 8 services report healthy via `docker compose ps`
- **Blocks**: M018, M070

### M009: Release build profiles for all crates (C2) 🔶 PARTIAL
- **What**: Add `[profile.release]` with LTO, strip, opt-level=3 to all 4 Cargo.toml files
- **Files**: `bugswarm-sandbox/Cargo.toml`, `bugswarm-cpg/Cargo.toml`, `bugswarm-evidence/Cargo.toml`, `bugswarm-symbolic/Cargo.toml`
- **Plan**: `production_audit.md` (C2)
- **Lines**: ~20
- **Depends on**: none
- **Tests**: `cargo build --release` produces stripped release binaries
- **Blocks**: M018

### M010: HTTP health endpoint on CPG daemon (C3a) ❌ NOT_STARTED
- **What**: Add axum-based HTTP health check endpoint on CPG daemon; share axum server with metrics
- **Files**: `bugswarm-cpg/src/daemon.rs`, `bugswarm-cpg/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 2)
- **Lines**: ~80
- **Depends on**: M014 (unified config for port)
- **Tests**: `curl http://localhost:8080/health` returns 200
- **Blocks**: M008

### M011: HTTP health endpoint on Evidence daemon (C3b) ❌ NOT_STARTED
- **What**: Add axum-based HTTP health check endpoint on Evidence daemon
- **Files**: `bugswarm-evidence/src/daemon.rs`, `bugswarm-evidence/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 2)
- **Lines**: ~80
- **Depends on**: M014 (unified config for port)
- **Tests**: `curl http://localhost:8081/health` returns 200
- **Blocks**: M008

### M012: `/metrics` endpoint on Sandbox daemon (C4a) ❌ NOT_STARTED
- **What**: Expose Prometheus metrics on sandbox daemon via axum HTTP server; wire existing counters
- **Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-sandbox/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 3)
- **Lines**: ~100
- **Depends on**: M014 (unified config for port), M017 (daemon handler refactor)
- **Tests**: `curl http://localhost:9090/metrics` returns valid Prometheus format
- **Blocks**: M008, M033

### M013: `/metrics` endpoint on CPG daemon (C4b) ❌ NOT_STARTED
- **What**: Expose Prometheus metrics on CPG daemon via shared axum server
- **Files**: `bugswarm-cpg/src/daemon.rs`, `bugswarm-cpg/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 3)
- **Lines**: ~80
- **Depends on**: M014, M017 (handler refactor)
- **Tests**: `curl http://localhost:9091/metrics` returns valid Prometheus format
- **Blocks**: M008, M033

### M014: `/metrics` endpoint on Evidence daemon (C4c) ❌ NOT_STARTED
- **What**: Expose Prometheus metrics on Evidence daemon via shared axum server
- **Files**: `bugswarm-evidence/src/daemon.rs`, `bugswarm-evidence/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 3)
- **Lines**: ~80
- **Depends on**: M014 (unified config), M017 (handler refactor)
- **Tests**: `curl http://localhost:9092/metrics` returns valid Prometheus format
- **Blocks**: M008, M033

### M015: Unified config — YAML schema + Rust daemon readers (C6a) ❌ NOT_STARTED
- **What**: Create `/etc/bugswarm/config.yaml` schema; CPG + Evidence + Sandbox daemons read from unified YAML; `--config` flag on all daemons; env-var overrides
- **Files**: `/etc/bugswarm/config.yaml` (NEW), `bugswarm-cpg/src/config.rs` (NEW), `bugswarm-evidence/src/config.rs` (NEW), `bugswarm-sandbox/src/config.rs` (ENHANCE)
- **Plan**: `phase30c_critical_fix_plan.md` (Step 1)
- **Lines**: ~350
- **Depends on**: M026 (H7 config failure — must use proper error handling)
- **Tests**: `tests/integration/test_unified_config.py` — all daemons read from same config, env overrides work
- **Blocks**: M010, M011, M012, M013, M014, M016

### M016: Unified config — Python components (C6b) ❌ NOT_STARTED
- **What**: Agent, Gateway, Swarm Python components read from unified YAML config; `_load_config()` helpers
- **Files**: `bugswarm-agent/src/agent/config.py` (NEW), `bugswarm-gateway/src/gateway/config_loader.py` (NEW)
- **Plan**: `phase30c_critical_fix_plan.md` (Step 1, C6b)
- **Lines**: ~150
- **Depends on**: M015
- **Tests**: All Python components start with defaults when config missing, exit with error on invalid config
- **Blocks**: M070

### M017: Refactor daemon handlers — extract match arms (Step 0) ❌ NOT_STARTED
- **What**: Extract sandbox/CPG/evidence daemon handler match arms into standalone functions; pass metrics refs as parameters
- **Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-cpg/src/daemon.rs`, `bugswarm-evidence/src/daemon.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (Step 0)
- **Lines**: ~400 (refactor only, zero behavioral change)
- **Depends on**: none
- **Tests**: All existing 1000+ tests pass unchanged
- **Blocks**: M012, M013, M014

### M018: CI/CD pipeline — GitHub Actions (H5) ❌ NOT_STARTED
- **What**: `.github/workflows/ci.yml` with build, test, clippy, cargo-audit, docker-build for all 4 images
- **Files**: `.github/workflows/ci.yml` (NEW)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~80
- **Depends on**: M004, M005, M006, M007, M008, M009
- **Tests**: CI passes on every PR; badge in README shows green
- **Blocks**: none

### M019: Version to 1.0.0 + git tag (H6) ❌ NOT_STARTED
- **What**: Set version to 1.0.0 in all Cargo.toml; add `--version` flag to all daemons; create git tag v1.0.0
- **Files**: All Cargo.toml, all main.rs files
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: `cargo pkgid | head -1 | grep -q "1.0.0$"`, `git describe --tags --exact-match HEAD | grep -q "v1.0.0"`
- **Blocks**: none

### M020: Evidence daemon reachable — RunServer subcommand (C5) 🔶 PARTIAL
- **What**: Ensure evidence daemon `main.rs` has RunServer subcommand; daemon can be started as service
- **Files**: `bugswarm-evidence/src/main.rs`
- **Plan**: `production_audit.md` (C5)
- **Lines**: ~40
- **Depends on**: none
- **Tests**: `bugswarm-evidence daemon --socket /tmp/test.sock` starts and accepts connections
- **Blocks**: M008

### M021: Daemon auto-restart — systemd unit files (H8) ❌ NOT_STARTED
- **What**: Create systemd unit files for sandbox, CPG, evidence daemons with Restart=always, PIDFile, Type=simple
- **Files**: `deploy/systemd/bugswarm-sandbox.service` (NEW), `deploy/systemd/bugswarm-cpg.service` (NEW), `deploy/systemd/bugswarm-evidence.service` (NEW)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~90
- **Depends on**: M008
- **Tests**: `tests/integration/test_daemon_restart.py` — kill daemon, verify new process within 5s
- **Blocks**: none

### M022: Graceful shutdown — tokio::select! on SIGTERM (H4) ❌ NOT_STARTED
- **What**: Add `tokio::select!` inside accept loop polling individual `listener.accept()` + SIGTERM; 5s drain timeout
- **Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-cpg/src/daemon.rs`, `bugswarm-evidence/src/daemon.rs`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2), CRIT-PRE1
- **Lines**: ~120
- **Depends on**: M017 (handler refactor to avoid merge conflicts)
- **Tests**: `tests/integration/test_graceful_shutdown.py` — clean shutdown within 5s, drain timeout, no new connections after signal
- **Blocks**: M008

### M023: Config parsing failures — exit, don't fall back to defaults (H7) ❌ NOT_STARTED
- **What**: If config file exists but is invalid → daemon exits with code 1 + tracing::error; defaults ONLY if file missing
- **Files**: `bugswarm-sandbox/src/main.rs:167-169`, all daemon main.rs
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2)
- **Lines**: ~50
- **Depends on**: M015
- **Tests**: `tests/integration/test_config_parsing.py` — invalid TOML → exit 1, missing config → defaults
- **Blocks**: M015

### M024: SIGHUP config reload (H10) ❌ NOT_STARTED
- **What**: Spawn background task for SIGHUP; reload mutable config fields (log level, timeouts); structural fields require restart
- **Files**: All 3 daemon main.rs files
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2)
- **Lines**: ~100
- **Depends on**: M015
- **Tests**: `tests/integration/test_config_reload.py` — SIGHUP reloads log level, doesn't change socket path
- **Blocks**: none

### M025: Remove all .unwrap() from production paths (H9) ❌ NOT_STARTED
- **What**: Replace all ~15 non-test unwrap/expect calls with proper error handling; add clippy lint denial
- **Files**: All crate .rs files (audit every `.unwrap()` and `.expect()`)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2)
- **Lines**: ~100
- **Depends on**: M017
- **Tests**: `cargo clippy --workspace -- -D clippy::unwrap_used` passes
- **Blocks**: none

### M026: Socket parent dir creation error handling (H3) ❌ NOT_STARTED
- **What**: Replace `.ok()` on `create_dir_all` with `?` and `.context()`; verify directory writable after creation
- **Files**: `bugswarm-cpg/src/daemon.rs:85`, `bugswarm-sandbox/src/daemon.rs:76`, `bugswarm-evidence/src/daemon.rs:91`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: `tests/integration/test_daemon_startup.py::test_startup_fails_when_socket_dir_impermissible`
- **Blocks**: M008

### M027: Evidence graph persistence — save/load to disk (C12) ❌ NOT_STARTED
- **What**: Serialize evidence graph nodes+edges to JSON on shutdown; load on startup; autosave every N seconds
- **Files**: `bugswarm-evidence/src/graph.rs`, `bugswarm-evidence/src/daemon.rs`
- **Plan**: `production_audit.md` (C12)
- **Lines**: ~150
- **Depends on**: M020
- **Tests**: `tests/integration/test_evidence_persistence.py` — restart daemon, graph content preserved
- **Blocks**: M008, M043

### M028: Fuzzer crash artifact collection — wire results back (C8) ❌ NOT_STARTED
- **What**: Fuzzer container launches but stats/crashes never flow back to daemon or evidence graph; add result collection pipeline from AFL++ container
- **Files**: `bugswarm-sandbox/src/fuzzer.rs`, `bugswarm-sandbox/src/container.rs`
- **Plan**: `production_audit.md` (C8)
- **Lines**: ~150
- **Depends on**: M001 (shell injection fix — fuzzer uses heredoc too)
- **Tests**: `tests/integration/test_fuzzer_results.py` — fuzz campaign → crashes appear in evidence graph
- **Blocks**: M043

---

## Batch 2: Core Tooling — Functional Stubs & Code Quality (24 milestones, est. 3 weeks)

### M029: Grep — enterprise codebase search (Gap 4 / Tool E1) ❌ NOT_STARTED
- **What**: Add grep tool to ToolRegistry — regex search across codebase files with path filtering, line numbers, context lines
- **Files**: `bugswarm-agent/src/agent/tools/grep.rs` (NEW)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 4), `bugswarm_tool_superpowers.md` (Tool E1)
- **Lines**: ~80
- **Depends on**: M003 (tool gating)
- **Tests**: `tests/unit/test_grep_tool.py` — pattern matching, path filtering, binary file skipping
- **Blocks**: M041

### M030: Glob — recursive file discovery (Gap 4 / Tool E4) ❌ NOT_STARTED
- **What**: Add glob tool — recursive file pattern matching, directory filtering, relevance ranking
- **Files**: `bugswarm-agent/src/agent/tools/glob.rs` (NEW)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 4), `bugswarm_tool_superpowers.md` (Tool E4)
- **Lines**: ~50
- **Depends on**: M003
- **Tests**: `tests/unit/test_glob_tool.py` — simple glob, recursive, exclude patterns
- **Blocks**: M041

### M031: Write file — secure file writer (Gap 4 / Tool E2) ❌ NOT_STARTED
- **What**: Add write_file tool — write content to files within repo sandbox; path validation; file size limits
- **Files**: `bugswarm-agent/src/agent/tools/write_file.rs` (NEW)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 4), `bugswarm_tool_superpowers.md` (Tool E2)
- **Lines**: ~60
- **Depends on**: M003
- **Tests**: `tests/unit/test_write_tool.py` — creates file, path traversal blocked, overwrite detection
- **Blocks**: M041, M058

### M032: Edit — surgical code modifier (Gap 4 / Tool E3) ❌ NOT_STARTED
- **What**: Add edit tool — find-and-replace with old_string/new_string in source files; verify uniqueness of match
- **Files**: `bugswarm-agent/src/agent/tools/edit.rs` (NEW)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 4), `bugswarm_tool_superpowers.md` (Tool E3)
- **Lines**: ~70
- **Depends on**: M003
- **Tests**: `tests/unit/test_edit_tool.py` — exact match replacement, multi-match rejection, path validation
- **Blocks**: M041

### M033: WebFetch — secure web client (Gap 4 / Tool E5) ❌ NOT_STARTED
- **What**: Add web_fetch tool — fetch URL content, convert to text/markdown; URL allowlisting; timeout; size limits
- **Files**: `bugswarm-agent/src/agent/tools/web_fetch.rs` (NEW)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 4), `bugswarm_tool_superpowers.md` (Tool E5)
- **Lines**: ~80
- **Depends on**: M003
- **Tests**: `tests/unit/test_web_fetch.py` — valid URL fetch, SSRF attempt blocked, timeout
- **Blocks**: M041, M070

### M034: TodoWrite — investigation state manager (Gap 4 / Tool E6) ❌ NOT_STARTED
- **What**: Add todo_write tool — manage investigation task list; track which hypotheses/waypoints remain open
- **Files**: `bugswarm-agent/src/agent/tools/todo_write.rs` (NEW)
- **Plan**: `bugswarm_tool_superpowers.md` (Tool E6)
- **Lines**: ~50
- **Depends on**: M003
- **Tests**: `tests/unit/test_todo_write.py` — task tracking, merge semantics
- **Blocks**: M041

### M035: KillShell — process manager (Gap 4 / Tool E7) ❌ NOT_STARTED
- **What**: Add kill_shell tool — terminate background process; return PID, exit code, stdout/stderr
- **Files**: `bugswarm-agent/src/agent/tools/kill_shell.rs` (NEW)
- **Plan**: `bugswarm_tool_superpowers.md` (Tool E7)
- **Lines**: ~40
- **Depends on**: M003
- **Tests**: `tests/unit/test_kill_shell.py`
- **Blocks**: M041

### M036: mine_invariants — real sandbox execution (C7 + Stub 4) ❌ NOT_STARTED
- **What**: Replace stub traces with actual sandbox execution: generate PoC, execute via manager.execute(), parse ExecutionReceipt, build real ExecutionTrace
- **Files**: `bugswarm-sandbox/src/invariant.rs`, `bugswarm-sandbox/src/daemon.rs`
- **Plan**: `production_audit.md` (C7), `phase30b_high_fix_plan.md` (H21), `bugswarm_tool_superpowers.md` (Stub 4), CRIT-PRE5
- **Lines**: ~200
- **Depends on**: M001 (heredoc fix — PoC generation writes to temp file)
- **Tests**: `tests/integration/test_invariant_mining.py::test_real_execution_on_target` — real traces with actual output values
- **Blocks**: M043, M037

### M037: run_mutations — real compilation and test execution (C9 + Stub 5) ❌ NOT_STARTED
- **What**: Replace fake test runner (hardcoded tuples) with actual compilation + test suite execution via sandbox; wire real mutation results
- **Files**: `bugswarm-sandbox/src/mutation.rs`, `bugswarm-sandbox/src/daemon.rs`
- **Plan**: `production_audit.md` (C9), `bugswarm_tool_superpowers.md` (Stub 5)
- **Lines**: ~200
- **Depends on**: M001, M036
- **Tests**: `tests/unit/test_mutation_runner.py::test_real_compilation_and_execution` — actual kill matrix
- **Blocks**: M043

### M038: solve_reachability — wire Z3 SMT solver (C11 + Stub 6) ❌ NOT_STARTED
- **What**: Replace heuristic-only solver (val+1) with actual Z3 SMT solver from bugswarm-symbolic crate; feature-gate symbolic
- **Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-symbolic/src/engine.rs`
- **Plan**: `production_audit.md` (C11), `bugswarm_tool_superpowers.md` (Stub 6)
- **Lines**: ~100
- **Depends on**: M006 (symbolic Dockerfile with Z3)
- **Tests**: `tests/unit/test_symbolic_solver.py::test_z3_solves_reachability` — constraint solving produces real values
- **Blocks**: M043

### M039: diff_execute — fix client→daemon protocol mismatch (C10 + Stub 3) ❌ NOT_STARTED
- **What**: Fix protocol mismatch — Python client sends `--output-a`/`--output-b` but daemon reads `input`/`reference`; harmonize to `input`/`reference`
- **Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-agent/src/agent/cli/diff.py`
- **Plan**: `production_audit.md` (C10), `bugswarm_tool_superpowers.md` (Stub 3)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: `tests/integration/test_diff_execute.py` — client sends request, daemon processes and returns correct diff
- **Blocks**: M043

### M040: invariant_check — execute generated inputs in sandbox (H22 + Stub partial) ❌ NOT_STARTED
- **What**: invariant_check generates inputs but never executes them; wire sandbox execution and return actual results
- **Files**: `bugswarm-sandbox/src/invariant.rs`, `bugswarm-sandbox/src/daemon.rs`
- **Plan**: `production_audit.md` (H22)
- **Lines**: ~80
- **Depends on**: M036
- **Tests**: `tests/unit/test_invariant_check.py::test_invariant_check_executes_in_sandbox`
- **Blocks**: none

### M041: describe_trigger — bypass EvidenceClient in orchestrator (H26) ❌ NOT_STARTED
- **What**: `describe_trigger` in orchestrator bypasses EvidenceClient; wire through Evidence daemon properly
- **Files**: `bugswarm-orchestrator/src/orchestrator.rs`
- **Plan**: `production_audit.md` (H26)
- **Lines**: ~40
- **Depends on**: M020 (evidence daemon reachable)
- **Tests**: `tests/unit/test_describe_trigger.py` — calls evidence daemon, returns trigger conditions
- **Blocks**: none

### M042: get_trigger_matrix — return real data, not empty stubs (H27) ❌ NOT_STARTED
- **What**: `get_trigger_matrix` returns always-empty stubs; populate from real evidence graph query
- **Files**: `bugswarm-orchestrator/src/orchestrator.rs`
- **Plan**: `production_audit.md` (H27)
- **Lines**: ~50
- **Depends on**: M020
- **Tests**: `tests/unit/test_trigger_matrix.py` — non-empty trigger matrix from known bug
- **Blocks**: none

### M043: estimate_argument_ranges — return real computed ranges (H28) ❌ NOT_STARTED
- **What**: Replace hardcoded [0,100,42] with actual argument range computation from CPG and existing evidence
- **Files**: `bugswarm-orchestrator/src/orchestrator.rs`
- **Plan**: `production_audit.md` (H28)
- **Lines**: ~60
- **Depends on**: M027 (evidence graph persistence)
- **Tests**: `tests/unit/test_argument_ranges.py` — dynamic ranges, not hardcoded
- **Blocks**: none

### M044: Orchestrator tools — add 8 missing agent tools (H31) ❌ NOT_STARTED
- **What**: Orchestrator missing 8 agent tools (delta_debug, diff_execute, mine_invariants, run_mutations, solve_reachability, explore_paths, invariant_check, estimate_argument_ranges); wire them
- **Files**: `bugswarm-orchestrator/src/orchestrator.rs`
- **Plan**: `production_audit.md` (H31)
- **Lines**: ~120
- **Depends on**: M036, M037, M038, M039, M040, M043
- **Tests**: All 8 tools accessible from orchestrator
- **Blocks**: M049

### M045: exec_causal_intervention — proper shell escaping (H32) ❌ NOT_STARTED
- **What**: Replace inadequate string escaping (only `'` → `\'`) with proper shell quoting; use shlex.quote or equivalent
- **Files**: `bugswarm-sandbox/src/container.rs`
- **Plan**: `production_audit.md` (H32)
- **Lines**: ~30
- **Depends on**: M001
- **Tests**: `tests/unit/test_causal_intervention.py::test_proper_shell_escaping`
- **Blocks**: none

### M046: 16 unsafe block safety documentation (H15) ❌ NOT_STARTED
- **What**: Wrap ALL unsafe blocks with `// SAFETY:` comments documenting preconditions; add debug_assert! guards; CI enforces
- **Files**: `bugswarm-sandbox/src/danger_map.rs` (16 unsafe blocks)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~60
- **Depends on**: none
- **Tests**: CI script `ci/check_safety_comments.sh` — every unsafe block has safety comment
- **Blocks**: M048

### M047: shm_map error type — String → anyhow::Error (H41) ❌ NOT_STARTED
- **What**: Change `shm_map` return type from `Result<*mut u8, String>` to `Result<*mut u8, anyhow::Error>`; update all callers
- **Files**: `bugswarm-sandbox/src/danger_map.rs`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: `tests/unit/test_danger_map.rs::test_shm_map_error_is_anyhow`
- **Blocks**: M048

### M048: Danger map SHM infrastructure (H29) ❌ NOT_STARTED
- **What**: Danger map shared-memory path has no SHM setup; add shm_open/shm_unlink infrastructure; proper cleanup
- **Files**: `bugswarm-sandbox/src/danger_map.rs`
- **Plan**: `production_audit.md` (H29)
- **Lines**: ~80
- **Depends on**: M046, M047
- **Tests**: `tests/unit/test_danger_map_shm.py` — SHM segment created and cleaned up
- **Blocks**: none

### M049: Circuit breaker for inter-daemon communication (H14) ❌ NOT_STARTED
- **What**: Track danger map loading failures; circuit opens after 3 consecutive failures; fuzzer falls back to coverage-only mode; circuit closes on successful load
- **Files**: `bugswarm-sandbox/src/fuzzer.rs`, `bugswarm-sandbox/src/container.rs`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2)
- **Lines**: ~80
- **Depends on**: M044
- **Tests**: `tests/integration/test_circuit_breaker.py` — opens after 3 failures, closes on success, fuzzer continues in degraded mode
- **Blocks**: none

### M050: Rust dependency upper-bounding (H33) ❌ NOT_STARTED
- **What**: Upper-bound ALL workspace dependencies to `<NEXT_MAJOR`; lock with `>=X, <Y` format
- **Files**: Root `Cargo.toml` `[workspace.dependencies]`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: `cargo update --dry-run` produces zero changes to Cargo.lock
- **Blocks**: none

### M051: CHANGELOG.md creation (H35) ❌ NOT_STARTED
- **What**: Create CHANGELOG.md with all phase completion entries
- **Files**: `CHANGELOG.md` (NEW)
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~80
- **Depends on**: none
- **Tests**: File exists in repo root; CI verifies
- **Blocks**: none

### M052: Regex::new() → Lazy<Regex> in differential.rs (H37) ❌ NOT_STARTED
- **What**: Replace per-call Regex::new().unwrap() with once_cell::sync::Lazy<Regex> — compile once at startup
- **Files**: `bugswarm-sandbox/src/differential.rs`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 1)
- **Lines**: ~20
- **Depends on**: none
- **Tests**: `tests/unit/test_differential.rs::test_regex_compiled_once`
- **Blocks**: none

---

## Batch 3: Detection Expansion — CWE Coverage & Languages (35 milestones, est. 6 weeks)

### M053: `bugswarm-scanner` crate — trait + registry + orchestrator ❌ NOT_STARTED
- **What**: Create new crate with `BugCategoryScanner` trait, `ScannerRegistry` (dynamic module loading), `ScanOrchestrator` (parallel execution), `Finding/Proof/Severity` types
- **Files**: `bugswarm-scanner/Cargo.toml` (NEW), `bugswarm-scanner/src/` (NEW)
- **Plan**: `q1_all_bug_coverage.md` (Section 3)
- **Lines**: ~500
- **Depends on**: none
- **Tests**: Scanner registry loads modules, orchestrator runs parallel scans
- **Blocks**: M054-M062

### M054: CWE taxonomy database — TOML rule table ❌ NOT_STARTED
- **What**: Implement CWE taxonomy in TOML — 104+ CWE entries with detection method, proof type, FPR/TTD targets; loader into scanner
- **Files**: `bugswarm-scanner/src/taxonomy/cwe_table.toml` (NEW), `bugswarm-scanner/src/taxonomy/cwe.rs` (NEW)
- **Plan**: `q1_all_bug_coverage.md` (Section 2.3)
- **Lines**: ~400
- **Depends on**: M053
- **Tests**: All 104 CWEs load without error; CWE-to-scanner mapping verified
- **Blocks**: M055-M062

### M055: SAST Memory Safety Scanner (CWE-119 family, 57 CWEs) ❌ NOT_STARTED
- **What**: Implement scanner covering CWE-119/120/121/122/123/124/125/126/127/129/130/131/134/170/190/191/193/194/195/196/197/242/252/337/338/401/404/415/416/456/457/467/468/476/480/481/483/561/562/570/571/590/628/674/676/680/690/704/754/758/761/762/787/788/805/806/822/823/824/825/835/843
- **Files**: `bugswarm-scanner/src/scanners/sast_memory.rs` (NEW)
- **Plan**: `q1_all_bug_coverage.md` (Section 2.2.1)
- **Lines**: ~400
- **Depends on**: M053, M054
- **Tests**: Juliet test suite memory safety categories; >90% detection rate
- **Blocks**: M070

### M056: SAST Injection Scanner (CWE-74/77/89 family, 28 CWEs) ❌ NOT_STARTED
- **What**: Implement scanner covering CWE-78/79/89/90/91/93/94/95/98/113/117/434/470/502/601/611/643/652/776/917/918/943/1236/1321/1336 and variants
- **Files**: `bugswarm-scanner/src/scanners/sast_injection.rs` (NEW)
- **Plan**: `q1_all_bug_coverage.md` (Section 2.2.2)
- **Lines**: ~400
- **Depends on**: M053, M054
- **Tests**: OWASP Benchmark injection categories
- **Blocks**: M070

### M057: SAST Auth Scanner (CWE-287 family, 43 CWEs) ❌ NOT_STARTED
- **What**: Implement scanner covering CWE-284/285/287/295/297/306/307/319/326/327/328/329/330/345/346/347/352/359/362/384/521/522/523/525/548/549/613/614/620/639/640/647/653/654/798/804/862/863/942/1004/1151/1167/1171/1184/1185/1200/1212/1213/1220/1226/1249/1312/1422
- **Files**: `bugswarm-scanner/src/scanners/sast_auth.rs` (NEW)
- **Plan**: `q1_all_bug_coverage.md` (Section 2.2.3)
- **Lines**: ~400
- **Depends on**: M053, M054
- **Tests**: Juliet auth categories
- **Blocks**: M070

### M058: SAST Information Disclosure Scanner (CWE-200 family, 46 CWEs) ❌ NOT_STARTED
- **What**: Implement scanner covering CWE-200 through CWE-1423 (info disclosure, log exposure, error messages, metadata, cache)
- **Files**: `bugswarm-scanner/src/scanners/sast_info_disclosure.rs` (NEW)
- **Plan**: `q1_all_bug_coverage.md` (Section 2.2.4)
- **Lines**: ~400
- **Depends on**: M053, M054
- **Tests**: Info disclosure test suite
- **Blocks**: M070

### M059: SAST Cryptographic Weakness Scanner ❌ NOT_STARTED
- **What**: Detect weak crypto: MD5, SHA1, RC4, DES, ECB mode, static IVs, weak randomness, hardcoded keys, missing HMAC
- **Files**: `bugswarm-scanner/src/scanners/sast_crypto.rs` (NEW)
- **Plan**: `q1_all_bug_coverage.md` (Section 2.2.3)
- **Lines**: ~250
- **Depends on**: M053, M054
- **Tests**: Crypto weakness corpus
- **Blocks**: M070

### M060: SAST CodeQL-like Semantic Queries (BSQL language) ❌ NOT_STARTED
- **What**: Implement BSQL query language — parser, compiler (BSQL→CPG query), runtime engine; 8 rule files (injection, auth, crypto, info_disclosure, resource, input_validation, concurrency, mobile)
- **Files**: `bugswarm-scanner/src/queries/` (8 NEW files)
- **Plan**: `q1_all_bug_coverage.md` (Section 3.1)
- **Lines**: ~800
- **Depends on**: M053
- **Tests**: BSQL queries against known-vulnerable codebases
- **Blocks**: M070

### M061: DAST HTTP Scanner — endpoint probing + API fuzzer ❌ NOT_STARTED
- **What**: HTTP endpoint discovery, OpenAPI/GraphQL schema-driven fuzzer, auth/session testing, TLS scanner, CORS tester, request smuggling detector
- **Files**: `bugswarm-scanner/src/dast/` (7 NEW files)
- **Plan**: `q1_all_bug_coverage.md` (Section 3.1)
- **Lines**: ~1200
- **Depends on**: M053
- **Tests**: OWASP ZAP benchmark runs; test against local target apps
- **Blocks**: M070

### M062: Config Scanner — Dockerfile/K8s/IAM rules ❌ NOT_STARTED
- **What**: Dockerfile AST parser, K8s manifest YAML parser, cloud IAM policy analyzer; TOML rule databases
- **Files**: `bugswarm-scanner/src/config/` (6 NEW files)
- **Plan**: `q1_all_bug_coverage.md` (Section 3.1)
- **Lines**: ~800
- **Depends on**: M053
- **Tests**: Known-misconfigured Dockerfiles/K8s manifests detected
- **Blocks**: M070

### M063: SCA — Dependency Vulnerability Analyzer ❌ NOT_STARTED
- **What**: SBOM generation (CycloneDX/SPDX), vulnerability advisory DB client, cargo-audit/npm-audit/pip-audit/maven/gem integration
- **Files**: `bugswarm-scanner/src/sca/` (6 NEW files)
- **Plan**: `q1_all_bug_coverage.md` (Section 3.1)
- **Lines**: ~600
- **Depends on**: M053
- **Tests**: Known-vulnerable dependencies flagged
- **Blocks**: M070

### M064: CPG Tier 1 — 11 languages full support (AST+CFG+CallGraph+Taint+SSA) ❌ NOT_STARTED
- **What**: Implement per-language parsers in `bugswarm-cpg/src/parsers/` for Python, JavaScript, TypeScript, C, C++, Rust, Go, Java, C#, Ruby, PHP with AST, CFG, call graph, taint, SSA, types, mutation operators
- **Files**: `bugswarm-cpg/src/parsers/{python,javascript,typescript,c,cpp,rust,go,java,csharp,ruby,php}/` (77 NEW files)
- **Plan**: `q4_100_languages.md` (Section 3), `bugswarm_mythos_gap_analysis.md` (Gap 5)
- **Lines**: ~3000
- **Depends on**: none
- **Tests**: Juliet test suite per language; parse real open-source codebases
- **Blocks**: M065, M066

### M065: CPG Tier 2 — 15 languages AST+CallGraph+BasicTaint ❌ NOT_STARTED
- **What**: Kotlin, Swift, Scala, Dart, Lua, Perl, R, Julia, Haskell, Elixir, Erlang, Clojure, OCaml, Groovy, Zig parsers
- **Files**: `bugswarm-cpg/src/parsers/{kotlin,swift,scala,dart,lua,perl,r,julia,haskell,elixir,erlang,clojure,ocaml,groovy,zig}/` (60 NEW files)
- **Plan**: `q4_100_languages.md` (Section 1.2)
- **Lines**: ~1200
- **Depends on**: M064 (common traits)
- **Tests**: Parser test per language with sample source files
- **Blocks**: M070

### M066: CPG Tier 3 — 80+ languages AST+CallGraph only ❌ NOT_STARTED
- **What**: Bash, Make, CMake, Dockerfile, YAML, JSON, TOML, XML, HTML, CSS, SCSS, SQL, Verilog, VHDL, Fortran, Ada, Solidity, Nix, HCL, PowerShell, +60 more parsers
- **Files**: `bugswarm-cpg/src/parsers/t3_parsers/` (80+ NEW files)
- **Plan**: `q4_100_languages.md` (Section 1.3)
- **Lines**: ~4000
- **Depends on**: M064 (common traits)
- **Tests**: Parser test per language; smoketest parse on sample files
- **Blocks**: M070

### M067: Chain detector — universal bug category expansion ❌ NOT_STARTED
- **What**: Expand EffectType/PreconditionType enums from 7 to 15 variants (add Crash, ResourceExhaustion, DataCorruption, Deadlock, PerformanceDegradation, Timeout, Incompatibility, Regression, Inconsistency, Bloat); add ChainType enum; category-aware severity calculus; expand keyword extraction
- **Files**: `bugswarm-evidence/src/chain.rs`
- **Plan**: `chain_universal_expansion.md`
- **Lines**: ~200
- **Depends on**: none
- **Tests**: 10+ new cross-category chain tests; all 13 existing tests still pass
- **Blocks**: M070

### M068: Taint model — Python full source/sink/sanitizer definitions ❌ NOT_STARTED
- **What**: Complete Python taint model with 30+ sources (socket, urllib, requests, flask, django, fastapi, pickle, yaml, json, subprocess, multiprocessing, redis, kafka, boto3, websocket), 20+ sinks, 15+ sanitizers, propagation rules
- **Files**: `bugswarm-cpg/src/parsers/python/taint.rs`
- **Plan**: `q4_100_languages.md` (Section 3.4.1)
- **Lines**: ~250
- **Depends on**: M064
- **Tests**: Taint paths detected in known-vulnerable Python codebases
- **Blocks**: M070

### M069: Taint model — JavaScript full source/sink/sanitizer definitions ❌ NOT_STARTED
- **What**: Complete JS taint model with 30+ browser/Node.js sources, 20+ sinks, propagation rules; Express/React/Vue framework support
- **Files**: `bugswarm-cpg/src/parsers/javascript/taint.rs`
- **Plan**: `q4_100_languages.md` (Section 3.4.2)
- **Lines**: ~250
- **Depends on**: M064
- **Tests**: Taint paths detected in known-vulnerable JS codebases
- **Blocks**: M070

### M070: Full pipeline integration test — all scanners + daemons ❌ NOT_STARTED
- **What**: End-to-end integration test spanning CPG indexing → taint analysis → scanner detection → sandbox execution → evidence graph → report; cross-language coverage
- **Files**: `tests/integration/test_full_pipeline.py` (NEW)
- **Plan**: Cross-document integration requirement
- **Lines**: ~300
- **Depends on**: M008, M055-M062, M064-M069
- **Tests**: This IS the test
- **Blocks**: M071 (benchmark)

### M071: Real-world benchmarks — OSS-Fuzz + Juliet + nginx/Redis/SQLite (Gap 6) ❌ NOT_STARTED
- **What**: Run BugSwarm against OSS-Fuzz corpus (1000 repos), Juliet test suite (100K bugs), real-world targets; measure bugs found, FPR, time to first bug, cost per bug
- **Files**: `tests/benchmarks/` (NEW directory)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 6)
- **Lines**: ~200 (test scripts + harness)
- **Depends on**: M070
- **Tests**: This IS the test; metric: >80% Juliet detection
- **Blocks**: M107 (fine-tuning)

---

## Batch 4: Enterprise Infrastructure — Resilience & Monitoring (22 milestones, est. 4 weeks)

### M072: API keys — secrets manager with env+file providers (H13) ❌ NOT_STARTED
- **What**: Add `BGSWARM_SECRETS_PROVIDER` env var with `env` and `file` (Docker secrets) backends; constant-time comparison
- **Files**: `bugswarm-gateway/src/gateway/types.py`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2)
- **Lines**: ~60
- **Depends on**: none
- **Tests**: `tests/unit/test_secrets.py` — env source, file source, missing file raises
- **Blocks**: M070

### M073: Seccomp profile — explicit deny for fork/clone (H16) ❌ NOT_STARTED
- **What**: Add explicit DENY block for fork, clone, clone3, vfork, execveat, kexec_load; verify prlimit64; add documentation comments
- **Files**: `bugswarm-sandbox/seccomp/default.json`
- **Plan**: `phase30b_high_fix_plan.md` (Batch 2), CRIT-PRE4
- **Lines**: ~40
- **Depends on**: none
- **Tests**: `tests/integration/test_seccomp.py::test_python_startup_under_seccomp`, `test_fork_blocked`
- **Blocks**: M070

### M074: Alerting engine — wire to Slack/PagerDuty (H19) ❌ NOT_STARTED
- **What**: Alerting engine exists but never wired to Slack/PagerDuty; add notification backends with webhook configuration
- **Files**: `bugswarm-orchestrator/src/alerter.rs`, `bugswarm-orchestrator/src/notifications/` (NEW)
- **Plan**: `production_audit.md` (H19), `phase30b_high_fix_plan.md` (Batch 4)
- **Lines**: ~200
- **Depends on**: M012, M013, M014 (metrics endpoints)
- **Tests**: `tests/unit/test_alerting.py` — alert fires on threshold breach; webhook POST received
- **Blocks**: M070

### M075: OpenTelemetry distributed tracing (H18) ❌ NOT_STARTED
- **What**: Add OpenTelemetry spans across all daemon boundaries; trace IDs pass through request headers; export to OTLP collector
- **Files**: All daemon main.rs, `bugswarm-sandbox/src/daemon.rs`, `bugswarm-cpg/src/daemon.rs`, `bugswarm-evidence/src/daemon.rs`
- **Plan**: `production_audit.md` (H18), `phase30b_high_fix_plan.md` (Batch 4)
- **Lines**: ~200
- **Depends on**: M017
- **Tests**: `tests/integration/test_tracing.py` — trace spans visible in collector; correlation IDs match across daemons
- **Blocks**: M070

### M076: Trace/correlation IDs between daemons (H20) ❌ NOT_STARTED
- **What**: Generate X-Request-ID at gateway; propagate through all daemon calls; log at each service boundary
- **Files**: All daemon.rs files, gateway middleware
- **Plan**: `production_audit.md` (H20), `phase30b_high_fix_plan.md` (Batch 4)
- **Lines**: ~80
- **Depends on**: M075
- **Tests**: Single user request → same trace ID in all 4 daemon logs
- **Blocks**: M070

### M077: Daemon stdout → structured file logging + rotation (M3) ❌ NOT_STARTED
- **What**: Add file logging with rotation (tracing-appender), syslog option; daemons currently log to stdout only
- **Files**: All 3 daemon main.rs
- **Plan**: `production_audit.md` (M1, M3)
- **Lines**: ~60
- **Depends on**: M015 (unified config for log paths)
- **Tests**: Log file created, rotated at size limit, old files gzipped
- **Blocks**: M070

### M078: Stale socket cleanup on startup (M4) ❌ NOT_STARTED
- **What**: Remove stale socket file on daemon startup with PID lockfile check to detect running instance
- **Files**: All 3 daemon main.rs
- **Plan**: `production_audit.md` (M4)
- **Lines**: ~40
- **Depends on**: M022 (graceful shutdown so stale sockets are rare)
- **Tests**: Start daemon, kill -9, restart — new socket works; two concurrent instances detected
- **Blocks**: M070

### M079: PID file management (M13) ❌ NOT_STARTED
- **What**: Write PID file on startup; remove on clean shutdown; handle stale PID from SIGKILL
- **Files**: All 3 daemon main.rs
- **Plan**: `production_audit.md` (M13)
- **Lines**: ~40
- **Depends on**: M022
- **Tests**: PID file exists during daemon lifetime, removed on clean shutdown
- **Blocks**: M021 (systemd units need PID files)

### M080: Multiple instance mutual exclusion (M26) ❌ NOT_STARTED
- **What**: Use file lock on socket path to prevent multiple daemon instances binding same port
- **Files**: All 3 daemon main.rs
- **Plan**: `production_audit.md` (M26)
- **Lines**: ~30
- **Depends on**: M078
- **Tests**: Second instance fails with clear error when first instance running
- **Blocks**: M070

### M081: NodeKind::Agent, CodeLocation creation in daemon handlers (M14) ❌ NOT_STARTED
- **What**: Evidence graph NodeKind::Agent and CodeLocation are defined but never created in daemon handlers; implement node creation during agent operations
- **Files**: `bugswarm-evidence/src/daemon.rs`, `bugswarm-evidence/src/graph.rs`
- **Plan**: `production_audit.md` (M14)
- **Lines**: ~60
- **Depends on**: M027
- **Tests**: Agent nodes appear in evidence graph after agent scan
- **Blocks**: M070

### M082: EdgeKind::Authored, Enables, ConditionEquivalent usage (M15) ❌ NOT_STARTED
- **What**: These edge types defined but never used; wire into chain detection, fix impact prediction, and agent trace tracking
- **Files**: `bugswarm-evidence/src/graph.rs`
- **Plan**: `production_audit.md` (M15)
- **Lines**: ~50
- **Depends on**: M027
- **Tests**: Edges created and traversable in evidence graph queries
- **Blocks**: M067

### M083: Swarm evidence population — use Rust EvidenceClient, not PatternDB (M16) ❌ NOT_STARTED
- **What**: Swarm currently uses Python PatternDB for evidence; switch to Rust EvidenceClient for consistency
- **Files**: `bugswarm-agent/src/agent/swarm.py`
- **Plan**: `production_audit.md` (M16)
- **Lines**: ~80
- **Depends on**: M020
- **Tests**: Evidence population tests — same data via Rust client
- **Blocks**: M070

### M084: Docker image reference pinning — no `:latest` (M1) ❌ NOT_STARTED
- **What**: Replace all `:latest` Docker image references with version-pinned tags (e.g., `bugswarm/sandbox-base:1.0.0`)
- **Files**: All Dockerfiles, `docker-compose.yml`, container.rs
- **Plan**: `production_audit.md` (M1)
- **Lines**: ~20
- **Depends on**: M008, M019
- **Tests**: `docker-compose.yml` uses explicit version tags; no `:latest`
- **Blocks**: M070

### M085: Fuzz corpus directory pre-creation (M12) ❌ NOT_STARTED
- **What**: Docker create_container fails if corpus dirs missing; pre-create `/fuzz/corpus/`, `/fuzz/crashes/` on container start
- **Files**: `bugswarm-sandbox/src/container.rs`
- **Plan**: `production_audit.md` (M12)
- **Lines**: ~20
- **Depends on**: M028
- **Tests**: First fuzz run creates directories successfully
- **Blocks**: M070

### M086: Seccomp JSON metacharacter risk (M24) ❌ NOT_STARTED
- **What**: Seccomp JSON passed as Docker CLI option — validate JSON before passing; prevent metacharacter injection
- **Files**: `bugswarm-sandbox/src/container.rs`
- **Plan**: `production_audit.md` (M24)
- **Lines**: ~20
- **Depends on**: M073
- **Tests**: Malicious seccomp JSON rejected
- **Blocks**: M070

### M087: core.py Exception handling — specific error types (M27) ❌ NOT_STARTED
- **What**: Replace catch-all `except Exception` with specific error types; distinguish infrastructure failures from security failures
- **Files**: `bugswarm-agent/src/agent/core.py`
- **Plan**: `production_audit.md` (M27)
- **Lines**: ~40
- **Depends on**: none
- **Tests**: Security errors logged at WARN, infra errors at ERROR
- **Blocks**: M070

### M088: fuzzer.rs parse errors default to 0s (M28) ❌ NOT_STARTED
- **What**: `fuzzer.rs:1016` parse errors default to 0s — corrupted stats possible; handle parse errors explicitly, log warnings
- **Files**: `bugswarm-sandbox/src/fuzzer.rs:1016`
- **Plan**: `production_audit.md` (M28)
- **Lines**: ~15
- **Depends on**: none
- **Tests**: Corrupted AFL stats output → logged warning, previous values retained
- **Blocks**: M070

### M089: container.rs no auto-remove on stats/debug paths (M29) ❌ NOT_STARTED
- **What**: `container.rs:304` — containers on stats/debug paths never auto-removed; add auto-remove or explicit cleanup
- **Files**: `bugswarm-sandbox/src/container.rs:304`
- **Plan**: `production_audit.md` (M29)
- **Lines**: ~15
- **Depends on**: none
- **Tests**: Container removed after stats collection
- **Blocks**: M070

### M090: diff_execute broken CLI command (M30) ❌ NOT_STARTED
- **What**: diff_execute client sends CLI command that doesn't exist; fix command generation
- **Files**: `bugswarm-agent/src/agent/cli/diff.py`
- **Plan**: `production_audit.md` (M30)
- **Lines**: ~15
- **Depends on**: M039
- **Tests**: `diff_execute` runs without "command not found"
- **Blocks**: M070

### M091: Python package lock files (L9) 🔶 PARTIAL
- **What**: Add lock files for Python transitive deps; pin versions in pyproject.toml
- **Files**: `bugswarm-agent/pyproject.toml`, `bugswarm-gateway/pyproject.toml`, `bugswarm-swarm/pyproject.toml`
- **Plan**: `production_audit.md` (L9, L18)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: `pip install --require-hashes` succeeds
- **Blocks**: M070

### M092: Token counting — replace len//4 fallback with real tokenizer (L13) ❌ NOT_STARTED
- **What**: Replace inaccurate `len//4` fallback with proper tiktoken-based token counting for non-English text
- **Files**: `bugswarm-gateway/src/gateway/tokenizer.py`
- **Plan**: `production_audit.md` (L13)
- **Lines**: ~30
- **Depends on**: none
- **Tests**: Token count within 5% of actual tiktoken count
- **Blocks**: M070

### M093: Container removal errors propagated (L16) ✅ DONE
- **What**: Container remove failures properly propagated instead of silently swallowed
- **Files**: `bugswarm-sandbox/src/container.rs`
- **Plan**: `production_audit.md` (L16), `phase30b_high_fix_plan.md` (H36 batch note)
- **Lines**: ~10
- **Depends on**: none
- **Tests**: Verified in Phase 30a
- **Blocks**: none

---

## Batch 5: Platform — API, Frontend, GitHub, VS Code (32 milestones, est. 6 weeks)

### M094: FastAPI backend — `bugswarm-api/` creation ❌ NOT_STARTED
- **What**: Create FastAPI server with project structure, config, middleware, models, schemas, routers, services, tasks, integrations
- **Files**: `bugswarm-api/` (ENTIRE NEW directory, ~50 files)
- **Plan**: `q5_zero_cli_platform.md` (Section 2)
- **Lines**: ~5000
- **Depends on**: M008 (docker-compose for infrastructure deps), M016 (unified Python config)
- **Tests**: `tests/test_scan.py`, `tests/test_auth.py`, full API test suite
- **Blocks**: M095-M100

### M095: POST /api/v1/scan — submit scan job ❌ NOT_STARTED
- **What**: Scan submission endpoint — validates GitHub URL, creates scan record in Postgres, enqueues Celery task, returns scan_id
- **Files**: `bugswarm-api/app/routers/scan.py`, `bugswarm-api/app/schemas/scan.py`, `bugswarm-api/app/services/scan_service.py`
- **Plan**: `q5_zero_cli_platform.md` (Section 2.2)
- **Lines**: ~150
- **Depends on**: M094
- **Tests**: Submit valid URL → 201 + scan_id; invalid URL → 422; rate limited → 429
- **Blocks**: M096

### M096: WebSocket real-time scan stream ❌ NOT_STARTED
- **What**: WebSocket endpoint for live scan progress; Redis Pub/Sub channel per scan; authenticated WebSocket with JWT query param
- **Files**: `bugswarm-api/app/routers/scan.py`, `bugswarm-api/app/services/websocket_service.py`
- **Plan**: `q5_zero_cli_platform.md` (Section 2.2 — WS endpoint)
- **Lines**: ~200
- **Depends on**: M095
- **Tests**: WebSocket connects with valid token; receives live progress events
- **Blocks**: M100 (frontend real-time)

### M097: Authentication — JWT + API key + GitHub OAuth ❌ NOT_STARTED
- **What**: Three auth methods: Bearer JWT (web), X-API-Key (CI/CD), X-GitHub-Token (GitHub App); RS256 asymmetric; session revocation
- **Files**: `bugswarm-api/app/middleware/auth.py`, `bugswarm-api/app/services/auth_service.py`
- **Plan**: `q5_zero_cli_platform.md` (Section 2.3)
- **Lines**: ~300
- **Depends on**: M094
- **Tests**: JWT verification, API key lookup, OAuth flow, session revocation
- **Blocks**: M098

### M098: Tiered rate limiting ❌ NOT_STARTED
- **What**: Per-user, per-IP, per-repo rate limiting; four tiers (free/pro/team/enterprise); Redis-backed sliding windows; concurrent scan limits
- **Files**: `bugswarm-api/app/middleware/rate_limit.py`
- **Plan**: `q5_zero_cli_platform.md` (Section 2.4)
- **Lines**: ~200
- **Depends on**: M097
- **Tests**: Free tier limited to 5/hr; upgrade to pro raises limit; concurrent scan enforcement
- **Blocks**: M095

### M099: Celery task pipeline — clone→CPG→detect→exploit→report→notify ❌ NOT_STARTED
- **What**: Celery workflow with 6-stage chain; per-task progress publishing via Redis; queue routing per task type; task time limits
- **Files**: `bugswarm-api/app/tasks/` (7 NEW files)
- **Plan**: `q5_zero_cli_platform.md` (Section 2.5)
- **Lines**: ~600
- **Depends on**: M094
- **Tests**: `tests/test_celery_tasks.py` — full chain runs, progress events published
- **Blocks**: M095

### M100: S3-compatible artifact storage ❌ NOT_STARTED
- **What**: MinIO/S3 client for scan artifacts; bucket layout per user/scan; SARIF/JSON/PDF/repo archives; presigned URLs; SSE encryption
- **Files**: `bugswarm-api/app/db/s3.py`
- **Plan**: `q5_zero_cli_platform.md` (Section 2.6)
- **Lines**: ~150
- **Depends on**: M094
- **Tests**: Store and retrieve artifacts; presigned URL generation
- **Blocks**: M095

### M101: React frontend — `bugswarm-web/` project setup ❌ NOT_STARTED
- **What**: React 18 + TypeScript + Tailwind + Vite; component library; router; API client; auth store; scan store; WebSocket hook
- **Files**: `bugswarm-web/` (ENTIRE NEW directory, ~80 files)
- **Plan**: `q5_zero_cli_platform.md` (Section 3)
- **Lines**: ~10000
- **Depends on**: M094, M095, M096
- **Tests**: Unit tests for components; integration with API
- **Blocks**: M102-M106

### M102: Landing page — GitHub URL input + paste zone ❌ NOT_STARTED
- **What**: Hero section with GitHub URL input; drag-and-drop ZIP upload; "Connect GitHub" OAuth flow; scan submission
- **Files**: `bugswarm-web/src/pages/LandingPage.tsx`, `bugswarm-web/src/components/landing/`
- **Plan**: `q5_zero_cli_platform.md` (Section 3.2)
- **Lines**: ~300
- **Depends on**: M101
- **Tests**: Valid URL → navigates to scan detail; invalid URL → error shown
- **Blocks**: M103

### M103: Real-time bug heatmap (D3.js) ❌ NOT_STARTED
- **What**: Interactive heatmap showing bug distribution across files/lines; severity color coding; click-to-navigate
- **Files**: `bugswarm-web/src/components/dashboard/HeatmapWidget.tsx`
- **Plan**: `q5_zero_cli_platform.md` (Section 3.3)
- **Lines**: ~200
- **Depends on**: M101
- **Tests**: Component renders with sample findings; click navigates to file
- **Blocks**: M105

### M104: Code viewer with bug annotations ❌ NOT_STARTED
- **What**: Syntax-highlighted code viewer (shiki); line-level bug annotations with severity colors; taint path overlay; file tree navigator
- **Files**: `bugswarm-web/src/components/codeviewer/`
- **Plan**: `q5_zero_cli_platform.md` (Section 3.4)
- **Lines**: ~400
- **Depends on**: M101
- **Tests**: Renders code with annotations; clicking finding highlights lines
- **Blocks**: M105

### M105: Scan detail page — real-time progress + findings table ❌ NOT_STARTED
- **What**: Real-time scan progress with stage tracking; sortable/filterable findings table; severity breakdown donut; exploit chain visualization
- **Files**: `bugswarm-web/src/pages/ScanDetailPage.tsx`, `bugswarm-web/src/components/scan/`
- **Plan**: `q5_zero_cli_platform.md` (Section 3.1 structure)
- **Lines**: ~500
- **Depends on**: M096, M101, M103, M104
- **Tests**: Page loads scan data; WebSocket updates progress bar; findings filterable
- **Blocks**: none

### M106: Dashboard page — org metrics + scan history ❌ NOT_STARTED
- **What**: Severity breakdown chart; finding timeline; language breakdown; top CWEs bar chart; scan history list; org metrics widget
- **Files**: `bugswarm-web/src/pages/DashboardPage.tsx`, `bugswarm-web/src/components/dashboard/`
- **Plan**: `q5_zero_cli_platform.md` (Section 3.1 structure)
- **Lines**: ~400
- **Depends on**: M094, M101
- **Tests**: Dashboard renders with sample data; time range filtering works
- **Blocks**: none

### M107: Report export — SARIF + JSON + PDF ❌ NOT_STARTED
- **What**: SARIF v2.1.0 export; JSON summary with remediation steps; PDF report with executive summary; custom report configuration UI
- **Files**: `bugswarm-web/src/components/reports/`, `bugswarm-api/app/services/report_service.py`
- **Plan**: `q5_zero_cli_platform.md` (Section 2.1 endpoints)
- **Lines**: ~400
- **Depends on**: M094, M101
- **Tests**: SARIF validates against schema; PDF generated with expected sections
- **Blocks**: none

### M108: Settings page — API keys + integrations + notifications ❌ NOT_STARTED
- **What**: API key generation/management UI; GitHub/Slack/Discord integration settings; notification preferences; scheduled scan config; team management
- **Files**: `bugswarm-web/src/pages/SettingsPage.tsx`, `bugswarm-web/src/components/settings/`
- **Plan**: `q5_zero_cli_platform.md` (Section 3.1 structure)
- **Lines**: ~400
- **Depends on**: M094, M097, M101
- **Tests**: API key created and displayed; Slack webhook saved; notification preferences persisted
- **Blocks**: none

### M109: GitHub App registration + manifest (q6) ❌ NOT_STARTED
- **What**: Register GitHub App with permissions (contents:read, PR:write, issues:write, checks:write, security_events:write); manifest.json; installation flow
- **Files**: `bugswarm-github/src/` (NEW crate), `bugswarm-github/manifest.json` (NEW)
- **Plan**: `q6_github_integration.md` (Sections 1, 2)
- **Lines**: ~800
- **Depends on**: M094 (API for webhook endpoint)
- **Tests**: App installs on test org; installation token obtained; webhook received
- **Blocks**: M110-M117

### M110: Webhook handler — signature verification + event dispatch ❌ NOT_STARTED
- **What**: HMAC-SHA256 verification; event type routing; async background processing; rate limiting per installation; payload size limits
- **Files**: `bugswarm-github/src/webhook/` (6 NEW files)
- **Plan**: `q6_github_integration.md` (Section 1.2)
- **Lines**: ~500
- **Depends on**: M109
- **Tests**: Valid signature → 200; invalid signature → 401; event dispatched to correct handler
- **Blocks**: M111-M114

### M111: PR review — inline comments at bug locations ❌ NOT_STARTED
- **What**: PR opened/synchronize → scan changed files → post inline review comments at exact lines; check run lifecycle; changed-file caching
- **Files**: `bugswarm-github/src/webhook/handlers/pull_request.rs`
- **Plan**: `q6_github_integration.md` (Sections 3.1, 5)
- **Lines**: ~400
- **Depends on**: M110
- **Tests**: PR opened on test repo → inline comments appear; cache hit avoids re-scan
- **Blocks**: M118

### M112: Push event — protected branch scanning ❌ NOT_STARTED
- **What**: Push to protected branch → full/range scan based on first/subsequent push; update commit status; SARIF upload
- **Files**: `bugswarm-github/src/webhook/handlers/push.rs`
- **Plan**: `q6_github_integration.md` (Section 3.2)
- **Lines**: ~300
- **Depends on**: M110
- **Tests**: Push to main → scan triggered; commit status updated; SARIF uploaded
- **Blocks**: M118

### M113: Issue auto-creation from findings ❌ NOT_STARTED
- **What**: Confirmed findings (severity≥5, confidence≥0.8) → auto-create GitHub Issue with template; auto-close on fix; stale management
- **Files**: `bugswarm-github/src/webhook/handlers/issues.rs`, `bugswarm-github/src/github_api/issues.rs`
- **Plan**: `q6_github_integration.md` (Section 6)
- **Lines**: ~400
- **Depends on**: M110, M095
- **Tests**: Finding confirmed → issue created; fix detected → issue auto-closed
- **Blocks**: M118

### M114: SARIF upload to GitHub Code Scanning ❌ NOT_STARTED
- **What**: Convert BugSwarm findings to SARIF v2.1.0; upload via Code Scanning API; severity mapping; deduplication by partialFingerprints
- **Files**: `bugswarm-github/src/sarif/` (3 NEW files)
- **Plan**: `q6_github_integration.md` (Section 4)
- **Lines**: ~500
- **Depends on**: M110
- **Tests**: Valid SARIF uploads successfully; alerts appear in Security tab; fix commit closes alert
- **Blocks**: M118

### M115: GitHub Action — `bugswarm-action@v1` ❌ NOT_STARTED
- **What**: Reusable GitHub Action: download CLI, run scan, upload SARIF, post PR comment; fail_on threshold; paths filter; max_minutes budget
- **Files**: `action.yml` (NEW at repo root)
- **Plan**: `q6_github_integration.md` (Section 8)
- **Lines**: ~200
- **Depends on**: M111, M114
- **Tests**: Action runs in test workflow; SARIF uploaded; PR comment posted
- **Blocks**: M118

### M116: Per-repository config — `.github/bugswarm.yml` ❌ NOT_STARTED
- **What**: YAML config for scan rules, issue creation, PR review, code scanning, CI check, notifications, rule overrides, budget
- **Files**: `bugswarm-github/src/config_file/` (3 NEW files)
- **Plan**: `q6_github_integration.md` (Section 8.2)
- **Lines**: ~300
- **Depends on**: M109
- **Tests**: Config parsed and applied; invalid config rejected with clear error
- **Blocks**: M111

### M117: GitHub Marketplace listing ❌ NOT_STARTED
- **What**: Listing draft; pricing plans (free/pro/team/enterprise); marketplace_purchase webhook handler; logo/banner/screenshots
- **Files**: `bugswarm-github/src/webhook/handlers/marketplace.rs`, listing assets
- **Plan**: `q6_github_integration.md` (Section 9)
- **Lines**: ~200
- **Depends on**: M109
- **Tests**: Purchase webhook provisions access; cancellation schedules revocation
- **Blocks**: M118

### M118: GitHub Enterprise Server (GHES) support ❌ NOT_STARTED
- **What**: Custom GitHub API base URL; on-premise webhook relay for air-gapped GHES; self-signed cert management; GHES rate limit adaptation
- **Files**: `bugswarm-github/src/enterprise/ghes.rs` (NEW)
- **Plan**: `q6_github_integration.md` (Section 10.1)
- **Lines**: ~300
- **Depends on**: M109
- **Tests**: GHES installation detected; custom API URL used; webhook relay polls correctly
- **Blocks**: none

### M119: Enterprise audit log + cost tracking ❌ NOT_STARTED
- **What**: `bugswarm_audit_log` table with all actions; cost per scan/repo/org/user; CSV export; monthly email reports
- **Files**: `bugswarm-github/src/enterprise/audit.rs`, `bugswarm-github/src/enterprise/cost_tracking.rs`
- **Plan**: `q6_github_integration.md` (Sections 10.3, 10.4)
- **Lines**: ~400
- **Depends on**: M109
- **Tests**: SQL audit queries return expected aggregates; cost tracked per scan
- **Blocks**: none

### M120: VS Code extension — inline findings + status bar ❌ NOT_STARTED
- **What**: VS Code extension with inline bug annotations, status bar severity indicator, quick-fix actions, scan-on-save
- **Files**: `vscode-extension/` (NEW directory)
- **Plan**: `q5_zero_cli_platform.md` (Section 1, integration architecture)
- **Lines**: ~1500
- **Depends on**: M094
- **Tests**: Extension loads; findings appear as diagnostics; quick fix applies suggested patch
- **Blocks**: none

### M121: Slack/Discord integrations ❌ NOT_STARTED
- **What**: Slack Bolt app for `/bugswarm scan` command; Discord bot; channel alerts for critical findings; scheduled scans
- **Files**: `bugswarm-api/app/integrations/slack.py`, `bugswarm-api/app/integrations/discord.py`
- **Plan**: `q5_zero_cli_platform.md` (Section 1, integration architecture)
- **Lines**: ~400
- **Depends on**: M094, M074
- **Tests**: Slack command triggers scan; alert posted to channel; Discord bot responds
- **Blocks**: none

### M122: Deployment — Terraform (AWS/GCP) + Kubernetes (EKS/GKE) + Helm ❌ NOT_STARTED
- **What**: Terraform modules for AWS/GCP; Kubernetes deployment; Helm charts for all components; Prometheus + Grafana dashboards
- **Files**: `deploy/terraform/` (NEW), `deploy/kubernetes/` (NEW), `deploy/helm/` (NEW)
- **Plan**: `q5_zero_cli_platform.md` (Section 2.6 deployment)
- **Lines**: ~2000
- **Depends on**: M008, M094
- **Tests**: Terraform plan succeeds; Helm install brings up all services; metrics scrape working
- **Blocks**: none

### M123: API Python Dockerfiles — gateway/agent/swarm/minitia ❌ NOT_STARTED
- **What**: Create Dockerfiles for 4 Python components (gateway, agent, swarm, minitia) for docker-compose deployment
- **Files**: `bugswarm-gateway/Dockerfile` (NEW), `bugswarm-agent/Dockerfile` (NEW), `bugswarm-swarm/Dockerfile` (NEW)
- **Plan**: `phase30c_critical_fix_plan.md` (Step 4a), `production_audit.md` (H1)
- **Lines**: ~80
- **Depends on**: M094
- **Tests**: `docker build` for all 4 Python images succeeds
- **Blocks**: M008

### M124: Config migration tool — `--migrate-config` (M1 fix) ❌ NOT_STARTED
- **What**: Sandbox daemon subcommand to migrate old per-component YAML to unified config format; prints to stdout
- **Files**: `bugswarm-sandbox/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (M1 Fix)
- **Lines**: ~80
- **Depends on**: M015
- **Tests**: Old sandbox config → unified YAML output matches expected schema
- **Blocks**: none

### M125: Deprecation window for old config format (M2 fix) ❌ NOT_STARTED
- **What**: Old `--config` flag continues working for 2 releases; auto-detects old format; prints deprecation warning with migration instructions
- **Files**: `bugswarm-sandbox/src/main.rs`
- **Plan**: `phase30c_critical_fix_plan.md` (M2 Fix)
- **Lines**: ~60
- **Depends on**: M015
- **Tests**: Old format config loads with warning; new format loads without warning
- **Blocks**: none

---

## Batch 6: Advanced — ARIA, Fine-Tuning, Decision Scaffold (32 milestones, est. 8 weeks)

### M126: ARIA — Investigation Context Object (ICO) implementation ❌ NOT_STARTED
- **What**: Implement ICO data structure with TargetProfile, static facts, normalized tool outputs, pattern annotations, waypoints, investigation state, capability grammar
- **Files**: `bugswarm-agent/src/agent/aria/ico.py` (NEW)
- **Plan**: `ARIA.md` (Sections 3, 8)
- **Lines**: ~400
- **Depends on**: none
- **Tests**: ICO serialization round-trip; Layer 0/1/2 data correctly partitioned
- **Blocks**: M127-M134

### M127: ARIA — Pre-Computation Engine ❌ NOT_STARTED
- **What**: CPG indexing → pattern pre-run → CVE similarity → profile classification → waypoint generation → vector index build; ~6-10s pre-computation before model sees prompt
- **Files**: `bugswarm-agent/src/agent/aria/pre_computer.py` (NEW)
- **Plan**: `ARIA.md` (Section 11)
- **Lines**: ~300
- **Depends on**: M126, M064 (CPG languages)
- **Tests**: Pre-computation produces filled ICO with all layers; pattern matches found; CVE similarity scores computed
- **Blocks**: M133

### M128: ARIA — Capability Grammar (4 permanent categories) ❌ NOT_STARTED
- **What**: Map all tools into OBSERVE/TRANSFORM/SYNTHESIZE/VERIFY categories; model sees 4 categories, not 50 tools; type-compatible capability routing
- **Files**: `bugswarm-agent/src/agent/aria/capability_grammar.py` (NEW)
- **Plan**: `ARIA.md` (Section 4)
- **Lines**: ~200
- **Depends on**: M029-M035 (new tools to map into grammar)
- **Tests**: All registered tools mapped to correct category; type compatibility enforced
- **Blocks**: M133

### M129: ARIA — Waypoint Engine (generate + regenerate) ❌ NOT_STARTED
- **What**: Generate waypoints from CPG patterns, CVE similarity, open hypotheses, confirmed findings; regenerate after every ICO enrichment; deterministic, no LLM call
- **Files**: `bugswarm-agent/src/agent/aria/waypoint_engine.py` (NEW)
- **Plan**: `ARIA.md` (Section 5)
- **Lines**: ~250
- **Depends on**: M127, M128
- **Tests**: Waypoints generated from pattern hits; regenerated after tool output; old INVESTIGATED waypoints preserved
- **Blocks**: M133

### M130: ARIA — Layered Prompt Builder ❌ NOT_STARTED
- **What**: Build universal prompt with Layer 0 (facts), Layer 1 (patterns), Layer 2 (waypoints), Capability Grammar, Open Decision; identical structure for all models
- **Files**: `bugswarm-agent/src/agent/aria/prompt_builder.py` (NEW)
- **Plan**: `ARIA.md` (Section 6)
- **Lines**: ~250
- **Depends on**: M126
- **Tests**: Prompt includes all 3 layers; within context budget; model-agnostic structure
- **Blocks**: M133

### M131: ARIA — 3-Layer Intent Parser ❌ NOT_STARTED
- **What**: Layer A: structured extraction (category.name(args), HYPOTHESIS:, QUESTION:, CONCLUDE:); Layer B: semantic classification; Layer C: constrained fallback for STUCK
- **Files**: `bugswarm-agent/src/agent/aria/intent_parser.py` (NEW)
- **Plan**: `ARIA.md` (Section 7)
- **Lines**: ~250
- **Depends on**: M128
- **Tests**: Structured intent parsed with high confidence; free text classified; STUCK triggers fallback
- **Blocks**: M133

### M132: ARIA — Context Manager (3-tier memory) ❌ NOT_STARTED
- **What**: Active context (≤8K tokens), compressed context (≤2K tokens), archive (retrievable on request); cosine similarity relevance scoring
- **Files**: `bugswarm-agent/src/agent/aria/context_manager.py` (NEW)
- **Plan**: `ARIA.md` (Section 9)
- **Lines**: ~200
- **Depends on**: M126
- **Tests**: Context stays under budget; relevant findings ranked highest; archive retrievable via QUESTION
- **Blocks**: M133

### M133: ARIA — Runtime Main Loop (replaces IEP engine) ❌ NOT_STARTED
- **What**: Phase 0: Pre-computation → Phase 1: Build prompt → Phase 2: Model call → Phase 3: Parse intent → Phase 4: Dispatch → Phase 5: Enrich ICO → repeat until completion conditions met
- **Files**: `bugswarm-agent/src/agent/aria/runtime.py` (NEW)
- **Plan**: `ARIA.md` (Section 8)
- **Lines**: ~300
- **Depends on**: M127, M128, M129, M130, M131, M132
- **Tests**: Full investigation cycle completes; completion conditions trigger; waypoints regenerated after each cycle
- **Blocks**: M146

### M134: ARIA — Completion Condition Logic ❌ NOT_STARTED
- **What**: Multi-condition completion: model_declared_complete OR (all high-priority waypoints resolved AND no open high-confidence hypotheses) OR (diminishing returns AND budget exhausted)
- **Files**: `bugswarm-agent/src/agent/aria/completion.py` (NEW)
- **Plan**: `ARIA.md` (Section 8.2)
- **Lines**: ~100
- **Depends on**: M126
- **Tests**: Completion triggers on all three conditions; max_steps enforced
- **Blocks**: M133

### M135: ARIA — Investigation State Graph (ISG) ❌ NOT_STARTED
- **What**: Directed graph — nodes = knowledge states, edges = reasoning steps; integrates tool outputs; compiles final InvestigationReport
- **Files**: `bugswarm-agent/src/agent/aria/isg.py` (NEW)
- **Plan**: `ARIA.md` (Appendix A)
- **Lines**: ~200
- **Depends on**: M126
- **Tests**: Hypotheses tracked in graph; tool outputs linked to hypotheses; report compilation
- **Blocks**: M133

### M136: ARIA — Agent loop.py deprecated ❌ NOT_STARTED
- **What**: Mark existing 354-line IEP Engine as deprecated; keep for backward compat; new ARIA runtime is primary loop
- **Files**: `bugswarm-agent/src/agent/loop.py` (deprecation notice)
- **Plan**: `ARIA.md` (Section 10.1)
- **Lines**: ~10
- **Depends on**: M133
- **Tests**: Existing tests that use loop.py pass with deprecation warning
- **Blocks**: M146

### M137: Decision Scaffold Engine — 12 question types ❌ NOT_STARTED
- **What**: Reasoning influence tool that asks the model 12 structured questions at each decision point; guides investigation without constraining
- **Files**: `bugswarm-agent/src/agent/decision_scaffold.rs` (NEW)
- **Plan**: `bugswarm_tool_superpowers.md` (Part 5)
- **Lines**: ~400
- **Depends on**: M126
- **Tests**: Each question type produces correct scaffolding; model responses classified
- **Blocks**: M133

### M138: Pre-Computation Tools — CPG Pre-Compute Engine (Tool A1 upgrade) ❌ NOT_STARTED
- **What**: Shift from on-demand to index-time: full taint analysis, call graph distances, danger scores, attack surface map, function embeddings → SQLite cache with mmap'd hot cache
- **Files**: `bugswarm-cpg/src/precompute/` (9 NEW files)
- **Plan**: `bugswarm_tool_superpowers.md` (Tool A1)
- **Lines**: ~1500
- **Depends on**: M064, M065, M066
- **Tests**: Pre-computation completes in <60s for 50K LOC; cache stale after code change; 100% detection on known taint paths
- **Blocks**: M139-M145

### M139: Pre-Computation Tools — Danger Map Pre-Computation (Tool A2 upgrade) ❌ NOT_STARTED
- **What**: Multi-dimensional danger scoring (memory, injection, logic, crypto, auth) at index time; composite score with ROI ranking; persisted to SQLite
- **Files**: `bugswarm-cpg/src/precompute/danger_scores.rs` (NEW)
- **Plan**: `bugswarm_tool_superpowers.md` (Tool A2)
- **Lines**: ~400
- **Depends on**: M138
- **Tests**: Juliet buggy functions in top 10%; clean functions below 0.8 composite; cross-language consistency
- **Blocks**: M127

### M140: Pre-Computation Tools — Pattern Database Pre-Loading (Tool A3 upgrade) ❌ NOT_STARTED
- **What**: Load ALL CVE patterns at scan start; batch-compute embeddings; pre-annotate source files; O(1) in-memory lookups; zero ChromaDB queries during investigation
- **Files**: `bugswarm-agent/src/agent/pattern_preloader.py` (NEW)
- **Plan**: `bugswarm_tool_superpowers.md` (Tool A3)
- **Lines**: ~300
- **Depends on**: M138
- **Tests**: Embedding quality on Juliet; false match rate <2%; <2GB memory for 1M LOC; cache invalidates on DB update
- **Blocks**: M127

### M141: Pre-Computation Tools — Invariant Pre-Mining (Tool A4) ❌ NOT_STARTED
- **What**: Run invariant mining at index time for all pure functions; store learned invariants in CPG cache DB; flag pre-existing violations before agent investigates
- **Files**: `bugswarm-cpg/src/precompute/invariant_mine.rs` (NEW)
- **Plan**: `bugswarm_tool_superpowers.md` (Tool A4)
- **Lines**: ~500
- **Depends on**: M138
- **Tests**: Known invariants discovered; no false invariants on random functions; violations flagged
- **Blocks**: M127

### M142: Validation Tools — PoC Validator + Hypothesis Cross-Validator (Tools B1-B6) ❌ NOT_STARTED
- **What**: B1: Verify PoC generates expected crash/sanitizer output; B2: Cross-validate new hypotheses against existing findings; B3: Validate tool call args before execution; B4: Check finding consistency; B5: Classify sandbox output; B6: Verify evidence chain completeness
- **Files**: `bugswarm-agent/src/agent/validators/` (6 NEW files)
- **Plan**: `bugswarm_tool_superpowers.md` (Category B)
- **Lines**: ~800
- **Depends on**: M003, M028
- **Tests**: Each validator catches known-bad inputs; false positive rate measured
- **Blocks**: M133

### M143: Narrowing Tools — Prioritizer + Recommender + Compressor (Tools C1-C6) ❌ NOT_STARTED
- **What**: C1: Rank investigation targets by danger score × payoff ratio; C2: Recommend tools based on current context; C3: Compress context before hitting budget; C4: Decision tree for investigation branching; C5: Pre-score files; C6: Rank taint paths by severity
- **Files**: `bugswarm-agent/src/agent/narrowing/` (6 NEW files)
- **Plan**: `bugswarm_tool_superpowers.md` (Category C)
- **Lines**: ~600
- **Depends on**: M139
- **Tests**: Prioritizer ranks known-buggy functions first; compressor keeps context under budget
- **Blocks**: M133

### M144: Automation Tools — Batch Fuzzer + Invariant Miner + Mutation Tester (Tools D1-D6) ❌ NOT_STARTED
- **What**: D1: Batch fuzz multiple targets; D2: Batch mine invariants across all functions; D3: Batch mutation test; D4: Auto-triage crashes; D5: Auto-build exploit chains; D6: Auto-generate patches for confirmed bugs
- **Files**: `bugswarm-agent/src/agent/automation/` (6 NEW files)
- **Plan**: `bugswarm_tool_superpowers.md` (Category D)
- **Lines**: ~800
- **Depends on**: M028, M036, M037, M067
- **Tests**: Batch operations complete without OOM; crash triage correctly classifies; chain builder finds known chains
- **Blocks**: M133

### M145: Multi-provider LLM Gateway — 20+ providers (Tier 1: Native + Tier 2: OpenAI-compat) ❌ NOT_STARTED
- **What**: Implement BaseProviderAdapter trait; 5 native adapters (OpenAI, Anthropic, DeepSeek, Gemini, Ollama); 10 OpenAI-compatible via single shared adapter; adapter factory; auto-detection
- **Files**: `bugswarm-gateway/src/gateway/providers/` (15+ NEW files)
- **Plan**: `q7_multi_provider_pluggable.md` (Sections 1, 2)
- **Lines**: ~3000
- **Depends on**: M072 (secrets manager)
- **Tests**: Each adapter completes → returns CompletionResponse; health check reports status; error mapping correct
- **Blocks**: M146

### M146: Multi-provider — Model Registry (100+ models) ❌ NOT_STARTED
- **What**: YAML-based model registry with 100+ models across all providers; pricing, context window, speed tier, reasoning quality, phase constraints; periodic CDN refresh
- **Files**: `bugswarm-gateway/config/models.yaml` (NEW), `bugswarm-gateway/src/gateway/model_registry.rs` (NEW)
- **Plan**: `q7_multi_provider_pluggable.md` (Section 3)
- **Lines**: ~800
- **Depends on**: M145
- **Tests**: All models load; pricing correct; phase constraints enforced; deprecated models filtered
- **Blocks**: M147

### M147: Multi-provider — Tier 3 custom adapters (Bedrock + Azure + Vertex AI + Workers AI) ❌ NOT_STARTED
- **What**: AWS Bedrock (SigV4, model ARNs), Azure OpenAI (resource naming, content filter), Vertex AI (GCP auth, regional routing), Cloudflare Workers AI
- **Files**: `bugswarm-gateway/src/gateway/providers/bedrock.rs`, `azure.rs`, `vertex.rs`, `cloudflare.rs`
- **Plan**: `q7_multi_provider_pluggable.md` (Sections 1, 2)
- **Lines**: ~600
- **Depends on**: M145
- **Tests**: Each adapter authenticates and completes on respective cloud
- **Blocks**: M148

### M148: Multi-provider — Model Selector + Fallback Chain + Budget Enforcer ❌ NOT_STARTED
- **What**: Automatic model selection per BugSwarmPhase; transparent fallback chain on failure; budget enforcement per scan/month; parallel tool execution; streaming tool results
- **Files**: `bugswarm-gateway/src/gateway/selector.rs` (NEW), `bugswarm-gateway/src/gateway/fallback.rs` (NEW), `bugswarm-gateway/src/gateway/budget.rs` (NEW)
- **Plan**: `q7_multi_provider_pluggable.md` (Section 4)
- **Lines**: ~800
- **Depends on**: M146
- **Tests**: Model selected by cost/quality; fallback activates on provider error; budget exceeded → circuit open
- **Blocks**: M133 (ARIA model calls)

### M149: Tool Calling Architecture — provider-agnostic schema mapping ❌ NOT_STARTED
- **What**: ToolProtocol handler: BugSwarm ToolDefinition → provider tool schema; parse provider-specific tool_calls; format tool_results; manage call lifecycle; support parallel tool calls
- **Files**: `bugswarm-gateway/src/gateway/tool_protocol.rs` (NEW)
- **Plan**: `q2_tool_calling_architecture.md` (Sections 2, 3)
- **Lines**: ~600
- **Depends on**: M145, M128
- **Tests**: Tool definitions converted to OpenAI/Anthropic/DeepSeek/Gemini formats; tool calls parsed from each format
- **Blocks**: M133

### M150: Tool Calling — Dynamic tool discovery from plugins ❌ NOT_STARTED
- **What**: Plugin-based tool loader; runtime tool registration; capability-based tool selection per agent context; priority ordering
- **Files**: `bugswarm-agent/src/agent/tool_executor.rs`, `bugswarm-gateway/src/gateway/plugin_loader.rs` (NEW)
- **Plan**: `q2_tool_calling_architecture.md` (Section 3)
- **Lines**: ~300
- **Depends on**: M149
- **Tests**: Tool plugin loaded from .so; tool appears in registry; applicable filter filters by language/task
- **Blocks**: M133

### M151: Tool Calling — parallel tool execution + streaming results ❌ NOT_STARTED
- **What**: Execute multiple tool calls concurrently; streaming intermediate results to WebSocket; final result to model
- **Files**: `bugswarm-gateway/src/gateway/parallel_executor.rs` (NEW), `bugswarm-agent/src/agent/streaming_executor.rs` (NEW)
- **Plan**: `q2_tool_calling_architecture.md` (Sections 4, 5)
- **Lines**: ~300
- **Depends on**: M149
- **Tests**: 3 parallel read_file calls complete in < total_time/3; intermediate chunks streamed to WebSocket
- **Blocks**: M133

### M152: Tool Output Normalizers — CPG, Fuzzer, Sandbox, Mutation, Invariant ❌ NOT_STARTED
- **What**: Every tool outputs NormalizedToolOutput with code_locations, data_flow_paths, constraints, artifacts, confidence; consumable_by mapping
- **Files**: `bugswarm-agent/src/agent/tool_normalizers/` (NEW)
- **Plan**: `ARIA.md` (Appendix B)
- **Lines**: ~400
- **Depends on**: M126
- **Tests**: Each normalizer produces valid NormalizedToolOutput; type compatibility verified
- **Blocks**: M133

### M153: Fix Impact Prediction — BFS call graph + value range analysis (Phase 30) ❌ NOT_STARTED
- **What**: Inter-procedural impact analysis via CPG call graph BFS; value range overlap detection; regression test synthesis; Bayesian fix confidence scorer; historical outcome tracking
- **Files**: `bugswarm-evidence/src/fix_impact.rs` (NEW), `bugswarm-evidence/src/bayesian.rs` (NEW)
- **Plan**: `phase30.md`, `production_audit.md` (C7)
- **Lines**: ~1200
- **Depends on**: M027 (evidence graph persistence), M064 (CPG call graph)
- **Tests**: SC-1 through SC-12 from phase30.md
- **Blocks**: none

### M154: Fix-Induced Bug Prediction — Bayesian prior update loop ❌ NOT_STARTED
- **What**: Track fix outcomes; continuously update Bayesian priors per language; Brier score <0.05; auto-close loop: prediction → apply fix → observe outcome → update prior
- **Files**: `bugswarm-evidence/src/bayesian.rs`
- **Plan**: `phase30.md` (Section SC-6)
- **Lines**: ~300
- **Depends on**: M153
- **Tests**: Prior converges after 100 outcomes; Brier score calibration
- **Blocks**: none

### M155: Fine-tuning pipeline — NVD/GHSA/OSS-Fuzz data collectors ❌ NOT_STARTED
- **What**: Implement 8 data source collectors: NVD API 2.0, GHSA GraphQL, OSS-Fuzz GCS, Juliet Test Suite, Exploit-DB, MITRE CWE examples, BugSwarm internal, CodeQL queries
- **Files**: `ml/nvd_collector.py` (NEW), `ml/ghsa_collector.py` (NEW), `ml/ossfuzz_collector.py` (NEW), etc.
- **Plan**: `q3_finetuning_pipeline.md` (Section 1)
- **Lines**: ~1500
- **Depends on**: M107 (production data from scans)
- **Tests**: Each collector fetches data; schema validated; deduplication working
- **Blocks**: M156, M157

### M156: Fine-tuning — data preprocessing pipeline ❌ NOT_STARTED
- **What**: Ingestion → Cleaning → Enrichment → Tokenization → Instruction-tuning format → Train/Val/Test split; special tokens; quality validation
- **Files**: `ml/preprocessing.py` (NEW), `ml/dataset_validator.py` (NEW)
- **Plan**: `q3_finetuning_pipeline.md` (Section 2)
- **Lines**: ~800
- **Depends on**: M155
- **Tests**: Dataset composition matches target weights; all required CWEs covered; no duplicates; token length distribution normal
- **Blocks**: M157

### M157: Fine-tuning — QLoRA + Full fine-tune training scripts ❌ NOT_STARTED
- **What**: QLoRA config (rank 64, NF4 quantization); Full fine-tune config (DeepSpeed ZeRO-3); W&B logging; checkpoint management; model evaluation on held-out test set
- **Files**: `ml/qlora_config.py` (NEW), `ml/full_finetune_config.py` (NEW), `ml/train.py` (NEW)
- **Plan**: `q3_finetuning_pipeline.md` (Sections 3, 4)
- **Lines**: ~600
- **Depends on**: M156
- **Tests**: QLoRA training completes; CWE classification accuracy >90%; validation loss converges
- **Blocks**: none

### M158: Single-model mode — prompt-engineering investigation vs judging ❌ NOT_STARTED
- **What**: Support single-model operation where same model is prompted differently for investigation vs judging phases; model selector routes phases to same provider with different parameters
- **Files**: `bugswarm-gateway/src/gateway/selector.rs`
- **Plan**: `q7_multi_provider_pluggable.md` (Section 4), `bugswarm_mythos_gap_analysis.md` (Gap 1)
- **Lines**: ~200
- **Depends on**: M145
- **Tests**: Same model used for investigation and judge with different prompts/temperatures
- **Blocks**: M133

---

## Batch 7: Verification & Hardening (8 milestones, est. 2 weeks)

### M159: Cross-crate Rust integration tests (L20) ❌ NOT_STARTED
- **What**: Tests that exercise multiple crates together: sandbox→CPG→evidence roundtrip; full scan workflow; fuzzer→evidence collection
- **Files**: `tests/integration_cross/` (NEW)
- **Plan**: `production_audit.md` (L20)
- **Lines**: ~300
- **Depends on**: M070
- **Tests**: This IS the test suite
- **Blocks**: none

### M160: Cargo.lock consolidation — workspace lock (M17) 🔶 PARTIAL
- **What**: Consolidate per-crate Cargo.lock files into single workspace-level lock to prevent version divergence
- **Files**: Root `Cargo.toml`, all crate `Cargo.toml`
- **Plan**: `production_audit.md` (M17)
- **Lines**: ~10
- **Depends on**: M048
- **Tests**: `cargo build --workspace` produces single Cargo.lock
- **Blocks**: none

### M161: thiserror v1/v2 conflict resolution (M20) ❌ NOT_STARTED
- **What**: Resolve thiserror v1 vs v2 version conflict across workspace; ensure single version used
- **Files**: Root `Cargo.toml` `[workspace.dependencies]`
- **Plan**: `production_audit.md` (M20)
- **Lines**: ~10
- **Depends on**: M048
- **Tests**: `cargo tree --duplicates` shows no thiserror duplicates
- **Blocks**: none

### M162: `bugswarm/sandbox-python:latest` image build (L1) 🔶 PARTIAL
- **What**: Ensure sandbox-python Docker image is actually built by a Dockerfile and tagged correctly
- **Files**: `bugswarm-sandbox/docker/sandbox-python.Dockerfile` (NEW or RENAME)
- **Plan**: `production_audit.md` (L1, H34)
- **Lines**: ~20
- **Depends on**: M007
- **Tests**: `docker build` on sandbox-python Dockerfile succeeds
- **Blocks**: M008

### M163: CPG/Evidence daemon config file support (L2) ❌ NOT_STARTED
- **What**: CPG/Evidence daemons currently CLI-only; add config file reading support via unified config
- **Files**: `bugswarm-cpg/src/main.rs`, `bugswarm-evidence/src/main.rs`
- **Plan**: `production_audit.md` (L2)
- **Lines**: ~30
- **Depends on**: M015
- **Tests**: CPG daemon starts from config file without CLI args
- **Blocks**: none

### M164: API key validation produces error, not warning on missing key (L3) ❌ NOT_STARTED
- **What**: Missing API key should be hard error, not warning
- **Files**: `bugswarm-gateway/src/gateway/types.py`
- **Plan**: `production_audit.md` (L3)
- **Lines**: ~10
- **Depends on**: M072
- **Tests**: Start without API key → error exit, not warning
- **Blocks**: none

### M165: Orbit benchmark integration — performance regression detection ❌ NOT_STARTED
- **What**: Integrate Orbit benchmark harness for continuous performance regression detection; track TTD, FPR, throughput per commit
- **Files**: `tests/benchmarks/orbit/` (NEW)
- **Plan**: `phase30.md` (SC references), `q1_all_bug_coverage.md` (metrics targets)
- **Lines**: ~300
- **Depends on**: M070
- **Tests**: Orbit runs on every PR; performance regression blocks merge
- **Blocks**: none

### M166: Exploit generation — binary analysis + GDB/pwntools integration (Gap 7, post-MVP) ❌ NOT_STARTED
- **What**: Autonomous exploit writing: ROP chains, heap sprays, shellcode; binary analysis tooling; exploit verification sandbox; requires Mythos-class model
- **Files**: `bugswarm-exploit/` (NEW crate)
- **Plan**: `bugswarm_mythos_gap_analysis.md` (Gap 7)
- **Lines**: ~3000
- **Depends on**: M153, M157 (fine-tuned model)
- **Tests**: Known-exploitable bugs → valid exploit generated in sandbox
- **Blocks**: none

---

## Summary

| Batch | Name | Milestones | Est. Weeks | Blocking |
|-------|------|-----------|------------|----------|
| 1 | Critical Fixes | M001-M028 | 4 | Everything |
| 2 | Core Tooling | M029-M052 | 3 | Batch 3 |
| 3 | Detection Expansion | M053-M071 | 6 | Batch 5 |
| 4 | Enterprise Infrastructure | M072-M093 | 4 | Batch 5 |
| 5 | Platform | M094-M125 | 6 | Batch 6 |
| 6 | Advanced | M126-M158 | 8 | None |
| 7 | Verification & Hardening | M159-M166 | 2 | None |
| **Total** | | **166** | **33** | |

### Status Breakdown
- ❌ NOT_STARTED: 160
- 🔶 PARTIAL: 5 (M009 release profiles, M020 evidence daemon, M091 lock files, M160 Cargo.lock, M162 sandbox-python image)
- ✅ DONE: 1 (M093 container removal errors — fixed in Phase 30a)

### Source Coverage
- `production_audit.md` (105 findings): M001-M028, M046-M052, M072-M093, M159-M164
- `phase30b_high_fix_plan.md` (41 findings): M001-M052
- `phase30c_critical_fix_plan.md` (8 gaps): M008, M010-M017, M020, M123-M125
- `q1_all_bug_coverage.md` (104 CWE): M053-M062
- `q2_tool_calling_architecture.md`: M149-M151
- `q3_finetuning_pipeline.md`: M155-M157
- `q4_100_languages.md`: M064-M069
- `q5_zero_cli_platform.md`: M094-M108, M120-M124
- `q6_github_integration.md`: M109-M119
- `q7_multi_provider_pluggable.md`: M145-M148, M158
- `ARIA.md`: M126-M136
- `bugswarm_tool_superpowers.md` (33 tools): M029-M035, M138-M144
- `bugswarm_mythos_gap_analysis.md` (7 gaps): M029-M035, M064, M071
- `chain_universal_expansion.md`: M067
- `phase24.md` (Differential): referenced in M039, M052
- `phase25.md` (Invariants): referenced in M036, M040
- `phase30.md` (Fix Impact): M153-M154

---

# APPENDIX A: Parallel Execution Plan

## Rules
- Milestones in the SAME parallel group can run simultaneously — they touch different files and have no shared state
- Milestones on separate lines within a batch are sequential — they depend on earlier milestones completing first
- "ALONE" means this milestone must run solo because it mutates shared infrastructure (daemon loop, config schema, agent dispatch) that would break parallel agents

---

### Batch 1 Parallel Groups (28 milestones)

**Group 1A (4 parallel)**: M001 (shell injection), M002 (path traversal), M004 (CPG Dockerfile), M005 (Evidence Dockerfile)
→ These touch container.rs, core.py, and new Dockerfiles — zero shared state

**Group 1B (5 parallel)**: M006 (Symbolic Dockerfile), M007 (ASAN Dockerfile), M008 (docker-compose), M009 (release profiles), M019 (version + git tag)
→ Dockerfiles are independent; compose file is new; Cargo.toml changes are separate crates

**Group 1C — ALONE**: M003 (tool capability gating)
→ Mutates the central agent dispatch path; all other agent changes must happen before or after, not during

**Group 1D (4 parallel)**: M010 (CPG health), M011 (Evidence health), M012 (Sandbox metrics), M018 (CI/CD pipeline)
→ Different daemons, different files; CI is a new .github file

**Group 1E (3 parallel)**: M013 (CPG metrics), M014 (Evidence metrics), M015 (unified config Rust)
→ Config schema must be stable before M016 can run

**Group 1F (4 parallel)**: M016 (unified config Python), M020 (Evidence RunServer), M021 (systemd units), M022 (graceful shutdown)
→ M016 depends on M015 being done first

**Group 1G — ALONE**: M017 (refactor daemon handlers)
→ Mutates daemon.rs across ALL crates; must run alone to prevent merge conflicts

**Group 1H (4 parallel)**: M023 (config hard error), M024 (SIGHUP reload), M025 (unwrap removal), M026 (socket error handling)
→ Different files, different crates

**Group 1I — ALONE**: M027 (evidence graph persistence)
→ Mutates graph.rs core data structures; changes how nodes/edges are serialized

**Group 1J — ALONE**: M028 (fuzzer crash collection)
→ Mutates container.rs + fuzzer.rs; touches the execution pipeline

---

### Batch 2 Parallel Groups (24 milestones)

**Group 2A (7 parallel)**: M029 (grep), M030 (glob), M031 (write), M032 (edit), M033 (web_fetch), M034 (todo), M035 (kill_shell)
→ All are NEW standalone tools; zero shared state; each is a self-contained module

**Group 2B — ALONE**: M036 (mine_invariants real execution)
→ Mutates daemon.rs invariant handler + sandbox execution path

**Group 2C — ALONE**: M037 (run_mutations real execution)
→ Mutates daemon.rs mutation handler + sandbox execution path

**Group 2D — ALONE**: M038 (solve_reachability Z3 integration)
→ Adds new crate dependency; changes feature flags; mutates symbolic crate

**Group 2E (3 parallel)**: M039 (diff_execute protocol), M040 (invariant_check real), M041 (delta_debug wire-up)
→ Different daemon handlers; different files

**Group 2F (4 parallel)**: M042 (describe_trigger wire-up), M043 (get_trigger_matrix wire-up), M044 (suggest_chain wire-up), M045 (predict_fix_impact wire-up)
→ All evidence daemon handlers; independent methods

**Group 2G (3 parallel)**: M046 (orchestrator tools), M047 (fuzz_target tool), M048 (explore_paths tool)
→ Different Python files; orchestration vs agent

**Group 2H (2 parallel)**: M049 (explore_paths daemon), M050 (diff_execute daemon)
→ Different daemon handlers

**Group 2I (2 parallel)**: M051 (explore_paths agent), M052 (diff_execute agent)
→ Different Python client methods

---

### Batch 3 Parallel Groups (23 milestones)

**Group 3A (5 parallel)**: M053-M057 (CWE-89/79/20/22/78 detection)
→ New scanner modules with no shared state

**Group 3B (5 parallel)**: M058-M062 (CWE-190/416/787/125/476 detection)
→ More scanner modules; independent implementations

**Group 3C (6 parallel)**: M063 (fuzzer danger map SHM), M064 (C CPG parser), M065 (C++ CPG parser), M066 (Rust CPG parser), M067 (universal chain types), M068 (Go CPG parser)
→ M064-M066, M068 are independent parser files; M063 is sandbox; M067 is chain.rs enums

**Group 3D (2 parallel)**: M069 (Java CPG parser), M070 (auto-language detection)
→ M070 depends on having multiple parsers; M069 must finish first

**Group 3E (3 parallel)**: M071 (Z3 real calls in symbolic), M072 (danger map CPG wire-up), M073 (fuzz stats parsing real)
→ M071 is symbolic crate; M072 is sandbox→CPG; M073 is fuzzer.rs

**Group 3F (2 parallel)**: M074 (docker compose health checks), M075 (docker compose volumes)
→ Both modify docker-compose.yml; must sequentialize or be one milestone

---

### Batch 4 Parallel Groups (21 milestones)

**Group 4A (4 parallel)**: M076 (sandbox auto-restart), M077 (CPG auto-restart), M078 (Evidence auto-restart), M079 (docker restart policies)
→ Different systemd files; independent

**Group 4B (4 parallel)**: M080 (log rotation), M081 (structured error types), M082 (tracing spans), M083 (request ID propagation)
→ Different files across crates

**Group 4C — ALONE**: M084 (axum HTTP server migration)
→ Replaces raw TCP with axum in ALL daemons; must run alone

**Group 4D (5 parallel)**: M085 (OpenTelemetry), M086 (Slack alerts), M087 (secret provider), M088 (circuit breaker), M089 (unsafe audit comments)
→ Independent modules

**Group 4E (3 parallel)**: M090 (seccomp hardening), M091 (causal intervention escaping), M092 (config migration tool)
→ Different files

**Group 4F (2 parallel)**: M093 (container removal logging), M094 (temp file cleanup)
→ Different subsystems

**Group 4G (2 parallel)**: M095 (Python lock files), M096 (dependency upper bounds)
→ Different files

---

### Batch 5 Parallel Groups (38 milestones)

**Group 5A — ALONE**: M097 (FastAPI backend scaffold)
→ Creates entire new crate; all other platform work depends on this

**Group 5B (6 parallel)**: M098-M103 (REST endpoints: health, scan, status, findings, stream, artifacts)
→ All are route handlers in the same FastAPI app; can parallelize as different route files

**Group 5C (4 parallel)**: M104 (auth middleware), M105 (rate limiting), M106 (job queue), M107 (artifact storage)
→ Independent middleware/services

**Group 5D — ALONE**: M108 (React frontend scaffold)
→ Creates entire new frontend project

**Group 5E (5 parallel)**: M109 (landing page), M110 (dashboard), M111 (code viewer), M112 (settings), M113 (export)
→ Different React components; no shared state

**Group 5F (4 parallel)**: M114 (GitHub App), M115 (OAuth flow), M116 (webhook handler), M117 (PR comments)
→ M115 depends on M114; rest can parallelize after M114 done

**Group 5G (4 parallel)**: M118 (SARIF upload), M119 (auto-issues), M120 (check runs), M121 (branch protection)
→ Independent GitHub API integrations

**Group 5H (3 parallel)**: M122 (bugswarm-action), M123 (VS Code extension), M124 (Slack/Discord bots)
→ Independent projects

**Group 5I (3 parallel)**: M125 (Terraform), M126 (Helm chart), M127 (K8s manifests)
→ Infra-as-code; independent templates

**Group 5J (3 parallel)**: M128 (landing docs), M129 (pricing page), M130 (onboarding flow)
→ Frontend-only; independent pages

**Group 5K (2 parallel)**: M131 (marketplace listing), M132 (GitHub Enterprise)
→ External integrations

**Group 5L (2 parallel)**: M133 (org-wide policies), M134 (audit logging)
→ Enterprise features

---

### Batch 6 Parallel Groups (27 milestones)

**Group 6A — ALONE**: M135 (ARIA ICO schema)
→ Defines the central data structure everything else reads

**Group 6B (4 parallel)**: M136 (PreCompEngine), M137 (PromptBuilder), M138 (IntentParser), M139 (WaypointEngine)
→ All read from ICO schema (M135); no shared mutation of ICO

**Group 6C (3 parallel)**: M140 (ContextManager), M141 (Dispatch loop), M142 (Completion conditions)
→ Read from ICO; independent implementations

**Group 6D (3 parallel)**: M143 (tool normalizers), M144 (capability grammar), M145 (ISG integration)
→ Independent modules

**Group 6E (4 parallel)**: M146 (fine-tuning data pipeline), M147 (model evaluation framework), M148 (LoRA harness), M149 (model registry)
→ POST-MVP; all independent infrastructure for fine-tuning

**Group 6F (3 parallel)**: M150 (provider adapters Tier 2), M151 (provider adapters Tier 3), M152 (model registry catalog)
→ Independent adapter files; no shared state

**Group 6G (3 parallel)**: M153 (fix outcome tracking), M154 (Bayesian prior updating), M155 (chain bench integration)
→ Different subsystems

**Group 6H (3 parallel)**: M156 (explore_paths solver latency), M157 (constraint simplification measurement), M158 (seed recycling warm-start)
→ Independent performance tracking

**Group 6I (3 parallel)**: M159 (Decision Scaffold question types), M160 (scaffold runtime), M161 (scaffold per-task adaptation)
→ M160 depends on M159; M161 depends on M160

---

### Batch 7 Parallel Groups (11 milestones)

**Group 7A (5 parallel)**: M162 (end-to-end system test), M163 (load test 100 concurrent), M164 (security penetration test), M165 (documentation site), M166 (launch checklist)
→ Independent verification activities

**Group 7B (3 parallel)**: M167 (OSS-Fuzz corpus run), M168 (Juliet test suite run), M169 (real targets run)
→ Independent benchmark runs

**Group 7C (3 parallel)**: M170 (bug bounty program setup), M171 (community guidelines), M172 (contributor onboarding)
→ Community/launch activities

---

# APPENDIX B: Engineered Prompts Per Batch

## Batch 1 Prompt — Critical Fixes (M001-M028)

```
You are implementing CRITICAL production fixes for BugSwarm, an enterprise 
vulnerability detection system with 4 Rust crates and 3 Python packages.

CONTEXT:
- The system has 30 phases of implementation complete
- All 1,000+ tests pass, workspace compiles
- These fixes close 28 critical deployment/security gaps
- Everything must compile + pass tests after EACH milestone

RULES:
1. Never modify shared infrastructure (daemon.rs dispatch, config schema, 
   agent loop) while another agent is working — check with the coordinator
2. Each milestone is ONE commit. Commit after each passing test suite
3. Run `cargo check --workspace` after every Rust change
4. Run `python3 -m pytest` after every Python change  
5. If a milestone touches MULTIPLE crates, run ALL their test suites
6. Dockerfiles must pass `docker build` before committing

PARALLEL GROUPS:
- M001+M002+M004+M005 can run simultaneously (different files)
- M006+M007+M008+M009+M019 can run simultaneously
- M010+M011+M012+M018 can run simultaneously
- M013+M014+M015 can run simultaneously
- M016+M020+M021+M022 can run simultaneously (M016 AFTER M015)
- M023+M024+M025+M026 can run simultaneously

SEQUENTIAL:
- M003 (tool gating) runs AFTER M002, alone
- M017 (handler refactor) runs alone — modifies ALL daemon dispatchers
- M027 (graph persistence) runs alone
- M028 (fuzzer collection) runs alone

OUTPUT: After each milestone, report: what changed, test count, 
compilation status, any blocked dependencies.
```

---

## Batch 2 Prompt — Core Tooling (M029-M052)

```
You are building BugSwarm's core tooling — 7 new enterprise tools and 
fixing 9 functional stubs that currently return mock data.

CONTEXT:
- All Batch 1 fixes are complete. The system deploys and compiles.
- BugSwarm's ToolRegistry has 14 registered tools, 9 return stubs
- Claude Code has 17 tools BugSwarm doesn't. We're closing that gap.
- Each new tool must be enterprise-grade, not "few lines of code"

NEW TOOLS (7):
M029: Grep — regex search across codebase with ripgrep, result ranking, 
      context windows, caching. Rust backend for performance. ~300 lines
M030: Glob — recursive `**` pattern matching with relevance ranking, 
      file type detection, CPG metadata integration. ~200 lines
M031: Write — secure file writer with sandbox directory, atomic writes, 
      version history, rollback, path validation. ~250 lines  
M032: Edit — AST-aware surgical code modifier via tree-sitter, semantic 
      edits, multi-file refactoring, preview diffs. ~400 lines
M033: WebFetch — security-hardened web client with URL allowlist, content 
      sanitization, token budgeting, caching. ~300 lines
M034: TodoWrite — structured investigation task tracker with auto-status 
      updates, dependency tracking, progress reporting. ~250 lines
M035: KillShell — process lifecycle manager with graceful shutdown, 
      resource cleanup, orphan detection. ~200 lines

STUB FIXES (9):
M036-M044: Wire real sandbox execution, real Z3 integration, real test 
         compilation, real evidence daemon calls instead of mock returns

RULES:
1. Each tool is self-contained — add to tool_registry AFTER it's tested
2. Stub fixes mutete daemon handlers — run ALONE (M036, M037, M038)
3. Evidence daemon handlers (M042-M045) are independent — parallelize
4. Every tool needs: unit tests, integration test, error path test, 
   performance benchmark (must complete in <100ms for typical input)

OUTPUT: Tool name, lines of code, test count, benchmark results, 
registration status in ToolRegistry.
```

---

## Batch 3 Prompt — Detection Expansion (M053-M075)

```
You are expanding BugSwarm's detection capabilities from 6 bug categories 
to 100+ CWE categories, from 2 languages to 7, and from security-only 
chains to universal bug chains.

CONTEXT:
- Current CPG supports Python + JavaScript tree-sitter grammars  
- Current chain detector has 6 effect types (security only)
- Current fuzzer uses heuristic danger map, not real SHM transport
- Each CWE scanner is a standalone module implementing BugCategoryScanner

CWE SCANNERS (M053-M062):
- Each scanner: ~200-500 lines, detects one CWE category
- Must produce: bug report with file:line, confidence score, CPG evidence
- Must have: 90%+ true positive rate on Juliet test suite
- Test with: 100+ synthetic test cases per scanner

LANGUAGE PARSERS (M064-M069):
- Each parser: ~200 lines for AST, ~100 for call graph, ~200 for CFG
- Add tree-sitter-{lang} crate to Cargo.toml
- Must extract: functions, classes, call edges, control flow, taint sources/sinks
- Test with: 50+ real-world files per language

UNIVERSAL CHAINS (M067):
- Expand EffectType from 7 to 15 variants
- Expand PreconditionType from 7 to 15 variants  
- Add ChainType enum with 6 categories
- Category-aware severity calculus
- Test with: 30+ cross-category chain scenarios

SYMBOLIC Z3 (M071):
- Replace heuristic solver with real Z3 SMT calls
- Add z3 crate dependency
- Handle: bitvector, integer, string, array theories
- Test with: 100+ SMT-LIB2 benchmark problems

PARALLEL GROUPS:
- M053-M057: 5 CWE scanners simultaneously
- M058-M062: 5 more CWE scanners simultaneously  
- M064-M066, M068: 4 language parsers simultaneously
- M063: Fuzzer SHM — run after M072 (CPG danger map wire-up)
- M071: Z3 integration — run ALONE (changes solver architecture)

OUTPUT: Scanner name, CWE coverage, true/false positive rates, 
language parser: files parsed correctly, chain detector: new effect types 
working, Z3: solve rate improvement over heuristic.
```

---

## Batch 4 Prompt — Enterprise Infrastructure (M076-M096)

```
You are hardening BugSwarm for production deployment — systemd units, 
logging, monitoring, security hardening, migration tools.

CONTEXT:
- All Batch 1-3 features are complete and tested
- The system compiles and deploys but has zero production hardening
- These milestones make BugSwarm run reliably without human intervention

INFRASTRUCTURE:
M076-M079: systemd unit files with Restart=always, proper dependencies
M080: Log rotation via logrotate config or tracing-appender
M081: Replace String errors with thiserror enums in all crates
M082: Distributed tracing spans across daemon boundaries
M083: Request ID propagation from agent → sandbox → CPG → evidence

HTTP/HEALTH:
M084: Replace raw TCP health endpoints with axum Router
      (/health, /ready, /metrics on all 3 daemons)

SECURITY:
M085: OpenTelemetry OTLP export (feature-gated, default off)
M086: Slack/PagerDuty alerting via webhook
M087: Secrets provider — file-based API keys via Docker secrets
M088: Circuit breaker between daemons (3 consecutive failures → open)
M089: SAFETY comments on ALL unsafe blocks (audit + document)
M090: Seccomp hardening — explicit deny for fork/clone families
M091: Causal intervention escaping fix — temp file + bind mount
M092: Config migration tool — old sandbox.yaml → unified config.yaml

CLEANUP:  
M093: Container removal errors logged (not silently swallowed)
M094: Temp file atexit cleanup via signal handler
M095: Python lock files with exact pinned versions
M096: All Cargo.toml deps with upper bounds

RULES:
- M084 (axum migration) runs ALONE — touches ALL daemon HTTP code
- M076-M079 run simultaneously (different systemd files)
- M085-M089 run simultaneously (independent security modules)
- All changes must pass: `cargo check --workspace`, 
  `cargo test --workspace`, `cargo clippy -- -D warnings`

OUTPUT: Number of warnings fixed, security audit pass/fail, 
daemon restart test result, config migration test result.
```

---

## Batch 5 Prompt — Platform (M097-M134)

```
You are building BugSwarm's SaaS platform — FastAPI backend, React 
frontend, GitHub integration, VS Code extension, and deployment 
infrastructure. This transforms BugSwarm from a CLI tool into a 
zero-CLI platform where users paste GitHub URLs and get results.

CONTEXT:
- All previous batches are deployed and stable  
- BugSwarm currently requires CLI knowledge to operate
- This batch makes it accessible to anyone with a browser
- Target: user pastes repo URL → auto-clones → scans → streams results

BACKEND (M097-M107):
M097: FastAPI scaffold with project structure, middleware, config
M098-M103: REST endpoints for scan lifecycle (POST/GET/WS)
M104: JWT + API key + GitHub OAuth authentication
M105: Rate limiting per user/IP
M106: Redis + Celery job queue for long-running scans
M107: S3-compatible artifact storage for PoCs and crash files

FRONTEND (M108-M113):
M108: React + TypeScript + Tailwind scaffold
M109-M113: Landing page, dashboard, code viewer, settings, export

GITHUB (M114-M121):
M114: GitHub App manifest + installation flow
M115: OAuth token exchange
M116: Webhook event handler (PR opened, push, check suite)
M117: Inline PR review comments with bug annotations
M118: SARIF upload to GitHub Code Scanning
M119: Auto-create GitHub Issues for confirmed bugs
M120: Check runs API integration
M121: Branch protection — require BugSwarm scan before merge

EXTENSIONS (M122-M124):
M122: `bugswarm-action@v1` GitHub Action
M123: VS Code extension — scan current file, inline findings
M124: Slack/Discord notification bots

DEPLOYMENT (M125-M134):
M125-M127: Terraform, Helm chart, K8s manifests
M128-M130: Documentation site, pricing page, onboarding
M131: GitHub Marketplace listing
M132: GitHub Enterprise Server support
M133: Organization-wide scan policies
M134: Audit logging for compliance

PARALLEL GROUPS:
- M097 runs ALONE (backend scaffold)
- M098-M103: 6 endpoints simultaneously (different route files)
- M104-M107: 4 middleware/services simultaneously
- M108 runs ALONE (frontend scaffold)
- M109-M113: 5 frontend components simultaneously
- M114-M121: M115 depends on M114; M116-M121 after M114+M115
- M122-M124: 3 extensions simultaneously
- M125-M127: 3 IaC templates simultaneously

OUTPUT: Number of endpoints, frontend pages, GitHub events handled, 
deployment manifests created.
```

---

## Batch 6 Prompt — Advanced (M135-M161)

```
You are building BugSwarm's advanced capabilities — ARIA runtime, 
fine-tuning pipeline, decision scaffold, and multi-provider gateway.

CONTEXT:
- The platform is deployed and serving users
- These milestones are the "super powers" that make BugSwarm surpass 
  frontier models through tools, not model quality
- ARIA replaces the IEP engine with adaptive reasoning

ARIA RUNTIME (M135-M145):
M135: ICO schema — Investigation Context Object with 3 layers
M136: PreComputationEngine — CPG + CVE + patterns pre-run
M137: LayeredPromptBuilder — builds ICO-based prompts
M138: UniversalIntentParser — 3-layer (structured→semantic→constrained)
M139: WaypointEngine — generation + continuous regeneration
M140: ContextManager — 3-tier memory (active/compressed/archive)
M141: Dispatch loop — intent-driven execution
M142: Completion conditions — multi-factor investigation termination
M143: Tool output normalizers — every tool → NormalizedToolOutput
M144: CapabilityGrammar — 4-category self-describing manifest
M145: ISG integration — Investigation State Graph

FINE-TUNING (M146-M149):
M146: Data collection pipeline — NVD API poller, patch-diff parser
M147: Model evaluation framework — holdout sets, blind benchmarks
M148: LoRA/QLoRA training harness — 8×A100, $500-2000/run
M149: Model registry — versioning, rollback, canary deployment
⚠️ M146-M149 are POST-MVP — requires 3+ months production data first

MULTI-PROVIDER (M150-M152):
M150: Tier 2 adapters — OpenRouter, Together, Groq, Fireworks (10 providers)
M151: Tier 3 adapters — AWS Bedrock, Azure, Vertex AI (5 providers)
M152: Model registry catalog — 120+ models with capability metadata

FIX PREDICTION (M153-M154):
M153: FixOutcome tracking — record all fix outcomes for Bayesian update
M154: Bayesian prior convergence — language priors stabilize after 100 outcomes

PERFORMANCE (M155-M158):
M155: Chain detector bench integration — chains appear in bench queue
M156: Solver latency tracking — p50/p95/p99 per query
M157: Constraint simplification ratio measurement
M158: Seed recycling warm-start reuse measurement

DECISION SCAFFOLD (M159-M161):
M159: 12 question types — structured questions for every investigation phase
M160: Scaffold runtime — per-task adaptive level selection
M161: Per-task adaptation — same model gets different scaffold level per task type

PARALLEL GROUPS:
- M135 runs ALONE (ICO schema — everything depends on it)
- M136-M139: 4 ARIA components simultaneously
- M140-M142: 3 ARIA components simultaneously
- M143-M145: 3 ARIA components simultaneously
- M146-M149: 4 fine-tuning components simultaneously (POST-MVP)
- M150-M152: 3 provider adapters simultaneously
- M153-M154: 2 fix prediction components simultaneously
- M155-M158: 4 performance tracking components simultaneously
- M159-M161: SEQUENTIAL (scaffold depends on question types)

OUTPUT: ARIA test suite pass rate, model solve rate improvement, 
provider adapter count, fine-tuning pipeline status.
```

---

## Batch 7 Prompt — Launch (M162-M172)

```
You are launching BugSwarm — final verification, benchmarks, 
documentation, and community setup.

CONTEXT:
- All 161 previous milestones are complete
- The system is feature-complete and must now be proven
- These milestones verify that BugSwarm works at production scale

VERIFICATION (M162-M164):
M162: End-to-end system test — full pipeline against 10 real repos
M163: Load test — 100 concurrent scan requests
M164: Security penetration test — external audit

BENCHMARKS (M167-M169):
M167: OSS-Fuzz corpus — run against 1,000 known-vulnerable repos
M168: Juliet test suite — 100,000 synthetic bugs across 150 CWE categories
M169: Real-world targets — nginx, Redis, SQLite, OpenSSL

DOCUMENTATION (M165):
M165: Full documentation site — API docs, user guide, architecture docs

COMMUNITY (M170-M172):
M170: Bug bounty program setup — HackerOne or self-hosted
M171: Community guidelines and code of conduct
M172: Contributor onboarding — CONTRIBUTING.md, good first issues

PARALLEL GROUPS:
- M162-M164: 3 verification activities simultaneously
- M167-M169: 3 benchmarks simultaneously
- M165: Documentation (independent)
- M170-M172: 3 community activities simultaneously

OUTPUT: Test pass rates, bug counts per target, documentation completeness,
community readiness checklist.
```
