# Phase 18.5: Integration Wiring — Connect Built Algorithms to Pipeline

**Status**: NOT_STARTED
**Estimated Effort**: 8 hours
**Depends On**: Phase 16 (Sanitizer), Phase 17 (Data Flow), Phase 18 (Learning)
**Unblocks**: Phase 19 (ML Probability needs populated PatternDB), Phase 21 (Taint-guided fuzzing needs SSA taint)

---

## A1. What is being built?

Wire all three completed phases into the actual execution pipeline. Currently each phase's algorithms are built and tested in isolation but never called during a real run. This phase connects them: CPG uses SSA taint instead of substring matching, agent passes language metadata to sandbox for sanitizer image selection, orchestrator pushes findings to PatternDB after each run, and assessment reads hotspots + patterns before scout agent spawns.

---

## A2. Which specific gaps does it fill?

| # | Gap | Phase | Current State | Target |
|---|-----|-------|---------------|--------|
| G1 | CPG taint uses old `bfs_taint` | 17 | SSA engine built, CPG still calls substring-based `bfs_taint` in `find_taint_paths()` | `find_taint_paths()` calls `propagate_taint_peak()` |
| G2 | Orchestrator doesn't push to PatternDB | 18 | `BugPatternDB` built, orchestrator `run()` never calls `store_finding()` | After each confirmed finding → stored in PatternDB |
| G3 | Assessment doesn't read learning data | 18 | `HotspotTracker` built, scout agent has no prioritization | Scout receives top-N hotspot files + pattern matches |
| G4 | Agent doesn't pass language to sandbox | 16 | Sanitizer `select_image()` awaits `BGSWARM_LANGUAGE`, agent never sets it | Agent tool `exec_sandbox` passes language from file extension |
| G5 | ChromaDB unavailable | 18 | Falls back to in-memory. Patterns lost on restart. | Install chromadb in venv. Patterns persist across sessions. |

---

## A3. What is the success criteria?

| Metric | Target | Measurement |
|--------|--------|-------------|
| G1: SSA taint in use | `propagate_taint_peak()` called during indexing | Grep `find_taint_paths` → calls `propagate_taint_peak` |
| G1: Taint path count increase | ≥3x more paths on extreme-bugs repo | Compare `cpg stats` before/after wiring |
| G2: Patterns stored after run | ≥1 pattern per verified finding | Check PatternDB.count() after agent run with 3+ verified bugs |
| G3: Scout investigates hotspots first | First 3 agent turns target top-3 hotspot files | Log analysis of agent investigation order |
| G4: Language propagated | `BGSWARM_LANGUAGE` present in sandbox env vars | Check `env_vars` in ExecutionReceipt |
| G5: Patterns survive restart | Query PatternDB after process restart → patterns exist | Restart test |
| Regression | All existing gate tests still pass | Phase 4 (12), Phase 6 (10), Phase 16 (22), Phase 17 (33), Phase 18 (8) |

---

## A4. What is the priority and why?

**Blocking everything above it.** Phase 19 (ML) needs Phase 18's PatternDB to have data to train on. Phase 21 (taint-guided fuzzing) needs Phase 17's SSA taint for danger maps. None of the upper invincibility stack can proceed until the built algorithms are actually wired into the pipeline.

---

## A5. What is NOT being built?

- NOT new algorithms (all algorithms already built in 16/17/18)
- NOT refactoring existing modules (only wiring calls)
- NOT new tests for the algorithms themselves (already tested in their phases)
- NOT ChromaDB server deployment (local embedded mode only)
- NOT changing the SSA or taint propagation logic (already peak)

---

## B1. Integration point?

### G1: CPG taint wiring
**File**: `bugswarm-cpg/src/graph.rs:317-327` — `find_taint_paths()` method
**Change**: Replace body with call to `crate::taint::propagate_taint_peak(self)`
**Backward compat**: Return type `Vec<TaintPath>` vs `Vec<SsaTaintPath>` — add conversion or change return type

### G2: Orchestrator → PatternDB
**File**: `bugswarm-swarm/src/swarm/orchestrator.rs:442-486` — `run()` method
**Change**: After loop completes, iterate confirmed findings, call `pattern_db.store_finding()` for each
**New import**: `from swarm.pattern_db import BugPatternDB, HotspotTracker, AgentHistory`

### G3: Assessment → Learning data
**File**: `bugswarm-agent/src/agent/cli/wiring.py` — `wire_everything()` function
**Change**: Before creating IEPEngine, query hotspots and patterns. Inject into initial system prompt.
**New import**: `from swarm.pattern_db import BugPatternDB, HotspotTracker`

### G4: Agent → Sandbox language
**File**: `bugswarm-agent/src/agent/core.py:183-231` — `_exec_sandbox()` method (original core.py) or `bugswarm-agent/src/agent/cli/wiring.py` — `_exec_sandbox_async` handler
**Change**: Detect language from file extension, pass as `BGSWARM_LANGUAGE` in env vars

### G5: ChromaDB installation
**File**: `bugswarm-swarm/pyproject.toml`
**Change**: Add `chromadb>=0.5.0` to dependencies
**File**: `bugswarm-swarm/src/swarm/learning/store.py`
**Change**: Remove `persist_path` expansion bug (currently expands `~` in `_init_store` but `PatternStore.__init__` receives already-expanded path from `BugPatternDB`)

---

## B2. Data flow (after wiring)?

```
bugswarm run ./repo
  │
  ├─→ G3: Assessment reads HotspotTracker.get_top(20) + PatternDB.query_similar()
  │     └─ Top-20 hotspot files + top-10 pattern matches injected into scout prompt
  │
  ├─→ G1: CPG indexes repo. find_taint_paths() calls propagate_taint_peak()
  │     └─ SSA-based taint paths with multiplicative confidence returned
  │
  ├─→ G4: Agent calls exec_sandbox(poc_code). Language detected.
  │     └─ BGSWARM_LANGUAGE=python passed to sandbox → select_image("python") → ASAN image
  │
  ├─→ Agent investigates. Finds bugs. Sandbox confirms.
  │
  └─→ G2: Orchestrator.run() completes. For each verified finding:
        ├─ pattern_db.store_finding(finding, code_snippet, language)
        ├─ hotspot_tracker.record_bug(file_path, severity)
        └─ agent_history.record_run(agent_id, persona, model, bugs, fp, tokens, verified)
```

---

## B3. Types modified?

### G1: Return type change

```rust
// Before:
pub fn find_taint_paths(&self) -> Vec<TaintPath>

// After:
pub fn find_taint_paths(&self) -> Vec<crate::taint::SsaTaintPath>
```

`SsaTaintPath` has the same fields as `TaintPath` (source, sink, path, sanitized, sanitizer, length) plus `confidence` and `hops`. The `stats()` method that calls `.len()` on the result is unaffected.

### G4: Env var addition

```python
# exec_sandbox tool handler passes:
env = {"BGSWARM_LANGUAGE": detect_language_from_poc(poc_code)}
```

---

## B4. Modified modules?

| File | Gap | Change |
|------|-----|--------|
| `bugswarm-cpg/src/graph.rs:317` | G1 | `find_taint_paths()` → calls `propagate_taint_peak()` |
| `bugswarm-cpg/src/graph.rs:413` | G1 | `stats()` → handles new return type |
| `bugswarm-swarm/src/swarm/orchestrator.py:442` | G2 | After `run()`: iterate findings → `pattern_db.store_finding()` |
| `bugswarm-agent/src/agent/cli/wiring.py:18` | G3 | Before engine creation: read hotspots + patterns, inject into prompt |
| `bugswarm-agent/src/agent/cli/wiring.py:120` | G4 | `_exec_sandbox_async`: pass `BGSWARM_LANGUAGE` env var |
| `bugswarm-swarm/pyproject.toml` | G5 | Add `chromadb>=0.5.0` dependency |
| `bugswarm-swarm/src/swarm/learning/store.py:35` | G5 | Fix path expansion (already expanded by BugPatternDB) |

---

## B5. New dependencies?

None. All dependencies already exist. ChromaDB added to pyproject.toml but already imported as optional.

---

## C1. Core algorithm? (wiring, not new logic)

### G1: Replace taint call
```
// graph.rs line 317
pub fn find_taint_paths(&self) -> Vec<SsaTaintPath> {
    crate::taint::propagate_taint_peak(self)
}
```

### G2: Post-run storage
```
// orchestrator.rs after run() completes
for finding in all_findings:
    if finding.get("verified"):
        snippet = read_file(finding["location"], radius=25)
        lang = detect_language(finding["location"])
        pattern_db.store_finding(finding, snippet, lang)
        hotspot_tracker.record_bug(extract_file(finding["location"]), finding["severity"])
```

### G3: Pre-run prioritization
```
// wiring.py before engine creation
hotspots = hotspot_tracker.get_top(20)
patterns = pattern_db.query_similar(repo_summary, top_k=10)
scout_prompt += f"\nHotspot files (investigate first): {hotspots}"
scout_prompt += f"\nSimilar past bugs: {patterns}"
```

---

## C2. Failure modes?

| Gap | Failure | Handling |
|-----|---------|----------|
| G1 | `propagate_taint_peak` panics | Wrapped in `std::panic::catch_unwind`. Falls back to old `bfs_taint`. |
| G2 | PatternDB store fails (disk full) | Log ERROR. Continue. Finding is still reported. |
| G3 | Hotspot file corrupted | `HotspotTracker._load()` already handles. Resets to empty. |
| G4 | Language detection fails | Default to "python". Most PoCs are Python. |
| G5 | ChromaDB unavailable after install | Falls back to in-memory (already implemented). |

---

## C3. Edge cases?

| Edge Case | Behavior |
|-----------|----------|
| G1: Empty CPG (no functions) | `propagate_taint_peak` returns empty Vec. No crash. |
| G2: Run with 0 findings | PatternDB not called. Normal. |
| G3: First run (empty DB, no hotspots) | Scout receives empty lists. Falls through to uniform investigation. |
| G4: PoC in unsupported language (Go, Java) | Default to "python". Sanitizer falls back to vanilla image. |
| G5: ChromaDB path doesn't exist | `os.makedirs` called in store init. Created automatically. |

---

## C4. Concurrency?

G2: PatternDB writes happen after swarm terminates (single-threaded). No concurrency concern.
G3: PatternDB reads happen before agent spawns (single-threaded). No concurrency concern.
G1: CPG indexing is single-threaded. No concurrency concern.

---

## C5. Performance budget?

| Metric | Target | Measurement |
|--------|--------|-------------|
| G1: SSA taint overhead | <2x current taint time | Already measured in Phase 17 |
| G2: PatternDB store | <500ms for 10 findings | Batch insert |
| G3: Hotspot + pattern read | <200ms | YAML load + ChromaDB query |
| G4: Language detection | <1ms | File extension check |
| G5: ChromaDB startup | <1s | PersistentClient init |

---

## C6. Algorithmic Peak Analysis

No new algorithms. This phase is pure wiring. All algorithms are already at peak from Phases 16, 17, 18.

### C6.1 Algorithm Inventory

| # | Component | Approach | Status |
|---|-----------|----------|--------|
| 1 | Taint wiring | Delegate to `propagate_taint_peak` | Wiring — no algorithm change |
| 2 | Pattern storage | Delegate to `BugPatternDB.store_finding` | Wiring |
| 3 | Learning read | Delegate to `HotspotTracker.get_top` + `PatternDB.query_similar` | Wiring |
| 4 | Language detection | File extension lookup | Wiring |
| 5 | ChromaDB install | pip dependency | Infrastructure |

### C6.3 Zero-Gap Guarantee

```
Component: Taint — [x] Wiring only. Algorithm already peak (Phase 17).
Component: PatternDB — [x] Wiring only. Algorithm already peak (Phase 18).
Component: Hotspot — [x] Wiring only. Algorithm already peak (Phase 18).
Component: Language — [x] Wiring only. Trivial lookup.
Component: ChromaDB — [x] Infrastructure. Package install.
```

---

## D1. Aggressive unit tests? (8 tests, 5 aggressive)

| # | Test | Attack | Expected |
|---|------|-------|----------|
| 1 | `test_g1_taint_wired` | CPG.index → find_taint_paths() → SSA paths returned | Paths have `confidence` field (proof SSA was called) |
| 2 | `test_g1_fallback_on_panic` | **AGGRESSIVE**: Corrupt SSA state → panic | Falls back to old bfs_taint. No crash. |
| 3 | `test_g2_patterns_stored` | Run agent → 3 verified → check PatternDB | DB has 3+ patterns |
| 4 | `test_g3_scout_prioritized` | Hotspots set → run scout → check first turns | First 3 turns target hotspot files |
| 5 | `test_g4_language_passed` | **AGGRESSIVE**: Agent runs PoC → check receipt env_vars | `BGSWARM_LANGUAGE` present in receipt |
| 6 | `test_g5_persistence` | **AGGRESSIVE**: Store patterns → restart Python process → query | Patterns survive restart (with ChromaDB installed) |
| 7 | `test_regression_all_gates` | **AGGRESSIVE**: Run all 5 phase gate suites | All pass (85 total tests) |
| 8 | `test_g2_no_findings` | **AGGRESSIVE**: Run with 0 findings | PatternDB not called. No crash. |

---

## D2. Aggressive integration tests? (3 tests)

| # | Test | Attack | Expected |
|---|------|-------|----------|
| 1 | `test_full_wired_pipeline` | CPG → SSA taint → agent → sandbox → patternDB → hotspots | All 5 gaps closed. Full loop works. |
| 2 | `test_learning_accumulation` | **AGGRESSIVE**: Run 3 times on same repo. Check PatternDB grows. | Each run adds patterns. No duplicates. |
| 3 | `test_cross_phase_regression` | **AGGRESSIVE**: Run all gate suites after wiring | Phase 4,6,16,17,18 gates all pass. |

---

## D3. Extreme gate test — The Integration Crucible? (6 attack vectors)

1. **G1 verification**: Index extreme-bugs with SSA. Assert taint paths ≥3x baseline (measure before/after wiring).
2. **G2 verification**: Run agent against repo with known bugs. After run, PatternDB.count() ≥ number of verified findings.
3. **G3 verification**: Set hotspot on auth.py=100. Run scout. First agent turn MUST mention auth.py.
4. **G4 verification**: Submit Python PoC. Receipt env_vars contains `BGSWARM_LANGUAGE=python`.
5. **G5 verification**: Store 10 patterns. Kill process. Restart. Query. All 10 exist.
6. **Regression**: 85 existing gate tests still pass.

---

## E1. Estimated cost?

| Cost | Estimate |
|------|----------|
| Development | 8 hours |
| G1 wiring | 1 hour |
| G2 wiring | 2 hours |
| G3 wiring | 2 hours |
| G4 wiring | 30 min |
| G5 wiring | 30 min |
| Testing | 2 hours |

---

## Gate Receipt

```json
{"phase":"18.5","gate":"integration_crucible","attack_vectors":6,"passed":0,"failed":0,"verdict":"PHASE 18.5 NOT YET EXECUTED"}
```
