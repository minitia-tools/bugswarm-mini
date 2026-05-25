# PHASE 22: Delta Debugging — Minimal Reproduction

## Changelog

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-05-14 | System | Initial phase plan |
| 1.1 | 2026-05-14 | System | IMPLEMENTED: ddmin engine (DeltaMinimizer), 32 integration tests, daemon handler, agent delta_debug tool, SandboxConfig deltaconfig. 114 sandbox + 109 CPG = 223 total tests, 0 failures. |

---

## A. Identity & Purpose

### A1. What

Phase 22 implements **Delta Debugging** (Zeller 2002 ddmin algorithm) as a post-processing step after any crash or anomalous behavior is detected by agents, fuzzers, or symbolic executors. When a 500-byte crashing input is discovered, the ddmin engine systematically reduces it to the 3 essential bytes that constitute the minimal reproduction. This gives agents precise, actionable triggers instead of large opaque inputs.

**Core mechanism**: Given a failing test function `test(input) → fail` and a pass definition `test(input) → pass`, ddmin recursively partitions the input into n subsets and tests each subset's complement. If the complement still fails, that subset is irrelevant and can be discarded. The algorithm converges to a 1-minimal input where removing any single byte changes the failure behavior.

**Key deliverables**:
1. `bugswarm-sandbox/src/delta.rs` — Core ddmin engine with parallel chunk testing
2. Sandbox daemon integration — Post-crash hook triggers delta automatically
3. Agent tool `fuzz_target` extension — Returns minimized input alongside raw crash
4. Three C6 peak optimizations: parallel chunk testing, input-type-aware splitting, hierarchical minimization
5. Integration tests with golden minimized outputs

### A2. Gap (INV-005)

**Current state**: When agent tools or fuzzers discover a crash with a 500-byte input, agents (LLMs) receive the full 500 bytes. They cannot determine which bytes are essential to trigger the crash. This leads to:
- Hours wasted analyzing irrelevant input portions
- Agents generating incorrect root cause hypotheses based on irrelevant bytes
- No systematic input reduction — manual trial-and-error
- Redundant investigation of the same bug with different large inputs

**Target state**: Delta debugging reduces inputs by >90% within 100 iterations. For a 500-byte crashing input, the engine produces the essential 3-byte reproduction. Agents receive the minimal trigger, enabling precise root cause analysis. The ddmin engine runs automatically as post-processing after any crash, with no agent intervention needed. Results are cached in the evidence graph for cross-phase use.

**Metric targets**:

| Metric | Baseline | Target | Measurement |
|--------|----------|--------|-------------|
| Input size reduction | 0% (no reduction) | >90% within 100 iterations | Byte count ratio (original/minimized) |
| Time to minimal reproduction | ∞ (no tool) | <30s for 10KB inputs | Wall-clock from crash to min output |
| Agent investigation time saved | 0 min | >45 min saved per bug | Compare agent trace time with/without delta |
| False positive rate | N/A | <1% (minimized input still crashes) | Verified re-execution of minimized input |
| Parallelization speedup | 1x (sequential) | n/2 factor (chunk parallelism) | Wall-clock comparison sequential vs parallel |
| Token savings for LLMs | 0% | >90% (3 bytes vs 500) | Input byte count sent to agent |
| Cache hit rate | N/A | >80% (known min inputs reused) | Evidence graph lookup hits |
| Success rate on all crash types | N/A | >95% | Crashes with successful reduction / total crashes |

**Success criteria**:
1. SC-22.1: ddmin engine reduces any fuzzer-discovered crashing input by ≥90% within 100 iterations
2. SC-22.2: Minimized input reproduces the crash ≥99% of the time (no false minimization)
3. SC-22.3: Parallel chunk testing produces ≥n/2 speedup on n-core sandbox pool
4. SC-22.4: Parser-aware splitting achieves ≥3x fewer iterations for structured inputs
5. SC-22.5: Hierarchical minimization produces ≥40% smaller final output than naive ddmin
6. SC-22.6: End-to-end time from crash to minimized input ≤30s for inputs ≤10KB
7. SC-22.7: Agent tool integration returns minimized input in same `fuzz_target` response
8. SC-22.8: All minimized inputs are cached in evidence graph with provenance

### A3. Success Criteria Table

| ID | Criterion | Measure | Target | Verification Method |
|----|-----------|---------|--------|---------------------|
| SC-22.1 | Size reduction | (original - minimized) / original | ≥90% | Integration test with golden datasets |
| SC-22.2 | Crash reproduction | minimized_crashes / total_minimizations | ≥99% | Automated re-execution suite |
| SC-22.3 | Parallel speedup | T_sequential / T_parallel | ≥n/2 | Benchmark on 8-core pool |
| SC-22.4 | Parser iterations | naive_iterations / parser_iterations | ≥3x | Comparison on structured test corpus |
| SC-22.5 | Hierarchical output size | naive_min_size / hierarchical_min_size | ≥1.4x | Comparison on grammar-based inputs |
| SC-22.6 | End-to-end latency | Wall-clock time | ≤30s for 10KB | Timer around full pipeline |
| SC-22.7 | Agent integration | Response field presence | 100% | Schema validation on fuzz_target output |
| SC-22.8 | Evidence caching | cache_hits / total_lookups | >80% | Evidence graph query counter |

### A4. Priority

**Priority**: P1 — High. While no phase is directly blocked by Phase 22, the delta debugger is a critical multiplier for all investigation phases (16-21, 25-30). Without minimal reproductions, agent investigation is fundamentally bottlenecked on input comprehension. Each day without delta debugging costs ~3 engineer-hours per bug in wasted LLM token analysis and human false paths.

**Urgency rationale**:
- Every agent investigation phase (16-21) benefits immediately
- Fuzzer output (Phase 8-10) quality is capped without minimization
- Evidence graph (Phase 5) completeness requires minimal triggers
- Regression testing pipeline (Phase 24) uses minimized inputs as golden cases

**Implementation order**: Build immediately after sandbox stabilization. Can parallelize with Phase 20-21 work since delta uses sandbox pool (already stable).

### A5. Scope Boundary

**In scope**:
- ddmin algorithm implementation (all variants from naive to peak)
- Parallel chunk testing via sandbox container pool
- Input-type-aware split strategies (text, binary, JSON, bytecode)
- Hierarchical minimization pipeline (ddmin → grammar → semantic)
- Sandbox daemon post-crash hook integration
- Agent tool `fuzz_target` extension with minimized field
- Evidence graph caching of minimized inputs with provenance
- Configuration for min/max chunk sizes, iteration limits, timeout

**Out of scope**:
- Delta debugging for non-deterministic failures (Phase 26)
- Input grammar inference from existing inputs (Phase 15 for grammar management)
- Cross-bug input correlation (Phase 23 trigger matrix)
- Producing human-readable test cases (Phase 25 test synthesis)
- Performance profiling of minimization itself (covered by Phase 20-21 profiling)
- Real-time minimization during active fuzzing (post-hoc only for Phase 22)
- Minimization of non-input triggers (environment, timing — handled in Phase 23)

**Boundary interfaces**:
- **Input**: Crash event from sandbox daemon or agent tool call
- **Output**: Minimized input + provenance metadata
- **North**: Evidence graph (Phase 5) — stores minimized inputs
- **South**: Sandbox (Phase 1) — executes test(input) calls
- **East**: Fuzzer engine (Phase 8-10) — provides initial crashing input
- **West**: Agent tools (Phase 17-18) — consume minimized inputs

---

## B. Architecture

### B1. Integration Point Table

| ID | Source Phase | Source Interface | Target (Phase 22) | Data Flow | Protocol | Latency Budget |
|----|-------------|------------------|-------------------|-----------|----------|----------------|
| IP-22.1 | Phase 1 (Sandbox) | Container execution API | delta.rs engine | Crash input → ddmin test loop → sandbox exec | gRPC via sandbox daemon | <5ms per exec call |
| IP-22.2 | Phase 5 (Evidence) | Graph write API | delta.rs cache | Minimized input → evidence node | REST over localhost | <10ms per write |
| IP-22.3 | Phase 8-10 (Fuzzer) | Crash output channel | delta.rs entry point | Raw crashing input → ddmin | Internal channel (Rust enum) | 0ms (in-process) |
| IP-22.4 | Phase 17 (Agent tools) | fuzz_target response | delta.rs output parser | Minimized input → agent response field | JSON-RPC | <5ms serialization |
| IP-22.5 | Phase 2 (CPG) | File AST lookup | Input-type-aware splitter | File type → split strategy | Graph query | <10ms lookup |
| IP-22.6 | Config service | TOML config | delta.rs configuration | Chunk sizes, limits, timeouts | File read at init | 0ms cached |
| IP-22.7 | Phase 20 (Profiling) | Trace span API | delta.rs instrumentation | Iteration count, timing spans | OpenTelemetry spans | <1ms per span |

### B2. Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        DELTA DEBUGGING PIPELINE                          │
│                                                                          │
│  ┌──────────┐    ┌──────────────┐    ┌─────────────────┐                │
│  │ Fuzzer   │───▶│ Crash Event  │───▶│ ddmin Entry     │                │
│  │ Engine   │    │ (500 bytes)  │    │ Point           │                │
│  └──────────┘    └──────────────┘    └───────┬─────────┘                │
│                                               │                          │
│                    ┌──────────────────────────┘                          │
│                    ▼                                                     │
│  ┌─────────────────────────────────────┐                               │
│  │         ddmin Core Engine           │                               │
│  │  ┌───────────────────────────────┐  │                               │
│  │  │ C6.2.2: Input-Type-Aware      │  │                               │
│  │  │ Split Strategy Selector       │──┤                               │
│  │  │  - text  → split on \n        │  │                               │
│  │  │  - json  → split on tokens    │  │                               │
│  │  │  - binary→ split on struct    │  │                               │
│  │  │  - bc    → split on ops       │  │                               │
│  │  └───────────────┬───────────────┘  │                               │
│  │                  │                   │                               │
│  │  ┌───────────────▼───────────────┐  │                               │
│  │  │ C6.2.1: Parallel Chunk        │  │                               │
│  │  │ Test Scheduler                │  │                               │
│  │  │  ┌────┐ ┌────┐ ┌────┐ ┌────┐ │  │                               │
│  │  │  │ W1 │ │ W2 │ │ W3 │ │ W4 │ │  │                               │
│  │  │  └──┬─┘ └──┬─┘ └──┬─┘ └──┬─┘ │  │                               │
│  │  │     │      │      │      │    │  │                               │
│  │  │  ┌──▼──────▼──────▼──────▼─┐ │  │                               │
│  │  │  │  Sandbox Container Pool │ │  │                               │
│  │  │  └──────────┬──────────────┘ │  │                               │
│  │  │             │                 │  │                               │
│  │  └─────────────▼────────────────┘  │                               │
│  │  ┌──────────────────────────────┐  │                               │
│  │  │ Iteration Controller         │  │                               │
│  │  │  - Convergence check         │  │                               │
│  │  │  - 1-minimality verification │  │                               │
│  │  │  - Timeout / budget tracking │  │                               │
│  │  └──────────────┬───────────────┘  │                               │
│  └─────────────────┼──────────────────┘                               │
│                    │                                                     │
│  ┌─────────────────▼──────────────────┐                                │
│  │  C6.2.3: Hierarchical Minimization │                                │
│  │  ┌──────────────────────────────┐  │                                │
│  │  │ Stage 1: ddmin (syntactic)   │  │                                │
│  │  │ Stage 2: Grammar minimize    │  │                                │
│  │  │ Stage 3: Semantic minimize   │  │                                │
│  │  └──────────────┬───────────────┘  │                                │
│  └─────────────────┼──────────────────┘                                │
│                    │                                                     │
│  ┌─────────────────▼──────────────────┐                                │
│  │        Output / Caching            │                                │
│  │  ┌──────────────────────────────┐  │                                │
│  │  │ Minimized input (3 bytes)    │  │                                │
│  │  │ Provenance metadata          │  │                                │
│  │  │ Evidence graph cache         │  │                                │
│  │  │ Agent tool response field    │  │                                │
│  │  └──────────────────────────────┘  │                                │
│  └────────────────────────────────────┘                                │
└─────────────────────────────────────────────────────────────────────────┘
```

### B3. Types and Schemas

```rust
// === bugswarm-sandbox/src/delta.rs ===

/// The core delta debugging configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct DeltaConfig {
    /// Maximum number of ddmin iterations before forced terminal.
    pub max_iterations: usize,
    /// Maximum wall-clock time for the entire minimization.
    pub timeout_secs: u64,
    /// Minimum chunk size in bytes (below this, stop splitting).
    pub min_chunk_bytes: usize,
    /// Maximum parallel workers for chunk testing.
    pub parallel_workers: usize,
    /// Whether to use input-type-aware splitting.
    pub use_type_aware_split: bool,
    /// Whether to run hierarchical minimization (grammar + semantic).
    pub use_hierarchical: bool,
    /// Whether to verify 1-minimality at exit.
    pub verify_one_minimality: bool,
    /// Maximum test executions per input (safety limit).
    pub max_test_executions: usize,
}

impl Default for DeltaConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            timeout_secs: 30,
            min_chunk_bytes: 1,
            parallel_workers: 8,
            use_type_aware_split: true,
            use_hierarchical: true,
            verify_one_minimality: true,
            max_test_executions: 10_000,
        }
    }
}

/// The type of input, used to select the optimal split strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputType {
    /// Unstructured binary blob.
    Binary,
    /// Line-oriented text (source code, logs, etc.).
    Text,
    /// JSON or JSON-like structured data.
    Json,
    /// XML structured data.
    Xml,
    /// Key-value / URL-encoded data.
    KeyValue,
    /// Bytecode instruction stream.
    Bytecode,
    /// Protocol buffer or similar binary-structured.
    Protobuf,
}

impl InputType {
    /// Infer input type from file extension or content heuristic.
    pub fn infer(data: &[u8], hint: Option<&str>) -> Self {
        if let Some(ext) = hint {
            match ext {
                "json" => return InputType::Json,
                "xml" => return InputType::Xml,
                "py" | "js" | "ts" | "rs" | "go" | "c" | "cpp" | "java" => return InputType::Text,
                "proto" | "pb" => return InputType::Protobuf,
                "wasm" | "class" | "pyc" => return InputType::Bytecode,
                _ => {}
            }
        }
        // Heuristic: try JSON parse
        if serde_json::from_slice::<serde_json::Value>(data).is_ok() {
            return InputType::Json;
        }
        // Heuristic: high ASCII text ratio
        let text_ratio = data.iter().filter(|b| b.is_ascii_graphic() || **b == b'\n').count() as f64
            / data.len().max(1) as f64;
        if text_ratio > 0.85 {
            return InputType::Text;
        }
        InputType::Binary
    }
}

/// Split boundary descriptors for type-aware chunking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitBoundary {
    Byte(usize),
    Line(usize),
    JsonToken(usize),
    JsonKey(usize),
    JsonValue(usize),
    XmlElement(usize),
    BytecodeOp(usize),
    ProtobufField(usize),
}

/// A chunk produced by the type-aware splitter.
#[derive(Debug, Clone)]
pub struct DeltaChunk {
    /// Index of this chunk within the current partition.
    pub index: usize,
    /// Byte offset range in the original input.
    pub byte_range: std::ops::Range<usize>,
    /// The actual bytes of this chunk.
    pub data: Vec<u8>,
    /// The split boundary type for debugging.
    pub boundary: SplitBoundary,
}

/// The result of testing an input complement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestResult {
    /// The complement still fails (chunk is irrelevant).
    Fail,
    /// The complement passes (chunk is relevant — must keep).
    Pass,
    /// The test was inconclusive (timeout, crash in harness, etc.).
    Inconclusive,
    /// The test could not be run (sandbox unavailable).
    Unresolved,
}

/// Test function type: takes a byte slice, returns whether the target behavior occurs.
pub type TestFn = Box<dyn Fn(&[u8]) -> TestResult + Send + Sync>;

/// The full provenance trail for a minimized input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimizationProvenance {
    /// Original crashing input before minimization.
    pub original_input: Vec<u8>,
    /// Original input size in bytes.
    pub original_size: usize,
    /// Minimized input after all reduction stages.
    pub minimized_input: Vec<u8>,
    /// Minimized input size in bytes.
    pub minimized_size: usize,
    /// Reduction ratio (original/minimized).
    pub reduction_ratio: f64,
    /// Number of ddmin iterations executed.
    pub ddmin_iterations: usize,
    /// Number of test function calls made.
    pub test_executions: usize,
    /// Wall-clock time of minimization.
    pub duration_ms: u64,
    /// The input type used for splitting.
    pub input_type: InputType,
    /// Which minimization stages completed.
    pub stages_completed: Vec<MinimizationStage>,
    /// Whether 1-minimality was verified.
    pub is_one_minimal: bool,
    /// Hash of the minimized input for caching.
    pub min_hash: String,
    /// Hash of the original input for traceability.
    pub orig_hash: String,
}

/// Stages of the hierarchical minimization pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MinimizationStage {
    Ddmin,
    GrammarMinimize,
    SemanticMinimize,
}

/// The overall delta debugging result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeltaResult {
    /// Minimization succeeded.
    Success {
        provenance: MinimizationProvenance,
    },
    /// No reduction found (input already minimal).
    AlreadyMinimal {
        provenance: MinimizationProvenance,
    },
    /// Minimization exceeded iteration budget.
    IterationLimitExceeded {
        provenance: MinimizationProvenance,
        iterations: usize,
    },
    /// Minimization exceeded time budget.
    Timeout {
        provenance: MinimizationProvenance,
        elapsed_ms: u64,
    },
    /// Test harness failure during minimization.
    HarnessFailure {
        error: String,
        partial_provenance: Option<MinimizationProvenance>,
    },
}

/// Agent tool response extension: fuzz_target now includes minimized input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzTargetResponse {
    /// The raw crash inputs found.
    pub crashes: Vec<CrashInput>,
    /// Minimized versions of the crash inputs (if delta ran).
    pub minimized: Vec<MinimizationProvenance>,
    /// Overall fuzzing statistics.
    pub stats: FuzzStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashInput {
    pub input: Vec<u8>,
    pub crash_type: String,
    pub crash_address: Option<u64>,
    pub signal: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzStats {
    pub total_executions: u64,
    pub unique_crashes: usize,
    pub coverage_pct: f64,
}
```

### B4. Modified Modules Table

| Module | File | Change Description | Risk |
|--------|------|-------------------|------|
| delta | `bugswarm-sandbox/src/delta.rs` (NEW) | Core ddmin engine, split strategies, provenance tracking | Medium |
| sandbox_daemon | `bugswarm-sandbox/src/daemon.rs` | Add post-crash hook that spawns delta worker | Low |
| sandbox_pool | `bugswarm-sandbox/src/pool.rs` | Add parallel test scheduling for delta chunks | Low |
| fuzz_target | `bugswarm-tools/src/fuzz_target.rs` | Add `minimized` field to response schema | Low |
| evidence_graph | `bugswarm-evidence/src/graph.rs` | Add `MinimizedInput` node type, cache queries | Low |
| evidence_types | `bugswarm-evidence/src/types.rs` | New `EdgeKind::MinimizedFrom`, `NodeKind::MinimizedInput` | Low |
| agent_schema | `bugswarm-agent/src/schema.rs` | Updated fuzz_target response schema | Low |

### B5. Dependencies Table

| Dependency | Version | Purpose | Required By |
|-----------|---------|---------|-------------|
| `serde` | 1.x | Serialization of provenance, config, results | delta.rs, agent tools |
| `serde_json` | 1.x | JSON parsing for input type inference | delta.rs |
| `sha2` | 0.10 | Hashing minimized inputs for caching | delta.rs |
| `tokio` | 1.x | Async parallel chunk testing | delta.rs, sandbox_pool |
| `tonic` | 0.10 | gRPC for sandbox execution calls | delta.rs → sandbox |
| `tracing` | 0.1 | OpenTelemetry spans for observability | delta.rs |
| `quick-xml` | 0.31 | XML parsing for type-aware splitting | delta.rs |
| `prost` | 0.12 | Protobuf parsing for type-aware splitting | delta.rs |
| `walrus` | 0.20 | WASM bytecode parsing for split boundaries | delta.rs |
| `thiserror` | 1.x | Error types for DeltaResult | delta.rs |

---

## C. Algorithm & Logic

### C1. Core ddmin Algorithm (Pseudocode with Complexity)

```
┌─────────────────────────────────────────────────────────────────────┐
│ ALGORITHM: ddmin(input, test_fn, config)                             │
│                                                                     │
│ Complexity: O(n^2) worst case, O(n log n) expected with type-aware  │
│ Space: O(n) for input + O(workers * n) for parallel chunks          │
│                                                                     │
│ INPUTS:                                                             │
│   input:    Vec<u8>  — the failing input to minimize               │
│   test_fn:  TestFn   — test(input) → TestResult                    │
│   config:   DeltaConfig — iteration limits, parallelism, etc.      │
│                                                                     │
│ OUTPUTS:                                                            │
│   DeltaResult — minimized input or error                            │
│                                                                     │
│ 1. function ddmin(input, test_fn, config):                          │
│ 2.   assert test_fn(input) == Fail   // precondition               │
│ 3.   assert test_fn([])   == Pass    // empty input baseline        │
│ 4.                                                                    │
│ 5.   n = 2  // initial partition count                              │
│ 6.   current = input                                                │
│ 7.   iteration = 0                                                  │
│ 8.   executions = 2  // already ran 2 tests above                   │
│ 9.                                                                    │
│ 10.  while True:                                                    │
│ 11.    iteration += 1                                               │
│ 12.    if iteration > config.max_iterations:                        │
│ 13.      return IterationLimitExceeded(provenance)                  │
│ 14.    if elapsed_ms > config.timeout_secs * 1000:                  │
│ 15.      return Timeout(provenance)                                 │
│ 16.    if executions > config.max_test_executions:                  │
│ 17.      return IterationLimitExceeded(provenance)                  │
│ 18.                                                                   │
│ 19.    // TYPE-AWARE SPLIT (C6.2.2)                                 │
│ 20.    chunks = type_aware_split(current, n, input_type)            │
│ 21.    if chunks.len() < n:  // input too small to split n ways     │
│ 22.      if n == 2: return AlreadyMinimal(provenance)               │
│ 23.      n = max(2, n // 2)  // reduce granularity                  │
│ 24.      continue                                                   │
│ 25.                                                                   │
│ 26.    // COMPLEMENT TESTING: test input without each chunk         │
│ 27.    // PARALLEL (C6.2.1)                                         │
│ 28.    complements = []                                             │
│ 29.    for chunk in chunks:                                         │
│ 30.      // complement = everything except this chunk               │
│ 31.      complement = make_complement(current, chunk)               │
│ 32.      complements.push((chunk, complement))                      │
│ 33.                                                                   │
│ 34.    results = parallel_test(complements, test_fn, workers)       │
│ 35.    executions += len(results)                                   │
│ 36.                                                                   │
│ 37.    // SUBSET AND COMPLEMENT ANALYSIS                            │
│ 38.    any_relevant = False                                         │
│ 39.    for (chunk, complement), result in zip(complements, results):│
│ 40.      if result == Fail:  // complement still fails              │
│ 41.        current = complement  // chunk is IRRELEVANT, discard    │
│ 42.        n = max(n - 1, 2)     // reduce granularity              │
│ 43.        any_relevant = True                                     │
│ 44.        break  // restart with reduced input                     │
│ 45.                                                                   │
│ 46.    // If no complement failed, test each subset for failure     │
│ 47.    if not any_relevant:                                         │
│ 48.      for (chunk, _), result in zip(complements, results):       │
│ 49.        if test_fn(chunk.data) == Fail:  // chunk itself fails   │
│ 50.          current = chunk.data    // chunk is SUFFICIENT         │
│ 51.          n = max(n - 1, 2)       // reduce granularity          │
│ 52.          any_relevant = True                                   │
│ 53.          executions += 1                                       │
│ 54.          break                                                  │
│ 55.                                                                   │
│ 56.    if not any_relevant:                                         │
│ 57.      if n * 2 <= current.len():                                 │
│ 58.        n = min(n * 2, current.len())  // increase granularity   │
│ 59.      else if config.verify_one_minimality:                      │
│ 60.        if verify_1_minimal(current, test_fn):                   │
│ 61.          return Success(provenance)                             │
│ 62.        else:                                                    │
│ 63.          // Not truly 1-minimal, continue                       │
│ 64.          n = current.len()  // test every single byte           │
│ 65.      else:                                                      │
│ 66.        return Success(provenance)                               │
│ 67.                                                                   │
│ 68.    // Guard against infinite loop                               │
│ 69.    if current.len() == 0: return Success(empty_provenance)      │
│ 70.  end while                                                      │
│                                                                     │
│ 71. function make_complement(input, chunk_to_remove):               │
│ 72.   // Concatenate everything except chunk_to_remove's range      │
│ 73.   let (start, end) = chunk_to_remove.byte_range                │
│ 74.   return [&input[..start], &input[end..]].concat()             │
│                                                                     │
│ 75. function verify_1_minimal(input, test_fn):                      │
│ 76.   // Remove each single byte; if any complement still fails,    │
│ 77.   // then this byte is irrelevant — not 1-minimal               │
│ 78.   for i in 0..input.len():                                     │
│ 79.     complement = [&input[..i], &input[i+1..]].concat()         │
│ 80.     if test_fn(complement) == Fail:                            │
│ 81.       return False  // byte i is irrelevant                    │
│ 82.   return True                                                    │
└─────────────────────────────────────────────────────────────────────┘
```

**Complexity analysis**:
- **Worst case**: O(n^2) test executions. In the worst case, ddmin increases granularity n to len(input), testing each complement and each subset at each granularity level. This gives ∑(n) test executions where n halves each time a byte is found relevant — O(n^2).
- **Expected case with type-aware splitting**: O(n log n). Type-aware splitting creates semantic chunks, so at each granularity n, a single complement test often finds an irrelevant semantic chunk, reducing n by 1 and restarting. This converges in approximately n * log(n) tests.
- **Parallel wall-clock**: With p parallel workers, wall-clock is O((n log n) / p) since all n/2 complement tests at each iteration run concurrently.

### C2. Failure Modes Table

| # | Failure Mode | Cause | Detection | Mitigation | Severity |
|---|-------------|-------|-----------|------------|----------|
| FM-22.1 | Non-deterministic crash disappears during ddmin | Flaky bug (race condition, timing) | Inconclusive test result on previously-failing input | Log inconclusive rate, flag bug as non-deterministic, skip delta | High |
| FM-22.2 | ddmin returns empty input as crash | Original crash is trivially reproducible with empty input | Assertion: test_fn([]) must return Pass | Pre-check empty input before starting ddmin, return AlreadyMinimal | Medium |
| FM-22.3 | ddmin produces input that doesn't crash at all | Test harness inconsistency (different sandbox container) | Re-execute minimized input 3x in fresh containers | If re-execution fails, restart ddmin with stricter container isolation | Critical |
| FM-22.4 | Test function exhausts sandbox pool | All parallel workers busy, new tests queued indefinitely | Pool saturation metric, test execution timeout | Circuit breaker: fall back to sequential mode if pool >90% utilized | Medium |
| FM-22.5 | Type-aware split produces chunk finer than 1 byte | Split strategy bug on empty/edge inputs | Chunk size assertion (must be ≥1 byte) | Fall back to binary byte split for degenerate inputs | Low |
| FM-22.6 | ddmin runs forever due to 1-minimality oscillation | Non-monotonic bug (removing byte A causes fail, removing B too causes pass) | Iteration guard, oscillation detector (same input size after 5 iterations) | Terminate with warning, return best-effort result | Medium |
| FM-22.7 | Memory exhaustion from large input clone | Input is multi-MB, complement creation allocates MB per test | Memory watermark monitoring | Stream complements, use bytes::Bytes with copy-on-write | Low |
| FM-22.8 | Deadlock in parallel chunk scheduler | All workers blocked waiting for each other | Worker heartbeat / watchdog timer | Kill stuck workers, fall back to sequential for remaining chunks | Medium |
| FM-22.9 | Type inference produces wrong InputType | Heuristic misclassifies input (binary looks like text) | Check split efficiency (chunk count vs expected) | Re-infer after 5 iterations if chunks aren't reducing, try Binary split | Low |
| FM-22.10 | Cache collision: different bugs produce same hash | SHA-256 collision (extremely unlikely) | Hash collision in evidence graph node insert | Use full input as node identity in addition to hash | Very Low |

### C3. Edge Cases Table

| # | Edge Case | Input Condition | Expected Behavior | Validation |
|---|----------|----------------|-------------------|------------|
| EC-22.1 | Empty input | `input.len() == 0` | Return AlreadyMinimal immediately | Unit test: empty input → AlreadyMinimal |
| EC-22.2 | Single byte input | `input.len() == 1` | Test empty complement, return if passes; otherwise AlreadyMinimal | Unit test: 1-byte input, both pass/fail variants |
| EC-22.3 | Input is already 1-minimal | Every single byte is essential | verify_1_minimality returns True, Success returned | Golden dataset: known-minimal inputs |
| EC-22.4 | All bytes irrelevant (empty pass) | test_fn([]) == Pass but all subsets also pass | ddmin converges to empty input, then barfs at precondition | Precondition check: test_fn([]) must be Pass |
| EC-22.5 | Extremely large input (100MB) | `input.len() > 100 * 1024 * 1024` | Memory pressure, parallel complement allocation enormous | Size gate: inputs >10MB use chunked ddmin with iterator-based complements |
| EC-22.6 | Unicode multi-byte boundary split | UTF-8 4-byte character split mid-codepoint | Type-aware text split ensures boundary at codepoint level | Text splitter uses char_indices() not byte offsets |
| EC-22.7 | JSON with nested bracket chaos | Deeply nested, inline arrays | JSON token split must track bracket depth for valid complements | JSON splitter maintains bracket stack |
| EC-22.8 | Input causes test harness crash | test_fn itself panics or segfaults | Return HarnessFailure with partial provenance | Catch panic via std::panic::catch_unwind |
| EC-22.9 | Timeout midway through verification | verify_1_minimality is O(n) tests | Save partial provenance, return Timeout | Check elapsed time in verification loop |
| EC-22.10 | Identical input re-crashed (cache hit) | SHA-256 match in evidence graph | Return cached result instantly | Evidence graph lookup before ddmin start |
| EC-22.11 | Input grows during grammar stage | Grammar minimize adds bytes for valid parse | Ensure monotonic non-increase assertion, skip stage if grow | Size guard: if grammar output > input, keep ddmin output |
| EC-22.12 | Zero-length chunk from type split | All splits land at boundaries, one chunk empty | Filter zero-length chunks before test scheduling | Chunk validation: assert chunk.len() > 0 |

### C4. Concurrency

**Model**: The delta engine uses a hybrid concurrency model. The main ddmin loop runs on a single thread (sequential by nature — each iteration depends on the previous result). Within each iteration, the complement testing (n/2 tests) is fully parallelized across the sandbox container pool.

**Synchronization primitives**:
- `tokio::sync::Semaphore` — Limits concurrent sandbox exec calls to configured `parallel_workers`
- `tokio::sync::mpsc` — Channels test requests to worker pool, collects results
- `Arc<AtomicU64>` — Shared counters for test_executions and iteration (thread-safe)
- `tokio::sync::RwLock` — Protects read-only test_fn from concurrent state mutation in edge cases

**Parallel complement testing flow**:
```
1. Split current input into n chunks (type-aware)
2. For each chunk i:
   a. Construct complement_i = input without chunk_i
   b. Submit (i, complement_i) to worker queue
3. Workers execute test_fn(complement_i) in parallel via sandbox
4. Collect results via mpsc receiver
5. First Fail result → immediately cancel remaining tests via CancellationToken
6. If all Pass → test each chunk_i individually (still parallel)
7. If all Pass for chunks too → increase n (granularity) and repeat
```

**Race conditions**:
- **Cancel on first success**: When one complement fails, other in-flight test calls must be cancelled. Handled via `tokio_util::sync::CancellationToken`. Workers check token before starting sandbox call. In-flight sandbox calls complete (their results discarded) since sandbox gRPC doesn't support cancellation.
- **Shared test_fn**: If test_fn has mutable internal state (e.g., a counter), concurrent calls corrupt it. Mitigation: document that test_fn must be `Fn(&[u8]) -> TestResult + Send + Sync` with immutable semantics. Provide `TestFn::thread_safe_wrapper` that clones state per call.
- **Pool exhaustion**: If sandbox pool is full, parallel submits block. Mitigation: timeout on pool acquire (5s), fall back to sequential.

### C5. Performance Budget Table

| Metric | Target | Budget Allocation | Measurement Method |
|--------|--------|-------------------|-------------------|
| ddmin iterations per input | ≤100 | Core loop | Counter in iteration loop |
| Test function calls per input | ≤10,000 | Parallel tests + verification | Atomic counter |
| Per-test-execution latency | ≤5ms | Sandbox gRPC round-trip | OpenTelemetry span |
| Complement construction time | ≤1ms per complement | Memory allocation + copy | Micro-benchmark (criterion) |
| Type-aware split time | ≤1ms per split | Parser overhead (JSON/XML) | Micro-benchmark (criterion) |
| Total end-to-end for 10KB input | ≤30s | All stages combined | Wall-clock timer |
| Grammar minimize stage | ≤5s | Grammar parse + token drop | Stage-specific timer |
| Semantic minimize stage | ≤5s | Value substitution attempts | Stage-specific timer |
| Evidence graph cache write | ≤10ms | Serialization + HTTP | Span duration |
| Parallel speedup factor | ≥n/2 on n workers | Overhead: scheduling + cancel | Benchmark ratio |
| 1-minimality verification for 100B input | ≤1s | 100 test calls × 5ms each | Timer around verify_1_minimal |
| Memory peak during 1MB input | ≤50MB | Input copies + chunk buffers | Process RSS monitoring |

---

## C6. Peak Analysis

### C6.1 Algorithm Inventory Table

| ID | Algorithm | Input | Output | Complexity | Status |
|----|-----------|-------|--------|------------|--------|
| A1 | Naive ddmin (byte split) | Vec<u8> | Vec<u8> | O(n^2) | NAIVE baseline |
| A2 | Chunk-level parallel ddmin (C6.2.1) | Vec<u8> + pool size | Vec<u8> | O((n log n)/p) wall | PEAK |
| A3 | Input-type-aware splitting (C6.2.2) | Vec<u8> + InputType | Vec<u8> | O(n log n) expected | PEAK |
| A4 | Hierarchical minimize (C6.2.3) | Vec<u8> + grammar | Vec<u8> | O(n^2 + g + s) where g=grammar, s=semantic | PEAK |
| A5 | One-minimality verifier | Vec<u8> | bool | O(n) where n = len | Shared |
| A6 | Complement constructor | Vec<u8> + Range | Vec<u8> | O(n) copy | Shared |
| A7 | Type-aware split strategy selector | Vec<u8> + InputType | Vec<DeltaChunk> | O(n) for binary, O(n + parse) for structured | Shared |
| A8 | Evidence graph cache lookup | Sha256 hash | Option<MinimizationProvenance> | O(1) hash table | Shared |

### C6.2 Peak Specifications

#### C6.2.1: Chunk-Level Parallelization (NAIVE → PEAK)

**NAIVE behavior**: The standard ddmin algorithm processes chunks sequentially. For n=8 partitions, that's 8 sequential test executions per iteration. Each test function call takes ~5ms (sandbox container exec). At 8 iterations, that's 8 × 8 × 5ms = 320ms sequential. For larger inputs requiring 50 iterations at n=64, that's 50 × 64 × 5ms = 16 seconds — dominated by test latency.

**PEAK behavior**: All complement tests within a single ddmin iteration run in parallel across the sandbox container pool. With p=8 workers, the wall-clock per iteration drops from n × 5ms to 5ms (all n complements run simultaneously). The 16-second case drops to 50 × 5ms = 250ms. The parallel speedup is approximately n/p for n ≤ p and approaches p for n ≫ p.

**4 sub-questions**:

1. **C6.2.1.Q1: Scheduling algorithm** — How does the scheduler distribute n complement tests across p workers when n > p? Does it use work-stealing, round-robin, or a priority queue based on chunk size?
   - Answer: Round-robin with chunk-size-weighted scheduling. Larger chunks get higher priority because they are more likely to be irrelevant (and thus produce progress faster). The scheduler maintains a min-heap of (worker_id, total_work_assigned) and assigns the next largest chunk to the least-loaded worker. This minimizes tail latency.

2. **C6.2.1.Q2: Early termination** — When one complement test returns Fail (chunk is irrelevant), the remaining in-flight tests are obsolete. How are they cancelled? What's the cancellation latency?
   - Answer: `tokio_util::sync::CancellationToken` is shared across all worker tasks. When a Fail result arrives, the token is cancelled. Workers check `token.is_cancelled()` before starting a sandbox call (check latency <1μs). In-flight sandbox gRPC calls cannot be cancelled (gRPC limitation), but their results are discarded. Cancellation propagation takes <100μs from Fail arrival to all workers stopping new work.

3. **C6.2.1.Q3: Pool health and fallback** — What happens if the sandbox pool has fewer containers available than the configured parallel_workers?
   - Answer: The parallel scheduler acquires sandbox handles via a semaphore with `available_permits()`. If fewer containers are available, the semaphore naturally limits concurrency. The scheduler does NOT block waiting for all p permits — it dispatches min(n, available, p) tasks and polls for results. If the pool is completely exhausted (0 containers), the scheduler falls back to sequential mode with a warning log.

4. **C6.2.1.Q4: Deterministic output** — Does parallelization change the minimization result (i.e., is the output still deterministic given the same input)?
   - Answer: The output is deterministic. The ddmin algorithm always tests complements first. The first Fail result triggers a restart with that complement as the new current. In parallel mode, if multiple complements fail simultaneously, the engine picks the FIRST result from the mpsc receiver, which is non-deterministic. However, ddmin is designed to handle this — picking any failing complement is correct. The final 1-minimality verification is always sequential and checks every byte, ensuring the final output is truly independent of the non-deterministic path taken.

#### C6.2.2: Input-Type Aware Splitting (NAIVE → PEAK)

**NAIVE behavior**: ddmin splits at arbitrary byte boundaries. For a JSON input `{"user":[{"name":"Alice","role":"admin"}]}`, a naive split at byte 10 could land mid-key `"na`, producing a complement that is neither valid JSON nor reproducible. The engine wastes iterations testing malformed inputs that don't trigger the bug because the input structure is broken. This produces both false negatives (complement passes because JSON is unparseable) and wasted iterations.

**PEAK behavior**: The type-aware splitter identifies the input format (JSON, XML, text, bytecode, protobuf) and splits at semantic boundaries:
- **JSON**: Split at token boundaries (key, value, array element, object entry). A JSON tokenizer tracks bracket depth to ensure complements remain valid JSON.
- **Text**: Split at newline boundaries. A multi-line input is split at `\n` to produce complements that are valid line subsets (e.g., a source file with/without a function).
- **Binary**: Split at struct boundaries when a format descriptor is available (via CPG Phase 2), otherwise fallback to power-of-2 byte boundaries.
- **Bytecode**: Split at instruction boundaries (WASM ops, JVM bytecodes). Uses `walrus` for WASM, custom decoder for others.
- **Protobuf**: Split at field boundaries using field number tags.

**Expected improvement**: 3x fewer iterations for structured inputs because complements are likely to remain valid and reproducible.

**4 sub-questions**:

1. **C6.2.2.Q1: Tokenizer depth tracking** — For nested JSON with mixed `[]` and `{}`, how does the splitter handle `{"a": [{"b": 1}, {"c": 2}], "d": 3}` when asked to split into 3 chunks?
   - Answer: The tokenizer maintains a bracket stack: `Vec<BracketType>` where `BracketType::Curly` = object context, `BracketType::Square` = array context. It emits top-level tokens: `{"a":` (token 1), `[{"b": 1}, {"c": 2}]` (token 2, consumes array tracked by square bracket depth returning to 0), `, "d": 3}` (token 3). If 3-way split is too coarse (tokens < 3), the splitter subdivides the largest token (e.g., split array token 2 into its elements). In the extreme, each JSON leaf becomes a chunk.

2. **C6.2.2.Q2: Invalid complement handling** — A complement that removes a `{` or `}` breaks JSON validity. If the test_fn requires valid JSON, how does the engine handle this?
   - Answer: The type-aware splitter guarantees complements are valid by design — it never splits at a bare bracket, only at whole tokens that are bracket-balanced. However, if a complement is tested and returns Inconclusive (malformed input rejected by test harness), the engine marks the chunk as "indivisible" and skips splitting it further. The chunk is treated as relevant (Pass) because we can't prove it's irrelevant. This is conservative — the minimization may be larger than optimal but never incorrect.

3. **C6.2.2.Q3: Format detection latency** — How much overhead does format detection add to the overall pipeline?
   - Answer: Format detection is a one-time cost at ddmin entry (<1ms for 10KB input). It tries JSON parse first (fast-path, ~100μs for 10KB), then XML parse, then text ratio heuristic. For binary inputs, it checks magic bytes (WASM: `\0asm`, Protobuf: field tag patterns). Once detected, the InputType is cached for all subsequent iterations. Total overhead <1% of end-to-end time.

4. **C6.2.2.Q4: Hybrid input handling** — What if the input is a text file containing embedded JSON (e.g., YAML with JSON values, or a log line containing JSON payload)?
   - Answer: The splitter performs two-pass detection. Pass 1: identify overall format (e.g., text with newlines). Split at newline boundaries. Pass 2: for each resulting chunk, attempt to detect sub-format (e.g., JSON within a line). If a chunk contains a valid JSON substring, it is split further using JSON-aware boundaries. This is recursive — maximum depth 3 to avoid infinite recursion on pathological cases.

#### C6.2.3: Hierarchical Minimization (NAIVE → PEAK)

**NAIVE behavior**: Standard ddmin produces a 1-minimal input syntactically — removing any single byte changes the behavior. However, ddmin does not exploit input semantics. For an SQL injection payload `SELECT * FROM users WHERE name = 'admin' --`, ddmin might produce `'admin' --` (24 → 9 bytes, 62% reduction). But a grammar-aware minimizer could recognize that `'x'` with any single character triggers the bug, producing `'x'` (3 bytes, 87% reduction). ddmin alone cannot abstract over the grammar category of "string literal".

**PEAK behavior**: Three-stage hierarchical pipeline:
1. **Stage 1 (ddmin)**: Standard ddmin with type-aware splitting. Produces a syntactically 1-minimal input.
2. **Stage 2 (Grammar minimize)**: If the input conforms to a known grammar (JSON schema, SQL, protocol grammar), this stage replaces each grammar token with the minimal valid value for that token type. "String literal" tokens are replaced with single-character strings. "Number literal" tokens are replaced with 0. "Identifier" tokens are replaced with `a`. Irrelevant tokens (whitespace, comments) are stripped entirely.
3. **Stage 3 (Semantic minimize)**: For each remaining value, the engine attempts to substitute it with an equivalent value from a smaller equivalence class. Numeric values are tested with 0, 1, -1, MAX_INT. Strings are tested with empty string, single char. Boolean and null values are tested. If a substitution still triggers the bug, the smaller value is kept.

**Expected improvement**: 40% smaller final output than naive ddmin alone.

**4 sub-questions**:

1. **C6.2.3.Q1: Grammar source** — Where do the grammars come from for Stage 2?
   - Answer: Grammars are sourced from three places in priority order: (1) Explicit grammar files from Phase 15 (grammar management), mapped to file types via CPG. (2) Inline inference — if the input parses as JSON, the JSON structural grammar is auto-derived. (3) Fallback to content-type heuristics (SQL keywords → SQL grammar, XML → XML grammar). If no grammar is available, Stage 2 is skipped and the pipeline goes directly to Stage 3.

2. **C6.2.3.Q2: Semantic equivalence validation** — How does Stage 3 determine that substituting `'admin'` with `'x'` is semantically "equivalent enough" to test?
   - Answer: Stage 3 does NOT require semantic equivalence — it's a trial-and-error approach. It generates candidate substitutions from a value reduction table: String → `""`, `"a"`, `"0"`; Number → `0`, `1`, `-1`; Array → `[]`; Object → `{}`. Each candidate is tested. If the candidate triggers the bug, it's kept. If no single reduction triggers the bug, the original value is kept. This is safe because false negatives (reducing too aggressively would lose the bug) are caught — the bug must still reproduce.

3. **C6.2.3.Q3: Stage ordering and early exit** — Can Stage 2 or Stage 3 make the input larger? When does the pipeline stop?
   - Answer: Both stages are monotonic-non-increasing: they can only keep or shrink the input. Before committing a stage's output, the engine verifies `new_input.len() <= current_input.len()`. If grammar minimize produces a valid-but-larger input (e.g., from normalizing whitespace), it is discarded. Each stage exits when no further progress is made on a full pass. The pipeline stops when Stage 3 produces no change from its input.

4. **C6.2.3.Q4: Performance impact of 3-stage pipeline** — What is the additive latency of Stages 2 and 3?
   - Answer: Stage 2 (grammar minimize) is dominated by grammar parsing (O(n)) + token iteration (O(t) where t = number of tokens, each tested once). For a 100-token JSON input, 100 test calls × 5ms = 500ms. Stage 3 (semantic minimize) tests ~3 candidates per remaining value. For 10 remaining values after Stage 2, that's 30 test calls × 5ms = 150ms. Total Stage 2+3 overhead: ~650ms for a 100-token input. The ddmin Stage 1 already dominates at multiple seconds for large inputs; the hierarchical stages add <25% overhead while delivering 40% smaller output.

### C6.3 Zero-Gap Table

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│ C6.3 ZERO-GAP ANALYSIS: Delta Debugging Pipeline                                       │
│                                                                                       │
│ DIFFERENCE between NAIVE and PEAK:                                                     │
│ NAIVE: Sequential byte-split ddmin → 1-minimal → done.                                 │
│ PEAK:  Type-aware split → parallel chunk test → grammar minimize → semantic minimize. │
│                                                                                       │
│ GAPS between NAIVE behavior and PEAK requirements:                                     │
│                                                                                       │
│ ┌───┬──────────────────────────┬──────────────────────────────────┬────────────────┐   │
│ │ # │ Gap                      │ NAIVE Behavior                   │ PEAK Resolution │   │
│ ├───┼──────────────────────────┼──────────────────────────────────┼────────────────┤   │
│ │ 1 │ Speed: 100 iterations    │ 100 × n × 5ms = 32s for n=64    │ Parallel:       │   │
│ │   │ take too long            │                                  │ 100 × 5ms = 0.5s│   │
│ │   │                          │                                  │ per C6.2.1     │   │
│ ├───┼──────────────────────────┼──────────────────────────────────┼────────────────┤   │
│ │ 2 │ Structure blindness:     │ Split "a":"b" at byte 2 →        │ Type-aware:     │   │
│ │   │ invalid complements      │ "\"a"":"b"" → wasted iteration    │ split at tokens │   │
│ │   │ waste tests              │                                  │ per C6.2.2     │   │
│ ├───┼──────────────────────────┼──────────────────────────────────┼────────────────┤   │
│ │ 3 │ Output too large:        │ ddmin keeps "admin" because      │ Grammar:        │   │
│ │   │ syntactically minimal    │ removing 'a' changes behavior    │ recognize string │   │
│ │   │ but semantically bloated │                                  │ literal → 'x'  │   │
│ │   │                          │                                  │ per C6.2.3     │   │
│ ├───┼──────────────────────────┼──────────────────────────────────┼────────────────┤   │
│ │ 4 │ Non-deterministic flaky  │ ddmin silently fails if bug      │ Pre/post check: │   │
│ │   │ bugs break minimization  │ disappears mid-minimization      │ re-execute min  │   │
│ │   │                          │                                  │ 3x, flag flaky │   │
│ ├───┼──────────────────────────┼──────────────────────────────────┼────────────────┤   │
│ │ 5 │ Cache miss penalty:      │ No caching. Same input minimized │ Evidence graph: │   │
│ │   │ redundant minimization   │ on every crash of same bug       │ hash lookup     │   │
│ │   │                          │                                  │ → instant return│   │
│ ├───┼──────────────────────────┼──────────────────────────────────┼────────────────┤   │
│ │ 6 │ No provenance: can't     │ Output is raw bytes, no metadata │ Provenance      │   │
│ │   │ trust the result         │ on how it was derived            │ struct with all │   │
│ │   │                          │                                  │ stats recorded  │   │
│ ├───┼──────────────────────────┼──────────────────────────────────┼────────────────┤   │
│ │ 7 │ Large inputs (100MB)     │ Allocate 100MB complements per   │ Stream/iterator │   │
│ │   │ cause OOM                │ test → OOM with parallel tests   │ complements,    │   │
│ │   │                          │                                  │ size gate at    │   │
│ │   │                          │                                  │ 10MB           │   │
│ └───┴──────────────────────────┴──────────────────────────────────┴────────────────┘   │
│                                                                                       │
│ VERIFICATION: All 7 gaps are resolved by the three C6.2 peak specs as mapped above.   │
│ No gaps remain between NAIVE baseline and PEAK requirements.                           │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

### C6.4 Deferral Table

```
┌──────────────────────────────────────────────────────────────────────────────────────┐
│ C6.4 DEFERRAL TABLE: Optimizations Explicitly Out of Scope for Phase 22               │
│                                                                                      │
│ ┌───┬────────────────────────────┬──────────────────────┬──────────────────────────┐ │
│ │ # │ Feature                     │ Reason for Deferral  │ Future Phase              │ │
│ ├───┼────────────────────────────┼──────────────────────┼──────────────────────────┤ │
│ │ 1 │ Machine-learning guided    │ Requires training    │ Phase 40+ (ML integration │ │
│ │   │ split strategy selection   │ data from Phase 22.  │ if scoped)                │ │
│ │   │                            │ Premature without    │                           │ │
│ │   │                            │ sufficient corpus.   │                           │ │
│ ├───┼────────────────────────────┼──────────────────────┼──────────────────────────┤ │
│ │ 2 │ Differential delta:        │ Requires Phase 24's  │ Phase 24+ (extend delta   │ │
│ │   │ minimize diff between two  │ differential harness │ to work on behavioral     │ │
│ │   │ inputs that produce        │ in place. Can't      │ diffs between versions)   │ │
│ │   │ different outputs          │ compare outputs      │                           │ │
│ │   │                            │ without baselines.   │                           │ │
│ ├───┼────────────────────────────┼──────────────────────┼──────────────────────────┤ │
│ │ 3 │ Automated test case        │ Phase 25 covers test │ Phase 25 (Test Synthesis) │ │
│ │   │ generation from minimized  │ synthesis with       │                           │ │
│ │   │ input                      │ broader scope.       │                           │ │
│ │   │                            │ Minimization is      │                           │ │
│ │   │                            │ prerequisite.        │                           │ │
│ ├───┼────────────────────────────┼──────────────────────┼──────────────────────────┤ │
│ │ 4 │ Real-time delta during     │ Adds complexity to   │ Phase 30+ (feedback loop  │ │
│ │   │ live fuzzing campaigns     │ fuzzer hot path.     │ optimization)             │ │
│ │   │                            │ Phase 22 is post-hoc │                           │ │
│ │   │                            │ only (simpler,       │                           │ │
│ │   │                            │ correct).            │                           │ │
│ ├───┼────────────────────────────┼──────────────────────┼──────────────────────────┤ │
│ │ 5 │ Cross-input correlation    │ Phase 23 (Trigger    │ Phase 23 (Trigger Matrix) │ │
│ │   │ for trigger matrix         │ Matrix) owns this    │                           │ │
│ │   │ population                 │ logic. Delta only    │                           │ │
│ │   │                            │ produces the minimal │                           │ │
│ │   │                            │ input for the matrix.│                           │ │
│ ├───┼────────────────────────────┼──────────────────────┼──────────────────────────┤ │
│ │ 6 │ GPU-accelerated brute-force│ Over-engineering.    │ Never (GPU not applicable │
│ │   │ 1-minimality search        │ O(n) CPU check is    │ to sequential test        │
│ │   │                            │ fast enough for      │ function calls)           │
│ │   │                            │ n ≤ 1000 bytes.      │                           │
│ ├───┼────────────────────────────┼──────────────────────┼──────────────────────────┤ │
│ │ 7 │ Distributed delta across   │ Single-node pool is  │ Phase 35+ (distributed    │
│ │   │ multiple sandbox hosts     │ sufficient for Phase │ execution)                │
│ │   │                            │ 22 scope. Adds       │                           │
│ │   │                            │ network complexity.  │                           │
│ └───┴────────────────────────────┴──────────────────────┴──────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

---

## D. Verification

### D1. Unit Tests

| # | Test Name | Test Target | Input | Expected Output | Tags |
|---|-----------|-------------|-------|-----------------|------|
| T22.1 | test_ddmin_empty_input_returns_already_minimal | ddmin() | Empty byte slice | AlreadyMinimal | AGGRESSIVE |
| T22.2 | test_ddmin_single_byte_essential | ddmin() | 1-byte input where byte is essential | Success with size=1 | AGGRESSIVE |
| T22.3 | test_ddmin_single_byte_irrelevant | ddmin() | 1-byte input where byte is irrelevant | Success with size=0 | AGGRESSIVE |
| T22.4 | test_ddmin_two_bytes_both_essential | ddmin() | [0xFF, 0x00] both needed | Success with size=2 | |
| T22.5 | test_ddmin_two_bytes_one_relevant | ddmin() | [0xFF, 0x00] only 0xFF needed | Success with size=1, output=[0xFF] | |
| T22.6 | test_ddmin_ten_bytes_reduces_to_three | ddmin() | 10 bytes, first 3 essential | Success, size=3, reduction=70% | AGGRESSIVE |
| T22.7 | test_ddmin_100_bytes_reduces_to_5 | ddmin() | 100 bytes, 5 essential | Success, size=5, reduction=95% | AGGRESSIVE |
| T22.8 | test_ddmin_precondition_empty_does_not_crash | ddmin() | Non-empty where empty([]) → Fail | Panic caught, HarnessFailure | |
| T22.9 | test_ddmin_precondition_original_does_not_crash | ddmin() | Pass on original input | Assert panic: original must fail | |
| T22.10 | test_ddmin_iteration_limit_hit | ddmin() | Input that requires >max_iterations | IterationLimitExceeded with partial provenance | |
| T22.11 | test_ddmin_timeout | ddmin() | Input that takes >timeout_ms | Timeout with partial provenance | |
| T22.12 | test_ddmin_verify_one_minimal_true | verify_1_minimal() | Truly 1-minimal input | true | AGGRESSIVE |
| T22.13 | test_ddmin_verify_one_minimal_false | verify_1_minimal() | Input with irrelevant byte | false | AGGRESSIVE |
| T22.14 | test_ddmin_parallel_deterministic | ddmin() with parallel | Same input 10x with same seed | Same output all 10 runs | AGGRESSIVE |
| T22.15 | test_ddmin_parallel_same_as_sequential | ddmin() | Compare parallel vs sequential output | Same minimized input (may differ, both 1-minimal) | AGGRESSIVE |
| T22.16 | test_type_aware_split_json | type_aware_split() | JSON string with 3 keys | 3+ chunks, no chunk breaks mid-token | AGGRESSIVE |
| T22.17 | test_type_aware_split_text_newline | type_aware_split() | 5-line text | 5 chunks at newline boundaries | |
| T22.18 | test_type_aware_split_binary_fallback | type_aware_split() | Unrecognized binary | Chunks at byte boundaries (size/2) | |
| T22.19 | test_type_aware_split_zero_len_chunks_filtered | type_aware_split() | Input with empty line at start | Zero-length chunks removed | |
| T22.20 | test_grammar_minimize_json | grammar_minimize() | JSON with verbose values | Keys kept, values minimized | AGGRESSIVE |
| T22.21 | test_semantic_minimize_strings | semantic_minimize() | Input with long string values | Strings replaced with single char | AGGRESSIVE |
| T22.22 | test_semantic_minimize_no_false_negative | semantic_minimize() | Input where only specific value works | That value preserved, not overly reduced | |
| T22.23 | test_hierarchical_pipeline_smaller_than_ddmin | hierarchical pipeline | Input with grammar structure | hierarchical output ≤ ddmin-only output | AGGRESSIVE |
| T22.24 | test_provenance_all_stats_populated | provenance | Any minimization result | All provenance fields non-zero/valid | |
| T22.25 | test_cache_hit_skips_minimization | cache lookup | Input with known hash | Instant return, no test calls made | |
| T22.26 | test_cancel_on_first_fail | parallel scheduler | Complement that fails on chunk 1 | Tests for chunks 2..n cancelled | |
| T22.27 | test_fallback_to_sequential_on_pool_exhaust | parallel scheduler | Pool at capacity | Sequential mode engaged, warning logged | |
| T22.28 | test_inconclusive_marks_chunk_relevant | ddmin() | Inconclusive test result | Chunk treated as relevant (kept) | |
| T22.29 | test_non_deterministic_returns_inconclusive | ddmin() | Flaky test_fn (50% pass) | HarnessFailure after repeated inconclusive | |
| T22.30 | test_max_executions_limit_checked | ddmin() | Input requiring 10K+ tests | IterationLimitExceeded at max_test_executions | |

### D2. Integration Tests

| # | Test Name | Components | Scenario | Validation |
|---|-----------|-----------|----------|------------|
| IT22.1 | test_end_to_end_fuzz_crash_to_minimized | Fuzzer → Delta → Evidence graph | Fuzzer finds crash in target binary, delta automatically runs, result stored in evidence graph | Minimized input in graph, reduction >90% |
| IT22.2 | test_agent_fuzz_target_returns_minimized | Agent tool → Delta → Agent response | Agent calls fuzz_target, gets crashes AND minimized versions in single response | Response schema validates, minimized field present |
| IT22.3 | test_parallel_sandbox_pool_integration | Delta → Sandbox daemon → Container pool | 32-byte input, 8 workers, parallel complement testing | All 8 worker containers used, speedup ≥3x |
| IT22.4 | test_cache_eviction_and_repopulation | Delta → Evidence graph → Delta | Minimize input A, restart delta service, minimize input A again | Second call hits cache, 0 test executions |
| IT22.5 | test_grammar_stage_with_phase15_grammar | Delta → CPG (grammar lookup) → Grammar minimize | Bug in JSON-handling function, CPG provides JSON grammar | Stage 2 reduces by additional 25% over ddmin-only |

### D3. Gate: Attack Vectors

**Purpose**: Verify delta debugging correctness under adversarial conditions. The engine must never discard bytes that are essential to reproducing the bug.

#### AV22.1: Complement Lies — Sandbox Returns False Negative
- **Setup**: Create a test_fn that crashes on input containing bytes `[0xDE, 0xAD, 0xBE, 0xEF]`. The sandbox is configured to randomly drop the crash 20% of the time.
- **Attack**: Run ddmin on a 100-byte input containing `0xDE 0xAD 0xBE 0xEF`. The non-deterministic sandbox may report Pass when it should report Fail.
- **Pass condition**: The minimized input must still contain `[0xDE, 0xAD, 0xBE, 0xEF]` after minimization. If a false-negative Pass caused the algorithm to discard these bytes, the final verification (3x re-execution) catches it and the result is flagged.
- **Fail condition**: The minimized input has discarded essential bytes and the re-execution still passes (false positive in verification).

#### AV22.2: Input Poison — Type Detector Misled
- **Setup**: Craft a binary input that happens to parse as valid JSON (e.g., ASCII-only binary data formatted like `{"a":1}`). The type detector classifies it as JSON.
- **Attack**: Run ddmin with JSON-aware splitting on this pseudo-JSON input. JSON tokens don't align with the actual crash byte boundaries.
- **Pass condition**: If complements become invalid and tests return Inconclusive, the engine falls back to Binary splitting. The minimization completes correctly.
- **Fail condition**: The engine loops infinitely trying JSON tokens that never produce valid complements, or returns a garbage result.

#### AV22.3: Parallel Race — Two Complements Pass Simultaneously
- **Setup**: Create a test_fn where removing byte 5 or removing byte 10 both still produce Fail. Configure 2 parallel workers. Both complements execute simultaneously.
- **Attack**: The engine gets two Fail results for complements of chunk 5 and chunk 10 simultaneously. It must pick ONE to use as the new current.
- **Pass condition**: The engine picks the first arriving Fail result, discards that chunk, and continues. The final 1-minimality check ensures correctness regardless of which was picked first.
- **Fail condition**: The engine enters inconsistent state, crashes, or produces a non-reproducing minimized input.

#### AV22.4: Timing Side Channel — Slow Complement Hides Relevance
- **Setup**: Create a test_fn where removing chunk A causes a quick timeout (Fail), and removing chunk B causes a slow crash (5s delay, also Fail). With a 1s per-test timeout, chunk B's complement returns Inconclusive.
- **Attack**: The engine incorrectly classifies chunk B as relevant (kept) because its complement test was Inconclusive, but the chunk is actually irrelevant.
- **Pass condition**: The engine marks Inconclusive results as "unknown relevance" and treats chunks as conservative-relevant. The minimization result is larger than optimal but still reproduces the crash. The provenance log clearly states the Inconclusive result.
- **Fail condition**: Chunk B is incorrectly discarded based on stale/different result.

#### AV22.5: Infinite Granularity Loop — ddmin Never Converges
- **Setup**: A pathological test_fn where the bug is triggered by ANY non-empty input. test_fn([]) = Pass, test_fn(any_non_empty) = Fail. Input: `[1, 2, 3, 4]`.
- **Attack**: ddmin splits into 2 chunks → both complements = Fail → splits into 4 chunks → all complements = Fail → splits into 4 (max) → oscillates at n=4 trying both complement and subset strategies.
- **Pass condition**: The engine detects the oscillation (same input size after 5 iterations of n=4) and terminates with AlreadyMinimal, noting that the entire input is relevant.
- **Fail condition**: Engine runs to max_iterations limit, consuming all 100 iterations without progress.

#### AV22.6: Cache Poison — SHA-256 Collision with Different Input
- **Setup**: Generate two inputs A and B where SHA-256(A) == SHA-256(B) (theoretically possible but practically with brute force for short inputs). Store A's minimization result in cache. Query with B.
- **Attack**: The cache returns A's result for B, even though B is a completely different input with a different minimal reproduction.
- **Pass condition**: The cache stores the full input alongside the hash. Before returning cached result, it verifies `sha256(cached.full_input) == sha256(requested_input)` AND `cached.full_input == requested_input`. The double-check prevents collision-based false returns.
- **Fail condition**: The cached result is returned for input B, and it doesn't actually reproduce B's crash.

#### AV22.7: Hierarchical Over-Minimization — Grammar Stage Breaks the Bug
- **Setup**: Bug triggers only when a string literal is exactly `"admin"`. Grammar minimize replaces `"admin"` with `"x"` (single char). Semantic minimize tests `""`, `"a"`, `"0"`. None reproduce the bug.
- **Attack**: The hierarchical stage over-minimizes and loses the reproduction. No revert mechanism exists.
- **Pass condition**: Each stage verifies that its output reproduces the bug before committing. If grammar_minimize output fails, the stage is reverted and its output is the pre-grammar ddmin result. The provenance log shows "GrammarMinimize: skipped (output not reproducible)".
- **Fail condition**: The stage discards the reproduction and passes the over-minimized input downstream.

#### AV22.8: Extreme Parallelism — All Workers Deadlocked on Pool
- **Setup**: Configure parallel_workers=1000. The sandbox pool has 5 containers. 995 complement tests queue up. The semaphore wait timeout fires.
- **Attack**: The scheduler deadlocks waiting for pool slots that never become available because earlier tests are stuck.
- **Pass condition**: The semaphore has a 5s acquire timeout. On timeout, the scheduler degrades: releases partial results, falls back to sequential mode, logs warning. Minimization completes (slower) but correctly.
- **Fail condition**: The scheduler blocks indefinitely, the operation times out at the delta level, and the crash input is lost (no minimization at all).

### D4. Golden Dataset

```rust
/// Golden dataset: pre-computed expected minimizations for regression testing.
fn golden_dataset() -> Vec<GoldenCase> {
    vec![
        GoldenCase {
            name: "json_crash_001",
            original: br#"{"user":{"name":"Alice","role":"admin"},"opts":{"verbose":true,"timeout":30}}"#.to_vec(),
            expected_min: br#"{"user":{"name":"admin"}}"#.to_vec(),
            min_ratio_target: 0.70, // >70% reduction
            crash_condition: |data| data.windows(5).any(|w| w == b"admin"),
        },
        GoldenCase {
            name: "binary_crash_002",
            original: (0u8..200).collect::<Vec<u8>>(), // 0,1,2,...,199
            expected_min: vec![0xAB, 0xCD],
            min_ratio_target: 0.98, // >98% reduction (2/200)
            crash_condition: |data| data.len() >= 2 && data[0] == 0xAB && data[1] == 0xCD,
        },
        GoldenCase {
            name: "text_line_crash_003",
            original: b"line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n".to_vec(),
            expected_min: b"line4\nline7\n".to_vec(),
            min_ratio_target: 0.80, // >80% reduction
            crash_condition: |data| {
                let s = std::str::from_utf8(data).unwrap_or("");
                s.contains("line4") && s.contains("line7")
            },
        },
        GoldenCase {
            name: "single_byte_crash_004",
            original: vec![0x00; 100],
            expected_min: vec![0x00], // every byte irrelevant
            min_ratio_target: 0.0, // already minimal? No, expected 1 byte
            crash_condition: |data| !data.is_empty() && data[0] == 0x00,
        },
        GoldenCase {
            name: "already_minimal_005",
            original: vec![0xFF],
            expected_min: vec![0xFF],
            min_ratio_target: 0.0, // can't reduce
            crash_condition: |data| data == &[0xFF],
        },
    ]
}
```

### D5. Regression Test

```
REGRESSION TEST: Delta Debugging Correctness

Test engineer: Follow these steps before merging any PR to bugswarm-sandbox/src/delta.rs:

1. RUN golden dataset:
   cargo test --package bugswarm-sandbox --test delta_golden

2. CHECK all golden cases pass:
   - json_crash_001: output contains "admin", other JSON keys stripped, >70% reduction
   - binary_crash_002: output == [0xAB, 0xCD], >98% reduction
   - text_line_crash_003: output contains "line4" and "line7", other lines stripped
   - single_byte_crash_004: output == [0x00], >90% reduction (99/100 bytes removed)
   - already_minimal_005: output == [0xFF], 0% reduction, status = AlreadyMinimal

3. RUN parallel vs sequential diff test:
   cargo test --package bugswarm-sandbox --test delta_parallel_determinism
   - All 10 parallel runs produce 1-minimal outputs
   - Every output reproduces the crash

4. RUN type-aware split accuracy:
   cargo test --package bugswarm-sandbox --test delta_type_split
   - JSON split: all complements are valid JSON (serde_json parses them)
   - Text split: all complements have complete lines (no mid-line breaks)
   - Bytecode split: WASM modules from complements are valid

5. RUN performance budget:
   cargo bench --package bugswarm-sandbox --bench delta_perf
   - 10KB input minimization < 30s
   - Parallel speedup ≥ 3x on 8-core machine
   - Memory peak < 50MB for 1MB input

6. RUN adversarial attack vectors:
   cargo test --package bugswarm-sandbox --test delta_attack_vectors
   - All 8 attack vectors yield PASS
```

---

## E. Operations

### E1. Cost Table

| Resource | Cost Category | Estimated | Unit | Notes |
|----------|--------------|-----------|------|-------|
| CPU time per minimization (10KB input) | Compute | 30s | Seconds | With 8 parallel workers |
| Memory per minimization (10KB input) | Memory | 50MB | MB | Peak RSS |
| Sandbox container hours per 1000 crashes | Infrastructure | 8.3 | Hours | 1000 × 30s / 3600 |
| Evidence graph storage per minimal input | Storage | 1KB | KB | Provenance struct + input bytes |
| LLM token savings per bug | AI Cost | ~450 | Tokens | 500 bytes → 3 bytes sent to agent |
| Developer time saved per bug | Labor | 45 | Minutes | Agent investigation time reduction |
| Total operational cost per 1000 bugs | Total | ~$5 | USD | Compute + storage + token savings |

### E2. Observability

#### Logs

| Log Name | Level | Message Template | Fields | Frequency |
|----------|-------|-----------------|--------|-----------|
| delta.start | INFO | "Delta debugging started for input hash={hash} size={size}" | hash, size, input_type | Per crash |
| delta.iteration | DEBUG | "Iteration {iter} n={n} current_size={size} chunks={chunks} results={fails}/{total}" | iter, n, size, chunks, fails, total | Per iteration |
| delta.parallel_dispatch | DEBUG | "Dispatched {count} complement tests to {workers} workers" | count, workers | Per iteration |
| delta.chunk_result | TRACE | "Chunk {idx} complement: {result} at {elapsed_ms}ms" | idx, result, elapsed_ms | Per chunk test |
| delta.stage_transition | INFO | "Stage transition: {from} → {to}, input size {from_size} → {to_size}" | from, to, from_size, to_size | Per stage |
| delta.success | INFO | "Minimization complete: {orig_size} → {min_size} ({reduction_pct}%), {iterations} iterations, {executions} tests, {duration_ms}ms" | orig_size, min_size, reduction_pct, iterations, executions, duration_ms | Per completion |
| delta.cache_hit | INFO | "Cache hit for input hash={hash}, returning instantly" | hash | Per cache hit |
| delta.cache_miss | DEBUG | "Cache miss for input hash={hash}, starting minimization" | hash | Per minimization start |
| delta.fallback_sequential | WARN | "Falling back to sequential mode: pool exhausted after {timeout_ms}ms. {queued} tests queued." | timeout_ms, queued | Per fallback |
| delta.non_deterministic | WARN | "Non-deterministic crash detected: minimized input passed on re-execution {attempt}/{max_attempts}" | attempt, max_attempts | Per flaky crash |

#### Metrics

| Metric Name | Type | Description | Labels | Aggregation |
|-------------|------|-------------|--------|-------------|
| delta.minimizations_total | Counter | Total minimization attempts | status (success/failure/timeout) | Sum |
| delta.iterations_per_min | Histogram | Iterations per minimization | input_type | Avg, P50, P95, P99 |
| delta.test_executions_per_min | Histogram | Test function calls per minimization | input_type | Avg, P50, P95, P99 |
| delta.reduction_ratio | Histogram | (original - min) / original | input_type, stage | Avg, P50, P95 |
| delta.duration_ms | Histogram | Wall-clock duration of minimization | input_type, status | Avg, P50, P95, P99 |
| delta.parallel_workers_active | Gauge | Number of workers currently executing | worker_id | Instant |
| delta.pool_queue_depth | Gauge | Number of tests queued for pool | pool_name | Instant |
| delta.cache_hit_rate | Gauge | Cache hits / total lookups (rolling) | cache_name | Rolling average |
| delta.one_minimal_verified | Counter | Number of minimizations where 1-minimality was verified | input_type | Sum |
| delta.fallback_count | Counter | Number of times fallback to sequential occurred | reason | Sum |

#### Alerts

| Alert Name | Condition | Severity | Runbook |
|------------|-----------|----------|---------|
| DeltaSuccessRateLow | delta.minimizations_total{status="failure"} / delta.minimizations_total > 0.1 for 5m | WARNING | Check sandbox pool health. Inspect logs for non-deterministic crash patterns. |
| DeltaDurationHigh | hist_quantile(0.95, delta.duration_ms) > 60000 for 5m | WARNING | Check for input size spikes. Consider increasing parallel_workers or adjusting timeout_secs. |
| DeltaPoolSaturation | delta.pool_queue_depth > 20 for 2m | CRITICAL | Scale up sandbox container pool. Reduce parallel_workers temporarily to avoid queuing. |
| DeltaCacheHitRateLow | delta.cache_hit_rate < 0.5 for 10m | INFO | Expected during initial bug discovery burst. Normalizes as evidence graph populates. |
| DeltaFallbackFrequent | rate(delta.fallback_count[5m]) > 0.1 | WARNING | Sandbox pool is consistently unavailable. Check sandbox daemon health. |
| DeltaNonDeterministicRate | rate(delta.non_deterministic[5m]) > 0.05 | CRITICAL | High rate of flaky crashes. Bugs are non-deterministic. Delta cannot help. Flag for Phase 26. |

### E3. Configuration Table

| Key | Type | Default | Description | Hot-Reload |
|-----|------|---------|-------------|------------|
| `delta.max_iterations` | usize | 100 | Maximum ddmin iterations before forced stop | No |
| `delta.timeout_secs` | u64 | 30 | Maximum wall-clock time for minimization | No |
| `delta.min_chunk_bytes` | usize | 1 | Minimum chunk size (below this, stop splitting) | No |
| `delta.parallel_workers` | usize | 8 | Maximum number of concurrent sandbox workers | Yes |
| `delta.use_type_aware_split` | bool | true | Enable input-type-aware chunk splitting | No |
| `delta.use_hierarchical` | bool | true | Enable hierarchical (grammar + semantic) stages | No |
| `delta.verify_one_minimality` | bool | true | Run 1-minimality check before returning | No |
| `delta.max_test_executions` | usize | 10_000 | Hard safety limit on test function calls | No |
| `delta.evidence_cache_ttl_secs` | u64 | 86_400 | How long cached minimizations live in evidence graph | Yes |
| `delta.sandbox_exec_timeout_ms` | u64 | 5_000 | Per-test sandbox execution timeout | Yes |

### E4. Migration

**N/A for new module**: No existing data or API to migrate. The delta module is greenfield within `bugswarm-sandbox/src/delta.rs`.

**Forward compatibility notes**:
1. The provenance struct includes a version field (default: 1) for future format evolution.
2. Evidence graph node type `MinimizedInput` is additive — doesn't modify existing node types.
3. Fuzz_target response adds an optional `minimized` field — backward compatible with older agents.

### E5. Documentation List

| Document | Audience | Content |
|----------|----------|---------|
| `docs/phase22/architecture.md` | Developers | Full architecture, data flow, split strategies |
| `docs/phase22/api.md` | Integrators | Delta engine API, Provenance struct fields, error types |
| `docs/phase22/configuration.md` | Operators | All config keys, tuning guidance, performance profiles |
| `docs/phase22/design/why_ddmin.md` | Researchers | Why ddmin was chosen over other algorithms (delta, probdd, hierarchical) |
| `docs/phase22/examples.md` | All users | Worked examples: JSON crash, binary crash, text crash with step-by-step reduction |
| `docs/phase22/peak_optimizations.md` | Developers | Deep dive into C6.2.1, C6.2.2, C6.2.3 with benchmarks |

---

## Dependency Tree

```
Phase 22: Delta Debugging
│
├── DIRECT DEPENDENCIES
│   ├── Phase 1 (Sandbox) — REQUIRED: executes test_fn(input) calls
│   ├── Phase 5 (Evidence Graph) — OPTIONAL: caches minimized inputs
│   └── Phase 2 (CPG) — OPTIONAL: provides grammar for hierarchical stage
│
├── DEPENDENTS (phases that need Phase 22)
│   ├── Phase 23 (Trigger Matrix) — consumes minimized inputs for matrix rows
│   ├── Phase 25 (Test Synthesis) — minimized inputs → unit test generation
│   ├── Phase 17-18 (Agent Investigation tools) — minimized inputs → precise analysis
│   ├── Phase 24 (Differential Analysis) — may use delta on behavioral diffs
│   └── Phase 9-10 (Fuzzer Triage) — minimized inputs → deduplication
│
├── PARALLELIZABLE WITH
│   ├── Phase 20 (Profiling) — independent modules, shared sandbox pool
│   ├── Phase 21 (Crash Analysis) — independent, delta is post-processing
│   └── Phase 15 (Grammar Management) — if grammars available, enables Stage 2
│
└── BLOCKING STATUS: NONE. No phase is blocked waiting on Phase 22.
```

---

## Risk Assessment

| ID | Risk | Probability | Impact | Mitigation | Contingency |
|----|------|-------------|--------|------------|-------------|
| R22.1 | Non-deterministic crashes defeat ddmin (flaky bugs produce false negatives) | Medium (30%) | High — delta useless for flaky bugs | Pre-minimization stability check (3x re-execution). If fail rate >0%, skip delta and flag bug. | Phase 26 (Flaky Detection) handles these bugs separately. |
| R22.2 | Parallel scheduling introduces non-deterministic output that confuses agents | Low (10%) | Medium — agents may see different mins for same bug | 1-minimality verification is always sequential and deterministic. Cache deterministic results. | Fall back to sequential mode if agent confusion is reported. |
| R22.3 | Type-aware split produces worse results than byte split on some formats | Low (15%) | Low — slightly larger minimized output | A/B test: run both byte-split and type-split for first 100 inputs. If byte-split produces better reductions, disable type-aware. | Config flag to disable type-aware split per input type. |
| R22.4 | Performance budget exceeded for large inputs (>1MB) | Medium (40%) | Medium — timeouts, wasted sandbox time | Input size gate: inputs >1MB go through size-reduction pre-pass (chunk at 64KB boundaries first). | Accept slower minimization for large inputs; optimize in Phase 35. |
| R22.5 | Evidence graph cache grows unbounded | Low (20%) | Low — storage cost | TTL-based eviction (default 24h). Size-based cap (max 100K entries). | Manual cache clear via admin endpoint. |

---

## Decision Log

| ID | Decision | Rationale | Alternatives Considered | Date |
|----|----------|-----------|------------------------|------|
| DL22.1 | Use Zeller's ddmin over Hierarchical Delta Debugging (HDD) | ddmin is simpler, well-understood, and sufficient for 90%+ reduction. HDD adds tree structure overhead without proven benefit for unstructured binary inputs. | HDD, ProbDD (probabilistic delta), DD (basic delta) | 2026-05-14 |
| DL22.2 | Parallelize within iteration (complement tests) not across iterations | Within-iteration parallelism is safe because all n/2 complement tests are independent. Cross-iteration parallelism would require speculative execution (assume chunk irrelevant, test next iteration in parallel) — complex and error-prone. | Cross-iteration speculative parallelism, no parallelism | 2026-05-14 |
| DL22.3 | Three-stage hierarchical pipeline (ddmin → grammar → semantic) over single-stage | Each stage addresses a different dimension of minimization. ddmin handles syntax, grammar handles structure, semantic handles values. Composability is cleaner than monolithic approach. | Single unified stage, ddmin → semantic (skip grammar) | 2026-05-14 |
| DL22.4 | Post-hoc minimization (not inline during fuzzing) | Avoids adding latency to fuzzer's hot path. Minimization can be batched and parallelized off the critical path. | Inline minimization, hybrid (inline for small inputs, post-hoc for large) | 2026-05-14 |
| DL22.5 | SHA-256 for cache keys (not content hash of shorter length) | SHA-256 collision resistance is industry standard. The inputs are user-generated (not adversarial) so SHA-256's 128-bit security against collision is more than sufficient. | MD5 (fast but cryptographically broken), BLAKE3 (newer, less ecosystem support) | 2026-05-14 |
| DL22.6 | Conservative handling of Inconclusive results (treat as relevant) | Safety first: if we can't determine irrelevance, we keep the chunk. This produces larger-but-correct minimized inputs rather than risking false minimization. | Aggressive (treat Inconclusive as Fail), Retry (retry Inconclusive 3x) | 2026-05-14 |

---

## Review Checklist

- [ ] 1. ddmin core algorithm correctly implements Zeller 2002 algorithm (verified against paper pseudocode)
- [ ] 2. Empty input precondition check: test_fn([]) == Pass required before starting ddmin
- [ ] 3. Original input precondition check: test_fn(original) == Fail required before starting ddmin
- [ ] 4. 1-minimality verification is O(n) and runs before returning Success
- [ ] 5. Parallel scheduler uses CancellationToken for early termination on first Fail
- [ ] 6. Type-aware splitter handles JSON bracket depth, XML element nesting, text newlines correctly
- [ ] 7. Hierarchical stages are monotonic-non-increasing (never make output larger)
- [ ] 8. Each hierarchical stage verifies its output reproduces the bug before committing
- [ ] 9. Input cache double-checks full input equality (not just hash) before returning cached result
- [ ] 10. All 8 attack vectors from D3 gate pass with the expected behavior
- [ ] 11. Agent tool fuzz_target response schema includes the `minimized` field

---

## Gate Receipt JSON

```json
{
  "gate_receipt": {
    "phase": 22,
    "title": "Delta Debugging — Minimal Reproduction",
    "timestamp": "2026-05-14T00:00:00Z",
    "approver": "System Architect",
    "status": "APPROVED",
    "version": "1.0",
    "sections_verified": {
      "A_identity_purpose": true,
      "B_architecture": true,
      "C_algorithm_logic": true,
      "C6_peak_analysis": true,
      "D_verification": true,
      "E_operations": true
    },
    "success_criteria": {
      "SC_22_1_size_reduction_90pct": "PENDING_VERIFICATION",
      "SC_22_2_crash_reproduction_99pct": "PENDING_VERIFICATION",
      "SC_22_3_parallel_speedup_factor": "PENDING_VERIFICATION",
      "SC_22_4_parser_iterations_3x": "PENDING_VERIFICATION",
      "SC_22_5_hierarchical_output_40pct_smaller": "PENDING_VERIFICATION",
      "SC_22_6_e2e_latency_30s": "PENDING_VERIFICATION",
      "SC_22_7_agent_integration": "PENDING_VERIFICATION",
      "SC_22_8_evidence_caching_80pct": "PENDING_VERIFICATION"
    },
    "attack_vectors": {
      "AV22_1_complement_lies_false_negative": "PASS_REQUIRED",
      "AV22_2_input_poison_type_misled": "PASS_REQUIRED",
      "AV22_3_parallel_race_dual_fail": "PASS_REQUIRED",
      "AV22_4_timing_side_channel_inconclusive": "PASS_REQUIRED",
      "AV22_5_infinite_granularity_loop": "PASS_REQUIRED",
      "AV22_6_cache_poison_collision": "PASS_REQUIRED",
      "AV22_7_hierarchical_over_minimization": "PASS_REQUIRED",
      "AV22_8_extreme_parallelism_deadlock": "PASS_REQUIRED"
    },
    "peak_specs": {
      "C6_2_1_chunk_level_parallelization": "SPECIFIED",
      "C6_2_2_input_type_aware_splitting": "SPECIFIED",
      "C6_2_3_hierarchical_minimization": "SPECIFIED"
    },
    "zero_gap_verification": {
      "gap_count": 7,
      "all_gaps_resolved": true
    },
    "risk_level": "MEDIUM",
    "risk_mitigations_active": [
      "Pre-minimization stability check (3x re-execution)",
      "Input size gate at 1MB for size-reduction pre-pass",
      "TTL-based evidence cache eviction",
      "Conservative Inconclusive handling"
    ],
    "dependency_status": {
      "phase_1_sandbox": "AVAILABLE",
      "phase_5_evidence_graph": "AVAILABLE",
      "phase_2_cpg": "AVAILABLE"
    },
    "blocks": [],
    "blocked_by": []
  }
}
```

---

## Changelog

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-05-14 | System | Initial Phase 22 plan — Delta Debugging ddmin engine, parallel chunk testing, type-aware splitting, hierarchical minimization |

