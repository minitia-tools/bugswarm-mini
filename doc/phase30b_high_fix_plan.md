# Phase 30-B: HIGH Production Findings Remediation Plan (CORRECTED)

**Source**: `/root/a/production_audit.md` — 41 HIGH findings
**Status**: 2 findings already fixed by LOW/MEDIUM batches (H34+L1, H36+L16) → **39 remaining**
**Critique applied**: `/root/a/phase30b_plan_critique.md` — all 26 fixes incorporated (5 CRITICAL/HIGH, 12 MEDIUM, 5 LOW, 3 IMMUNITY, 1 COUNT)
**Revised effort estimate**: 70-90 hours (increased due to corrected PoC generation spec, feature-gating, and integration testing requirements)

---

## Section 1: Executive Summary

This plan remediates 39 HIGH-severity production findings across the BugSwarm fuzzing platform. Organized into 6 dependency-ordered batches (Batch 0 through Batch 5), with a critical pre-requisites section that must be completed before any batch begins.

**Why 39, not 41**: H34 (default config image) was fixed by L1 (sandbox-base.Dockerfile creation). H36 (container removal errors) was fixed by L16 (proper error propagation). Both moved to Batch 5 for verification-only.

**Corrected dependency order** (G23 fix: security-critical fixes moved to Batch 0):
- **Batch 0** — SECURITY CRITICAL (P0, 2 findings): H11, H12
- **Batch 1** — BUILD + INFRA (12 findings): H1, H2, H5, H6, H8, H15, H33, H35, H37, H38, H39, H40, H41 (note: H1/H2 corrected per G9/G4)
- **Batch 2** — DAEMON RESILIENCE (10 findings): H3, H4, H7, H9, H10, H13, H14, H16, H17, H19
- **Batch 3** — FUNCTIONAL WIRING (12 findings): H21, H22, H23, H24, H25, H26, H27, H28, H29, H30, H31, H32
- **Batch 4** — MONITORING + ORCHESTRATION (4 findings): H18, H20, docker-compose (G18 fix), H19(verify)
- **Batch 5** — VERIFICATION (3 findings): H34(verify), H25(verify), H30(verify)

---

## Section 2: Critical Pre-Requisites (Must Fix Before ANY Batch)

These are cross-cutting bugs in the fixes themselves. Implementing any batch without addressing these first would produce broken code. All 5 pre-requisite corrections are incorporated into the batch implementations below.

---

### CRIT-PRE1 (G2 fix): H4 Signal Handler — Correct tokio::select! Pattern

**Problem**: The original plan's code wraps the entire accept loop in a `tokio::select!` — the accept loop is an infinite future that never yields `Poll::Ready`, so the signal branch is polled only once and never again. The daemon would be unkillable via SIGTERM.

**Correct code** (used in H4 in Batch 2): The `tokio::select!` must be INSIDE the loop, polling individual `listener.accept()` calls:

```rust
// In bugswarm-sandbox/src/daemon.rs, bugswarm-cpg/src/daemon.rs, bugswarm-evidence/src/daemon.rs
// Replace the entire accept loop block with:

loop {
    tokio::select! {
        result = listener.accept() => {
            match result {
                Ok((stream, addr)) => {
                    let daemon_state = state.clone();
                    tokio::spawn(async move {
                        handle_connection(stream, addr, daemon_state).await;
                    });
                }
                Err(e) => {
                    if shutting_down.load(Ordering::Relaxed) {
                        break;
                    }
                    tracing::error!(error = %e, "Accept error");
                }
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("SIGTERM/SIGINT received — initiating graceful shutdown");
            shutting_down.store(true, Ordering::SeqCst);
            break;
        }
    }
}

// Drain active connections with hard timeout (G26 fix):
tracing::info!("Draining {} active connections...", active_connections.load(Ordering::Relaxed));
let drain_result = tokio::time::timeout(
    Duration::from_secs(5),
    drain_active_connections(&active_handles),
).await;

match drain_result {
    Ok(()) => tracing::info!("All connections drained successfully"),
    Err(_) => tracing::warn!("Drain timeout after 5s — forcing shutdown"),
}

// Cleanup: close listener, remove socket file, remove PID file
shutdown_cleanup(&state).await;
tracing::info!("Shutdown complete");
```

**Affected files**: `bugswarm-sandbox/src/daemon.rs:55-90`, `bugswarm-cpg/src/daemon.rs:65-100`, `bugswarm-evidence/src/daemon.rs:70-105`

---

### CRIT-PRE2 (G9 fix): H1 Dockerfiles — Correct Base Image (FROM debian:bookworm-slim)

**Problem**: The original plan suggested `FROM scratch` but Rust binaries dynamically link to glibc by default. Without static linking (`RUSTFLAGS="-C target-feature=+crt-static"`), `FROM scratch` fails with "exec format error". Simpler: use `FROM debian:bookworm-slim` for all images.

**Correct Dockerfile for CPG Daemon** (`bugswarm-cpg/Dockerfile` — NEW):

```dockerfile
FROM rust:1.77-bookworm AS builder
WORKDIR /root/a
COPY . .
RUN cargo build --release -p bugswarm-cpg

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates zlib1g && rm -rf /var/lib/apt/lists/*
COPY --from=builder /root/a/target/release/bugswarm-cpg /usr/local/bin/bugswarm-cpg
ENTRYPOINT ["bugswarm-cpg"]
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD bugswarm-cpg health || exit 1
```

**Correct Dockerfile for Evidence Daemon** (`bugswarm-evidence/Dockerfile` — NEW):

```dockerfile
FROM rust:1.77-bookworm AS builder
WORKDIR /root/a
COPY . .
RUN cargo build --release -p bugswarm-evidence

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates zlib1g && rm -rf /var/lib/apt/lists/*
COPY --from=builder /root/a/target/release/bugswarm-evidence /usr/local/bin/bugswarm-evidence
ENTRYPOINT ["bugswarm-evidence"]
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD bugswarm-evidence health || exit 1
```

**Correct Dockerfile for Symbolic Daemon** (`bugswarm-symbolic/Dockerfile` — ENHANCE existing):

```dockerfile
FROM rust:1.77-bookworm AS builder
RUN apt-get update && apt-get install -y cmake libz3-dev
WORKDIR /root/a
COPY . .
RUN cargo build --release -p bugswarm-symbolic

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates zlib1g libz3-4 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /root/a/target/release/bugswarm-symbolic /usr/local/bin/bugswarm-symbolic
ENTRYPOINT ["bugswarm-symbolic"]
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD bugswarm-symbolic health || exit 1
```

**Immunity**: CI `docker build bugswarm-cpg/ && docker build bugswarm-evidence/ && docker build bugswarm-symbolic/` succeeds. Smoke test: `docker run --rm bugswarm-cpg --version` prints version and exits 0.

---

### CRIT-PRE3 (G4 fix): H2 sandbox-python-asan — Execution Base Image, NOT a Service

**Problem**: The original plan added CMD, ENTRYPOINT, and HEALTHCHECK to an execution container. This image is an execution environment — it runs a single PoC script and exits. Adding CMD/ENTRYPOINT would make it print a message and exit. Adding HEALTHCHECK to a short-lived container is meaningless.

**Correct fix** (`bugswarm-sandbox/docker/sandbox-python-asan.Dockerfile`):

```dockerfile
# This is an EXECUTION BASE IMAGE — used as the runtime for sandboxed PoC execution.
# It is NOT a service. No CMD, ENTRYPOINT, or HEALTHCHECK.
# The orchestration layer specifies the command: python3 /sandbox/poc.py

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    python3 \
    python3-dev \
    gcc \
    clang \
    libasan6 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /sandbox
LABEL org.bugswarm.version="1.0.0"
LABEL org.bugswarm.description="Python sandbox with AddressSanitizer for PoC execution"
LABEL org.bugswarm.type="execution-base"
```

**Immunity**: CI: `docker inspect bugswarm/sandbox-python-asan:latest | jq '.[0].Config.Cmd'` is `null` and `.[0].Config.Entrypoint` is `null`. CI: `docker run --rm bugswarm/sandbox-python-asan:latest python3 -c "import sys; print(sys.version)"` works.

---

### CRIT-PRE4 (G5 fix): H16 Seccomp — Verify prlimit64 Before Removal

**Problem**: Removing `prlimit64` from allowed seccomp syscalls may break Python's `getrlimit()` call at startup. Docker sets ulimits via cgroups, but CPython calls `getrlimit()` during initialization. If `prlimit64` is blocked, Python may crash with SIGSYS.

**Verification procedure** (run BEFORE removing prlimit64):

```bash
# Step 1: Run test container with prlimit64 explicitly denied
docker run --rm \
  --security-opt seccomp=<(jq '.syscalls[0].names -= ["prlimit64"]' bugswarm-sandbox/seccomp/default.json) \
  bugswarm/sandbox-python-asan:latest \
  python3 -c "
import sys
import resource
print('recursion limit:', sys.getrecursionlimit())
print('RLIMIT_NOFILE:', resource.getrlimit(resource.RLIMIT_NOFILE))
print('OK')
"

# Step 2: If Step 1 exits 0 and prints "OK", prlimit64 can be safely removed.
# Step 3: If Step 1 exits with SIGSYS or non-zero, keep prlimit64 in allowed list.
```

**Correct fix — if Python breaks without prlimit64**: Keep prlimit64 in the allowed list with a documentation comment. The allowed syscalls block gains:

```json
{
  "names": ["read", "write", "open", ..., "prlimit64"],
  "action": "SCMP_ACT_ALLOW",
  "comment": "prlimit64: Required by Python resource.getrlimit at startup. Container constrained by Docker ulimits."
}
```

**If Python works without prlimit64**: Remove from allowed list, add to explicit deny block.

**Immunity**: `tests/integration/test_seccomp.py::test_python_startup_under_seccomp` and `tests/integration/test_seccomp.py::test_fork_blocked`.

---

### CRIT-PRE5 (G15 fix): H21 PoC Generation Specification

**Problem**: The original plan says "create a PoC that calls the target function" with zero detail on HOW. PoC generation is the most complex functional fix and requires a detailed algorithm.

**Detailed algorithm** (used in H21 and H22 in Batch 3):

```
Algorithm: generate_poc_and_execute(function_name, input_values, manager)

Input:
  - function_name: str (e.g., "mypackage.mymodule.myfunc")
  - input_values: List[Any] (generated input arguments)
  - manager: SandboxManager (container execution handle)

Steps:
  1. Query CPG for function signature:
     signature = cpg_client.extract_function_signature(function_name)
     // Returns: { "module_path": "mypackage.mymodule", "func_name": "myfunc",
     //            "param_types": ["int", "str", "float"] }

  2. If CPG unavailable, fall back to heuristic:
     // Parse function_name: split on last ".", left = module, right = func
     // Assume all params are "Any" type — still executable

  3. Generate PoC code:
     module_path = signature["module_path"]
     func_name = signature["func_name"]
     param_types = signature["param_types"]

     args_literals = []
     for i, val in enumerate(input_values):
         param_type = param_types[i] if i < len(param_types) else "Any"

         // Format value as valid Python literal based on type:
         if param_type in ("int", "float", "bool"):
             formatted = repr(val)      // e.g., 42, 3.14, True
         elif param_type == "str":
             formatted = repr(val)      // properly escaped: "hello\nworld"
         elif param_type == "NoneType":
             formatted = "None"
         elif param_type in ("list", "tuple", "dict", "set"):
             formatted = repr(val)
         else:
             formatted = repr(val)      // fallback
         args_literals.append(formatted)

     poc_code = f'''
import sys
import json
from {module_path} import {func_name}

try:
    result = {func_name}({", ".join(args_literals)})
    print("BGSWRM_OUT:" + json.dumps({{"status": "ok", "value": repr(result)}}))
except Exception as e:
    print("BGSWRM_OUT:" + json.dumps({{"status": "error", "type": type(e).__name__, "message": str(e)}}))
    sys.exit(1)
'''

  4. Execute via manager:
     receipt = manager.execute(poc_code, env={}, timeout=30)
     // manager.execute handles: write poc.py to temp file, bind-mount,
     // run container, capture stdout/stderr/exit_code

  5. Parse stdout for return value:
     for line in receipt.stdout.split('\n'):
         if line.startswith("BGSWRM_OUT:"):
             output = json.loads(line[len("BGSWRM_OUT:"):])
             break

  6. Build ExecutionTrace from parsed output:
     trace = ExecutionTrace {
         function: function_name,
         input_values: input_values,
         output: output.get("value"),
         exception: output.get("message"),
         exception_type: output.get("type"),
         exit_code: receipt.exit_code,
         execution_time_ms: receipt.duration_ms,
     }
     return trace
```

**Immunity**: Test `tests/unit/test_poc_generation.py::test_generate_poc_for_known_function` — given function with 3 params (int, str, bool), input values [42, "hello", True], asserts generated code contains `myfunc(42, "hello", True)`. Test `tests/integration/test_invariant_mining.py::test_real_execution_on_target` — mines invariants on actual test target, verifies ExecutionTrace contains real output values.

---

## Section 3: Implementation Plan — Corrected Batches

---

### Batch 0 — SECURITY CRITICAL (P0, 2 findings)

**Rationale**: H11 (shell injection) allows arbitrary command execution escaping the sandbox. H12 (path traversal) allows reading any file on the host. Every minute these exist is exposure. Fixed first, deployed immediately.

**Pre-requisites**: None.

**Findings**: H11, H12

**Batch verification checklist**:
- [ ] `grep -r "BUGSWARM_EOF" bugswarm-sandbox/src/` returns zero matches
- [ ] Fuzz test runs 10,000 iterations with zero shell escapes
- [ ] `read_file("/etc/shadow")` returns error, not file contents
- [ ] All 12 test cases in path traversal test matrix pass
- [ ] `grep -r 'startswith("/")' bugswarm-agent/src/` returns zero matches (old bypass check removed)

---

#### H11: Shell injection via heredoc delimiter (BUGSWARM_EOF attack)

**Category**: Security
**Root cause**: PoC content containing "BUGSWARM_EOF" on its own line exits the heredoc early, executing subsequent lines as shell commands. Container command uses `python3 << 'BUGSWARM_EOF'` to inline the PoC.

**Permanent fix** (G12 corrected — complete specification): Write PoC to temp file on host, bind-mount to `/sandbox/poc.py` inside container. Container command becomes `python3 /sandbox/poc.py`. Remove heredoc mechanism entirely.

**Files**: `bugswarm-sandbox/src/container.rs:274-276`

**Exact code**:

```rust
// REMOVE old heredoc-based command:
// let cmd = format!("python3 << 'BUGSWARM_EOF'\n{}\nBUGSWARM_EOF", poc_content);

// NEW: Write PoC to temp file, bind-mount it
use uuid::Uuid;
use std::io::Write;

let poc_filename = format!("poc-{}.py", Uuid::new_v4());
let poc_host_path = std::env::temp_dir().join("bugswarm").join(&poc_filename);

// Ensure /tmp/bugswarm/ exists
std::fs::create_dir_all(poc_host_path.parent().unwrap())?;

// Write PoC content
let mut file = std::fs::File::create(&poc_host_path)
    .context("Failed to create PoC temp file")?;
file.write_all(poc_content.as_bytes())
    .context("Failed to write PoC content")?;

// Bind-mount host path -> /sandbox/poc.py in container
let container_path = PathBuf::from("/sandbox/poc.py");
let bind_mount = docker::BindMount {
    source: poc_host_path.clone(),
    target: container_path.clone(),
    read_only: true,
};

// Container command: just the interpreter + mounted file
let cmd = vec![
    "python3".to_string(),
    "/sandbox/poc.py".to_string(),
];

// Add bind mount to container config
container_config.add_bind_mount(bind_mount);

// Register cleanup: delete temp file after container exits
let cleanup_path = poc_host_path.clone();
manager.register_post_hook(move || {
    let _ = std::fs::remove_file(&cleanup_path);
});
```

**Immunity** (G25 fix — fuzz testing approach specified):

Fuzz test: `tests/fuzz/test_shell_injection.py` — uses Python `hypothesis` to generate strings containing the `BUGSWARM_EOF` delimiter in various positions (alone on a line, prefixed, suffixed, repeated, nested). Each generated input is wrapped in a PoC template and submitted for sandbox execution. Test asserts: (1) no unexpected shell commands execute, (2) PoC output matches expected output, (3) exit code matches expected. Run with `python -m hypothesis` or `cargo-fuzz`.

Integration test: `tests/integration/test_sandbox_execution.py::test_bind_mount_heredoc_free` — submits a PoC containing `BUGSWARM_EOF\nos.system("echo pwned")`, verifies "pwned" is NOT present in output, verifies no child processes spawned inside container.

**Success criteria**:
- `grep -r "BUGSWARM_EOF" bugswarm-sandbox/src/` returns zero matches
- Fuzz test runs 10,000 iterations with zero shell escapes
- Integration test confirms PoC with delimiter executes normally
- No regression on existing sandbox execution tests

---

#### H12: Path traversal in agent read_file (absolute path and ../ bypass)

**Category**: Security
**Root cause**: Agent's `read_file` tool allowed absolute paths if file existed: `if path.startswith("/") && Path(path).exists():`. Allows reading any host file. Relative paths with `../` resolve above repo root.

**Permanent fix** (G1 corrected — reject BEFORE resolution, test against ALL vectors):

```python
# File: bugswarm-agent/src/agent/core.py
# File: bugswarm-agent/src/agent/cli/wiring.py

from pathlib import Path

def _validate_sandbox_path(repo_root: Path, user_path: str) -> Path:
    """
    Resolve user path relative to repo_root. Reject absolute paths and
    '../' patterns BEFORE any filesystem resolution. The resolved path
    MUST be a descendant of repo_root.
    """
    # Reject empty paths
    if not user_path or not user_path.strip():
        raise ValueError("Path must be non-empty and relative to repository root")

    # Reject absolute paths (starts with /)
    if user_path.startswith("/"):
        raise ValueError(
            f"Absolute paths are forbidden. Path must be relative to repository root. "
            f"Got: {user_path}"
        )

    # Reject paths containing '..' segments (traversal attempt)
    # Catches: ../etc/passwd, subdir/../../etc/passwd, ./../../etc/passwd
    normalized = user_path.replace("\\", "/")
    segments = normalized.split("/")
    if ".." in segments:
        raise ValueError(
            f"Path traversal blocked: '../' sequences are forbidden. Got: {user_path}"
        )

    # Resolve relative to repo_root
    resolved = (repo_root / user_path).resolve()

    # Belt-and-suspenders: verify resolved path is within repo_root
    repo_root_resolved = repo_root.resolve()
    try:
        resolved.relative_to(repo_root_resolved)
    except ValueError:
        raise ValueError(
            f"Path traversal blocked: resolved path '{resolved}' is outside "
            f"repository root '{repo_root_resolved}'"
        )

    # Verify the resolved file exists (for read_file semantics)
    if not resolved.exists():
        raise FileNotFoundError(f"File not found: {user_path} (resolved to {resolved})")

    return resolved


def _read_file(repo_root: str, path: str) -> ToolResult:
    try:
        root = Path(repo_root)
        safe_path = _validate_sandbox_path(root, path)
        content = safe_path.read_text()
        return ToolResult(True, content, file_path=str(safe_path))
    except ValueError as e:
        return ToolResult(False, str(e))
    except FileNotFoundError as e:
        return ToolResult(False, str(e))
```

**Immunity** (G24 fix — automated regression test matrix):

Test file: `tests/unit/test_read_file_path_validation.py`

```python
import pytest
from pathlib import Path

# Test matrix — covers ALL traversal vectors
TEST_CASES = [
    # (input_path, expected_error_keyword, description)
    ("/etc/passwd", "Absolute paths are forbidden", "absolute path root fs"),
    ("/root/.ssh/id_rsa", "Absolute paths are forbidden", "absolute path home dir"),
    ("../etc/passwd", "traversal blocked", "parent directory traversal"),
    ("../../etc/passwd", "traversal blocked", "double parent traversal"),
    ("subdir/../../etc/passwd", "traversal blocked", "nested parent traversal"),
    ("./../../etc/passwd", "traversal blocked", "dot-then-traversal"),
    ("subdir/../sibling/file.py", "traversal blocked", "parent-dot sibling"),
    ("valid/file.py", None, "legitimate relative path succeeds"),
    ("src/main.rs", None, "root-level relative path succeeds"),
    ("subdir/nested/file.py", None, "nested relative path succeeds"),
    ("", "non-empty", "empty path rejected"),
    ("   ", "non-empty", "whitespace-only path rejected"),
]

def test_all_path_traversal_vectors(tmp_path):
    """Verify all traversal vectors are blocked."""
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "valid").mkdir()
    (repo / "valid" / "file.py").write_text("print('ok')")
    (repo / "src").mkdir()
    (repo / "src" / "main.rs").write_text("fn main() {}")
    (repo / "subdir").mkdir()
    (repo / "subdir" / "nested").mkdir(parents=True)
    (repo / "subdir" / "nested" / "file.py").write_text("x=1")

    for input_path, expected_error, desc in TEST_CASES:
        try:
            result = _validate_sandbox_path(repo, input_path)
            if expected_error is not None:
                pytest.fail(
                    f"{desc}: expected error containing '{expected_error}', "
                    f"but got success: {result}"
                )
            else:
                assert result.exists(), f"{desc}: resolved path must exist"
        except (ValueError, FileNotFoundError) as e:
            if expected_error is None:
                pytest.fail(f"{desc}: expected success, but got error: {e}")
            else:
                assert expected_error in str(e), \
                    f"{desc}: expected '{expected_error}' in error, got: {e}"
```

**CI check**: `tests/unit/test_read_file_path_validation.py` runs on every PR. Any code change that removes or weakens path validation fails CI.

**Success criteria**:
- All 12 test cases in test matrix pass
- `grep -r 'startswith("/")' bugswarm-agent/src/` returns zero matches
- Integration test: agent `read_file("/etc/shadow")` returns error, not file contents

---

### Batch 1 — BUILD + INFRA (12 findings)

**Rationale**: Build system, versioning, containerization, and code quality fixes. Independent of daemon logic changes. Can be parallelized.

**Pre-requisites**: None (CRIT-PRE2 and CRIT-PRE3 corrections applied to H1 and H2).

**Findings**: H1, H2, H5, H6, H8, H15, H33, H35, H37, H38, H39, H40, H41

**Batch verification checklist**:
- [ ] `docker build bugswarm-cpg/` succeeds
- [ ] `docker build bugswarm-evidence/` succeeds
- [ ] `docker build bugswarm-symbolic/` succeeds
- [ ] `docker build bugswarm-sandbox/docker/` succeeds
- [ ] `cargo build --workspace` succeeds with upper-bounded deps
- [ ] `cargo test --workspace` passes all existing tests
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo audit` passes
- [ ] `git tag -l "v1.0.0"` shows tag exists
- [ ] `CHANGELOG.md` exists with all phase entries

---

#### H1: Rust daemons have no Dockerfiles

**Category**: Deployment
**Root cause**: Only sandbox Dockerfiles exist. CPG, Evidence, and Symbolic daemons run bare-metal.
**Permanent fix** (G9 corrected): Create multi-stage Dockerfiles using `FROM debian:bookworm-slim` (NOT `FROM scratch` — avoids static linking requirement). Install runtime deps: `ca-certificates`, `zlib1g`. Copy binary from builder. Set ENTRYPOINT and HEALTHCHECK.

**Files**: `bugswarm-cpg/Dockerfile` (NEW), `bugswarm-evidence/Dockerfile` (NEW), `bugswarm-symbolic/Dockerfile` (ENHANCE)

**Exact code**: See CRIT-PRE2 above for all three complete Dockerfiles.

**Immunity**: CI workflow `docker-build` job builds all 4 images. Smoke test: `docker run --rm bugswarm-cpg --version` prints version and exits 0. CI check: script `ci/check_dockerfiles.sh` verifies every crate with `main.rs` has a `Dockerfile`.

**Success criteria**: All 4 Docker images build and their `--version` flag works.

---

#### H2: sandbox-python-asan.Dockerfile has no CMD/ENTRYPOINT (CORRECTED)

**Category**: Deployment
**Root cause**: Container was defined as base image only. Original plan incorrectly treated it as a service image.
**Permanent fix** (G4 corrected): This is an EXECUTION BASE IMAGE. Add `WORKDIR /sandbox`. Add `LABEL` with version and type metadata. NO CMD/ENTRYPOINT/HEALTHCHECK (orchestration layer provides command at runtime).

**Files**: `bugswarm-sandbox/docker/sandbox-python-asan.Dockerfile`

**Exact code**: See CRIT-PRE3 above.

**Immunity**: CI: `docker inspect bugswarm/sandbox-python-asan:latest | jq '.[0].Config.Cmd'` is `null` and `.[0].Config.Entrypoint` is `null`. CI: `docker run --rm bugswarm/sandbox-python-asan:latest python3 -c "import sys; print(sys.version)"` works.

**Success criteria**: Execution container works as base image. Orchestration can run `docker run --rm bugswarm/sandbox-python-asan:latest python3 /sandbox/poc.py`.

---

#### H5: No CI/CD pipeline

**Category**: DevOps
**Root cause**: No `.github/workflows/` directory exists.
**Permanent fix** (G10 corrected): Create CI pipeline with build, test, lint, security audit (with `cargo install cargo-audit` step first), Docker build for all 4 images.

**Files**: `.github/workflows/ci.yml` (NEW)

**Exact code**:

```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - name: Build
        run: cargo build --workspace --all-features
      - name: Test
        run: cargo test --workspace --all-features
      - name: Clippy
        run: cargo clippy --workspace --all-features -- -D warnings
      - name: Format check
        run: cargo fmt --all -- --check
      - name: Install cargo-audit
        run: cargo install cargo-audit --locked
      - name: Security audit
        run: cargo audit --deny warnings

  docker-build:
    runs-on: ubuntu-latest
    needs: build-test
    steps:
      - uses: actions/checkout@v4
      - name: Build sandbox base image
        run: docker build -t bugswarm/sandbox-python-asan:latest bugswarm-sandbox/docker/
      - name: Build CPG image
        run: docker build -t bugswarm/cpg:latest bugswarm-cpg/
      - name: Build Evidence image
        run: docker build -t bugswarm/evidence:latest bugswarm-evidence/
      - name: Build Symbolic image
        run: docker build -t bugswarm/symbolic:latest bugswarm-symbolic/
      - name: Verify images
        run: |
          docker run --rm bugswarm/cpg:latest --version
          docker run --rm bugswarm/evidence:latest --version
          docker run --rm bugswarm/symbolic:latest --version
```

**Immunity**: CI fails on any regression. Branch protection requires CI pass before merge.

**Success criteria**: CI workflow runs on every PR. All jobs pass. Badge in README shows green.

---

#### H6: No version number beyond 0.1.0. No git tags.

**Category**: Build
**Root cause**: Version never incremented from initial `cargo init`.
**Permanent fix**: Set version to `1.0.0` in all Cargo.toml. Create git tag `v1.0.0`. Add version output to all daemon binaries.

**Files**: All 4 Cargo.toml files, root Cargo.toml, all main.rs files

**Implementation**: Replace `version = "0.1.0"` with `version = "1.0.0"`. Add version flag to each daemon's main.rs:

```rust
if args.flag_version {
    println!("bugswarm-{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    std::process::exit(0);
}
```

`git tag -a v1.0.0 -m "Release v1.0.0"`

**Immunity**: CI: `cargo pkgid | head -1 | grep -q "1.0.0$"`. CI: `git describe --tags --exact-match HEAD | grep -q "v1.0.0"`.

**Success criteria**: All daemons print correct version. Git tag exists.

---

#### H8: No auto-restart for Rust daemons. Panic = dead process.

**Category**: Deployment
**Root cause**: No process supervisor configured.
**Permanent fix** (G19 awareness): Provide systemd unit files with `Type=simple` (NOT `Type=notify` — daemons don't implement sd_notify), `Restart=always`, `RestartSec=5`, `PIDFile=` pointing to known path. Stale PID file handling: on SIGKILL, Drop does NOT run — stale PID remains. Mitigation: systemd's `Restart=always` restarts daemon, creates new PID file (overwrites stale). Document this.

**Files**: `deploy/systemd/bugswarm-sandbox.service` (NEW), `deploy/systemd/bugswarm-cpg.service` (NEW), `deploy/systemd/bugswarm-evidence.service` (NEW)

**Exact code** (`deploy/systemd/bugswarm-sandbox.service`):

```ini
[Unit]
Description=BugSwarm Sandbox Daemon
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
User=bugswarm
Group=bugswarm
ExecStart=/usr/local/bin/bugswarm-sandbox daemon
ExecStop=/bin/kill -TERM $MAINPID
Restart=always
RestartSec=5
PIDFile=/var/run/bugswarm/sandbox.pid
StandardOutput=journal
StandardError=journal
SyslogIdentifier=bugswarm-sandbox
LimitNOFILE=65536
LimitNPROC=4096
NoNewPrivileges=yes
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
```

**Immunity**: Integration test `tests/integration/test_daemon_restart.py` — start daemon, `kill -9 $PID`, verify new process appears within 5s, new PID file contains new PID, daemon responds to requests.

**Success criteria**: All 3 systemd services enable and start. Kill test confirms restart within 5s.

---

#### H15: 16 unsafe blocks in danger_map.rs. No audit trail.

**Category**: Code Quality
**Root cause**: POSIX shared memory operations require unsafe blocks, but none have safety documentation.
**Permanent fix**: Wrap ALL unsafe blocks with `// SAFETY:` comments documenting preconditions. Add `debug_assert!` guards.

**Files**: `bugswarm-sandbox/src/danger_map.rs` (all 16 unsafe blocks)

**Template for each unsafe block**:

```rust
// SAFETY: mmap is called with all documented invariants satisfied:
//   - fd: valid file descriptor returned by shm_open (checked for -1 above)
//   - addr: ptr::null_mut() — kernel chooses address
//   - length: size from fstat, verified > 0
//   - prot: PROT_READ | PROT_WRITE — no execute permission
//   - flags: MAP_SHARED — shared memory requires shared mapping
//   - offset: 0 — no offset in fd
// The returned pointer is wrapped in MmapGuard that handles munmap on Drop.
let ptr = unsafe { libc::mmap(...) };
debug_assert_ne!(ptr, libc::MAP_FAILED, "mmap failed — precondition violated");
```

**Immunity**: CI script `ci/check_safety_comments.sh` — scans all `.rs` files for `unsafe {` or `unsafe fn` and verifies preceding non-empty line contains `// SAFETY:`. Fails CI if any unsafe block lacks a safety comment.

**Success criteria**: Every unsafe block has a `// SAFETY:` comment. CI enforces for all future changes.

---

#### H33: Floating dependency versions

**Category**: Build
**Root cause**: `tokio = "1.41"` means `>=1.41, <2.0` — allows major version bumps within SemVer-incompatible ranges.
**Permanent fix**: Upper-bound ALL workspace dependencies to `<NEXT_MAJOR`.

**Files**: Root `Cargo.toml` `[workspace.dependencies]`

**Exact code**:

```toml
[workspace.dependencies]
tokio = { version = ">=1.41, <2.0", features = ["full"] }
serde = { version = ">=1.0, <2.0", features = ["derive"] }
serde_json = ">=1.0, <2.0"
anyhow = ">=1.0, <2.0"
tracing = ">=0.1, <1.0"
tracing-subscriber = ">=0.3, <0.4"
once_cell = ">=1.19, <2.0"
regex = ">=1.10, <2.0"
uuid = { version = ">=1.10, <2.0", features = ["v4"] }
clap = { version = ">=4.5, <5.0", features = ["derive"] }
bollard = ">=0.17, <0.18"
libc = ">=0.2, <0.3"
```

**Immunity**: CI: `cargo update --dry-run` must produce zero changes to `Cargo.lock`. CI nightly: `cargo update -Z minimal-versions && cargo build` verifies lower bounds.

**Success criteria**: All deps upper-bounded. CI verifies no unpinned major version changes.

---

#### H35: No CHANGELOG

**Category**: Documentation
**Root cause**: Never created.
**Permanent fix**: Create `CHANGELOG.md` with all phase completion entries.

**Files**: `CHANGELOG.md` (NEW)
**Immunity**: CI: `CHANGELOG.md` must exist in repo root.

**Success criteria**: CHANGELOG exists with all completed phase entries.

---

#### H37: Regex::new().unwrap() in differential.rs — per-call compile can panic

**Category**: Code Quality
**Root cause**: `Regex::new(PATTERN).unwrap()` called on every differential check. Panics on invalid pattern; recompiles from scratch each call.

**Permanent fix**: Use `once_cell::sync::Lazy<Regex>` — compile once at startup.

**Files**: `bugswarm-sandbox/src/differential.rs`

**Exact code**:

```rust
use once_cell::sync::Lazy;
use regex::Regex;

static DIFF_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"some_pattern").expect("Invalid differential regex pattern — fix immediately")
});

// Usage: replace all Regex::new(PATTERN).unwrap() with &*DIFF_REGEX
fn check_differential(output: &str) -> bool {
    DIFF_REGEX.is_match(output)
}
```

**Immunity**: Test `tests/unit/test_differential.rs::test_regex_compiled_once` — verifies regex is static and matches expected patterns. Test `tests/unit/test_differential.rs::test_regex_does_not_compile_per_call` — checks `&DIFF_REGEX as *const _` returns same pointer across calls.

**Success criteria**: No `Regex::new(` in non-test production code within differential.rs. Static Lazy used.

---

#### H38: ssa.rs:208 — s.chars().next().unwrap() panics on empty string

**Category**: Code Quality
**Root cause**: `.unwrap()` on `Option` from `.next()` on empty string iterator.

**Permanent fix**: Replace with `.ok_or_else()`.

**Files**: `bugswarm-cpg/src/ssa.rs:208`

**Exact code**:

```rust
// Before:
// let first_char = s.chars().next().unwrap();

// After:
let first_char = s.chars().next()
    .ok_or_else(|| anyhow::anyhow!("Empty string in SSA variable name at position {}", pos))?;
```

**Immunity**: Test `tests/unit/test_ssa.rs::test_parse_empty_variable_returns_error` — passes empty string, asserts returns `Err` with message containing "Empty string".

**Success criteria**: No `.unwrap()` on `.next()` calls in ssa.rs. Empty input produces clean error, not panic.

---

#### H39: parser.rs:954,960 — regex .unwrap() on compile

**Category**: Code Quality
**Root cause**: Same pattern as H37 — `Regex::new().unwrap()` called frequently, panics on bad pattern.

**Permanent fix**: Use `once_cell::sync::Lazy<Regex>` for all parser regex patterns.

**Files**: `bugswarm-cpg/src/parser.rs`

**Exact code**:

```rust
use once_cell::sync::Lazy;
use regex::Regex;

static FUNCTION_CALL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^pattern$").expect("Invalid parser regex FUNCTION_CALL_REGEX")
});

static IDENTIFIER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").expect("Invalid parser regex IDENTIFIER_REGEX")
});

// Usage: replace Regex::new(...).unwrap() with &*FUNCTION_CALL_REGEX etc.
```

**Immunity**: Test `tests/unit/test_parser.rs::test_all_regex_patterns_compile` and `tests/unit/test_parser.rs::test_regex_patterns_correctness`.

**Success criteria**: No `Regex::new(` in non-test production code within parser.rs. All regex patterns are Lazy statics.

---

#### H40: main.rs:94 — unwrap() in main binary on serialization

**Category**: Code Quality
**Root cause**: `serde_json::to_string_pretty(&stats).unwrap()` panics if Stats struct can't be serialized.

**Permanent fix**: Replace with `.context("Failed to serialize stats")?`.

**Files**: `bugswarm-evidence/src/main.rs:94`

**Exact code**:

```rust
// Before:
// let json = serde_json::to_string_pretty(&stats).unwrap();

// After:
let json = serde_json::to_string_pretty(&stats)
    .context("Failed to serialize evidence stats to JSON")?;
```

**Immunity**: Test `tests/unit/test_evidence_main.rs::test_stats_serialization_handles_error` — creates Stats with edge-case fields, verifies serialization succeeds or returns Err with context.

**Success criteria**: No `unwrap()` on serialization calls in main.rs.

---

#### H41: shm_map returns Result<*mut u8, String> — inconsistent error type

**Category**: Code Quality
**Root cause**: Uses `String` as error type instead of `anyhow::Error`.

**Permanent fix**: Change return type to `Result<*mut u8, anyhow::Error>`.

**Files**: `bugswarm-sandbox/src/danger_map.rs`

**Exact code**:

```rust
// Before:
// pub fn shm_map(name: &str) -> Result<*mut u8, String> {
//     return Err(format!("shm_open failed: {}", errno));

// After:
use anyhow::{Context, Result};

pub fn shm_map(name: &str) -> Result<*mut u8> {
    // All error returns use anyhow::bail! or .context()
    // Before: return Err(format!(...));
    // After:  anyhow::bail!("shm_open({}) failed: errno={}", name, errno);
}

// Update all callers to use anyhow::Result
```

**Immunity**: Test `tests/unit/test_danger_map.rs::test_shm_map_error_is_anyhow` — verifies error type is `anyhow::Error`. Test `tests/unit/test_danger_map.rs::test_shm_map_error_contains_context` — verifies error message includes SHM segment name.

**Success criteria**: No `Result<_, String>` return type in danger_map.rs. All errors are `anyhow::Error` with context.

---

### Batch 2 — DAEMON RESILIENCE (10 findings)

**Rationale**: Daemon robustness, error handling, signal handling, circuit breakers, tool gating, and alerting. Depends on infrastructure from Batch 1.

**Pre-requisites**: Batch 1 complete. H12 fix applied (required for H17 tool gating — G6 fix).

**Findings**: H3, H4, H7, H9, H10, H13, H14, H16, H17, H19

**Batch verification checklist**:
- [ ] All 3 daemons start and accept connections
- [ ] `kill -TERM $PID` triggers graceful shutdown within 5s (G26: drain timeout prevents hang)
- [ ] `kill -9 $PID` triggers systemd restart within 5s
- [ ] Invalid config file → daemon exits with error code 1, not silent fallback
- [ ] SIGHUP reloads config without restart
- [ ] Seccomp profile blocks fork/clone (tested)
- [ ] Tool gating rejects oversized PoC and absolute paths
- [ ] Alert notifier sends to Slack (if webhook configured)
- [ ] `cargo clippy -- -D warnings` passes on all crates

---

#### H3: Socket parent dir creation silently ignores permission errors

**Category**: Reliability
**Root cause**: `.ok()` swallows `EPERM`/`EACCES` — daemon starts but listener unreachable.

**Permanent fix**: Replace `.ok()` with `?` and `.context()`. Exit with clear error if socket directory cannot be created.

**Files**: `bugswarm-cpg/src/daemon.rs:85`, `bugswarm-sandbox/src/daemon.rs:76`, `bugswarm-evidence/src/daemon.rs:91`

**Exact code**:

```rust
// Before:
// std::fs::create_dir_all(parent).ok();

// After:
let parent = socket_path.parent()
    .context("Socket path has no parent directory")?;
std::fs::create_dir_all(parent)
    .with_context(|| format!("Failed to create socket directory: {}", parent.display()))?;

// Verify directory is writable after creation
let metadata = std::fs::metadata(parent)
    .with_context(|| format!("Failed to stat socket directory: {}", parent.display()))?;
if metadata.permissions().readonly() {
    anyhow::bail!("Socket directory is read-only: {}", parent.display());
}
```

**Immunity**: Integration test `tests/integration/test_daemon_startup.rs::test_startup_fails_when_socket_dir_impermissible` — sets `TMPDIR` to root-owned directory, verifies exit code non-zero, stderr contains "Failed to create socket directory".

**Success criteria**: No `.ok()` on `create_dir_all` in any daemon.rs file.

---

#### H4: Daemon loop has no graceful shutdown (CORRECTED — G2 fix)

**Category**: Reliability
**Root cause**: Daemon main loop is `loop { accept().await }` with no signal handling.

**Permanent fix** (G2 corrected + G26 drain timeout): See CRIT-PRE1 for complete corrected implementation. Key elements: `tokio::select!` INSIDE the loop polling single `accept()` call, `shutting_down` AtomicBool, 5-second drain timeout.

**Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-cpg/src/daemon.rs`, `bugswarm-evidence/src/daemon.rs`

**Exact code**: See CRIT-PRE1 above — the complete corrected implementation.

**Immunity**:
- `tests/integration/test_graceful_shutdown.py::test_clean_shutdown_within_5s` — opens 3 connections, sends SIGTERM, verifies daemon PID disappears within 5 seconds.
- `tests/integration/test_graceful_shutdown.py::test_drain_timeout_on_stuck_connections` — opens connection that never closes, sends SIGTERM, verifies PID disappears within 10 seconds (5s drain + 5s grace).
- `tests/integration/test_graceful_shutdown.py::test_no_new_connections_after_signal` — sends SIGTERM, immediately attempts to connect, verifies connection refused.

**Success criteria**: All 3 daemons shut down cleanly on SIGTERM. Drain timeout prevents indefinite hang. No connections accepted after signal.

---

#### H7: Config parsing failures silently fall back to defaults

**Category**: Reliability
**Root cause**: `unwrap_or_else` swallows config errors — daemon starts with wrong settings.

**Permanent fix** (G11 corrected): If config file exists but is invalid, daemon exits with code 1. Use `tracing::error!` (NOT `eprintln!` — daemon uses structured JSON logging). Fallback to defaults ONLY if config file does not exist.

**Files**: `bugswarm-sandbox/src/main.rs:167-169`, all other daemon main.rs

**Exact code**:

```rust
let config_path = std::path::Path::new("/etc/bugswarm/sandbox.toml");

let config = if config_path.exists() {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Cannot read config: {}", config_path.display()))?;
    match toml::from_str::<Config>(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!(
                path = %config_path.display(),
                error = %e,
                "Configuration file exists but is invalid. Refusing to start with defaults."
            );
            std::process::exit(1);
        }
    }
} else {
    tracing::info!("No config file found at {} — using defaults", config_path.display());
    Config::default()
};
```

**Immunity**: `tests/integration/test_config_parsing.py::test_invalid_config_fails_startup` — malformed TOML, verifies exit code 1, log contains "invalid". `tests/integration/test_config_parsing.py::test_missing_config_uses_defaults` — starts without config file, verifies daemon starts with defaults.

**Success criteria**: Invalid config → exit 1. Missing config → start with defaults. No `eprintln!` in production daemon code.

---

#### H9: Multiple .unwrap() in non-test production paths

**Category**: Robustness
**Root cause**: ~15 unwrap/expect calls in production code across all crates.

**Permanent fix**: Replace all non-test unwraps with proper error handling. Keep only in test code and documented infallible cases.

**Files**: All crate `.rs` files (audit every `.unwrap()` and `.expect()`)

**Patterns**:

```rust
// Pattern 1: In functions returning Result
// Before: let x = some_op().unwrap();
// After:  let x = some_op().context("Failed to do some_op")?;

// Pattern 2: Provably safe (document why):
// Before: let x = vec.first().unwrap();
// After:  let x = vec.first().expect("vec is non-empty per loop invariant at L{n}");
//         // SAFETY: vec is never empty because <reason>

// Pattern 3: Tests (acceptable):
// #[cfg(test)] let x = some_op().unwrap(); // OK
```

**Immunity**: Add to each crate's Cargo.toml:

```toml
[lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
```

Add `#[allow(clippy::unwrap_used, clippy::expect_used)]` to each `#[cfg(test)] mod tests {}` block.

**Success criteria**: `cargo clippy --workspace -- -D clippy::unwrap_used` passes. Remaining unwraps in test code or documented as infallible.

---

#### H10: No SIGHUP handling for config reload

**Category**: Operability
**Root cause**: Configuration read once at startup, never reloaded.

**Permanent fix**: Spawn background task listening for SIGHUP. On signal, reload mutable config fields (log level, timeouts, limits). Structural changes (socket paths) require restart — document.

**Files**: All 3 daemon main.rs files

**Exact code**:

```rust
use tokio::signal::unix::{signal, SignalKind};

// Spawn before accept loop:
let config_reload_handle = {
    let config = config.clone(); // Arc<RwLock<Config>>
    tokio::spawn(async move {
        let mut sighup = signal(SignalKind::hangup())
            .expect("Failed to register SIGHUP handler");
        loop {
            sighup.recv().await;
            tracing::info!("SIGHUP received — reloading configuration");
            match reload_config(&config).await {
                Ok(()) => tracing::info!("Configuration reloaded successfully"),
                Err(e) => tracing::error!(error = %e, "Failed to reload config — keeping current"),
            }
        }
    })
};

async fn reload_config(config: &Arc<RwLock<Config>>) -> anyhow::Result<()> {
    let new_config = read_config_file()?;
    let mut current = config.write().await;
    // Only mutable fields:
    current.log_level = new_config.log_level;
    current.request_timeout_secs = new_config.request_timeout_secs;
    current.max_concurrent_connections = new_config.max_concurrent_connections;
    // Do NOT update: socket_path, pid_file_path (structural — require restart)
    Ok(())
}

// On shutdown: config_reload_handle.abort();
```

**Immunity**: `tests/integration/test_config_reload.py::test_sighup_reloads_log_level` — modify log level, send SIGHUP, verify debug messages appear. `tests/integration/test_config_reload.py::test_sighup_does_not_change_socket_path` — modify socket path, send SIGHUP, verify daemon still on original socket.

**Success criteria**: SIGHUP reloads mutable config fields. Structural fields require restart — documented.

---

#### H13: API keys loaded from env vars only. No secrets manager.

**Category**: Security
**Root cause**: Only env var source. No Docker secrets or file-based provider support.

**Permanent fix**: Add `BGSWARM_SECRETS_PROVIDER` env var: `env` (default), `file` (Docker secrets — reads from `/run/secrets/`).

**Files**: `bugswarm-gateway/src/gateway/types.py`

**Exact code**:

```python
import os
from pathlib import Path

def load_secret(key_name: str, default: str = "") -> str:
    """
    Providers:
      - env (default): Read from os.environ[key_name].
      - file: Read from /run/secrets/{key_name.lower()} or ${KEY_NAME}_FILE.
    """
    provider = os.getenv("BGSWARM_SECRETS_PROVIDER", "env")

    if provider == "file":
        file_env = f"{key_name}_FILE"
        file_path = os.getenv(file_env, f"/run/secrets/{key_name.lower()}")
        try:
            return Path(file_path).read_text().strip()
        except FileNotFoundError:
            raise SecretLoadError(
                f"Secret file not found for {key_name} at {file_path}. "
                f"Set {file_env} to override path."
            )
        except PermissionError:
            raise SecretLoadError(
                f"Permission denied reading secret file for {key_name} at {file_path}"
            )
    elif provider == "env":
        return os.getenv(key_name, default)
    else:
        raise SecretLoadError(f"Unknown secrets provider: {provider}. Valid: env, file")


class SecretLoadError(Exception):
    pass
```

**Immunity**: `tests/unit/test_secrets.py::test_load_secret_from_env`, `tests/unit/test_secrets.py::test_load_secret_from_file`, `tests/unit/test_secrets.py::test_load_secret_from_file_missing_raises`.

**Success criteria**: Secrets loadable from env or file. Docker secrets pattern documented.

---

#### H14: No circuit breaker between daemons. CPG unreachable → silent failure.

**Category**: Resilience
**Root cause**: Danger map loading fails silently — fuzzer continues with zero guidance.

**Permanent fix** (G7 simplified — no separate health endpoint): Track danger map loading failures as health signal. After 3 consecutive failures, set `circuit_open` flag. Fuzzer falls back to coverage-only mode. On successful load, reset counter (circuit closes).

**Files**: `bugswarm-sandbox/src/fuzzer.rs`, `bugswarm-sandbox/src/container.rs`

**Exact code**:

```rust
use std::sync::atomic::{AtomicU32, AtomicBool, Ordering};

struct CpgCircuitBreaker {
    consecutive_failures: AtomicU32,
    failure_threshold: u32,      // default: 3
    circuit_open: AtomicBool,
}

impl CpgCircuitBreaker {
    fn new(threshold: u32) -> Self {
        Self {
            consecutive_failures: AtomicU32::new(0),
            failure_threshold: threshold,
            circuit_open: AtomicBool::new(false),
        }
    }

    fn is_open(&self) -> bool {
        self.circuit_open.load(Ordering::Relaxed)
    }

    fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::SeqCst);
        if self.circuit_open.swap(false, Ordering::SeqCst) {
            tracing::info!("CPG circuit breaker CLOSED — danger map loaded successfully");
        }
    }

    fn record_failure(&self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= self.failure_threshold && !self.circuit_open.load(Ordering::Relaxed) {
            self.circuit_open.store(true, Ordering::SeqCst);
            tracing::warn!(
                failures = failures,
                threshold = self.failure_threshold,
                "CPG circuit breaker OPEN — danger map unavailable. Fuzzer continues in coverage-only mode."
            );
        }
    }
}

// Usage in danger map loading:
if breaker.is_open() {
    tracing::debug!("Circuit breaker open — skipping danger map load");
    return Ok(None);
}
match load_danger_map_from_cpg(&state.cpg_socket_path).await {
    Ok(map) => { breaker.record_success(); Ok(Some(map)) }
    Err(e) => {
        breaker.record_failure();
        if breaker.is_open() { Ok(None) } else { Err(e.into()) }
    }
}
```

**Immunity**: `tests/integration/test_circuit_breaker.py::test_circuit_opens_after_consecutive_failures`, `tests/integration/test_circuit_breaker.py::test_circuit_closes_after_successful_load`, `tests/integration/test_circuit_breaker.py::test_fuzzer_continues_in_coverage_mode`.

**Success criteria**: Circuit opens after 3 consecutive failures. Circuit closes on first success. Fuzzer runs in degraded mode when circuit open.

---

#### H16: prlimit64 allowed, fork/clone not explicitly listed in seccomp deny

**Category**: Security
**Root cause**: Seccomp profile uses default-deny but dangerous syscalls not explicitly denied, and prlimit64 may or may not be safe to block.

**Permanent fix** (G5 verified): Run CRIT-PRE4 verification first. Then add explicit DENY block for `fork`, `clone`, `clone3`, `vfork`, `execveat`, `kexec_load`, `kexec_file_load`. Handle `prlimit64` based on verification outcome.

**Files**: `bugswarm-sandbox/seccomp/default.json`

**Exact code** (updated seccomp JSON — explicit deny block):

```json
{
  "defaultAction": "SCMP_ACT_ERRNO",
  "architectures": ["SCMP_ARCH_X86_64"],
  "syscalls": [
    {
      "names": ["read", "write", "open", "close", "fstat", "mmap", "mprotect",
                 "munmap", "brk", "rt_sigaction", "futex", "epoll_create",
                 "epoll_ctl", "epoll_wait", "sched_yield", "getpid", "gettid",
                 "prlimit64"],
      "action": "SCMP_ACT_ALLOW"
    },
    {
      "names": ["clone3", "execveat", "kexec_load", "kexec_file_load",
                 "mount", "umount2", "pivot_root", "chroot", "setuid",
                 "setgid", "setgroups", "ptrace", "process_vm_readv",
                 "process_vm_writev", "bpf", "seccomp", "personality",
                 "init_module", "finit_module", "delete_module",
                 "reboot", "swapon", "swapoff", "adjtimex",
                 "clock_settime", "settimeofday"],
      "action": "SCMP_ACT_KILL_PROCESS"
    }
  ]
}
```

Note: `prlimit64` shown in ALLOW pending CRIT-PRE4 verification. Update based on outcome.

**Immunity**: `tests/integration/test_seccomp.py::test_fork_blocked` — `os.fork()` raises `OSError` errno 1. `tests/integration/test_seccomp.py::test_clone3_blocked`. `tests/integration/test_seccomp.py::test_python_startup_under_seccomp` — Python starts successfully.

**Success criteria**: Fork/clone variants blocked. Python startup works under seccomp. prlimit64 status determined and documented.

---

#### H17: Agent tool dispatch directly from LLM JSON. No capability gating.

**Category**: Security
**Root cause**: LLM can call any tool with any arguments. Needs input validation per tool.

**Permanent fix** (G6 corrected — after H12): Add pre-execution validators. `exec_sandbox` requires code size limit (1MB max) and blocks dangerous patterns. `read_file` must be within repo boundaries (reuses H12 fix). Add rate limiting per tool type (max calls/minute).

**Files**: `bugswarm-agent/src/agent/core.py`, `bugswarm-agent/src/agent/tools.py` (NEW)

**Exact code**:

```python
# File: bugswarm-agent/src/agent/tools.py (NEW)

import time
from collections import defaultdict
from typing import Any, Callable, Dict

MAX_POC_SIZE_BYTES = 1_000_000  # 1MB
MAX_CALLS_PER_MINUTE: Dict[str, int] = {
    "exec_sandbox": 100,
    "mutate_source": 50,
    "read_file": 200,
    "write_file": 100,
    "grep_source": 200,
    "run_command": 50,
}

class ToolGate:
    def __init__(self):
        self._call_timestamps: Dict[str, list[float]] = defaultdict(list)

    def validate(self, tool_name: str, args: Dict[str, Any]) -> tuple[bool, str]:
        # Rate limit check
        rate_ok, rate_reason = self._check_rate_limit(tool_name)
        if not rate_ok:
            return False, rate_reason

        # Tool-specific validators
        validator = TOOL_VALIDATORS.get(tool_name)
        if validator is not None:
            ok, reason = validator(args)
            if not ok:
                return False, reason

        self._call_timestamps[tool_name].append(time.time())
        return True, ""

    def _check_rate_limit(self, tool_name: str) -> tuple[bool, str]:
        max_calls = MAX_CALLS_PER_MINUTE.get(tool_name)
        if max_calls is None:
            return True, ""
        now = time.time()
        self._call_timestamps[tool_name] = [
            ts for ts in self._call_timestamps[tool_name] if now - ts < 60
        ]
        count = len(self._call_timestamps[tool_name])
        if count >= max_calls:
            return False, (
                f"Rate limit exceeded: {tool_name} called {count} times "
                f"in 60s (max: {max_calls}/minute)"
            )
        return True, ""


def _validate_exec_sandbox(args: Dict[str, Any]) -> tuple[bool, str]:
    """Validate exec_sandbox tool arguments."""
    poc_code = args.get("poc_code", "")
    if not poc_code:
        return False, "exec_sandbox requires non-empty 'poc_code'"
    if len(poc_code.encode("utf-8")) > MAX_POC_SIZE_BYTES:
        return False, f"PoC code too large: {len(poc_code.encode('utf-8'))} bytes (max 1MB)"

    dangerous = ["import ctypes", "import subprocess", "os.system", "eval(", "exec(", "compile(", "__builtins__"]
    for pattern in dangerous:
        if pattern in poc_code:
            return False, f"exec_sandbox rejected: blocked pattern '{pattern}'"
    return True, ""


def _validate_read_file(args: Dict[str, Any]) -> tuple[bool, str]:
    """Validate read_file — uses H12 path validation logic."""
    path = args.get("path", "")
    if not path:
        return False, "read_file requires non-empty 'path'"
    if path.startswith("/"):
        return False, f"read_file rejected: absolute path '{path}' forbidden"
    if ".." in path.replace("\\", "/").split("/"):
        return False, f"read_file rejected: path traversal blocked for '{path}'"
    return True, ""


def _validate_write_file(args: Dict[str, Any]) -> tuple[bool, str]:
    path = args.get("path", "")
    content = args.get("content", "")
    if not path:
        return False, "write_file requires non-empty 'path'"
    if path.startswith("/"):
        return False, f"write_file rejected: absolute path '{path}' forbidden"
    if ".." in path.replace("\\", "/").split("/"):
        return False, f"write_file rejected: path traversal blocked for '{path}'"
    if len(content.encode("utf-8")) > 10_000_000:
        return False, "write_file content too large (max 10MB)"
    return True, ""


TOOL_VALIDATORS: Dict[str, Callable] = {
    "exec_sandbox": _validate_exec_sandbox,
    "read_file": _validate_read_file,
    "write_file": _validate_write_file,
}


# Usage in core.py:
_tool_gate = ToolGate()

def _dispatch_tool(tool_name: str, args: Dict[str, Any]) -> ToolResult:
    allowed, reason = _tool_gate.validate(tool_name, args)
    if not allowed:
        return ToolResult(False, f"Tool rejected by capability gate: {reason}")
    # ... proceed to actual tool execution
```

**Immunity**: `tests/unit/test_tool_gating.py::test_exec_sandbox_rejects_oversized_code`, `test_exec_sandbox_rejects_dangerous_patterns`, `test_read_file_rejects_absolute_path`, `test_rate_limit_enforced`.

**Success criteria**: Tool gating rejects oversized PoCs, absolute paths, ../ traversals, dangerous patterns, rate-limited calls.

---

#### H19: Alerting engine never wired to Slack/PagerDuty

**Category**: Monitoring
**Root cause**: `AlertEngine` exists but `_notifier` is always `None`.

**Permanent fix** (G14 corrected — async-safe HTTP): Add Slack webhook notifier using `aiohttp` (NOT blocking `requests.post()`). Wire into AlertEngine.

**Files**: `bugswarm-swarm/src/swarm/observability.py`

**Exact code**:

```python
import os
import aiohttp
from typing import Optional

class SlackNotifier:
    def __init__(self, webhook_url: Optional[str] = None):
        self.webhook_url = webhook_url or os.getenv("BGSWARM_ALERT_SLACK_WEBHOOK", "")
        self._session: Optional[aiohttp.ClientSession] = None

    async def _get_session(self) -> aiohttp.ClientSession:
        if self._session is None or self._session.closed:
            self._session = aiohttp.ClientSession()
        return self._session

    async def notify(self, alert: "Alert") -> bool:
        if not self.webhook_url:
            return False

        payload = {
            "text": f"[{alert.severity.value}] {alert.message}",
            "attachments": [{
                "color": {"critical": "#FF0000", "high": "#FF6600"}.get(
                    alert.severity.value, "#CCCCCC"),
                "fields": [
                    {"title": "Source", "value": alert.source, "short": True},
                    {"title": "Timestamp", "value": alert.timestamp.isoformat(), "short": True},
                ],
                "text": alert.details or "",
            }],
        }

        try:
            session = await self._get_session()
            async with session.post(
                self.webhook_url, json=payload,
                timeout=aiohttp.ClientTimeout(total=10),
            ) as resp:
                if resp.status != 200:
                    body = await resp.text()
                    raise RuntimeError(f"Slack webhook returned {resp.status}: {body[:200]}")
                return True
        except Exception as e:
            import logging
            logging.getLogger("bugswarm.alerts").error(f"Failed to send Slack alert: {e}")
            return False

    async def close(self):
        if self._session and not self._session.closed:
            await self._session.close()


class AlertEngine:
    def __init__(self):
        self._notifiers: list = []
        slack_url = os.getenv("BGSWARM_ALERT_SLACK_WEBHOOK", "")
        if slack_url:
            self._notifiers.append(SlackNotifier(slack_url))

    async def emit(self, alert: "Alert"):
        for notifier in self._notifiers:
            await notifier.notify(alert)
```

**Immunity**: `tests/unit/test_alerting.py::test_slack_notifier_sends_correct_payload` (mocked), `test_slack_notifier_handles_http_error`, `test_alert_engine_no_notifiers_when_unconfigured`.

**Success criteria**: Slack notifier sends alerts. HTTP errors handled gracefully. No blocking I/O in async context.

---

### Batch 3 — FUNCTIONAL WIRING (12 findings)

**Rationale**: Replace placeholder implementations with real logic. Most complex batch due to PoC generation, mutation testing, and Z3 integration.

**Pre-requisites**: Batches 0, 1, 2 complete. H12 fix (path traversal) available. H11 fix (bind-mount execution) available.

**Findings**: H21, H22, H23, H24, H25, H26, H27, H28, H29, H30, H31, H32

**Batch pre-requisites**:
- H21/H22 depend on CRIT-PRE5 (PoC generation spec)
- H23 depends on sandbox image having `python3 -m unittest` (stdlib)
- H24 depends on feature-gating `bugswarm-symbolic` (G3 corrected)
- H29/H30 must not conflict on write paths (G17 fix applied below)
- H30 depends on evidence daemon (M11 save_graph/load_graph) — coordinates via file queue (G8 fix)
- H31 depends on orchestrator wiring

**Batch verification checklist**:
- [ ] All 12 functional daemon handlers return real data (not stubs)
- [ ] `mine_invariants` produces real ExecutionTrace objects
- [ ] `invariant_check` executes generated inputs, returns real pass/fail
- [ ] `run_mutations` compiles and tests real mutants via `python3 -m unittest`
- [ ] `solve_reachability` uses Z3 (when feature enabled) or falls back gracefully
- [ ] `diff_execute` client-server protocol matches (JSON keys correct)
- [ ] `describe_trigger` actually calls EvidenceClient
- [ ] `get_trigger_matrix` returns real data
- [ ] `estimate_argument_ranges` uses CPG AST analysis
- [ ] Danger map SHM segment created before campaign start
- [ ] Crash files collected from container and written to JSONL queue
- [ ] All 15 agent tools registered in orchestrator

---

#### H21: mine_invariants returns 100% stub traces

**Category**: Functional
**Root cause**: Daemon generates fabricated traces instead of executing real code.

**Permanent fix** (G15 corrected — PoC generation): Wire handler to execute target function with generated inputs in sandbox container. For each input -> real PoC execution -> real ExecutionTrace. Uses CRIT-PRE5 algorithm.

**Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-sandbox/src/invariant.rs`

**Exact code** (daemon handler integration, uses PoC generation from CRIT-PRE5):

```rust
async fn handle_mine_invariants(
    state: &DaemonState,
    request: MineInvariantsRequest,
) -> anyhow::Result<MineInvariantsResponse> {
    let target_function = &request.target_function;
    let num_inputs = request.num_inputs.unwrap_or(500);
    let mut traces = Vec::with_capacity(num_inputs);

    // 1. Query CPG for function signature
    let cpg_client = CpgClient::new(&state.cpg_socket_path);
    let signature = cpg_client.extract_function_signature(target_function).await
        .context("Failed to extract function signature from CPG")?;

    // 2. Generate input value sets
    let input_generator = InputGenerator::new(&signature);
    let inputs = input_generator.generate(num_inputs);

    // 3. For each input, generate PoC and execute in sandbox
    let manager = &state.container_manager;
    for (idx, input_values) in inputs.iter().enumerate() {
        if idx % 50 == 0 {
            tracing::debug!(progress = format!("{}/{}", idx, num_inputs), "Mining invariants");
        }

        let poc_code = generate_poc_code(target_function, &signature, input_values);
        let receipt = manager.execute(
            &poc_code,
            &ExecutionConfig { timeout_secs: 30, max_memory_mb: 512 },
        ).await?;

        let trace = build_execution_trace(input_values, &receipt);
        traces.push(trace);
    }

    tracing::info!(total_traces = traces.len(), "Invariant mining complete");
    Ok(MineInvariantsResponse { traces })
}
```

**Immunity**: `tests/integration/test_invariant_mining.py::test_mine_invariants_produces_real_traces` — mines known target, verifies traces contain real output values (not "result_0"). `tests/integration/test_invariant_mining.py::test_mine_invariants_handles_exceptions` — function raises on certain inputs, verifies traces record exception type/message correctly.

**Success criteria**: All ExecutionTrace objects contain real execution data (not fabricated stubs).

---

#### H22: invariant_check generates inputs but never executes them

**Category**: Functional
**Root cause**: Same as H21 — generates inputs but skips execution.

**Permanent fix**: Reuse H21's execution pipeline. For each invariant check: generate PoC, execute in sandbox, compare output against invariant expectation.

**Files**: `bugswarm-sandbox/src/daemon.rs`

**Implementation**: Identical execution flow to H21, but compares output to expected invariant.

**Immunity**: `tests/integration/test_invariant_check.py::test_invariant_check_executes_real_code` — provides function with invariant `x * 2 == x + x`, verifies check returns True for satisfying inputs, False for violating.

**Success criteria**: Invariant checks execute real code and return real pass/fail results.

---

#### H23: run_mutations uses fake test runner

**Category**: Functional
**Root cause**: String matching on operators instead of real compilation + test execution.

**Permanent fix** (G16 corrected): For each mutant: (1) write mutated source to temp file, (2) copy test file alongside it, (3) execute `python3 -m unittest` (stdlib — no pytest needed) inside sandbox. Non-zero exit code = mutant KILLED. Exit code 0 = mutant SURVIVED.

**Files**: `bugswarm-sandbox/src/daemon.rs`, `bugswarm-sandbox/src/mutation.rs`

**Exact code**:

```rust
async fn handle_run_mutations(
    state: &DaemonState,
    request: RunMutationsRequest,
) -> anyhow::Result<RunMutationsResponse> {
    let source_code = &request.source_code;
    let test_code = &request.test_code;
    let num_mutants = request.num_mutants.unwrap_or(50);
    let mut results = Vec::with_capacity(num_mutants);

    // 1. Generate mutants
    let mutant_generator = MutantGenerator::new();
    let mutants = mutant_generator.generate(source_code, num_mutants);
    let manager = &state.container_manager;

    // 2. For each mutant, execute test suite
    for mutant in mutants.iter() {
        let temp_dir = format!("/tmp/bugswarm-{}", Uuid::new_v4());
        let source_path = format!("{}/mutated.py", temp_dir);
        let test_path = format!("{}/test_source.py", temp_dir);

        // Use python3 -m unittest discover (stdlib, no extra install)
        let cmd = format!(
            "python3 -m unittest discover -s {temp_dir} -p 'test_*.py'",
            temp_dir = temp_dir
        );

        let receipt = manager.execute_with_files(
            &[
                (source_path.clone(), mutant.code.as_bytes()),
                (test_path.clone(), test_code.as_bytes()),
            ],
            &cmd,
            &ExecutionConfig { timeout_secs: 60, max_memory_mb: 1024 },
        ).await?;

        let outcome = if receipt.exit_code == 0 {
            MutantOutcome::Survived
        } else {
            MutantOutcome::Killed
        };

        results.push(MutationResult {
            mutant_id: mutant.id.clone(),
            operator: mutant.operator.clone(),
            line_number: mutant.line_number,
            outcome,
            execution_time_ms: receipt.duration_ms,
        });
    }

    let killed = results.iter().filter(|r| r.outcome == MutantOutcome::Killed).count();
    tracing::info!(tested = results.len(), killed = killed, "Mutation testing complete");
    Ok(RunMutationsResponse { results })
}
```

**Immunity**: `tests/integration/test_mutation_testing.py::test_mutant_with_covered_line_is_killed` and `tests/integration/test_mutation_testing.py::test_mutant_on_uncovered_line_survives`.

**Success criteria**: Mutants on covered lines are killed. On uncovered lines they survive. Tests execute via `python3 -m unittest` (stdlib — no pytest dependency).

---

#### H24: solve_reachability uses heuristic, not Z3

**Category**: Functional
**Root cause**: Daemon handler uses `extract_solution_value()` (+1 to ints) instead of actual Z3.

**Permanent fix** (G3 corrected — feature-gated): Add `bugswarm-symbolic` as optional dependency with `symbolic` feature. When enabled, call Z3 solver. When not enabled, return clear "solver not available" message.

**Files**: `bugswarm-sandbox/Cargo.toml`, `bugswarm-sandbox/src/daemon.rs`

**Exact code (Cargo.toml)**:

```toml
# bugswarm-sandbox/Cargo.toml
[features]
default = []
symbolic = ["bugswarm-symbolic"]

[dependencies]
bugswarm-symbolic = { path = "../bugswarm-symbolic", optional = true }
```

**Exact code (daemon.rs)**:

```rust
async fn handle_solve_reachability(
    state: &DaemonState,
    request: SolveReachabilityRequest,
) -> anyhow::Result<SolveReachabilityResponse> {
    #[cfg(feature = "symbolic")]
    {
        let engine = bugswarm_symbolic::engine::SymbolicEngine::new(&state.z3_config)
            .context("Failed to initialize symbolic engine")?;

        let solution = engine.solve_reachability(
            &request.constraints, &request.variables, request.max_depth.unwrap_or(100),
        ).context("Z3 solver failed")?;

        return Ok(SolveReachabilityResponse {
            solution: Some(solution),
            solver_used: "z3".to_string(),
        });
    }

    #[cfg(not(feature = "symbolic"))]
    {
        tracing::warn!(
            "solve_reachability called but 'symbolic' feature not enabled. "
            "Rebuild with: cargo build --features symbolic"
        );
        return Ok(SolveReachabilityResponse {
            solution: None,
            solver_used: "none".to_string(),
        });
    }
}
```

**Immunity**: `tests/unit/test_solve_reachability.rs::test_solve_with_z3_when_feature_enabled` (compiled with `--features symbolic`), `tests/unit/test_solve_reachability.rs::test_solve_returns_none_when_feature_disabled`, `tests/integration/test_solve_reachability.py::test_solve_simple_constraint` — `x > 1000 && y == "admin"`, verifies `x >= 1001` and `y == "admin"`.

**Success criteria**: Z3 solver works with symbolic feature. Graceful fallback without. No compilation failure without libz3 installed.

---

#### H25: diff_execute client-daemon protocol mismatch

**Category**: Functional
**Root cause**: Client sends `--output-a`/`--output-b` as CLI args; daemon reads `input`/`reference` as JSON keys.

**Permanent fix**: Verify daemon handler reads correct keys (`input` and `reference`). Client sends JSON via `--request` (fixed in M30). Add assertion comments documenting the contract.

**Files**: `bugswarm-agent/src/agent/sandbox_client.py`, `bugswarm-sandbox/src/daemon.rs`

**Exact code (daemon struct)**:

```rust
#[derive(Deserialize)]
struct DiffExecuteRequest {
    /// Matches client field 'input' (NOT 'output_a')
    input: String,
    /// Matches client field 'reference' (NOT 'output_b')
    reference: String,
    #[serde(default = "default_strategy")]
    strategy: String,
}
```

**Exact code (client)**:

```python
async def diff_execute(self, input_code: str, reference_code: str, strategy: str = "ast") -> DiffResult:
    payload = {
        "input": input_code,        # Must match daemon's 'input' field
        "reference": reference_code, # Must match daemon's 'reference' field
        "strategy": strategy,
    }
    response = await self._send_request("diff_execute", payload)
    return DiffResult.from_dict(response)
```

**Immunity**: `tests/integration/test_diff_execute.py::test_protocol_match` — sends diff_execute via agent client, verifies daemon processes successfully, response fields match client schema.

**Success criteria**: Protocol mismatch eliminated. Integration test passes end-to-end.

---

#### H26: describe_trigger in orchestrator bypasses EvidenceClient

**Category**: Functional
**Root cause**: Returns `{"recorded": True}` without daemon call.

**Permanent fix**: Actually call `EvidenceClient.add_trigger_condition()`.

**Files**: `bugswarm-swarm/src/swarm/orchestrator.py`

**Exact code**:

```python
# Before: return {"recorded": True}

# After:
async def describe_trigger(self, trigger: Trigger) -> dict:
    client = await self._get_evidence_client()
    result = await client.add_trigger_condition(
        trigger_name=trigger.name,
        condition=trigger.condition,
        source_campaign=trigger.campaign_id,
        metadata=trigger.metadata,
    )
    return {
        "recorded": result.success,
        "trigger_id": result.trigger_id,
        "graph_version": result.graph_version,
    }
```

**Immunity**: `tests/integration/test_orchestrator.py::test_describe_trigger_calls_evidence_client` — creates trigger, calls describe_trigger, verifies EvidenceClient received call, trigger_id returned.

**Success criteria**: Describe_trigger returns real trigger_id from evidence daemon.

---

#### H27: get_trigger_matrix in orchestrator returns always-empty stub

**Category**: Functional
**Root cause**: Hardcoded `{"conditions": []}`.

**Permanent fix**: Actually call `EvidenceClient.get_trigger_matrix()`.

**Files**: `bugswarm-swarm/src/swarm/orchestrator.py`

**Exact code**:

```python
# Before: return {"conditions": []}

# After:
async def get_trigger_matrix(self, campaign_id: str) -> dict:
    client = await self._get_evidence_client()
    result = await client.get_trigger_matrix(campaign_id=campaign_id)
    return {
        "campaign_id": campaign_id,
        "conditions": result.conditions,
        "total": len(result.conditions),
        "active": sum(1 for c in result.conditions if c.status == "active"),
        "fired": sum(1 for c in result.conditions if c.status == "fired"),
    }
```

**Immunity**: `tests/integration/test_orchestrator.py::test_get_trigger_matrix_returns_real_data`.

**Success criteria**: Trigger matrix populated from evidence daemon data.

---

#### H28: estimate_argument_ranges returns hardcoded [0, 100, 42]

**Category**: Functional
**Root cause**: Placeholder implementation.

**Permanent fix**: Query CPG for caller source code, parse argument types from AST. Fall back to heuristic for unknown types.

**Files**: `bugswarm-evidence/src/fix_predict.rs`

**Exact code**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentRange {
    pub param_name: String,
    pub param_type: String,
    pub min_value: Option<i64>,
    pub max_value: Option<i64>,
    pub common_values: Vec<String>,
    pub is_nullable: bool,
}

pub fn estimate_argument_ranges(
    cpg_client: &CpgClient,
    function_name: &str,
) -> anyhow::Result<Vec<ArgumentRange>> {
    // 1. Query CPG for callers and their args
    let callers = cpg_client.get_callers_with_args(function_name)
        .context("Failed to query CPG for callers")?;

    // 2. For each parameter, analyze observed values
    let signature = cpg_client.extract_function_signature(function_name)
        .context("Failed to extract signature")?;

    let mut ranges = Vec::new();
    for param in &signature.parameters {
        let observed: Vec<&str> = callers.iter()
            .filter_map(|caller| caller.args.get(&param.name))
            .map(|v| v.as_str())
            .collect();

        let range = if observed.is_empty() {
            heuristic_range_for_type(&param.ptype)
        } else {
            analyze_observed_values(&param, &observed)
        };
        ranges.push(range);
    }
    Ok(ranges)
}

fn heuristic_range_for_type(ptype: &str) -> ArgumentRange {
    match ptype {
        "int" | "i32" | "i64" => ArgumentRange {
            param_type: ptype.into(), min_value: Some(-1000), max_value: Some(1000),
            common_values: vec!["0".into(), "1".into(), "-1".into()],
            is_nullable: false, param_name: String::new(),
        },
        "str" | "string" => ArgumentRange {
            param_type: ptype.into(), min_value: None, max_value: None,
            common_values: vec!["".into(), "test".into()],
            is_nullable: false, param_name: String::new(),
        },
        "bool" => ArgumentRange {
            param_type: ptype.into(), min_value: None, max_value: None,
            common_values: vec!["true".into(), "false".into()],
            is_nullable: false, param_name: String::new(),
        },
        _ => ArgumentRange {
            param_type: ptype.into(), min_value: None, max_value: None,
            common_values: vec![], is_nullable: false, param_name: String::new(),
        },
    }
}
```

**Immunity**: `tests/unit/test_argument_ranges.rs::test_estimate_from_callers` and `test_fallback_heuristic_for_unknown_type`.

**Success criteria**: Ranges computed from real CPG data. Fallback heuristics apply when no callers found. No hardcoded `[0, 100, 42]`.

---

#### H29: Danger map shared-memory path has no SHM setup infrastructure

**Category**: Functional
**Root cause**: `load_danger_map_if_configured()` tries `danger_map_from_shm()` but no code creates the SHM segment.

**Permanent fix** (G17 fix — collision avoidance): Before campaign start, if `danger_map_shm_name` is `Some`, call `danger_map_to_shm()` to create and populate the segment. Use namespace prefix to avoid collision with crash files: `/bugswarm_danger_{campaign_id}` NOT `/bugswarm_danger_map`.

**Files**: `bugswarm-sandbox/src/fuzzer.rs`, `bugswarm-sandbox/src/container.rs`

**Exact code**:

```rust
// In fuzzer.rs — campaign startup:
pub async fn start_fuzz_campaign(config: &FuzzConfig) -> anyhow::Result<FuzzHandle> {
    // ... campaign setup ...

    // Create SHM segment if danger map is configured
    if let Some(ref shm_name) = config.danger_map_shm_name {
        tracing::info!(shm_name = %shm_name, "Creating danger map SHM segment");
        let danger_map = load_danger_map(&config.danger_map_path).await
            .context("Failed to load danger map data")?;

        danger_map_to_shm(&danger_map, shm_name)
            .with_context(|| format!("Failed to create SHM segment {}", shm_name))?;

        tracing::info!(shm_name = %shm_name, map_size = danger_map.len(), "SHM segment created");
    }

    // ... launch container ...
}
```

**SHM naming convention** (documented in code and README):

```
SHM segments:       /bugswarm_danger_{campaign_id}      (e.g., /bugswarm_danger_camp_abc123)
Crash files:        /tmp/bugswarm-crashes.jsonl           (separate namespace)
Fuzzer corpus:      /tmp/bugswarm/corpus/{campaign_id}/   (on-disk)
```

SHM segments and crash file paths are in entirely different namespaces — no collision possible.

**Immunity**: `tests/integration/test_danger_map_shm.py::test_shm_segment_created_before_campaign` and `test_shm_naming_no_collision_with_crash_files`.

**Success criteria**: SHM segment exists before fuzzer reads it. Naming convention prevents collision.

---

#### H30: Fuzz campaign crash artifacts never collected back from container

**Category**: Functional
**Root cause**: Container launches AFL++ but no result monitoring. Crashes stay in container and are lost.

**Permanent fix** (G8 corrected — via file queue, NOT direct evidence daemon call): Poll `/fuzz/corpus/out/crashes/` every 5 seconds via `tokio::time::interval`. Track processed files via in-memory `HashSet`. For new crash files: read content, compute stack hash, write to JSONL queue file `/tmp/bugswarm-crashes.jsonl`. Agent/orchestrator reads queue and injects into evidence — sandbox does NOT call evidence daemon directly.

**Files**: `bugswarm-sandbox/src/container.rs`, `bugswarm-sandbox/src/fuzzer.rs`

**Exact code**:

```rust
use std::collections::HashSet;
use tokio::time::{interval, Duration};
use std::sync::Arc;
use tokio::sync::Mutex;

struct CrashCollector {
    crash_queue_path: PathBuf,       // /tmp/bugswarm-crashes.jsonl
    corpus_crashes_path: PathBuf,    // /fuzz/corpus/out/crashes/
    processed_files: Arc<Mutex<HashSet<String>>>,
    campaign_id: String,
}

impl CrashCollector {
    fn new(campaign_id: &str) -> Self {
        Self {
            crash_queue_path: PathBuf::from("/tmp/bugswarm-crashes.jsonl"),
            corpus_crashes_path: PathBuf::from("/fuzz/corpus/out/crashes/"),
            processed_files: Arc::new(Mutex::new(HashSet::new())),
            campaign_id: campaign_id.to_string(),
        }
    }

    async fn run(&self) -> anyhow::Result<()> {
        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            match self.scan_for_new_crashes().await {
                Ok(found) => {
                    if found > 0 {
                        tracing::info!(campaign = %self.campaign_id, new_crashes = found, "Crash files collected");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "Failed to scan crash directory"),
            }
        }
    }

    async fn scan_for_new_crashes(&self) -> anyhow::Result<usize> {
        let mut found = 0;
        let mut entries = tokio::fs::read_dir(&self.corpus_crashes_path).await?;
        let mut processed = self.processed_files.lock().await;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let filename = path.file_name().unwrap().to_string_lossy().to_string();

            if processed.contains(&filename) { continue; }

            let content = tokio::fs::read_to_string(&path).await?;
            let stack_hash = compute_stack_hash(&content);

            // Write to JSONL queue for agent/orchestrator to pick up
            let record = serde_json::json!({
                "campaign_id": self.campaign_id,
                "filename": filename,
                "stack_hash": stack_hash,
                "content_size": content.len(),
                "content": content,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });

            let mut queue_file = tokio::fs::OpenOptions::new()
                .create(true).append(true).open(&self.crash_queue_path).await?;
            use tokio::io::AsyncWriteExt;
            queue_file.write_all(
                format!("{}\n", serde_json::to_string(&record)?).as_bytes()
            ).await?;

            processed.insert(filename);
            found += 1;
            tracing::info!(filename = %filename, stack_hash = %stack_hash, "Crash collected");
        }
        Ok(found)
    }
}

fn compute_stack_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let stack_lines: Vec<&str> = content.lines()
        .filter(|line| line.contains(" #") || line.contains(" at ") || line.contains("File \""))
        .collect();
    let mut hasher = DefaultHasher::new();
    for line in stack_lines { line.hash(&mut hasher); }
    format!("{:016x}", hasher.finish())
}
```

**Integration with fuzzer startup**: Spawn crash collector as background task. Store handle for abort on campaign stop.

**Immunity**: `tests/integration/test_crash_collection.py::test_crash_files_collected_to_jsonl` — launches fuzz campaign, waits for crash, verifies JSONL queue entry with matching stack_hash. `tests/integration/test_crash_collection.py::test_duplicate_crashes_deduplicated` — two crashes with same stack hash produce only one JSONL entry.

**Success criteria**: Crash files detected within 5s of creation. Deduplication works. JSONL queue is append-only.

---

#### H31: Orchestrator tools missing 8 agent tools

**Category**: Functional
**Root cause**: `setup_tools()` registers only 7 tools; 8 more exist in wiring.py but are never registered.

**Permanent fix**: Register all 15 tools in orchestrator's `setup_tools()`.

**Files**: `bugswarm-swarm/src/swarm/orchestrator.py`

**Exact code**:

```python
def setup_tools(self) -> None:
    """Register all agent tools with the orchestrator."""
    tools = [
        # Existing 7:
        "read_file", "write_file", "grep_source", "exec_sandbox",
        "run_command", "mutate_source", "search_codebase",
        # Missing 8 (from wiring.py):
        "mine_invariants", "invariant_check", "diff_execute",
        "solve_reachability", "run_mutations", "describe_trigger",
        "get_trigger_matrix", "estimate_argument_ranges",
    ]
    for tool_name in tools:
        self._register_tool(tool_name)
    assert len(self._tools) == 15, f"Expected 15 tools, got {len(self._tools)}"
```

**Immunity**: `tests/unit/test_orchestrator_tools.py::test_all_15_tools_registered`.

**Success criteria**: All 15 tools registered. Assertion fails CI if count doesn't match 15.

---

#### H32: exec_causal_intervention uses inadequate string escaping

**Category**: Functional / Security
**Root cause**: Only escapes single quotes, not `$`, backticks, `\n`, `\r`.

**Permanent fix**: Reuse H11's bind-mount pattern (preferred — eliminates shell injection surface entirely). Fallback: use `shell-escape` crate.

**Files**: `bugswarm-sandbox/src/container.rs`

**Exact code (bind-mount — preferred)**:

```rust
async fn exec_causal_intervention(
    manager: &ContainerManager,
    intervention_code: &str,
    target_args: &[String],
) -> anyhow::Result<ExecutionReceipt> {
    // Write intervention code to temp file, bind-mount (reuses H11 pattern)
    let poc_path = write_poc_to_temp_file(intervention_code)?;
    let bind_mount = BindMount {
        source: poc_path.clone(),
        target: PathBuf::from("/sandbox/intervention.py"),
        read_only: true,
    };

    let mut cmd = vec!["python3".to_string(), "/sandbox/intervention.py".to_string()];
    cmd.extend(target_args.iter().cloned());

    manager.execute_with_mounts(cmd, vec![bind_mount]).await
}
```

**Fallback (shell-escape crate)**: use `shell_escape::unix::escape()` for any remaining inline arguments.

**Immunity**: `tests/unit/test_shell_escape.rs::test_causal_intervention_escape_newlines`, `test_causal_intervention_escape_dollars`, `test_causal_intervention_escape_backticks`.

**Success criteria**: Bind-mount used for causal intervention code (reuses H11 pattern). No shell escaping vulnerabilities.

---

### Batch 4 — MONITORING + ORCHESTRATION (4 findings)

**Rationale**: Distributed tracing, trace correlation, Docker Compose orchestration, and alerting verification.

**Pre-requisites**: Batches 0-3 complete. Docker images built. Daemons accept connections.

**Findings**: H18, H20, docker-compose (G18 fix), H19(verify)

**Batch verification checklist**:
- [ ] `docker compose up -d` starts all 4 services
- [ ] `docker compose ps` shows all 4 services healthy
- [ ] Agent generates X-Request-ID for each investigation
- [ ] All log lines from one investigation share the same request_id
- [ ] OpenTelemetry traces appear in collector (if OTEL endpoint configured)
- [ ] Slack alerts fire on configured trigger conditions

---

#### H18: No OpenTelemetry integration. No distributed tracing.

**Category**: Monitoring
**Root cause**: Only JSON logs, no trace context propagation across daemons.

**Permanent fix** (G13 corrected — feature-gated): Add OpenTelemetry behind `telemetry` Cargo feature. Default off to avoid ~50 extra deps. CI compiles with `--all-features`. When enabled, initialize OTLP exporter, propagate `traceparent` via Unix socket JSON metadata.

**Files**: All Cargo.toml files, all daemon.rs files

**Exact code (Cargo.toml additions)**:

```toml
# Add to bugswarm-cpg, bugswarm-evidence, bugswarm-sandbox Cargo.toml:
[features]
default = []
telemetry = ["opentelemetry", "tracing-opentelemetry", "opentelemetry-otlp"]

[dependencies]
opentelemetry = { version = "0.23", optional = true }
tracing-opentelemetry = { version = "0.24", optional = true }
opentelemetry-otlp = { version = "0.16", optional = true }
opentelemetry_sdk = { version = "0.23", optional = true, features = ["rt-tokio"] }
```

**Exact code (daemon init)**:

```rust
fn init_telemetry(service_name: &str) {
    #[cfg(feature = "telemetry")]
    {
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_sdk::trace::TracerProvider;
        use opentelemetry_otlp::{WithExportConfig, ExportConfig};
        use tracing_opentelemetry::OpenTelemetryLayer;
        use tracing_subscriber::layer::SubscriberExt;

        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:4317".to_string());

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic().with_endpoint(endpoint).build()
            .expect("Failed to create OTLP exporter");

        let provider = TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .build();

        let tracer = provider.tracer(service_name.to_string());
        let telemetry_layer = OpenTelemetryLayer::new(tracer);

        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().json())
            .with(telemetry_layer)
            .init();

        tracing::info!(service = service_name, "OpenTelemetry tracing enabled");
    }

    #[cfg(not(feature = "telemetry"))]
    {
        tracing_subscriber::fmt().json().init();
        tracing::info!(service = service_name, "OpenTelemetry not enabled (compile with --features telemetry)");
    }
}
```

**Immunity**: `tests/integration/test_telemetry.py::test_trace_spans_have_parent_child` (compiled with `--features telemetry`). CI job `test-all-features` compiles with `--all-features`.

**Success criteria**: Telemetry works when feature enabled. No compile-time impact when disabled.

---

#### H20: No trace IDs/correlation IDs between daemons

**Category**: Monitoring
**Root cause**: Each daemon generates independent logs with no linking. Impossible to trace a request across agent -> sandbox -> CPG -> evidence.

**Permanent fix**: Agent generates `X-Request-ID: uuid4()` at start of each investigation. Passed as `request_id` field in all Unix socket JSON requests. Daemons extract and include in `tracing::Span`. All log lines from one investigation share the same request_id.

**Files**: `bugswarm-agent/src/agent/core.py`, all daemon.rs files

**Exact code (agent side)**:

```python
# File: bugswarm-agent/src/agent/core.py
import uuid

class Investigation:
    def __init__(self, ...):
        self.request_id = str(uuid.uuid4())

    async def _call_sandbox_daemon(self, endpoint: str, payload: dict) -> dict:
        """Inject request_id into every daemon call."""
        payload["request_id"] = self.request_id
        return await self._send_json_over_unix_socket(
            socket_path=self.sandbox_socket, payload=payload,
        )
```

**Exact code (daemon side — all 3 daemons)**:

```rust
// In each daemon's connection handler:

async fn handle_connection(stream: UnixStream, state: Arc<DaemonState>) -> anyhow::Result<()> {
    let (reader, _writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let request: serde_json::Value = serde_json::from_str(&line)?;

    // Extract request_id from JSON payload
    let request_id = request.get("request_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Create a tracing span with the request_id
    let span = tracing::info_span!(
        "daemon_request",
        request_id = %request_id,
        daemon = env!("CARGO_PKG_NAME"),
    );
    let _enter = span.enter();

    tracing::info!(method = %request.get("method").unwrap_or(&json!(null)), "Processing request");

    // All subsequent logs in this handler include: request_id="abc-123-def"
    // ... handle the request ...

    // Include request_id in response
    let response = serde_json::json!({
        "status": "ok",
        "request_id": request_id,
        "data": result,
    });
    // ... write response ...
    Ok(())
}
```

**Immunity**: `tests/integration/test_request_correlation.py::test_all_logs_share_request_id` — makes multi-step request, greps all daemon logs for request_id, verifies every log line contains same request_id. `tests/integration/test_request_correlation.py::test_different_investigations_have_different_ids`.

**Success criteria**: Every daemon log line for an investigation shares the same request_id. Different investigations have unique request_ids.

---

#### docker-compose (G18 fix): Service orchestration specification

**Category**: Deployment
**Root cause**: No docker-compose.yml to coordinate multi-daemon startup. Missing startup order, health check deps, volumes, restart policies.

**Permanent fix**: Create `docker-compose.yml` with all 4 services, health check dependencies, named volumes, restart policies, resource limits.

**Files**: `docker-compose.yml` (NEW)

**Exact code**:

```yaml
version: "3.8"

services:
  bugswarm-cpg:
    image: bugswarm/cpg:latest
    build:
      context: .
      dockerfile: bugswarm-cpg/Dockerfile
    container_name: bugswarm-cpg
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "bugswarm-cpg", "health"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s
    volumes:
      - bugswarm-run:/var/run/bugswarm
      - bugswarm-lib:/var/lib/bugswarm
    tmpfs:
      - /var/run/bugswarm:uid=1000,gid=1000,mode=0755
    environment:
      - BUGSWARM_LOG_LEVEL=info
      - BUGSWARM_SOCKET_PATH=/var/run/bugswarm/cpg.sock
    deploy:
      resources:
        limits:
          memory: 512M
          cpus: "1.0"

  bugswarm-evidence:
    image: bugswarm/evidence:latest
    build:
      context: .
      dockerfile: bugswarm-evidence/Dockerfile
    container_name: bugswarm-evidence
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "bugswarm-evidence", "health"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s
    depends_on:
      bugswarm-cpg:
        condition: service_healthy
    volumes:
      - bugswarm-run:/var/run/bugswarm
      - bugswarm-data:/var/lib/bugswarm/data
    tmpfs:
      - /var/run/bugswarm:uid=1000,gid=1000,mode=0755
    environment:
      - BUGSWARM_LOG_LEVEL=info
      - BUGSWARM_SOCKET_PATH=/var/run/bugswarm/evidence.sock
      - BUGSWARM_DATA_DIR=/var/lib/bugswarm/data
    deploy:
      resources:
        limits:
          memory: 1G
          cpus: "1.0"

  bugswarm-symbolic:
    image: bugswarm/symbolic:latest
    build:
      context: .
      dockerfile: bugswarm-symbolic/Dockerfile
    container_name: bugswarm-symbolic
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "bugswarm-symbolic", "health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 30s
    volumes:
      - bugswarm-run:/var/run/bugswarm
    tmpfs:
      - /var/run/bugswarm:uid=1000,gid=1000,mode=0755
    environment:
      - BUGSWARM_LOG_LEVEL=info
      - BUGSWARM_SOCKET_PATH=/var/run/bugswarm/symbolic.sock
    deploy:
      resources:
        limits:
          memory: 2G
          cpus: "2.0"

  bugswarm-sandbox:
    image: bugswarm/sandbox-python-asan:latest
    build:
      context: bugswarm-sandbox/docker/
      dockerfile: sandbox-python-asan.Dockerfile
    container_name: bugswarm-sandbox
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "bugswarm-sandbox", "health"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s
    depends_on:
      bugswarm-cpg:
        condition: service_healthy
      bugswarm-evidence:
        condition: service_healthy
    volumes:
      - bugswarm-run:/var/run/bugswarm
      - /var/run/docker.sock:/var/run/docker.sock
    tmpfs:
      - /var/run/bugswarm:uid=1000,gid=1000,mode=0755
    environment:
      - BUGSWARM_LOG_LEVEL=info
      - BUGSWARM_SOCKET_PATH=/var/run/bugswarm/sandbox.sock
      - BUGSWARM_CPG_SOCKET=/var/run/bugswarm/cpg.sock
      - BUGSWARM_EVIDENCE_SOCKET=/var/run/bugswarm/evidence.sock
    deploy:
      resources:
        limits:
          memory: 4G
          cpus: "4.0"

volumes:
  bugswarm-lib:
    name: bugswarm_lib
  bugswarm-data:
    name: bugswarm_data
  bugswarm-run:
    name: bugswarm_run
    driver_opts:
      type: tmpfs
      device: tmpfs
```

**Immunity**: `tests/integration/test_docker_compose.py::test_all_services_start_and_healthy` — runs `docker compose up -d`, waits for all 4 to become healthy (120s timeout), verifies `docker compose ps` shows healthy. `tests/integration/test_docker_compose.py::test_service_restart_on_failure` — kills sandbox, verifies Docker restarts within 30s.

**Success criteria**: `docker compose up -d` starts all 4 services in correct order. Health checks confirm all running.

---

#### H19(verify): Alerting engine now wired (verification)

**Status**: Already implemented in Batch 2 (H19). This is verification.
**Verification**: Integration test confirms Slack webhook receives POST on alert condition. If no webhook configured, test verifies graceful no-op.

---

### Batch 5 — VERIFICATION (3 findings)

**Rationale**: End-to-end verification of all fixes. Build all images, run full integration suite, verify cross-daemon protocols.

**Pre-requisites**: All previous batches complete. All Docker images built. docker-compose.yml functional.

**Findings**: H34(verify), H36(verify), H25(verify), H30(verify)

**Batch verification checklist**:
- [ ] `docker build` all 4 images passes clean
- [ ] `docker compose up -d` starts all services healthy
- [ ] Full E2E test passes: agent investigation -> sandbox -> CPG -> evidence -> results
- [ ] diff_execute protocol verified end-to-end
- [ ] Crash collection verified with real fuzz campaign

---

#### H34(verify): Default config image builds and works

**Status**: Already fixed by L1 (sandbox-base.Dockerfile creation).
**Verification**:
```bash
docker build -t bugswarm/sandbox-python-asan:latest bugswarm-sandbox/docker/
docker run --rm bugswarm/sandbox-python-asan:latest python3 --version
```
**Immunity**: CI docker-build job builds this image on every PR.

---

#### H36(verify): Container removal errors now properly propagated

**Status**: Already fixed by L16 (replaced `let _ = remove_container()` with proper error propagation).
**Verification**:
```bash
# Verify removal errors are propagated, not silently swallowed
grep -r 'let _ = remove_container' bugswarm-sandbox/src/ && echo "FAIL: still using let _ =" || echo "PASS: no silent error discard"
grep 'remove_container.*context\b' bugswarm-sandbox/src/container.rs && echo "PASS: contains error context" || echo "FAIL: missing context"
```
**Immunity**: CI clippy check catches `let _ =` patterns in unsafe/fallible contexts.

---

#### H25(verify): diff_execute protocol verified end-to-end

**Status**: Implementation in Batch 3. Verification confirms client and daemon speak same protocol.
**Verification**:
```python
# tests/integration/test_diff_execute_e2e.py
async def test_diff_execute_end_to_end():
    client = SandboxClient(socket_path="/var/run/bugswarm/sandbox.sock")
    result = await client.diff_execute(
        input_code="x = 1 + 2; print(x)",
        reference_code="x = 2 + 1; print(x)",
    )
    assert result.success is True
    assert result.diff_type in ("identical", "semantic-diff", "syntactic-diff")
```
**Success criteria**: No protocol mismatch errors. Response fields match client expectations.

---

#### H30(verify): Crash collection verified with real fuzz campaign

**Status**: Implementation in Batch 3. Verification confirms crash files reach JSONL queue.
**Verification**:
```bash
# Run a real fuzz campaign against a known-crashy target
docker compose up -d
python -m bugswarm.agent investigate --target test_targets/crash_prone.py
python -c "
import json, time
# Wait for crash collector to detect and write
time.sleep(15)
with open('/tmp/bugswarm-crashes.jsonl') as f:
    entries = [json.loads(line) for line in f]
assert len(entries) > 0, 'No crash entries found'
for entry in entries:
    assert 'stack_hash' in entry
    assert 'campaign_id' in entry
    assert 'content' in entry
print(f'{len(entries)} crash entries collected successfully')
"
```
**Success criteria**: At least 1 crash entry in JSONL queue. Entries have valid stack_hash and campaign_id.

---

## Section 4: Verification Matrix (39 findings)

| ID | Category | Batch | Fix Type | Test Type | Automated? | Test Location |
|----|----------|-------|----------|-----------|------------|---------------|
| H11 | Security | 0 | Code (bind-mount) | Fuzz + Integration | Yes | `tests/fuzz/test_shell_injection.py`, `tests/integration/test_sandbox_execution.py` |
| H12 | Security | 0 | Code (path validation) | Unit + Integration | Yes | `tests/unit/test_read_file_path_validation.py`, `tests/integration/test_agent_tools.py` |
| H1 | Deployment | 1 | Code (Dockerfiles) | CI + Smoke | Yes | `ci/check_dockerfiles.sh`, CI docker-build job |
| H2 | Deployment | 1 | Code (Dockerfile) | CI | Yes | CI docker-build job, docker inspect checks |
| H5 | DevOps | 1 | Code (CI yaml) | CI | Yes | `.github/workflows/ci.yml` |
| H6 | Build | 1 | Code (version) | CI | Yes | CI version check |
| H8 | Deployment | 1 | Code (systemd) | Integration | Yes | `tests/integration/test_daemon_restart.py` |
| H15 | Code Quality | 1 | Code (SAFETY docs) | CI | Yes | `ci/check_safety_comments.sh` |
| H33 | Build | 1 | Code (dep bounds) | CI | Yes | CI `cargo update --dry-run` |
| H35 | Docs | 1 | Code (changelog) | CI | No | CI file existence check |
| H37 | Code Quality | 1 | Code (Lazy regex) | Unit | Yes | `tests/unit/test_differential.rs` |
| H38 | Code Quality | 1 | Code (error handle) | Unit | Yes | `tests/unit/test_ssa.rs` |
| H39 | Code Quality | 1 | Code (Lazy regex) | Unit | Yes | `tests/unit/test_parser.rs` |
| H40 | Code Quality | 1 | Code (context) | Unit | Yes | `tests/unit/test_evidence_main.rs` |
| H41 | Code Quality | 1 | Code (anyhow) | Unit | Yes | `tests/unit/test_danger_map.rs` |
| H3 | Reliability | 2 | Code (error prop) | Integration | Yes | `tests/integration/test_daemon_startup.rs` |
| H4 | Reliability | 2 | Code (tokio select) | Integration | Yes | `tests/integration/test_graceful_shutdown.py` |
| H7 | Reliability | 2 | Code (hard error) | Integration | Yes | `tests/integration/test_config_parsing.py` |
| H9 | Robustness | 2 | Code (unwrap removal) | CI (clippy) | Yes | `cargo clippy -- -D clippy::unwrap_used` |
| H10 | Operability | 2 | Code (SIGHUP) | Integration | Yes | `tests/integration/test_config_reload.py` |
| H13 | Security | 2 | Code (secrets) | Unit | Yes | `tests/unit/test_secrets.py` |
| H14 | Resilience | 2 | Code (breaker) | Integration | Yes | `tests/integration/test_circuit_breaker.py` |
| H16 | Security | 2 | Code (seccomp) | Integration | Yes | `tests/integration/test_seccomp.py` |
| H17 | Security | 2 | Code (gating) | Unit | Yes | `tests/unit/test_tool_gating.py` |
| H19 | Monitoring | 2 | Code (Slack) | Unit | Yes | `tests/unit/test_alerting.py` |
| H21 | Functional | 3 | Code (PoC exec) | Integration | Yes | `tests/integration/test_invariant_mining.py` |
| H22 | Functional | 3 | Code (PoC exec) | Integration | Yes | `tests/integration/test_invariant_check.py` |
| H23 | Functional | 3 | Code (unittest) | Integration | Yes | `tests/integration/test_mutation_testing.py` |
| H24 | Functional | 3 | Code (feat-gated) | Unit | Yes | `tests/unit/test_solve_reachability.rs` |
| H25 | Functional | 3 | Verify (protocol) | Integration | Yes | `tests/integration/test_diff_execute.py` |
| H26 | Functional | 3 | Code (wire-up) | Integration | Yes | `tests/integration/test_orchestrator.py` |
| H27 | Functional | 3 | Code (wire-up) | Integration | Yes | `tests/integration/test_orchestrator.py` |
| H28 | Functional | 3 | Code (CPG query) | Unit | Yes | `tests/unit/test_argument_ranges.rs` |
| H29 | Functional | 3 | Code (SHM setup) | Integration | Yes | `tests/integration/test_danger_map_shm.py` |
| H30 | Functional | 3 | Code (crash coll.) | Integration | Yes | `tests/integration/test_crash_collection.py` |
| H31 | Functional | 3 | Code (wire-up) | Unit | Yes | `tests/unit/test_orchestrator_tools.py` |
| H32 | Functional | 3 | Code (bind-mount) | Unit | Yes | `tests/unit/test_shell_escape.rs` |
| H18 | Monitoring | 4 | Code (feat-gated) | Integration | Yes | `tests/integration/test_telemetry.py` |
| H20 | Monitoring | 4 | Code (req-id) | Integration | Yes | `tests/integration/test_request_correlation.py` |
| G18 | Deployment | 4 | Code (compose) | Integration | Yes | `tests/integration/test_docker_compose.py` |
| H34 | Build | 5 | Verify | CI | Yes | CI docker-build job |
| H36 | Build | 5 | Verify | CI (clippy) | Yes | `cargo clippy -- -D warnings` |
| H25 | Functional | 5 | Verify (e2e) | E2E | Yes | `tests/e2e/test_full_investigation.py` |
| H30 | Functional | 5 | Verify (e2e) | E2E | Yes | `tests/e2e/test_full_investigation.py` |

---

## Section 5: Rollback Strategy

### Per-Batch Rollback
Each batch is independently committable and revertible:
```bash
# Roll back a single batch (example: Batch 3)
git revert <batch-3-commit> --no-edit
```

### Docker Image Rollback
Tag previous version before pushing new:
```bash
# Before deploying Batch 1:
docker tag bugswarm/cpg:latest bugswarm/cpg:backup-pre-batch1
docker tag bugswarm/evidence:latest bugswarm/evidence:backup-pre-batch1

# Roll back if needed:
docker tag bugswarm/cpg:backup-pre-batch1 bugswarm/cpg:latest
docker tag bugswarm/evidence:backup-pre-batch1 bugswarm/evidence:latest
docker compose up -d
```

### Data Migration Rollback
Before any schema change (evidence graph, SHM format):
1. Create backup of `/var/lib/bugswarm/data/`
2. Increment schema version number in the data
3. On rollback, restore from backup and revert schema version

### Dead PID File Recovery (G19 mitigation)
On daemon kill with SIGKILL, stale PID files persist (`Drop` doesn't run). Recovery:
```bash
# Check if PID is actually alive
if ! kill -0 $(cat /var/run/bugswarm/sandbox.pid) 2>/dev/null; then
    rm /var/run/bugswarm/sandbox.pid
    systemctl restart bugswarm-sandbox
fi
```
This is documented in systemd unit file comments and in operational runbook.

---

## Section 6: Dependency Graph

```
CRIT-PRE1..5 (must complete before any batch)
          |
    +-----+-----+
    |           |
  Batch 0    Batch 1
  (H11,H12)  (H1,H2,H5,H6,H8,H15,H33,H35,H37-H41)
    |           |
    +-----+-----+
          |
       Batch 2
  (H3,H4,H7,H9,H10,H13,H14,H16,H17,H19)
          |
       Batch 3
  (H21-H32)
          |
       Batch 4
  (H18,H20,docker-compose,H19-verify)
          |
       Batch 5
  (H34-verify,H25-verify,H30-verify)
```

**Parallelism opportunities**:
- Batch 0 and Batch 1 can run in parallel (no dependencies between them)
- Within Batch 1: all 12 findings are independent, can be parallelized across developers
- Within Batch 2: H3, H4, H7, H9, H10, H13, H16, H19 are independent. H14 and H17 have minor cross-depends (H14 uses H4's tracing, H17 uses H12)
- Within Batch 3: H21/H22 share execution pipeline. H29/H30 share SHM/crash infrastructure. Others independent.

**No finding's fix breaks another** — each batch is tested independently before proceeding. Integration tests run at batch boundaries to catch regressions.
