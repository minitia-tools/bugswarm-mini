# Phase 20: Coverage-Guided Fuzzing — AFL++ in Sandbox

**Status**: NOT_STARTED
**Estimated Effort**: 18 hours
**Depends On**: Phase 1 (Sandbox), Phase 17 (Data Flow Analysis)
**Unblocks**: Phase 21 (Taint-Guided Fuzzing)

---

## A1. What is being built?

AFL++ integrated into the sandbox daemon. The fuzzer runs continuously inside isolated containers, generating thousands of mutated inputs per second. Coverage instrumentation tracks which code paths are discovered. Any crash is automatically triaged, deduplicated by stack trace hash, and injected into the evidence graph as an auto-discovered finding. Agents investigate and explain the crashes.

---

## A2. Which specific gap does it fill?

**Gap ID**: INV-003 (from `invincible.md`)
**Current behavior**: PoCs are handwritten by agents one at a time. An agent tests `x=0` and `x=1` but never finds the crash at `x=2^31`.
**Target behavior**: Fuzzer generates 1000s of inputs/second. Discovers crashes agents never consider. Auto-injects into evidence graph.

---

## A3. What is the success criteria?

| Metric | Target | Measurement |
|--------|--------|-------------|
| Crash discovery rate | ≥1 unique crash per 10K fuzzer executions | Count unique stack trace hashes |
| Coverage gain | ≥20% new code paths within 60s | Compare before/after coverage maps |
| Deduplication accuracy | 100% — no duplicate crash reports | All crashes with same stack trace hash merged |
| Sandbox isolation | 0 host escapes across 1M fuzzer executions | Host integrity check before/after |
| Agent handoff | 100% of fuzzer crashes investigated by agents | Every crash → evidence graph → agent receives notification |

---

## A4. What is the priority and why?

**Priority**: 5th in the invincibility stack. Independent of previous phases except sandbox. Highest ROI for crash-based bugs (memory errors, buffer overflows, use-after-free). Unlocks Phase 21 (taint-guided fuzzing for 10x efficiency).

---

## A5. What is NOT being built?

- NOT taint-guided fuzzing (that's Phase 21 — this is uniform coverage-guided)
- NOT a custom fuzzer (AFL++ is the gold standard, battle-tested for 10+ years)
- NOT fuzzing of network protocols (only local binaries/Python via AFL++'s forkserver)
- NOT grammar-based fuzzing (mutation-based, not generation-based)

---

## B1. Integration point?

**Primary**: Sandbox daemon (`bugswarm-sandbox/src/container.rs`) — new fuzz execution mode
**New files**:
- `bugswarm-sandbox/src/fuzzer.rs` — AFL++ integration: spawn, monitor, collect crashes, deduplicate
- `bugswarm-sandbox/docker/sandbox-fuzz.Dockerfile` — Image with AFL++ compiled in

**Modified files**:
- `bugswarm-sandbox/src/container.rs` — Add `fuzz()` method alongside `execute()`
- `bugswarm-sandbox/src/config.rs` — Add fuzzer config (timeout, seed inputs, coverage map path)
- `bugswarm-sandbox/src/daemon.rs` — Add `fuzz` method to daemon protocol
- `agent/src/agent/tools.py` — Add `fuzz_target` tool for agents to invoke fuzzer

---

## B2. Data flow?

```
Agent submits fuzz request: {target: "auth.py:login", seeds: [input1, input2], duration: 60s}
  │
  ├─→ Sandbox creates fuzzer container
  │     ├─ AFL++ compiled with coverage instrumentation
  │     ├─ Seed inputs copied to container
  │     └─ Fuzzer loop starts:
  │         ├─ Mutate input (bit flip, byte insert, arithmetic, havoc)
  │         ├─ Execute target with mutated input
  │         ├─ Check coverage map — new paths?
  │         │   ├─ Yes → keep input in queue, mutate further
  │         │   └─ No → discard
  │         └─ Crash?
  │             ├─ Yes → capture stack trace + input
  │             │         hash = sha256(stack_trace)
  │             │         if hash not in seen_crashes:
  │             │           save crash
  │             │           inject into evidence graph
  │             └─ No → continue
  │
  ├─→ After duration: fuzzer stops
  │     └─ Returns: {unique_crashes: 3, total_executions: 50000, coverage: 67%}
  │
  └─→ Agent receives: "3 unique crashes found. Investigating crash #1..."
        → Agent reads crash input, analyzes stack trace, explains bug
```

---

## B3. New types/schemas?

### Rust: FuzzResult (in `bugswarm-sandbox/src/config.rs`)

```rust
pub struct FuzzResult {
    pub unique_crashes: Vec<FuzzCrash>,
    pub total_executions: u64,
    pub coverage_pct: f64,
    pub duration_secs: f64,
    pub seed_inputs: Vec<String>,
}

pub struct FuzzCrash {
    pub crash_id: String,            // SHA256 of stack trace
    pub input_hex: String,           // Hex-encoded crashing input
    pub input_size: usize,
    pub stack_trace: Vec<String>,    // Top 10 frames
    pub crash_type: String,          // SIGSEGV, SIGABRT, SIGFPE
    pub exit_code: i32,
    pub execution_time_ms: u64,
    pub is_unique: bool,
}
```

---

## C1. Core algorithm?

```
function fuzz_target(target_binary, seeds, duration_secs, container):
    # Prepare AFL++ environment
    mount_coverage_map(container, "/dev/shm/afl_map")
    set_cpu_affinity(container, cpu_core=0)  # Pin to one core
    
    # Write seeds
    for (i, seed) in enumerate(seeds):
        write_seed_file(container, f"seed_{i}", seed)
    
    # Start AFL++ with forkserver
    afl_cmd = f"afl-fuzz -i /seeds -o /output -t 1000+ -- {target_binary} @@"
    container.exec(afl_cmd, background=True)
    
    seen_crashes = HashSet()
    crashes = []
    
    start = now()
    while now() - start < duration_secs:
        sleep(1)
        
        # Check for new crashes in AFL output dir
        for crash_file in container.list_dir("/output/crashes/"):
            if crash_file == "README.txt": continue
            
            stack_trace = container.get_stack_trace(crash_file)
            crash_hash = sha256(stack_trace)
            
            if crash_hash not in seen_crashes:
                seen_crashes.insert(crash_hash)
                crashes.push(FuzzCrash { crash_id: crash_hash, ... })
    
    # Stop fuzzer
    container.kill("afl-fuzz")
    
    return FuzzResult {
        unique_crashes: crashes,
        total_executions: parse_afl_stats("/output/fuzzer_stats"),
        coverage_pct: compute_coverage("/dev/shm/afl_map"),
        duration_secs: now() - start,
    }
```

**Complexity**: O(E × M) where E = executions, M = mutation cost. AFL++ handles this internally.

---

## C6. Algorithmic Peak Analysis

### C6.1 Algorithm Inventory

| # | Component | Current Approach | Naive/Peak |
|---|-----------|-----------------|------------|
| 1 | Fuzzer engine | AFL++ 4.0 | **PEAK** — gold standard, forkserver-based, coverage-guided |
| 2 | Crash deduplication | Stack trace SHA256 | **PEAK** — deterministic, collision-resistant |
| 3 | Coverage tracking | AFL bitmap (64KB shared memory) | **PEAK** — standard AFL approach |
| 4 | Mutation strategy | AFL havoc (deterministic + random) | **PEAK** — battle-tested for 10+ years |
| 5 | Seed selection | Agent-provided PoC inputs | NAIVE → Coverage-weighted seed selection (C6.2.1) |

### C6.2.1 Seed Selection — Coverage-Weighted

**What**: Instead of using all agent PoCs as equal seeds, run each seed once, measure coverage, rank by coverage contribution, and fuzz the top-N seeds first. Seeds that discover unique coverage get fuzzed 10x longer.

**Deferred**: Requires per-seed coverage measurement (adds ~1s per seed). Acceptable for Phase 20 — uniform seed treatment is sufficient.

---

## D1. Aggressive unit tests? (12 tests)

| Test | Attack | Expected |
|------|--------|----------|
| `test_afl_binary_available` | Check AFL++ in sandbox image | Binary found at `/usr/bin/afl-fuzz` |
| `test_fuzz_simple_crash` | Fuzz a binary with known buffer overflow | Crash detected within 10s |
| `test_crash_deduplication` | Same crash twice → same hash | One unique crash reported |
| `test_fuzz_no_crash_target` | Fuzz a crash-free binary | 0 crashes. Coverage report generated. |
| `test_seed_input_used` | Provide 3 seeds | AFL++ starts with all 3 seeds in queue |
| `test_fuzz_timeout_respected` | Set duration=5s | Fuzzer stops at 5s ±1s |
| `test_fuzz_sandbox_isolation` | **AGGRESSIVE**: 100K fuzzer executions | Host filesystem unchanged |
| `test_fuzz_crash_injected_to_evidence` | Fuzzer finds crash → evidence graph | Finding with `finding_source="fuzzer"` |
| `test_fuzz_concurrent_containers` | **AGGRESSIVE**: 4 fuzzers on 4 cores | All 4 produce results. No interference. |
| `test_fuzz_empty_seeds` | **AGGRESSIVE**: 0 seeds provided | AFL++ generates random seed. Fuzzes normally. |
| `test_fuzz_large_crash_input` | **AGGRESSIVE**: 1MB crashing input | Stored as hex. Receipt <20KB. |
| `test_fuzz_coverage_map_read` | Parse AFL bitmap | Coverage percentage computed correctly |

---

## D3. Extreme gate test — The Fuzz Crucible? (8 attack vectors)

1. Fuzz a binary with 5 known crashes (different stack traces). All 5 detected. All deduplicated correctly.
2. 60-second fuzz run on a crash-free binary. 0 false positives.
3. Host integrity: sha256sum /etc before/after 1M fuzzer executions. Identical.
4. 4 concurrent fuzzer containers. All produce independent results. No shared memory corruption.
5. Agent submits fuzz request → fuzzer runs → crash found → evidence graph updated → agent investigates.
6. Fuzzer killed mid-execution (SIGKILL). No orphaned containers. No corrupted state.
7. 10K-execution stress test. No memory leak in sandbox daemon.
8. Coverage map correctly reports >0% coverage after fuzzing (if target has any code).

---

## Gate Receipt

```json
{"phase": 20, "gate": "fuzz_crucible", "status": "PENDING"}
```
