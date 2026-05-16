# BugSwarm Production Readiness Audit — Consolidated Report

**Date**: 2026-05-14
**Scope**: 4 Rust crates, 3 Python packages, 30 phases
**Tests passing**: 1,000+
**Audit coverage**: Infrastructure, Security, Functional Completeness

---

## CRITICAL FINDINGS (12 — must fix before any deployment)

| # | Category | Finding |
|---|----------|---------|
| C1 | Deploy | **No orchestration config** — No docker-compose.yml, no systemd units, no supervisor config. 8 components must be started manually. |
| C2 | Deploy | **No release build profile** — All 4 Cargo.toml files lack `[profile.release]`. No LTO, no strip, no opt-level=3. |
| C3 | Monitor | **No HTTP health checks** — Zero daemons expose HTTP endpoints. Load balancers can't monitor. Unix socket `health` method only. |
| C4 | Monitor | **No Prometheus metrics endpoint** — Metrics exist in-code but not exposed. |
| C5 | Deploy | **Evidence daemon unreachable** — `main.rs` has no `RunServer` subcommand. Cannot be started. |
| C6 | Config | **No unified config** — 8 components each have separate config mechanisms. |
| C7 | Functional | **Invariant mining returns stub data** — `mine_invariants` handler generates fabricated traces with hardcoded values. No real execution. |
| C8 | Functional | **Fuzzer never collects results** — AFL++ container launches but stats/crashes never flow back to daemon or evidence graph. |
| C9 | Functional | **Mutation testing uses fake test runner** — `run_mutations` handler returns hardcoded (1,1)/(2,0) tuples. No compilation or test execution. |
| C10 | Functional | **Diff CLI+daemon protocol mismatch** — Python client sends `--output-a`/`--output-b` but daemon handler reads `input`/`reference` keys. |
| C11 | Functional | **Symbolic solve in daemon is heuristic-only** — `solve_reachability` uses `extract_solution_value()` (+1 to ints). Z3 from `bugswarm-symbolic` crate never called. |
| C12 | Functional | **Evidence graph has no persistence** — All nodes/edges are in-memory. Restart = total data loss. |

---

## HIGH FINDINGS (41 — would cause production outages)

| # | Category | Finding |
|---|----------|---------|
| H1 | Deploy | Rust daemons have no Dockerfiles. Only 2 Docker images defined (fuzz, python-asan). |
| H2 | Deploy | `sandbox-python-asan.Dockerfile` has no CMD/ENTRYPOINT |
| H3 | Deploy | Unix socket paths hardcoded, parent dir creation silently ignores permission errors |
| H4 | Deploy | Daemon loop has no graceful shutdown — no `tokio::select!` on SIGTERM |
| H5 | Deploy | No CI/CD pipeline — no `.github/workflows/`, no Makefile, no build script |
| H6 | Deploy | No version number beyond 0.1.0. No git tags. No release strategy. |
| H7 | Deploy | Config parsing failures silently fall back to defaults |
| H8 | Deploy | No auto-restart for Rust daemons. Panic = dead process. |
| H9 | Deploy | Multiple `.unwrap()` in non-test production paths |
| H10 | Deploy | No SIGHUP handling for config reload |
| H11 | Security | **Shell injection via heredoc delimiter** — `BUGSWARM_EOF` in PoC content escapes heredoc in container.rs:274 |
| H12 | Security | **Path traversal in agent read_file** — `core.py:240` allows absolute paths, bypassing repo sandbox |
| H13 | Security | API keys loaded from env vars only. No secrets manager. No key rotation. |
| H14 | Security | No circuit breaker between daemons. CPG unreachable → silent failure. |
| H15 | Security | 16 unsafe blocks in danger_map.rs for shared memory. No audit trail. |
| H16 | Security | `prlimit64` allowed in seccomp profile. Fork/clone not explicitly listed. |
| H17 | Security | Agent tool dispatch directly from LLM JSON. No capability gating beyond read_file. |
| H18 | Monitor | No OpenTelemetry integration. No distributed tracing. |
| H19 | Monitor | Alerting engine exists but never wired to Slack/PagerDuty |
| H20 | Monitor | No trace IDs/correlation IDs between daemons |
| H21 | Functional | `mine_invariants` daemon handler uses 100% stub traces |
| H22 | Functional | `invariant_check` generates inputs but never executes them |
| H23 | Functional | `run_mutations` fake test runner |
| H24 | Functional | `solve_reachability` heuristic, not Z3 |
| H25 | Functional | `diff_execute` client→daemon protocol mismatch |
| H26 | Functional | `describe_trigger` in orchestrator bypasses EvidenceClient |
| H27 | Functional | `get_trigger_matrix` in orchestrator returns always-empty stubs |
| H28 | Functional | `estimate_argument_ranges` always returns hardcoded [0,100,42] regardless of caller |
| H29 | Functional | Danger map shared-memory path has no SHM setup infrastructure |
| H30 | Functional | Fuzz campaign crash artifacts never collected back from container |
| H31 | Functional | Orchestrator tools missing 8 agent tools (delta_debug, diff_execute, mine_invariants, etc.) |
| H32 | Functional | `exec_causal_intervention` uses inadequate string escaping (only `'` → `\'`) |
| H33 | Build | Floating dependency versions. No lock file at workspace root. |
| H34 | Build | Default config image `bugswarm/sandbox-python:latest` doesn't exist |
| H35 | Build | No CHANGELOG |
| H36 | Code | Docker container removal errors silently swallowed — resource leak |
| H37 | Code | Regex::new().unwrap() in differential.rs compiled per-call — panics + perf |
| H38 | Code | `ssa.rs:208` — `s.chars().next().unwrap()` — panics on empty string |
| H39 | Code | `parser.rs:954,960` — regex `.unwrap()` on compile |
| H40 | Code | `main.rs:94` — `serde_json::to_string_pretty(&stats).unwrap()` — panics in main binary |
| H41 | Code | `shm_map` returns `Result<*mut u8, String>` — inconsistent error type |

---

## MEDIUM FINDINGS (30)

| # | Finding |
|---|---------|
| M1 | All Docker image references use `:latest` — unreproducible builds |
| M2 | AFL++ cloned from GitHub with no checksum verification |
| M3 | Daemons log to stdout only — no file logging, no rotation, no syslog |
| M4 | Stale socket removal on startup — race if another instance running |
| M5 | Sandbox daemon HTTP endpoint doesn't exist — no API for external tools |
| M6 | Docker daemon unavailable = fatal error, no retry |
| M7 | Ollama default URL hardcoded to `http://localhost:11434` — no HTTPS |
| M8 | No authentication on Unix sockets — any local process can execute |
| M9 | No TLS/mTLS anywhere |
| M10 | Hardcoded paths: `/var/run/bugswarm/`, `/etc/bugswarm/`, `/fuzz/corpus/` |
| M11 | Evidence graph in-memory — no save/load for nodes+edges |
| M12 | Fuzz corpus directories not pre-created — Docker create_container fails if missing |
| M13 | No PID file management |
| M14 | NodeKind::Agent, CodeLocation never created in daemon handlers |
| M15 | EdgeKind::Authored, Enables, ConditionEquivalent never used |
| M16 | Swarm evidence population uses PatternDB (Python), never Rust EvidenceClient |
| M17 | `Cargo.lock` files per crate may diverge |
| M18 | `tree-sitter` v0.23 — API-breaking changes possible |
| M19 | `bollard` v0.17 — Docker API compatibility risk |
| M20 | `thiserror` v1 vs v2 conflict across workspace |
| M21 | `z3` crate v0.14 requires native C++ Z3 on host — not containerized |
| M22 | `prlimit64` allowed in seccomp default.json |
| M23 | Agent tool dispatch from LLM JSON — injection risk |
| M24 | Seccomp JSON passed as Docker CLI option — metacharacter risk |
| M25 | No PID file — systemd can't track daemons |
| M26 | Multiple instances can run concurrently with no mutual exclusion |
| M27 | `core.py` uses catch-all Exception — swallows infrastructure vs security failures |
| M28 | `fuzzer.rs:1016` parse errors default to 0s — corrupted stats possible |
| M29 | `container.rs:304` no container auto-remove on stats/debug paths |
| M30 | `diff_execute` client sends broken CLI command that doesn't exist |

---

## LOW FINDINGS (22)

| # | Finding |
|---|---------|
| L1 | `bugswarm/sandbox-python:latest` image not built by any Dockerfile |
| L2 | No config file for CPG/Evidence daemon — CLI-only |
| L3 | API key validation produces warning, not error on missing key |
| L4 | No key rotation support |
| L5 | `UnifiedScanner._shannon_entropy` simplified calculation |
| L6 | `wiring._extract_cpg_functions` uses heuristic, not actual CPG queries |
| L7 | `orchestrator._maybe_retrain_model` has incomplete CPG extraction |
| L8 | Python metrics exist but not exposed |
| L9 | Python packages lack lock files for transitive deps |
| L10 | `nix` crate — Linux-specific, untested elsewhere |
| L11 | Socket permission `.ok()` swallows errors |
| L12 | PII pattern `aws_secret` regex too broad — false positives |
| L13 | Token counting fallback uses `len//4` — inaccurate for non-English |
| L14 | Gateway `from_env()` returns empty config, no error |
| L15 | `parse_env` accepts empty key `=VAL` |
| L16 | Container remove failures silently swallowed |
| L17 | Temp file leak if process killed between creation and unlink |
| L18 | No version pinning in Python pyproject.toml |
| L19 | Multiple crates have slightly different major versions of shared deps |
| L20 | No Rust cross-crate integration tests |
| L21 | Full pipeline test skips LLM path when API key not set |
| L22 | No `profile.release` in any Cargo.toml |

---

## WHAT WORKS (verified)

| Component | Status |
|-----------|--------|
| CPG indexing + taint analysis | ✅ Fully functional |
| Sandbox execute + execute_statistical | ✅ Fully functional |
| Delta debugging (ddmin) | ✅ Fully functional, 4 split strategies |
| Fuzzer controller + config | ✅ Controller works, container launches |
| Chain detection + severity calculus | ✅ Fully functional |
| Fix impact prediction + Bayesian scoring | ✅ Fully functional |
| Trigger matrix + semantic dedup | ✅ Fully functional |
| Concolic execution (explore_paths) | ✅ Fully functional with Z3 |
| Differential output normalization | ✅ 5 normalizers functional |
| Agent read_file, list_dir, query_cpg | ✅ Fully functional |
| Agent exec_sandbox | ✅ Fully functional |
| ChromaDB + HotspotTracker + AgentHistory | ✅ Persists correctly |
| BugProbabilityModel (XGBoost) | ✅ Trains + predicts |
| All 30 phase plans | ✅ Enterprise standard, 1,000-2,000 lines each |
| 1,000+ unit/integration/gate tests | ✅ All passing |

---

## FIX PRIORITY

### Must fix before ANY deployment (12 CRITICAL):
C1 (orchestration) → C2 (release build) → C3 (health checks) → C4 (metrics) → C5 (evidence daemon) → C6 (unified config) → C7-C12 (functional stubs)

### Must fix before production use (41 HIGH):
H1-H10 (deployment infra) → H11-H17 (security) → H18-H20 (monitoring) → H21-H32 (functional gaps) → H33-H41 (build/code quality)

### Should fix before public release (30 MEDIUM)
### Nice to have (22 LOW)
