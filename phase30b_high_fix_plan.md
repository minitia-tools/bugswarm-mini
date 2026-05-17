# Phase 30-B: HIGH Production Findings Remediation Plan

**Source**: `/root/a/production_audit.md` — 41 HIGH findings
**Dependencies solved**: 22 LOW + 30 MEDIUM already fixed
**Estimated effort**: 60-80 hours

---

## Finding Categories

### H1-H10: Deployment Infrastructure (10 findings)
### H11-H17: Security Vulnerabilities (7 findings)  
### H18-H20: Monitoring & Observability (3 findings)
### H21-H32: Functional Gaps (12 findings)
### H33-H41: Build & Code Quality (9 findings)

---

## Category A: Deployment Infrastructure (H1-H10)

### H1: Rust daemons have no Dockerfiles
**Root cause**: Only sandbox Dockerfiles exist. CPG, Evidence, Symbolic daemons run bare-metal.
**Permanent fix**: Create multi-stage Dockerfiles for CPG, Evidence, and Symbolic. Use `cargo build --release` in builder stage, `FROM scratch` or `FROM debian:bookworm-slim` in runtime.
**Files**: `bugswarm-cpg/Dockerfile` (NEW), `bugswarm-evidence/Dockerfile` (NEW), `bugswarm-symbolic/Dockerfile` (enhance existing)
**Immunity**: CI checks that every crate with a `main.rs` has a corresponding `Dockerfile`.
**Implementation**:
1. Create `bugswarm-cpg/Dockerfile` — multi-stage, copies `bugswarm-cpg` binary
2. Create `bugswarm-evidence/Dockerfile` — multi-stage, copies `bugswarm-evidence` binary
3. Enhance `bugswarm-symbolic/Dockerfile` — add health check
4. Add `docker-compose.yml` at workspace root with all 4 services

### H2: sandbox-python-asan.Dockerfile has no CMD/ENTRYPOINT
**Root cause**: Container was defined as base image only, never finalized.
**Permanent fix**: Add `ENTRYPOINT ["python3"]` and `CMD ["--version"]`. Add `HEALTHCHECK`.
**Files**: `bugswarm-sandbox/docker/sandbox-python-asan.Dockerfile`
**Immunity**: CI check: every Dockerfile must have either CMD or ENTRYPOINT.
**Implementation**:
1. Add `ENTRYPOINT ["python3"]` 
2. Add `CMD ["-c", "print('sandbox ready')"]`
3. Add `HEALTHCHECK --interval=30s CMD python3 -c "print('ok')" || exit 1`

### H3: Socket parent dir creation silently ignores permission errors
**Root cause**: `.ok()` swallows `EPERM`/`EACCES` — daemon starts but unreachable.
**Permanent fix**: Replace `.ok()` with proper error propagation and logging. If `/var/run/bugswarm/` cannot be created, daemon should exit with clear error.
**Files**: `bugswarm-cpg/src/daemon.rs:85`, `bugswarm-sandbox/src/daemon.rs:76`, `bugswarm-evidence/src/daemon.rs:91`
**Immunity**: Runtime assertion: after `create_dir_all`, verify directory exists and is writable.
**Implementation**:
1. Replace `create_dir_all(parent).ok()` with `create_dir_all(parent).context("Failed to create socket directory")?`
2. After creation, verify permissions with `std::fs::metadata`

### H4: Daemon loop has no graceful shutdown — no tokio::select! on SIGTERM
**Root cause**: Daemon main loop is `loop { accept().await }` with no signal handling.
**Permanent fix**: Wrap accept loop in `tokio::select!` with `tokio::signal::ctrl_c()` arm. On SIGTERM, stop accepting new connections, drain active connections, close socket, exit.
**Files**: All 3 daemon.rs files, all 3 main.rs files
**Immunity**: Integration test: start daemon, send SIGTERM, verify clean shutdown within 5s.
**Implementation**:
1. Add `tokio::signal` feature to Cargo.toml for all 3 crates
2. In `run_daemon()`: `tokio::select! { _ = accept_loop() => {}, _ = tokio::signal::ctrl_c() => { shutdown() } }`
3. `shutdown()`: close listener, drain active tasks, remove PID file, remove socket, exit 0

### H5: No CI/CD pipeline
**Root cause**: No `.github/workflows/` directory exists.
**Permanent fix**: Create CI pipeline with: build, test, lint, security audit, Docker build.
**Files**: `.github/workflows/ci.yml` (NEW)
**Immunity**: CI fails on any regression. PRs blocked without passing CI.
**Implementation**:
1. Create CI workflow with jobs: `cargo test --workspace`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo audit`, `docker build` for all images
2. Add status badge to README

### H6: No version number beyond 0.1.0. No git tags.
**Root cause**: Version never incremented from initial `cargo init`.
**Permanent fix**: Set version to `1.0.0` in all Cargo.toml. Create git tag `v1.0.0`. Add version to `BUGSWARM_VERSION` env var.
**Files**: All 4 Cargo.toml files, root Cargo.toml
**Immunity**: CI check: `Cargo.toml` version must match git tag.
**Implementation**:
1. Update all `version = "0.1.0"` to `version = "1.0.0"`
2. Add `env!("CARGO_PKG_VERSION")` usage in main.rs version output
3. `git tag v1.0.0`

### H7: Config parsing failures silently fall back to defaults
**Root cause**: `unwrap_or_else` with warning swallows config errors.
**Permanent fix**: Config parsing failure should be a hard error, not silent fallback.
**Files**: `bugswarm-sandbox/src/main.rs:167-169`
**Immunity**: Test: provide invalid YAML → daemon refuses to start with clear error.
**Implementation**:
1. Change: if config file exists but parse fails, `eprintln!` + `std::process::exit(1)`
2. Keep fallback ONLY if config file does not exist at all

### H8: No auto-restart for Rust daemons. Panic = dead process.
**Root cause**: No process supervisor configured.
**Permanent fix**: Provide systemd unit files with `Restart=always` and `RestartSec=5`.
**Files**: `deploy/systemd/bugswarm-sandbox.service` (NEW), `deploy/systemd/bugswarm-cpg.service` (NEW), `deploy/systemd/bugswarm-evidence.service` (NEW)
**Immunity**: Integration test: kill daemon, verify systemd restarts within 5s.
**Implementation**:
1. Create 3 systemd unit files with proper `After=docker.service`, `Restart=always`, `RestartSec=5`
2. Add `Type=notify` or `Type=simple` with PID file

### H9: Multiple .unwrap() in non-test production paths
**Root cause**: ~15 unwrap/expect calls in production code.
**Permanent fix**: Replace all non-test unwraps with proper error handling.
**Files**: Various — `ssa.rs:208`, `parser.rs:954,960`, `main.rs:94,131,171`, `container.rs`, `concolic.rs:1802`
**Immunity**: Clippy lint: `#![deny(clippy::unwrap_used)]` on all crates, excluding test modules.
**Implementation**:
1. Audit every `.unwrap()` — keep only in test code and infallible cases
2. Replace with `.context("...")?` or `.unwrap_or_else(|| { log::error!(...); default })`
3. Add clippy deny to lib.rs

### H10: No SIGHUP handling for config reload
**Root cause**: Configuration read once at startup, never reloaded.
**Permanent fix**: Add SIGHUP handler that re-reads config file and applies non-breaking changes.
**Files**: All 3 main.rs files
**Immunity**: Test: modify config file, send SIGHUP, verify daemon uses new values.
**Implementation**:
1. Spawn `tokio::spawn(async { signal(SIGHUP).recv(); reload_config() })`
2. `reload_config()`: re-read config file, update log level, timeouts, limits
3. Structural changes (new fields) require restart — document

---

## Category B: Security Vulnerabilities (H11-H17)

### H11: Shell injection via heredoc delimiter (BUGSWARM_EOF attack)
**Root cause**: PoC content containing "BUGSWARM_EOF" on its own line exits heredoc early, executing subsequent lines as shell commands.
**Permanent fix**: Write PoC to temp file via Docker COPY or Docker API file injection instead of heredoc. Alternative: use base64 encoding inside heredoc.
**Files**: `bugswarm-sandbox/src/container.rs:274-276`
**Immunity**: Fuzz test: generate inputs containing the delimiter, verify no shell execution.
**Implementation**:
```rust
// Option A: Write PoC to temp file, bind-mount into container
let tmp_path = format!("/tmp/bugswarm-poc-{}.py", Uuid::new_v4());
std::fs::write(&tmp_path, poc_content)?;
// Add bind mount to container config
// Remove heredoc entirely from command

// Option B: Base64 encode
let encoded = base64::encode(poc_content.as_bytes());
let cmd = format!("echo {} | base64 -d > /tmp/poc.py && python3 /tmp/poc.py", encoded);
```
**Prefer Option A** — bind mount is cleaner and eliminates shell injection surface entirely.

### H12: Path traversal in agent read_file (absolute path bypass)
**Root cause**: `if path.startswith("/") && Path(path).exists()` allows reading any file.
**Permanent fix**: Always resolve paths relative to repo_root. Never allow absolute paths.
**Files**: `bugswarm-agent/src/agent/core.py:_read_file`, `bugswarm-agent/src/agent/cli/wiring.py:_read_file_async`
**Immunity**: Test: `read_file("/etc/passwd")` → returns error "Path must be relative to repository".
**Implementation**:
```python
# Before: if path.startswith("/") and Path(path).exists(): full_path = Path(path)
# After: always resolve relative to repo_path, reject absolute paths
path = path.lstrip('/')  # Strip leading / to prevent absolute path bypass
full_path = (repo_path / path).resolve()
if not str(full_path).startswith(str(repo_path.resolve())):
    return ToolResult(False, f"Path traversal blocked: {path}")
```

### H13: API keys loaded from env vars only. No secrets manager. No key rotation.
**Root cause**: Only env var source, no Vault/KMS integration.
**Permanent fix**: Add `BGSWARM_SECRETS_PROVIDER` env var with support for: `env` (default), `file` (read from `/run/secrets/`), `vault` (future). Document Docker secrets pattern.
**Files**: `bugswarm-gateway/src/gateway/types.py`
**Immunity**: Test: set `BGSWARM_SECRETS_PROVIDER=file`, `BGSWARM_API_KEY_FILE=/tmp/key`, verify key loaded from file.
**Implementation**:
```python
def _load_api_key(provider: str, key_name: str) -> str:
    if provider == "file":
        path = os.getenv(f"{key_name}_FILE", f"/run/secrets/{key_name.lower()}")
        return Path(path).read_text().strip()
    elif provider == "env":
        return os.getenv(key_name, "")
    # Future: vault, aws-secrets-manager
    return ""
```

### H14: No circuit breaker between daemons. CPG unreachable → silent failure.
**Root cause**: Danger map loading fails silently, fuzzer continues without guidance.
**Permanent fix**: Add health check before relying on CPG. If CPG is unreachable for >3 consecutive checks, emit WARN log and set `circuit_open` flag. Fuzzer falls back to coverage-only mode.
**Files**: `bugswarm-sandbox/src/fuzzer.rs`, `bugswarm-sandbox/src/container.rs`
**Immunity**: Test: kill CPG daemon, verify fuzzer continues in degraded mode with clear log message.
**Implementation**:
```rust
struct CpgCircuitBreaker {
    failures: AtomicU32,
    threshold: u32,
    state: AtomicU8, // 0=closed, 1=open, 2=half_open
}
// In danger map loading:
if breaker.is_open() {
    log::warn!("CPG circuit breaker open — running without danger guidance");
    return;
}
if cpg_health_check().is_err() {
    breaker.record_failure();
}
```

### H15: 16 unsafe blocks in danger_map.rs. No audit trail.
**Root cause**: POSIX shared memory operations require unsafe.
**Permanent fix**: Wrap all unsafe blocks with `// SAFETY:` comments documenting: preconditions, why invariants hold, what could go wrong. Add debug_assert! guards.
**Files**: `bugswarm-sandbox/src/danger_map.rs`
**Immunity**: CI check: every unsafe block must have a `// SAFETY:` comment.
**Implementation**:
1. For each unsafe block, add:
```rust
// SAFETY: mmap is called with a valid file descriptor from shm_open,
// size from fstat, PROT_READ only, MAP_SHARED. The returned pointer
// is guarded by from_raw_parts with the same size.
let ptr = unsafe { libc::mmap(...) };
```

### H16: prlimit64 allowed, fork/clone not explicitly listed in seccomp
**Root cause**: Seccomp profile uses default-deny but doesn't list all blocked critical syscalls.
**Permanent fix**: Add explicit DENY entries for fork/clone/clone3/vfork families in the seccomp JSON. Remove prlimit64 from allowed list.
**Files**: `bugswarm-sandbox/seccomp/default.json`
**Immunity**: Test: verify `fork()` inside sandbox returns `-1 EPERM`.
**Implementation**:
1. Add blocked syscalls: `clone`, `clone3`, `fork`, `vfork`, `execveat`, `kexec_load`, `kexec_file_load`
2. Remove `prlimit64` from allowed list
3. Update seccomp profile version to 2

### H17: Agent tool dispatch directly from LLM JSON. No capability gating.
**Root cause**: LLM can call any tool with any arguments. Already partially fixed in MEDIUM (M23), but needs hardening.
**Permanent fix**: Add input validation per tool. `exec_sandbox` requires code size limit (1MB max). `read_file` must be within repo boundaries (H12 fix). Add rate limiting per tool type.
**Files**: `bugswarm-agent/src/agent/core.py`, `bugswarm-agent/src/agent/tools.py`
**Immunity**: Test: LLM generates `exec_sandbox` with 100MB PoC → rejected. LLM generates `read_file` of `/etc/shadow` → rejected.
**Implementation**:
1. Add pre-execution validators per tool:
```python
TOOL_VALIDATORS = {
    "exec_sandbox": lambda args: len(args.get("poc_code", "")) < 1_000_000,
    "read_file": lambda args: ".." not in args.get("path", "") and not args.get("path","").startswith("/"),
}
```
2. Validate before dispatch. Reject with clear error message.

---

## Category C: Monitoring & Observability (H18-H20)

### H18: No OpenTelemetry integration. No distributed tracing.
**Root cause**: Only JSON logs, no trace context propagation.
**Permanent fix**: Add `tracing-opentelemetry` crate. Propagate `traceparent` header across daemon calls via Unix socket metadata.
**Files**: All Cargo.toml files, all daemon.rs files
**Immunity**: Test: make a request, verify trace spans appear with correct parent-child relationships.
**Implementation**:
1. Add `opentelemetry`, `tracing-opentelemetry` deps
2. Initialize OTLP exporter pointing to `localhost:4317` (configurable)
3. Add trace context to Unix socket protocol (include `trace_id` in JSON request)
4. Forward trace context in agent→sandbox→CPG calls

### H19: Alerting engine never wired to Slack/PagerDuty
**Root cause**: `AlertEngine` exists but `_notifier` is always `None`.
**Permanent fix**: Add webhook notifier for Slack. Add `BGSWARM_ALERT_SLACK_WEBHOOK` env var.
**Files**: `bugswarm-swarm/src/swarm/observability.py`
**Immunity**: Test: trigger alert condition, verify Slack webhook receives POST.
**Implementation**:
```python
class SlackNotifier:
    def __init__(self, webhook_url: str):
        self.url = webhook_url
    def notify(self, alert: Alert):
        requests.post(self.url, json={"text": f"[{alert.severity}] {alert.message}"})
```

### H20: No trace IDs/correlation IDs between daemons
**Root cause**: Each daemon generates independent logs with no linking.
**Permanent fix**: Generate `x-request-id` at agent entry point. Pass through all daemon calls. Include in all log lines.
**Files**: `bugswarm-agent/src/agent/core.py`, all daemon.rs files
**Immunity**: Test: make request, verify all log lines from all daemons share the same request_id.
**Implementation**:
1. Agent generates `request_id = uuid4()` at start of each investigation
2. Passes via `--request-id` flag or JSON field to all daemons
3. Daemons include `request_id` in all `tracing::info!` spans

---

## Category D: Functional Gaps (H21-H32)

### H21: mine_invariants returns 100% stub traces
**Root cause**: Daemon generates fabricated traces instead of executing real code.
**Permanent fix**: Wire `mine_invariants` handler to actually execute the target function with generated inputs in a sandbox container. Each input → real execution → real ExecutionTrace.
**Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-sandbox/src/invariant.rs`
**Immunity**: Test: call mine_invariants on known function, verify traces contain real output values, not "result_0", "result_1".
**Implementation**:
1. For each generated input, create a PoC that calls the target function
2. Execute PoC in sandbox container via `manager.execute()`
3. Extract return value, exceptions, exit code from ExecutionReceipt
4. Build real ExecutionTrace

### H22: invariant_check generates inputs but never executes them
**Root cause**: Same as H21 — generates but skips execution.
**Permanent fix**: Same fix as H21. Generate + Execute in sandbox.
**Files**: `bugswarm-sandbox/src/daemon.rs`
**Implementation**: Reuse H21 execution pipeline.

### H23: run_mutations uses fake test runner
**Root cause**: String matching on operators, not real compilation + test execution.
**Permanent fix**: For each mutant: (1) write mutated source to temp file, (2) execute existing test suite via sandbox, (3) check if any tests fail. Real test execution.
**Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-sandbox/src/mutation.rs`
**Immunity**: Test: provide source + test file, verify mutants are actually compiled and tested.
**Implementation**:
1. For each mutant, generate mutated source file
2. In sandbox: `python3 -m pytest test_file.py` or equivalent
3. Check exit code: 0 = all tests pass (survivor), non-zero = at least one failed (killed)

### H24: solve_reachability uses heuristic, not Z3
**Root cause**: Daemon handler uses `extract_solution_value()` (+1 to ints) instead of actual Z3.
**Permanent fix**: Either: (a) call `bugswarm-symbolic` crate from sandbox daemon, or (b) make symbolic daemon handle this RPC.
**Files**: `bugswarm-sandbox/src/daemon.rs`
**Immunity**: Test: solve `x > 1000 && y == "admin"`, verify solution `x=1001, y=admin` comes from Z3, not heuristic.
**Implementation**:
1. Add `bugswarm-symbolic` as dependency of sandbox
2. In handler: `let engine = bugswarm_symbolic::engine::SymbolicEngine::new(...); let result = engine.solve_reachability(...);`

### H25: diff_execute client→daemon protocol mismatch
**Root cause**: Client sends `--output-a`/`--output-b` as CLI args; daemon reads `input`/`reference` as JSON keys.
**Permanent fix**: Client sends JSON via `--request` (fixed in M30). Verify daemon handler reads correct keys. Add integration test.
**Files**: `bugswarm-agent/src/agent/sandbox_client.py`, `bugswarm-sandbox/src/daemon.rs`

### H26: describe_trigger in orchestrator bypasses EvidenceClient
**Root cause**: Returns `{"recorded": True}` without daemon call.
**Permanent fix**: Actually call `EvidenceClient.add_trigger_condition()`.
**Files**: `bugswarm-swarm/src/swarm/orchestrator.py`

### H27: get_trigger_matrix in orchestrator returns always-empty stub
**Root cause**: Hardcoded `{"conditions": []}`.
**Permanent fix**: Actually call `EvidenceClient.get_trigger_matrix()`.
**Files**: `bugswarm-swarm/src/swarm/orchestrator.py`

### H28: estimate_argument_ranges returns hardcoded [0,100,42]
**Root cause**: Placeholder implementation.
**Permanent fix**: Use CPG AST analysis to extract actual argument types and value ranges from caller source code. Fall back to heuristic for unknown types.
**Files**: `bugswarm-evidence/src/fix_predict.rs`

### H29: Danger map shared-memory path has no SHM setup infrastructure
**Root cause**: `load_danger_map_if_configured()` tries `danger_map_from_shm()` but no code creates the SHM segment.
**Permanent fix**: Add SHM creation step: before starting fuzz campaign, if danger_map_enabled and SHM name set, create the SHM segment and populate it from the danger map data.
**Files**: `bugswarm-sandbox/src/fuzzer.rs`, `bugswarm-sandbox/src/container.rs`

### H30: Fuzz campaign crash artifacts never collected back from container
**Root cause**: Container launches AFL++ but no result monitoring.
**Permanent fix**: Periodically scan `/corpus/out/crashes/` from the host side. For each new crash file, parse stack trace, compute hash, inject into evidence graph.
**Files**: `bugswarm-sandbox/src/container.rs`, `bugswarm-sandbox/src/fuzzer.rs`

### H31: Orchestrator tools missing 8 agent tools
**Root cause**: `setup_tools()` registers only 7 tools; 8 more exist in wiring.py.
**Permanent fix**: Register all 15 tools in orchestrator's `setup_tools()`.
**Files**: `bugswarm-swarm/src/swarm/orchestrator.py`

### H32: exec_causal_intervention uses inadequate string escaping
**Root cause**: Only escapes single quotes, not other special characters.
**Permanent fix**: Use proper shell escaping via `shell-escape` crate, or write to temp file instead of inline.
**Files**: `bugswarm-sandbox/src/container.rs`

---

## Category E: Build & Code Quality (H33-H41)

### H33: Floating dependency versions
**Root cause**: `tokio = "1.41"` means `>=1.41, <2.0` — major version can change.
**Permanent fix**: Upper-bound ALL workspace dependencies: `tokio = ">=1.41, <2.0"`.
**Files**: Root Cargo.toml `[workspace.dependencies]`

### H34: Default config image doesn't exist
**Root cause**: Referenced `bugswarm/sandbox-python:latest` never built.
**Permanent fix**: Already handled in L1 (sandbox-base.Dockerfile). Verify image builds.

### H35: No CHANGELOG
**Root cause**: Never created.
**Permanent fix**: Create `CHANGELOG.md` with all phase completion entries.
**Files**: `CHANGELOG.md` (NEW)

### H36: Container removal errors silently swallowed
**Root cause**: `let _ = remove_container()`.
**Permanent fix**: Fixed in L16. Verify.

### H37: Regex::new().unwrap() in differential.rs
**Root cause**: Compiled per-execution, can panic.
**Permanent fix**: Use `once_cell::sync::Lazy` or `lazy_static` to compile regex once.
**Files**: `bugswarm-sandbox/src/differential.rs`

### H38: ssa.rs:208 — s.chars().next().unwrap() panics on empty string
**Root cause**: No guard for empty strings.
**Permanent fix**: Add empty string check before calling .next().
**Files**: `bugswarm-cpg/src/ssa.rs:208`

### H39: parser.rs:954,960 — regex .unwrap() on compile
**Root cause**: Panics if regex pattern is invalid.
**Permanent fix**: Use Lazy static regex, or handle compile error with proper error type.
**Files**: `bugswarm-cpg/src/parser.rs`

### H40: main.rs:94 — unwrap() in main binary
**Root cause**: Serialization can fail.
**Permanent fix**: Replace with proper error handling.
**Files**: `bugswarm-evidence/src/main.rs`

### H41: shm_map returns Result<*mut u8, String> — inconsistent error type
**Root cause**: Uses String instead of structured error.
**Permanent fix**: Use `anyhow::Error` or a proper error enum.
**Files**: `bugswarm-sandbox/src/danger_map.rs`

---

## Implementation Order (dependency-aware)

### Batch 1 (independent — can run in parallel): 16 findings
H6 (version), H35 (changelog), H2 (docker CMD), H1 (dockerfiles), H5 (CI), H8 (systemd), H33 (dep versions), H15 (unsafe audit), H36-H41 (code quality fixes)

### Batch 2 (depends on Batch 1): 8 findings  
H3 (socket perms), H4 (graceful shutdown), H9 (unwrap removal), H10 (SIGHUP), H13 (secrets), H16 (seccomp), H17 (tool gating)

### Batch 3 (functional — depends on Batch 1+2): 12 findings
H11 (shell injection), H12 (path traversal), H14 (circuit breaker), H21-H32 (functional stubs)

### Batch 4 (monitoring — depends on Batch 2): 3 findings
H18 (OpenTelemetry), H19 (alerts), H20 (trace IDs)

### Batch 5 (verify — depends on all): 2 findings
H34 (verify image), H25 (verify protocol)

## Success Criteria per finding
Every finding is fixed when:
1. Code change is committed
2. Test exists that verifies the fix
3. Test passes
4. CI check (if applicable) exists

## Rollback safety
Each batch is independently committable and revertible. No finding's fix breaks another.
