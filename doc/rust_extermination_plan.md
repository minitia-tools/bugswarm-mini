# Rust Extermination Plan — Fix Every Gap & Prove It Stays Dead

---

## PHASE R0: CRITICAL BROKEN (3 days — blocks everything)

### R0.1 `execute_statistical` returns garbage

**File**: `bugswarm-sandbox/src/container.rs:545-599`

**Root cause**: `tokio::spawn` can't capture `&self` across thread boundaries. The closure tries to call `self_ref.execute()` but `self` is borrowed from the outer async context which outlives the spawn.

**Fix**: Replace the spawn-with-pointer pattern with sequential batch execution:

```rust
// Replace L562-578 with:
for _ in chunk_start..chunk_end {
    match self.execute(&poc_content, env_vars, false).await {
        Ok(receipt) => { /* process receipt */ }
        Err(e) => { failures += 1; warn!("Statistical exec error: {}", e); }
    }
}
```

**Test**:
1. Submit a PoC that always exits code 0. Run `execute_statistical(poc, 50)`. Assert `failures == 0`, `passes == 50`.
2. Submit a PoC that always exits code 1. Run with 50 iterations. Assert `failures == 50`.
3. Submit a PoC that randomly exits 0 or 1. Run 200 iterations. Assert `|failure_rate - 0.5| < 0.15`.
4. Submit a PoC that triggers OOM. Assert `ooms > 0`, `failure_rate > 0`.
5. Submit a PoC that sleeps 200s. Assert `timeouts == total` (killed by wall-clock timeout).

### R0.2 `seal()` doesn't hash full node state

**File**: `bugswarm-evidence/src/types.rs:173-190`

**Root cause**: `seal()` only hashes `id:label:description:author`. Metadata, severity, timestamps excluded.

**Fix**: Include all mutable fields in the hash:

```rust
pub fn seal(&mut self) -> String {
    let content = format!(
        "{}:{}:{}:{}:{}:{}:{}:{}:{}",
        self.id, self.label, self.description, self.author,
        self.kind, self.severity.unwrap_or(0),
        self.independently_verified,
        serde_json::to_string(&self.metadata).unwrap_or_default(),
        self.created_at
    );
    let hash = hex::encode(Sha256::digest(content.as_bytes()));
    self.content_hash = Some(hash.clone());
    self.immutable = true;
    hash
}
```

**Test**:
1. Create node, seal it, modify metadata, call `verify()` → assert false
2. Create node, seal it, modify severity, call `verify()` → assert false
3. Create node, seal it, modify description, call `verify()` → assert false
4. Create node, DON'T seal it, call `verify()` → assert true (no hash = valid)
5. Create 1000 nodes, seal all, modify random fields, verify all → all corrupted detected

---

## PHASE R1: BUGS (4 days)

### R1.1 JS call handler never resolves local functions

**File**: `bugswarm-cpg/src/parser.rs:656-668`

**Fix**: Add `find_function` check before creating external stub:

```rust
// Before creating external stub, check if function exists locally:
let existing = cpg.find_function(call);
if let Some(target) = existing {
    cpg.add_edge(func_id, target, GraphEdge {
        kind: EdgeKind::Calls, label: Some(call.clone()),
        confidence: 0.9, metadata: HashMap::new(),
    });
} else {
    // Create external stub
    let stub = cpg.add_node(GraphNode { /* ... */ });
    cpg.add_edge(func_id, stub, GraphEdge {
        kind: EdgeKind::Calls, label: Some(call.clone()),
        confidence: 0.5, metadata: HashMap::new(),
    });
}
```

**Test**:
1. Parse a JS file where `foo()` calls `bar()` defined in the same file. Assert call edge confidence == 0.9 (not 0.5).
2. Parse a JS file where `foo()` calls `external_lib.something()`. Assert stub created with confidence 0.5.
3. Parse JS + Python files in same repo. Verify cross-language edges exist.

### R1.2 `reaching_to` is O(N^2)

**File**: `bugswarm-cpg/src/graph.rs:466-487`

**Fix**: Use petgraph's built-in reverse walker:

```rust
fn reaching_to(&self, target: NodeId) -> Vec<NodeId> {
    use petgraph::visit::{Dfs, Reversed};
    let mut dfs = Dfs::new(&self.graph, target);
    // Walk incoming edges by reversing the graph view
    let mut result = Vec::new();
    while let Some(node) = dfs.next(Reversed(&self.graph)) {
        result.push(node);
    }
    result
}
```

**Test**:
1. Build graph with 1000 nodes in a chain A1→A2→...→A1000. Call `reaching_to(A500)`. Assert 500 nodes returned. Assert < 10ms.
2. Build star graph: 999 nodes all point to center. Call `reaching_to(center)`. Assert 1000 nodes. Assert < 10ms.

### R1.3 Taint BFS visited set shadows alternative paths

**File**: `bugswarm-cpg/src/graph.rs:328-379`

**Fix**: Replace global visited set with per-path tracking. Use a struct holding `(node, path, sanitized)` and maintain visited per-BFS-level:

```rust
// Key change: visited tracks (node, is_sanitized) so a tainted path
// can revisit a node that was only seen on a sanitized path
let mut visited: HashSet<(NodeId, bool)> = HashSet::new();
visited.insert((source, false));

while let Some((current, path, sanitized, sanitizer)) = queue.pop_front() {
    // ... check for sink match ...
    for edge in self.graph.edges(current) {
        let target = edge.target();
        let is_sanitized = sanitized || edge.weight().kind == EdgeKind::Sanitized;
        let key = (target, is_sanitized);
        if !visited.contains(&key) {
            visited.insert(key);
            // ... enqueue ...
        }
    }
}
```

**Test**:
1. Build graph: Source → Sink (via DataFlow). Also Source → Sanitizer → Sink (via Sanitized). Run taint. Assert TWO paths found: one sanitized, one not.
2. Build diamond: Source → A → Sink AND Source → B → Sink. Run taint. Assert both paths found.

### R1.4 TOCTOU race in evidence graph add_* methods

**Files**: `bugswarm-evidence/src/graph.rs:54-57, 73-75, 91-93`

**Fix**: Use a single write lock for the entire add operation. Replace the read-then-write pattern:

```rust
pub fn add_sandbox_run(&self, label: &str, description: &str, author: &str, receipt_json: &str) -> NodeId {
    let mut nodes = self.nodes.write();
    let id = nodes.len();
    let mut node = EvidenceNode::new(id, NodeKind::SandboxRun, label, author);
    node.description = description.to_string();
    node.metadata.insert("receipt".into(), receipt_json.to_string());
    node.seal();
    nodes.push(node);
    id
}
```

**Test**:
1. 10 threads each call `add_sandbox_run` 100 times simultaneously. Assert exactly 1000 nodes created, all with unique IDs.
2. 10 threads each call `add_claim` 100 times. Assert exactly 1000 claims, no ID collisions.
3. Mix `add_claim` + `add_sandbox_run` + `add_edge` from 20 threads. Assert graph consistent: no dangling edges, all nodes reachable.

### R1.5 `ensure_image` returns OK on pull failure

**File**: `bugswarm-sandbox/src/container.rs:65-87`

**Fix**: Track whether any layers failed and return error:

```rust
let mut had_error = false;
while let Some(result) = stream.next().await {
    match result {
        Ok(_) => {}
        Err(e) => {
            had_error = true;
            warn!("Image pull error: {}", e);
        }
    }
}
if had_error {
    return Err(SandboxError::DockerUnavailable(format!("Failed to pull image: {}", image)));
}
Ok(image.clone())
```

**Test**:
1. Set config image to `nonexistent-image:999`. Call `ensure_image()`. Assert Err returned.
2. Set config image to valid image. Call `ensure_image()`. Assert Ok returned, image SHA available.
3. Kill Docker daemon mid-pull. Call `ensure_image()`. Assert Err returned, not panic.

---

## PHASE R2: DEAD CODE + STUBS (3 days)

### R2.1 Remove 4 unused SandboxError variants

**File**: `bugswarm-sandbox/src/error.rs:13-24`

Remove: `Timeout`, `OomKilled`, `PiiDetected`, `ReceiptValidation`. If they represent real errors the system should handle, implement them. Otherwise delete.

**Test**: `cargo build` must pass. Grep for removed variant names — zero matches.

### R2.2 Implement real memory profiling

**File**: `bugswarm-sandbox/src/container.rs:267-270, 309, 321-326, 357-361`

**Fix**: Enable Docker stats stream during execution. Track peak RSS:

```rust
// Replace polling loop with stats stream:
let mut stats_stream = self.docker.stats(&container_name, Some(StatsOptions { stream: true, one_shot: false }));

loop {
    tokio::select! {
        stats = stats_stream.next() => {
            if let Some(Ok(s)) = stats {
                if let Some(mem) = s.memory_stats {
                    peak_memory = peak_memory.max(mem.usage.unwrap_or(0));
                }
            }
        }
        _ = tokio::time::sleep(Duration::from_millis(200)) => {
            // Check container state
        }
    }
}
```

**Test**:
1. Run PoC that allocates 100MB. Assert `memory_profile.peak_mb >= 90`.
2. Run PoC that allocates in a loop (gradual growth). Assert `growth_rate_mb_per_sec > 0`.
3. Run PoC that triggers OOM (allocate 600MB with 512MB limit). Assert `oom_killed == true`, `peak_mb >= 500`.

### R2.3 Implement daemon mode (RunServer)

**File**: `bugswarm-sandbox/src/main.rs:290-297`

**Fix**: Create a Unix socket listener that accepts execution requests:

```rust
// Minimal daemon:
let listener = UnixListener::bind(&socket_path)?;
for stream in listener.incoming() {
    let mut stream = stream?;
    // Read JSON request: { "poc": "...", "env": {...} }
    // Call manager.execute()
    // Write JSON receipt response
}
```

**Test**:
1. Start daemon, send valid PoC via socket, receive receipt JSON back.
2. Start daemon, send invalid JSON, receive error response (daemon doesn't crash).
3. Start daemon, send SIGTERM, daemon shuts down cleanly with no orphaned containers.

### R2.4 Wire seccomp profile into Docker containers

**File**: `bugswarm-sandbox/src/container.rs:620-656`

**Fix**: Add `security_opt` with seccomp profile to `HostConfig`:

```rust
let seccomp_json = SeccompProfile::default_profile()?.to_docker_string()?;
security_opt: Some(vec![
    "no-new-privileges".into(),
    format!("seccomp={}", seccomp_json),
]),
```

**Test**:
1. Run container with seccomp profile. Inside container: `ls /proc/1/ns/pid` → works (read is allowed).
2. Inside container: `ptrace` → killed by seccomp. Verify exit code and log.
3. Inside container: `mount` → blocked by seccomp. Verify in stderr.

### R2.5 Implement config file loading

**File**: `bugswarm-sandbox/src/main.rs:164`

**Fix**: Actually read the config file if it exists:

```rust
let config = if cli.config.exists() {
    let content = std::fs::read_to_string(&cli.config)?;
    serde_yaml::from_str(&content)?
} else {
    SandboxConfig::default()
};
```

**Test**:
1. Create valid sandbox.yaml with `wall_clock_timeout_secs: 30`. Assert config loaded with correct value.
2. Create invalid YAML. Assert clear error message, not panic.
3. No config file. Assert defaults used.

---

## PHASE R3: PANIC SITES (1 day)

### R3.1 Replace all unwrap() calls with proper error handling

| File | Line | Current | Fix |
|------|------|---------|-----|
| `container.rs` | 135 | `escape_matches.first().unwrap()` | `.ok_or(SandboxError::Other("no match"))?` |
| `container.rs` | 328 | `exit_code.unwrap()` | Already guarded by `map_or` — ok, add comment |
| `main.rs` | 275 | `path.to_str().unwrap()` | `.ok_or_else(|| anyhow!("non-UTF8 path"))?` |
| `main.rs` | 301-302 | nested `unwrap()` | Use proper error propagation |
| `scanner.rs` | 132 | `Self::new().expect(...)` | Return Result from Default or use lazy_static |
| `parser.rs` | 887,893 | `Regex::new(...).unwrap()` | Use `lazy_static!` or `OnceLock` |

---

## PHASE R4: SILENT FAILURES (2 days)

### R4.1 Log all dropped errors

**Files**: `container.rs:259, 312, 347, 349, 392`

Every `let _ = self.docker.xxx()` → `if let Err(e) = self.docker.xxx() { warn!("cleanup failed: {}", e); }`

### R4.2 Log container inspection failures

**File**: `container.rs:304`

Replace `.ok()` with explicit error logging:

```rust
let inspect = self.docker.inspect_container(&container_name, None).await;
let (exit_code, oom_killed) = match inspect {
    Ok(i) => (i.state.as_ref().and_then(|s| s.exit_code),
              i.state.as_ref().and_then(|s| s.oom_killed).unwrap_or(false)),
    Err(e) => {
        warn!("Container inspect failed: {}", e);
        (None, false)
    }
};
```

### R4.3 Warn on malformed env vars

**File**: `main.rs:149-161`

Add `warn!("Skipping malformed env var: {}", arg)` when `parts.len() != 2`.

---

## PHASE R5: PERFORMANCE (3 days)

### R5.1 Cache compiled regexes

**Files**: `container.rs:449-532`

```rust
use std::sync::OnceLock;
static PYTHON_TRACEBACK: OnceLock<Regex> = OnceLock::new();
// Use PYTHON_TRACEBACK.get_or_init(|| Regex::new(...).unwrap())
```

### R5.2 Replace polling loop with Docker wait API

**File**: `container.rs:252-283`

Use `self.docker.wait_container(&container_name)` instead of polling `inspect_container` every 200ms.

### R5.3 Cache CPG between commands

**File**: `cpg/src/main.rs:92-94, 113-115, 145-147, 159-161`

Add a `--cache` flag. If set, serialize CPG to disk after indexing, load from disk for subsequent commands. Use `serde` or a binary format.

### R5.4 Fix O(n^4) echo chamber

**File**: `evidence/src/graph.rs:312-357`

Replace the triple-nested-loop approach with a single pass that groups claims by agent, then checks edges only between agents that actually have edges (not all pairs).

---

## PHASE RX: ENTERPRISE GAPS (5 days)

### RX.1 Add persistence to Evidence Graph

**File**: `evidence/src/graph.rs`

Add `save(path: &Path)` and `load(path: &Path)` methods using `serde_json` or a binary format. Call `save()` after every batch of changes. Call `load()` on startup.

### RX.2 Add Prometheus metrics endpoint

**File**: `sandbox/src/main.rs` (in daemon mode)

Add `GET /metrics` endpoint returning Prometheus text format with counters for: executions_total, executions_failed, executions_oom, bytes_stdout_total, pii_redactions_total.

### RX.3 Add graceful shutdown

**File**: `sandbox/src/main.rs`

Register SIGTERM/SIGINT handler. On signal: drain execution queue, wait for running containers (with timeout), clean up, exit.

### RX.4 Add shared type library crate

Create `bugswarm-types` crate with `ExecutionReceipt`, `MemoryProfile`, `NodeKind`, `EdgeKind` shared between sandbox, CPG, and evidence.

### RX.5 Wire sandbox ↔ CPG ↔ evidence via IPC

Add a gRPC or Unix-socket-based protocol so the three crates can communicate. Sandbox receives PoC execution requests from CPG/agent. CPG receives codebase indexing requests from agent. Evidence receives sandbox receipts for immutability tracking.

---

## EXTREME AGGRESSIVE TESTING PLAN

After each Rust phase, execute these torture tests. Any failure → fix before proceeding.

### TORTURE-0: Statistical Execution (tests R0.1)

```bash
# Deterministic PoC: always pass
echo 'import sys; sys.exit(0)' > /tmp/always_pass.py
bugswarm-sandbox execute-statistical --poc /tmp/always_pass.py --count 100
# ASSERT: failures == 0, passes == 100

# Deterministic PoC: always fail
echo 'import sys; sys.exit(1)' > /tmp/always_fail.py
bugswarm-sandbox execute-statistical --poc /tmp/always_fail.py --count 100
# ASSERT: failures == 100, passes == 0

# Flaky PoC: 50% fail
cat > /tmp/flaky.py << 'EOF'
import random, sys; sys.exit(0 if random.random() > 0.5 else 1)
EOF
bugswarm-sandbox execute-statistical --poc /tmp/flaky.py --count 200
# ASSERT: 40 <= failures <= 60, is_significant == true
```

### TORTURE-1: Evidence Integrity (tests R0.2, R1.4)

```bash
# Concurrent write storm
# 25 threads, each doing 100 add_claim + add_edge. Verify zero corruption.
# All 2500 claims must exist. All edges must reference valid nodes.
# No duplicate IDs.

# Seal corruption detection
# Create 500 nodes, seal all. Modify 50 random fields in 50 random nodes.
# verify_integrity() must return exactly 50 corrupted IDs.
```

### TORTURE-2: CPG Correctness (tests R1.1, R1.2, R1.3)

```bash
# JS local call resolution
# Parse JS with 10 internal cross-calls. All must have confidence >= 0.9.

# Taint path completeness
# Graph with 5 sources, 5 sinks, 25 DataFlow edges between them.
# find_taint_paths() must return 25 paths (not blocked by visited set).

# Performance: 10,000-node graph
# stats() must complete in < 5 seconds.
# reaching_to() must complete in < 100ms.
```

### TORTURE-3: Sandbox Resilience (tests R1.5, R2.2, R2.4, R4)

```bash
# Memory profiling accuracy
# PoC allocates 200MB in 50MB chunks.
# peak_mb must be >= 190, growth_rate > 0.

# Seccomp enforcement
# PoC calls ptrace. Container killed. stderr contains "Operation not permitted".

# Image pull failure
# Set image to nonexistent. ensure_image() returns Err, not Ok.

# Container cleanup
# Submit 50 PoCs. After all complete, docker ps -a shows zero bugswarm containers.
```

### TORTURE-4: Daemon Mode (tests R2.3, R2.5, RX.2, RX.3)

```bash
# Start daemon, send 100 concurrent execution requests via Unix socket.
# All 100 receive valid receipts. Daemon doesn't crash.

# Kill daemon with SIGTERM. Verify no orphaned containers.
# Restart daemon. Verify it resumes accepting requests.

# Hit /metrics endpoint. Verify all counters present and incrementing.

# Send malformed JSON. Daemon returns error, stays alive.
# Send PoC with escape attempt. Daemon rejects, logs CRITICAL.
```

### TORTURE-5: Integration (tests RX.4, RX.5)

```bash
# Full pipeline integration test:
# 1. CPG indexes a Python repo with known vulnerabilities
# 2. Evidence graph receives taint paths from CPG
# 3. Agent requests sandbox execution for each taint path
# 4. Sandbox returns receipts
# 5. Evidence graph links receipts to claims
# 6. Verify end-to-end: claim → CPG taint → sandbox receipt → evidence confirmation
```

---

## EXECUTION ORDER

| Phase | Days | Blocks | Cumulative Gate |
|-------|------|--------|-----------------|
| R0: Critical | 3 | Everything | `execute_statistical` works, `seal()` unbreakable |
| R1: Bugs | 4 | — | JS resolution, O(N) reaching, taint paths complete, no TOCTOU |
| R2: Dead + Stubs | 3 | — | Memory real, daemon alive, seccomp wired, config loads |
| R3: Panics | 1 | — | Zero `unwrap()` in production paths |
| R4: Silent Failures | 2 | — | Every error logged, nothing dropped |
| R5: Performance | 3 | — | O(N²) eliminated, regex cached, CPG cached |
| RX: Enterprise | 5 | — | Persistence, metrics, graceful shutdown, shared types, IPC |

**Total: 21 days of focused Rust remediation.** Torture tests run after each phase. Zero failures to proceed.
