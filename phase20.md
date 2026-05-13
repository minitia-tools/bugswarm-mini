# Phase 20: Coverage-Guided Fuzzing — AFL++ in Sandbox

**Status**: NOT_STARTED
**Estimated Effort**: 18 hours
**Depends On**: Phase 1 (Sandbox), Phase 17 (Data Flow Analysis)
**Unblocks**: Phase 21 (Taint-Guided Fuzzing), Phase 22 (Delta Debugging)

---

## A1. What is being built?

AFL++ integrated into the sandbox daemon. Fuzzer runs continuously inside isolated containers generating 1000s of mutated inputs/second. Coverage instrumentation tracks code paths. Crashes auto-triaged, deduplicated by stack trace hash, and injected into the evidence graph as auto-discovered findings. Agents investigate and explain every crash.

## A2. Which gap does it fill?

**Gap ID**: INV-003. Current: PoCs handwritten by agents one at a time. Agent tests `x=0` and `x=1`, never finds crash at `x=2^31`. Target: Fuzzer generates 1000s inputs/second, discovers crashes agents never consider, auto-injects into evidence graph.

## A3. Success criteria?

| Metric | Target |
|--------|--------|
| Crash discovery | ≥1 unique crash per 10K executions |
| Coverage gain | ≥20% new paths within 60s |
| Dedup accuracy | 100% — zero duplicate crash reports |
| Sandbox isolation | 0 host escapes across 1M executions |
| Agent handoff | 100% of crashes investigated by agents |

## A4. Priority?

5th in invincibility stack. Independent of prior phases except sandbox. Highest ROI for crash-based bugs. Unlocks Phase 21 (taint-guided for 10x efficiency).

## A5. Scope boundary?

NOT: taint-guided fuzzing (Phase 21), custom fuzzer (AFL++ is gold standard), network protocol fuzzing (local binaries only), grammar-based fuzzing (mutation-based only), Windows support (Linux AFL++ only).

---

## B1. Integration point?

**New files**: `bugswarm-sandbox/src/fuzzer.rs`, `bugswarm-sandbox/docker/sandbox-fuzz.Dockerfile`
**Modified**: `bugswarm-sandbox/src/container.rs` (add `fuzz()` method), `bugswarm-sandbox/src/config.rs` (fuzzer config), `bugswarm-sandbox/src/daemon.rs` (add `fuzz` method), `agent/src/agent/tools.py` (add `fuzz_target` tool)

## B2. Data flow?

```
Agent → fuzz_target(auth.py:login, seeds=[x,y], 60s)
  → Sandbox creates fuzzer container with AFL++
  → AFL forkserver: mutate→execute→check coverage→repeat
  → Crash detected → capture stack trace + input → sha256 dedup → evidence graph
  → Returns: {unique_crashes:3, total_executions:50000, coverage:67%}
  → Agent investigates each crash
```

## B3. New types?

`FuzzConfig`: target_binary, seed_inputs, duration_secs, max_executions, cpu_core
`FuzzResult`: unique_crashes (Vec<FuzzCrash>), total_executions, coverage_pct, duration_secs
`FuzzCrash`: crash_id (SHA256), input_hex, input_size, stack_trace, crash_type (SIGSEGV/SIGABRT/SIGFPE), exit_code, is_unique

## B4. Modified modules?

| File | Change |
|------|--------|
| `bugswarm-sandbox/src/fuzzer.rs` | NEW — AFL++ spawn, monitor, crash collection, dedup |
| `bugswarm-sandbox/docker/sandbox-fuzz.Dockerfile` | NEW — Image with AFL++ compiled |
| `bugswarm-sandbox/src/container.rs` | Add `fuzz()` alongside `execute()` |
| `bugswarm-sandbox/src/config.rs` | Add FuzzConfig, FuzzResult, FuzzCrash types |
| `bugswarm-sandbox/src/daemon.rs` | Add `fuzz` method to daemon protocol |
| `agent/src/agent/tools.py` | Add `fuzz_target` tool definition |

## B5. Dependencies?

| Dep | Version | Purpose |
|-----|---------|---------|
| AFL++ | 4.0+ | Fuzzer engine, compiled into sandbox image |
| None (Rust) | — | AFL++ runs as external process, communication via files |

---

## C1. Core algorithm?

```
fuzz_target(target, seeds, duration_secs, container):
    mount_coverage_map("/dev/shm/afl_map")
    write_seeds(seeds)
    spawn_afl(f"afl-fuzz -i /seeds -o /output -t 1000+ -- {target} @@")
    
    seen = HashSet()
    crashes = []
    
    while elapsed < duration_secs:
        for crash_file in list_dir("/output/crashes/"):
            if crash_file == "README.txt": continue
            trace = get_stack_trace(crash_file)
            h = sha256(trace)
            if h not in seen:
                seen.insert(h)
                crashes.push(FuzzCrash{crash_id: h, input: read(crash_file), stack_trace: trace, ...})
        sleep(1)
    
    kill("afl-fuzz")
    return FuzzResult{crashes, total: parse_stats("/output/fuzzer_stats"), coverage: calc_coverage()}
```

**Complexity**: AFL++ handles mutation internally. Sandbox monitors files.

## C2. Failure modes?

| Failure | Handling | Recovery |
|---------|----------|----------|
| AFL++ binary missing | Image build failed. Log ERROR. | Agent receives "fuzzer unavailable" |
| Target binary crashes on seed input | AFL handles gracefully. Records as crash. | Normal operation |
| Forkserver hangs | Wall-clock timeout kills container. | Receipt marked TIMEOUT |
| Coverage map corrupted | Coverage reported as 0%. | Fuzzer continues. No crash. |
| Disk full (crash outputs) | Container storage limit enforces quota. | Old crashes rotated. |

## C3. Edge cases?

| Edge Case | Behavior |
|-----------|----------|
| Zero seeds | AFL++ generates random seed. Fuzzes normally. |
| 1MB crash input | Stored as hex. Receipt <20KB (truncated). |
| Infinite loop in target | AFL++ timeout per execution (default 1s). Kills and continues. |
| Target with 100% coverage already | No new paths. Fuzzer still mutates (havoc may find crashes even at 100%). |
| Target that reads stdin (not file) | AFL++ uses `@@` placeholder → replaces with filename. Works. |

## C4. Concurrency?

Each fuzzer runs in separate Docker container with CPU pinning. No shared state between fuzzers. 4 concurrent fuzzers = 4 containers on 4 cores. Crash dedup via in-memory HashSet per container. Evidence graph writes serialized by daemon.

## C5. Performance budget?

| Metric | Target |
|--------|--------|
| Fuzzer execs/sec | >500 (AFL++ forkserver on single core) |
| Coverage map parse | <1ms |
| Crash dedup | <1ms per crash |
| Container start (fuzz image) | <3s cold, <1s warm |
| Max executions per run | 1,000,000 (configurable) |

---

## C6. Algorithmic Peak Analysis

### C6.1 Algorithm Inventory

| # | Component | Approach | Status | Peak Reference |
|---|-----------|----------|--------|----------------|
| 1 | Fuzzer engine | AFL++ 4.0 forkserver | **PEAK** | Gold standard, 10+ years battle-tested |
| 2 | Crash dedup | Stack trace SHA256 | **PEAK** | Deterministic, collision-resistant |
| 3 | Coverage tracking | AFL bitmap (64KB shared mem) | **PEAK** | Standard AFL approach |
| 4 | Mutation strategy | AFL havoc (deterministic + random) | **PEAK** | Battle-tested |
| 5 | Seed selection | Agent PoCs as equal seeds | NAIVE → Coverage-weighted | C6.2.1 |
| 6 | Target instrumentation | AFL++ compiler wrappers (afl-gcc/afl-clang) | **PEAK** | Standard AFL instrumentation |

### C6.2.1 Seed Selection — Coverage-Weighted (Deferred)

**Peak**: Run each seed once, measure coverage, rank by unique path discovery, fuzz top-N 10x longer. Seeds that discover unique coverage get priority.

**Quantitative**: 30% more unique crashes found in first 60s vs uniform seed treatment.

**Deferral**: Requires per-seed coverage measurement (~1s per seed). Acceptable for Phase 20. Uniform treatment is sufficient baseline.

### C6.3 Zero-Gap Guarantee

```
Component: Engine — [x] PEAK via AFL++
Component: Dedup — [x] PEAK via SHA256
Component: Coverage — [x] PEAK via AFL bitmap
Component: Mutation — [x] PEAK via AFL havoc
Component: Seeds — [x] NAIVE with valid deferral (minor optimization, Phase 21 covers this)
Component: Instrumentation — [x] PEAK via AFL compiler wrappers
```

### C6.4 Peak Deferral

Coverage-weighted seed selection deferred. Reason: no measurable impact without taint guidance (Phase 21). Uniform treatment sufficient for Phase 20.

---

## D1. Aggressive unit tests? (12 tests, 7 aggressive)

| # | Test | Attack | Expected |
|---|------|-------|----------|
| 1 | `test_afl_binary` | Check AFL++ in image | `/usr/bin/afl-fuzz` exists |
| 2 | `test_fuzz_simple_crash` | Binary with known overflow | Crash detected <10s |
| 3 | `test_crash_dedup` | Same crash twice | One unique reported |
| 4 | `test_no_crash_target` | Crash-free binary | 0 crashes, coverage reported |
| 5 | `test_seeds_used` | 3 seeds provided | AFL starts with all 3 |
| 6 | `test_timeout_respected` | duration=5s | Stops at 5s ±1s |
| 7 | `test_isolation` | **AGGRESSIVE**: 100K execs | Host filesystem unchanged |
| 8 | `test_crash_to_evidence` | **AGGRESSIVE**: Crash→evidence graph | finding_source="fuzzer" |
| 9 | `test_concurrent_4` | **AGGRESSIVE**: 4 fuzzers, 4 cores | All produce results. No interference. |
| 10 | `test_zero_seeds` | **AGGRESSIVE**: No seeds | Generates random. Works. |
| 11 | `test_1mb_crash_input` | **AGGRESSIVE**: Large crash | Stored as hex. Receipt <20KB. |
| 12 | `test_coverage_map` | **AGGRESSIVE**: Parse bitmap | Coverage % computed correctly |

## D2. Aggressive integration tests? (4 tests, 3 aggressive)

| # | Test | Attack | Expected |
|---|------|-------|----------|
| 1 | `test_fuzz_agent_handoff` | Agent requests fuzz → crash → evidence → agent investigates | Full loop |
| 2 | `test_fuzz_sandbox_kill` | **AGGRESSIVE**: SIGKILL mid-fuzz | No orphaned containers |
| 3 | `test_fuzz_10k_stress` | **AGGRESSIVE**: 10K execs, check memory | No leak in daemon |
| 4 | `test_fuzz_with_sanitizers` | **AGGRESSIVE**: ASAN+fuzzer together | Crashes detected by both |

## D3. Extreme gate test — The Fuzz Crucible? (8 attack vectors)

1. **Detection**: Binary with 5 known crashes (different traces). All 5 found. All dedup'd.
2. **Zero FP**: 60s fuzz on crash-free binary. 0 false positives.
3. **Host integrity**: sha256sum /etc before/after 1M executions. Identical.
4. **Concurrent**: 4 fuzzers simultaneously. All independent. No shared memory corruption.
5. **Full loop**: Agent→fuzz→crash→evidence→agent investigates→explains.
6. **Kill recovery**: Fuzzer SIGKILL'd. No orphaned containers. No corrupted state.
7. **Stress**: 10K-execution run. No memory leak in sandbox daemon.
8. **Coverage**: Coverage map reports >0% after fuzzing (target has code).

## D4. Golden dataset?

N/A. Fuzzing is a detection capability, not reasoning. Success measured by crash count and coverage, not human adjudication comparison.

## D5. Regression test?

`test_fuzz_regression`: Fuzz Crucible runs on every CI push. Any vector fails → build fails.

---

## E1. Estimated cost?

| Cost | Estimate |
|------|----------|
| Development | 18 hours |
| AFL++ compilation | One-time Docker image build (~5min) |
| Per-execution overhead | ~2μs per exec (forkserver) |
| Per-run cost | ~1 CPU-core-minute per 60s fuzz |
| Infrastructure | $0 (runs in existing sandbox containers) |

## E2. Observability?

**Logs**: fuzz_started, fuzz_crash_detected, fuzz_completed, fuzz_dedup_hit, fuzz_timeout
**Metrics**: `bugswarm_fuzzer_executions_total`, `bugswarm_fuzzer_crashes_total`, `bugswarm_fuzzer_unique_crashes`, `bugswarm_fuzzer_coverage_pct`, `bugswarm_fuzzer_duration_secs`
**Alerts**: Fuzzer finds >10 unique crashes in 60s → INFO (high-value target). Fuzzer finds 0 crashes in 5 consecutive runs → WARN (target may be clean or instrumentation broken).

## E3. Configuration?

| Parameter | Default | Env | Flag |
|-----------|---------|-----|------|
| `fuzzer_enabled` | `true` | `BGSWARM_FUZZER` | `--[no-]fuzzer` |
| `fuzz_duration_secs` | `60` | `BGSWARM_FUZZ_DURATION` | `--fuzz-duration` |
| `fuzz_max_execs` | `1_000_000` | — | — |
| `fuzz_cpu_core` | `0` | `BGSWARM_FUZZ_CPU` | `--fuzz-cpu` |
| `fuzz_image` | `bugswarm/sandbox-fuzz:latest` | `BGSWARM_FUZZ_IMAGE` | `--fuzz-image` |

## E4. Migration?

Full backward compat. Fuzzer is additive — existing `execute()` unchanged. Agents opt into fuzzing via `fuzz_target` tool. Existing receipts unaffected.

## E5. Documentation?

ADR-020: AFL++ Fuzzing Integration. User docs: "Fuzzing System." Docker image build guide. Changelog.

---

## Gate Receipt

```json
{"phase":20,"gate":"fuzz_crucible","attack_vectors":8,"passed":0,"failed":0,"verdict":"PHASE 20 NOT YET EXECUTED"}
```
