# Rust Audit — Critical Gaps

## Severity Breakdown

| Severity | Count |
|----------|-------|
| **CRITICAL BROKEN** | 2 |
| **Bug** | 5 |
| **Dead Code** | 12 |
| **Stub** | 5 |
| **Panic Site** | 8 |
| **Silent Failure** | 7 |
| **Performance Bug** | 6 |
| **Missing Enterprise** | 15+ |

## CRITICAL BROKEN (2)

1. **`execute_statistical` produces garbage** — all spawned tasks return `Err("spawn context unavailable")` unconditionally. Counters never incremented. Every call returns 100% failure rate. `container.rs:562-599`

2. **`seal()` doesn't fully hash node state** — `metadata`, `severity`, `created_at`, `independently_verified` excluded from hash. Node can be silently tampered with after sealing. `types.rs:174-180`

## BUGS (5)

3. **JS call handler never resolves local functions** — creates external stubs with confidence 0.5 for ALL calls. Python handler correctly checks `find_function()` first. `parser.rs:656-668`

4. **`reaching_to` is O(N^2)** — iterates all nodes to find incoming edges instead of using `neighbors_directed(Incoming)`. `graph.rs:466-487`

5. **Taint BFS visited set shadows alternative paths** — a sanitized path can shadow a tainted one. `graph.rs:332-376`

6. **TOCTOU race condition in evidence graph** — `add_sandbox_run`, `add_claim`, `add_prediction` all read `nodes.len()` then acquire write lock separately. `graph.rs:54-57`

7. **`ensure_image` returns OK on pull failure** — Docker layer errors are logged then ignored. Returns `Ok()` even if pull completely failed. `container.rs:80-86`

## DEAD CODE (12)

- 4 of 11 `SandboxError` variants never constructed: `Timeout`, `OomKilled`, `PiiDetected`, `ReceiptValidation`
- `compute_memory_growth` — `#[allow(dead_code)]`, always returns `(0,0)`
- `_flaky_detection` parameter — accepted but never read
- `to_docker_string()` — defined but never called
- `_stack` variable — constructed then discarded
- `NodeKind::CodeLocation`, `EdgeKind::Authored`, `EdgeKind::References` — defined but never created
- `EvidenceQuery.sort_by` — field exists, query logic ignores it
- `SandboxConfig.seccomp_profile_path` — serde field, never read

## STUBS (5)

8. `RunServer` daemon mode — prints error and exits code 1
9. Memory profiling — all `peak_mb`, `growth_rate` hardcoded to zero
10. `execute_statistical` — all spawn blocks return early error
11. Config file loading — `--config` flag exists but `SandboxConfig::default()` always used
12. Seccomp profile never attached to Docker — `build_host_config` has no `security_opt` for seccomp

## PANIC SITES (8)

- `unwrap()` on `escape_matches.first()`, `exit_code`, `to_str()`, `join()`, serialization
- `expect()` in `OutputScanner::default()`
- Nested `unwrap()` on serialization fallback in main.rs

## SILENT FAILURES (7)

- Docker `remove_container`, `kill_container` errors dropped with `let _ =`
- Container inspection failure silently loses exit code/OOM status
- Docker logs stream error breaks loop, remaining logs lost
- `parse_env_vars` drops malformed entries without warning
- Image pull errors swallowed mid-stream

## PERFORMANCE BUGS (6)

- `reaching_to` O(N^2) — can be O(E) via `neighbors_directed(Incoming)`
- `stats()` calls `find_echo_chambers` which is O(A^2 × C × E) ≈ O(n^4)
- Polling loop: 600 API calls per container via `sleep(200ms) + inspect`
- 5+3 regexes compiled fresh on every `extract_stack_frames` / `classify_exception` call
- No caching — CPG re-indexes entire repo on every command
- Repeated `edges_to()` calls in `weakest_claim`, `strongest_claim`, `orphaned_claims`

## MISSING ENTERPRISE (15+)

- No auth/mTLS/API keys — zero security boundary
- No persistence — all state in-memory across all 3 crates
- No Prometheus metrics, OpenTelemetry, health check endpoints
- No config file loading (despite `--config` flags)
- No secrets management, rate limiting, circuit breakers
- No graceful shutdown — SIGTERM leaves Docker containers dangling
- No IPC protocol between sandbox ↔ CPG ↔ evidence crates
- No shared type library — types duplicated across crates
- No graph export (GraphML, DOT), no query language
- No incremental/differential indexing for CPG
- No pagination in evidence queries
- No container lifecycle management (orphaned cleanup)
