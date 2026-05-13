# Phase 16: Sanitizer Instrumentation — ASAN/UBSAN/TSAN/MSAN/LSAN Compiled Into Sandbox Images

**Status**: NOT_STARTED
**Estimated Effort**: 6 hours
**Depends On**: Phase 1 (Sandbox)
**Unblocks**: Phase 23 (Trigger Matrix), Phase 24 (Differential), Phase 25 (Spec Mining), Phase 26 (Mutation Testing)

---

## A1. What is being built?

Compile every sandbox Docker image with `-fsanitize=address,undefined,thread,memory,leak` flags so every PoC execution automatically detects memory corruption, undefined behavior, data races, uninitialized reads, and memory leaks. Sanitizer reports are captured as structured evidence and auto-injected into the evidence graph.

---

## A2. Which specific gap does it fill?

**Gap ID**: INV-005 (from `invincible.md`)
**Current behavior**: Sandbox runs PoCs in a vanilla container. Memory errors, undefined behavior, and data races happen silently — no crash, no stack trace, no detection. A buffer overflow writes past the array boundary and the program continues with corrupted memory. The agent never knows.
**Target behavior**: Every PoC execution runs with sanitizer coverage. A buffer overflow → ASAN immediately kills the process and produces a report with exact address, allocation site, and stack trace. This becomes a confirmed finding with sanitizer proof (strength 0.95).

---

## A3. What is the success criteria?

| Metric | Target | Measurement |
|--------|--------|-------------|
| Sanitized execution coverage | 100% of PoC executions | Every `execute()` call uses sanitizer-compiled image |
| Previously-undetected bugs found | ≥1 per 100 PoCs | Compare bug count with/without sanitizers on extreme-bugs repo |
| False positive rate | 0% | Every sanitizer report must correspond to a genuine memory/behavior error |
| Sanitizer report parse rate | 100% | Every sanitizer output must parse into structured `SanitizerReport` |
| Execution overhead | <3x wall-clock time | Sanitized run ≤ 360s vs 120s baseline |

---

## A4. What is the priority and why?

**Priority**: 1st in the invincibility stack (Phase 16 of 30).
**Justification**: Zero new code in the Rust daemon. Zero new binaries. Zero architecture change. The entire phase is compiling Docker images differently and adding a report parser. Highest return for lowest effort. Unlocks Phase 23-26 which need sanitizer data.

**Dependency graph**:
```
Nothing blocks this.
This unblocks: Phase 23 (Trigger Matrix needs sanitizer rows)
               Phase 24 (Differential needs clean execution)
               Phase 25 (Spec Mining needs sanitized environment)
               Phase 26 (Mutation Testing needs sanitizer coverage)
```

---

## A5. What is NOT being built?

- NOT adding sanitizers to the Rust sandbox daemon itself (only to sandbox TARGET images)
- NOT implementing custom sanitizer checks (using standard compiler flags only)
- NOT adding sanitizer-specific agent tools (agents use existing `exec_sandbox` tool)
- NOT changing the sandbox daemon's Rust code (configuration change only)
- NOT creating sanitizer-specific Docker images for every language (Python and C/C++ first, others follow)
- NOT fuzzing integration (that's Phase 20)

---

## B1. Integration point?

**Primary**: `bugswarm-sandbox/src/config.rs:121` — the default image path.
**New files**:
- `bugswarm-sandbox/docker/sandbox-python-asan.Dockerfile` — Python image with sanitizers
- `bugswarm-sandbox/docker/sandbox-cpp-asan.Dockerfile` — C/C++ image with sanitizers
- `bugswarm-sandbox/src/sanitizer_report.rs` — structured report parser (new module)
- `bugswarm-sandbox/src/config.rs` — add `sanitizer_enabled: bool` and `sanitizer_image` fields

**Modified files**:
- `bugswarm-sandbox/src/container.rs:121` — add sanitizer image selection logic
- `bugswarm-sandbox/src/config.rs:308` — add `SanitizerReport` to `ExecutionReceipt`
- `bugswarm-sandbox/src/lib.rs` — add `pub mod sanitizer_report`

---

## B2. Data flow?

```
Agent submits PoC via exec_sandbox(poc_code)
  │
  ├─→ Sandbox daemon receives request
  │     └─ Reads config: sanitizer_enabled = true
  │     └─ Selects image: sandbox-python-asan:latest (not sandbox-python:latest)
  │
  ├─→ Container created from sanitizer image
  │     └─ Container has ASAN_OPTIONS, UBSAN_OPTIONS, TSAN_OPTIONS env vars
  │     └─ PoC copied to container
  │     └─ PoC executed
  │
  ├─→ PoC runs normally → exit code 0, no sanitizer output
  │     └─ Receipt: status=Passed, sanitizer_report=None
  │
  ├─→ PoC triggers memory error → ASAN kills process, writes to stderr
  │     └─ Sandbox captures stderr
  │     └─ SanitizerReportParser extracts structured data:
  │         {sanitizer: "ASAN", error: "heap-buffer-overflow",
  │          address: 0x7f..., size: 256, stack_trace: [...]}
  │     └─ Receipt: status=Failed, exit_code=1,
  │         sanitizer_report={...}, finding_source="sanitizer"
  │
  ├─→ Receipt returned to agent
  │     └─ Agent sees: "ASAN detected heap-buffer-overflow at auth.py:42"
  │     └─ Agent investigates: "What caused this overwrite?"
  │
  └─→ Evidence graph receives finding
        └─ Proof type: sanitizer_report, strength: 0.95
        └─ Trigger matrix row: sanitizer → condition documented
```

---

## B3. New types/schemas?

### Rust: `SanitizerReport` (in `bugswarm-sandbox/src/sanitizer_report.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SanitizerType {
    ASAN,    // AddressSanitizer
    UBSAN,   // UndefinedBehaviorSanitizer
    TSAN,    // ThreadSanitizer
    MSAN,    // MemorySanitizer
    LSAN,    // LeakSanitizer
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizerReport {
    pub sanitizer: SanitizerType,
    pub error_type: String,              // "heap-buffer-overflow", "stack-use-after-return"
    pub error_message: String,           // Human-readable description
    pub address: Option<u64>,            // Faulting memory address
    pub size: Option<u64>,               // Allocation or access size
    pub stack_trace: Vec<SanitizerFrame>,// Where the error occurred
    pub allocation_site: Option<Vec<SanitizerFrame>>, // Where memory was allocated (ASAN)
    pub thread_id: Option<u64>,          // Thread that caused the error (TSAN)
    pub raw_output: String,              // Full sanitizer stderr for audit
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizerFrame {
    pub function: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub module: String,
}
```

### Rust: Config additions (in `bugswarm-sandbox/src/config.rs`)

```rust
// Added to SandboxConfig:
pub sanitizer_enabled: bool,           // default: true
pub sanitizer_image: Option<String>,   // default: None (auto-select based on language)

// Added to ExecutionReceipt:
pub sanitizer_report: Option<SanitizerReport>,
pub finding_source: Option<String>,    // "agent", "sanitizer", "fuzzer", etc.
```

### Python: Evidence graph integration

No new Python types needed. The existing `ExecutionReceipt.from_json()` already handles unknown fields. The `parser.py` in the agent will check for `sanitizer_report` in the receipt and flag accordingly.

---

## B4. Modified modules?

| File | Change | Impact |
|------|--------|--------|
| `bugswarm-sandbox/src/config.rs` | Add `sanitizer_enabled`, `sanitizer_image`, `SanitizerReport` to `ExecutionReceipt`, `finding_source` | Schema change — backward compat via `Option` |
| `bugswarm-sandbox/src/container.rs:65-87` | `ensure_image()` selects sanitizer image when enabled | Logic change — detects language, selects appropriate image |
| `bugswarm-sandbox/src/container.rs:170-175` | `execute()` passes `ASAN_OPTIONS` env vars to container | New feature |
| `bugswarm-sandbox/src/container.rs:282-305` | Log collection parses sanitizer stderr into `SanitizerReport` | New feature |
| `bugswarm-sandbox/src/lib.rs` | Add `pub mod sanitizer_report` | Module registration |
| `bugswarm-sandbox/src/main.rs:164` | Config loading includes sanitizer defaults | Config change |
| `bugswarm-sandbox/Cargo.toml` | Add `regex` dependency (already exists) for report parsing | No change needed |
| `bugswarm-agent/src/agent/core.py:543-558` | `_execute_turn()` checks `sanitizer_report` in receipt and auto-verifies finding | Logic enhancement |
| `bugswarm-swarm/src/swarm/compressor.py:141-174` | Extractive compression includes sanitizer findings in summary | Enhancement |

---

## B5. New dependencies?

| Dependency | Version | Purpose | Justification |
|-----------|---------|---------|---------------|
| None | — | — | All parsing uses existing `regex` crate. Sanitizers are compiler flags, not libraries. |

---

## C1. Core algorithm?

### Selecting the sanitizer image

```
function select_image(config: SandboxConfig, poc_language: str) -> str:
    if not config.sanitizer_enabled:
        return config.image  // vanilla image

    if config.sanitizer_image:
        return config.sanitizer_image  // user override

    // Auto-select based on language
    match poc_language:
        "python" → return "bugswarm/sandbox-python-asan:latest"
        "c" | "cpp" → return "bugswarm/sandbox-cpp-asan:latest"
        default → return config.image  // fallback to vanilla
```

### Parsing ASAN output

```
function parse_asan_stderr(stderr: str) -> SanitizerReport:
    report = SanitizerReport(sanitizer=ASAN)

    // Extract error type from first line
    // "==12345==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x7f..."
    error_match = regex(r"ERROR: AddressSanitizer: (\S+)").match(stderr)
    if error_match:
        report.error_type = error_match[1]

    // Extract address
    // "...on address 0x7f8a4c000800 at pc 0x55d..."
    addr_match = regex(r"on address (0x[0-9a-f]+)").match(stderr)
    if addr_match:
        report.address = parse_hex(addr_match[1])

    // Extract size
    // "...WRITE of size 256 at 0x7f..."
    size_match = regex(r"(READ|WRITE) of size (\d+)").match(stderr)
    if size_match:
        report.size = parse_int(size_match[2])

    // Extract stack trace
    // "    #0 0x55d... in function_name path/to/file.c:42:5"
    for line in stderr.lines():
        frame_match = regex(r"#\d+ .* in (\S+) (\S+):(\d+):(\d+)").match(line)
        if frame_match:
            report.stack_trace.push(SanitizerFrame(
                function: frame_match[1],
                file: frame_match[2],
                line: parse_int(frame_match[3]),
                column: parse_int(frame_match[4]),
            ))

    // Extract allocation site (ASAN-specific)
    // "...allocated by thread T0 here:"
    // "    #0 ... in malloc ..."
    if "allocated by thread" in stderr:
        report.allocation_site = parse_stack_frames(stderr, after="allocated by thread")

    report.raw_output = stderr
    return report
```

**Complexity**: O(N) where N = number of stderr lines. Single pass.

---

## C2. Failure modes?

| Failure | Handling | Recovery |
|---------|----------|----------|
| Sanitizer image not pulled | `ensure_image()` returns Err — agent gets "image unavailable" | Agent retries with vanilla image. Finding flagged as `NO_SANITIZER_COVERAGE`. |
| Sanitizer stderr format changed (new ASAN version) | Parser returns `SanitizerReport` with `error_type="unknown"` and `raw_output` preserved | Raw output available for manual review. Parser updated in next release. |
| Sanitizer overhead exceeds timeout | `execute()` timeout is 3x longer for sanitized runs (360s vs 120s) | If still times out, PoC is re-run without sanitizers. Flagged as `SANITIZER_TIMEOUT`. |
| Sanitizer image not available for language | Fall back to vanilla image. Log warning. | Finding source = "agent" not "sanitizer". |
| Multiple sanitizers report the same bug | Deduplication by stack trace hash | First report kept. Subsequent reports linked as additional evidence. |
| False positive from sanitizer | ASAN false positives are extremely rare but possible with custom allocators | Agent and judge review. If dismissed, recorded as `FALSE_POSITIVE:SANITIZER`. |

---

## C3. Edge cases?

| Edge Case | Behavior |
|-----------|----------|
| PoC with zero code (empty string) | Validated before execution — rejected by `_exec_sandbox`. Never reaches container. |
| PoC that is valid Python but intentionally corrupts memory via ctypes | ASAN detects the corruption in the C library call. Reported normally. |
| PoC that triggers multiple sanitizer violations | All captured. First one that kills the process is the primary report. Others in `raw_output`. |
| PoC in a language without sanitizer support (Go, Java) | Fall back to vanilla image. No sanitizer report in receipt. |
| Sanitizer produces >10MB of output (stress test) | Truncated to first 10KB in receipt. Full output stored on disk. Referenced by path. |
| Concurrent execution: 12 PoCs all trigger sanitizers simultaneously | Each container independent. No shared state. No race condition in report parsing. |
| PoC with shell script that invokes compiled binary | Shell scripts run in plain container. If they invoke a compiled binary, that binary must be in the sanitized image. Out of scope for Phase 16. |

---

## C4. Concurrency?

**Locking strategy**: None needed. Each sandbox execution runs in a separate Docker container. No shared memory between containers. The `SanitizerReportParser` is stateless — instantiated per execution. Thread-safe by construction.

**Race conditions**: None. Container isolation guarantees no cross-execution interference.

**Performance under load**: 12 simultaneous sanitized executions → 12 Docker containers, each with sanitizer overhead. Host must have sufficient memory (ASAN uses ~2x memory per process). If host OOM: cgroups limit kills containers. OOM killer produces `ExecutionStatus::OomKilled` — not a sanitizer false positive.

---

## C5. Performance budget?

| Metric | Target | Measurement |
|--------|--------|-------------|
| Execution overhead (wall-clock) | <3x baseline | Sanitized run time / vanilla run time on same PoC |
| Memory overhead | <3x baseline | Peak RSS of sanitized / vanilla container |
| Report parsing latency | <10ms | `parse_asan_stderr()` time for 10KB stderr |
| Image size increase | <2x | `sandbox-python-asan` vs `sandbox-python` |
| Startup time increase | <2s | Container start with sanitizer env vars |

---

## D1. Unit tests?

| Test Name | What It Tests |
|-----------|---------------|
| `test_parse_asan_heap_overflow` | Full ASAN heap-buffer-overflow report → correct `SanitizerReport` struct |
| `test_parse_asan_stack_use_after_return` | ASAN stack-use-after-return → correct error_type |
| `test_parse_asan_with_allocation_site` | ASAN report with "allocated by thread" section → `allocation_site` populated |
| `test_parse_ubsan_integer_overflow` | UBSAN signed integer overflow → correct sanitizer_type |
| `test_parse_ubsan_null_pointer` | UBSAN null pointer dereference → correct report |
| `test_parse_tsan_data_race` | TSAN data race report → correct sanitizer_type and thread_id |
| `test_parse_lsan_leak` | LSAN leak summary → correct sanitizer_type |
| `test_parse_empty_stderr` | Empty stderr → `None` returned, not a parse error |
| `test_parse_unrecognized_format` | Stderr that doesn't match any known pattern → `error_type="unknown"`, `raw_output` preserved |
| `test_select_image_sanitizer_enabled` | `sanitizer_enabled=true, language=python` → ASAN image selected |
| `test_select_image_sanitizer_disabled` | `sanitizer_enabled=false` → vanilla image selected |
| `test_select_image_user_override` | `sanitizer_image="custom:latest"` → user override used |
| `test_select_image_unsupported_language` | Go, Java → fallback to vanilla image |
| `test_execution_receipt_includes_sanitizer_report` | Mock execution with sanitizer stderr → receipt has `sanitizer_report` field |
| `test_execution_receipt_no_sanitizer_when_disabled` | Vanilla execution → receipt has `sanitizer_report: None` |

**Total: 15 unit tests.**

---

## D2. Integration tests?

| Test Name | Modules Tested | What It Verifies |
|-----------|---------------|-----------------|
| `test_asan_image_pulled_and_used` | ContainerManager + Docker | ASAN image exists and runs |
| `test_sanitizer_finding_flows_to_agent` | Sandbox + Agent | Agent receives `sanitizer_report` in receipt, auto-verifies finding |
| `test_sanitizer_finding_in_evidence_graph` | Sandbox + Evidence Graph | Sanitizer finding stored with `finding_source="sanitizer"` |
| `test_sanitizer_finding_in_trigger_matrix` | Sandbox + Evidence Graph | Trigger matrix populated with sanitizer row |
| `test_vanilla_fallback_when_asan_unavailable` | ContainerManager + Config | ASAN image missing → falls back to vanilla |

**Total: 5 integration tests.**

---

## D3. Gate test?

**Name**: The Sanitizer Gauntlet

**Design**: A Python script with 10 deliberately planted memory/behavior bugs. The script uses `ctypes` to trigger C-level memory errors that are invisible to Python's runtime but caught by ASAN/UBSAN.

```python
# Gate test PoC — triggers 10 sanitizer-detectable bugs
import ctypes, threading

# 1. Heap buffer overflow
buf = (ctypes.c_char * 10)()
ctypes.memset(buf, ord('A'), 20)  # Write 20 bytes to 10-byte buffer

# 2. Use-after-free
ptr = ctypes.c_char_p(ctypes.create_string_buffer(b"test"))
# ... free happens, but ptr still used ...

# 3. Stack buffer overflow
# ... etc for 10 bugs
```

**Pass condition**: All 10 bugs detected by sanitizers. Receipt contains `sanitizer_report` for each. Zero false positives. Zero crashes in the report parser.

**Gate receipt**:
```json
{"phase": 16, "gate": "sanitizer_gauntlet", "passed": 10, "failed": 0, "verdict": "PHASE 16 COMPLETE"}
```

---

## D4. Golden dataset?

**Not applicable** for this phase. Sanitizer instrumentation is a detection capability, not a reasoning capability. It doesn't produce verdicts that can be compared against human adjudication.

However: after Phase 16, re-run the existing extreme-bugs test suite with sanitizers enabled. Compare findings count before/after. The delta is the sanitizer's unique contribution.

---

## D5. Regression test?

**Name**: `test_sanitizer_coverage_regression`

**What**: Runs the Sanitizer Gauntlet (from D3) on every CI push. If any of the 10 planted bugs is NOT detected, the build fails.

**How**: `cargo test --test sanitizer_regression` in the sandbox crate. Uses a mock Docker environment with pre-recorded sanitizer stderr outputs.

---

## E1. Estimated cost?

| Cost Type | Estimate | Assumptions |
|-----------|----------|-------------|
| Development | 6 hours | 1 engineer |
| Docker image build time | 10 min/image × 2 images = 20 min | One-time |
| Docker image storage | ~500 MB additional | ASAN-instrumented Python + C/C++ images |
| Per-execution overhead | 2-3x wall-clock time | ASAN adds memory checks to every memory access |
| Per-execution memory overhead | 2-3x RAM | ASAN uses shadow memory |
| Token cost impact | $0 (no LLM involvement) | Sanitizer runs inside sandbox, not via API |
| Monthly infrastructure | $0 (reuses existing Docker host) | No new services |

---

## E2. Observability?

### Logs

| Level | Message | When |
|-------|---------|------|
| INFO | `sanitizer_image_selected` | Sanitizer image chosen for execution |
| INFO | `sanitizer_bug_detected` | Sanitizer caught a memory error |
| WARN | `sanitizer_image_unavailable` | Fallback to vanilla image |
| WARN | `sanitizer_parse_failed` | Report parsing failed, raw output preserved |
| DEBUG | `sanitizer_env_vars` | ASAN_OPTIONS, UBSAN_OPTIONS passed to container |

### Metrics (Prometheus)

```
bugswarm_sanitizer_executions_total{sanitizer="asan"}       counter
bugswarm_sanitizer_bugs_detected_total{sanitizer="asan"}  counter
bugswarm_sanitizer_parse_failures_total                    counter
bugswarm_sanitizer_fallback_total{reason="image_missing"} counter
```

### Alerts

| Condition | Severity | Channel |
|-----------|----------|---------|
| `sanitizer_parse_failures > 5 in 5 min` | WARN | Slack |
| `sanitizer_image_unavailable for >3 consecutive runs` | WARN | Slack |

### Traces

No new spans needed. Existing `sandbox_execution` span includes `sanitizer_enabled` attribute.

---

## E3. Configuration?

| Parameter | Default | Range | Env Var | CLI Flag |
|-----------|---------|-------|---------|----------|
| `sanitizer_enabled` | `true` | bool | `BGSWARM_SANITIZER_ENABLED` | `--[no-]sanitizer` |
| `sanitizer_image` | `None` (auto) | string or null | `BGSWARM_SANITIZER_IMAGE` | `--sanitizer-image` |
| `ASAN_OPTIONS` | `detect_leaks=1:halt_on_error=1:abort_on_error=1` | string | `BGSWARM_ASAN_OPTIONS` | — |
| `UBSAN_OPTIONS` | `print_stacktrace=1:halt_on_error=1` | string | `BGSWARM_UBSAN_OPTIONS` | — |
| `TSAN_OPTIONS` | `halt_on_error=1:history_size=7` | string | `BGSWARM_TSAN_OPTIONS` | — |

All parameters can be set via environment variable or `sandbox_config.yaml`. CLI flag `--no-sanitizer` disables sanitizers for debugging.

---

## E4. Migration?

**Backward compatibility**: Full. Sanitizer support is additive. Existing receipts gain `sanitizer_report: null` and `finding_source: null` fields. Existing code that doesn't check these fields continues working.

**Data migration**: None. `ExecutionReceipt` fields are additive via `Option`. Old receipts without the fields deserialize correctly.

**Docker image migration**: Build new `sandbox-python-asan:latest` and `sandbox-cpp-asan:latest` images alongside existing images. Existing `sandbox-python:latest` unchanged. Users who want sanitizers pull the new image. Users who don't are unaffected.

**Agent migration**: Agent's `_parse_finding` already handles unknown receipt fields. The auto-verify check for `sanitizer_report` is additive — if the field is missing, it falls through to existing logic.

---

## E5. Documentation?

| Document | Content |
|----------|---------|
| Code docs | Rust doc comments on `SanitizerReport`, `SanitizerReportParser`, new config fields |
| User docs | New section in README: "Sanitizer Instrumentation" — what sanitizers detect, how to enable/disable, performance impact |
| ADR | Architecture Decision Record: "ADR-016: Sanitizer Instrumentation via Compiler Flags" |
| Changelog | `feat: add sanitizer instrumentation (ASAN, UBSAN, TSAN, MSAN, LSAN) to sandbox images` |

---

## Dependency Tree

```
Before: Phase 1 (Sandbox)
This:   Phase 16 (Sanitizer Instrumentation)
After:  Phase 23 (Trigger Matrix — needs sanitizer rows)
        Phase 24 (Differential Analysis — needs clean execution env)
        Phase 25 (Specification Mining — needs sanitized environment)
        Phase 26 (Mutation Testing — needs sanitizer coverage for mutants)
```

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| ASAN image build fails on CI | Low | Medium | Build locally, push to registry. CI only runs gate tests against pre-built image. |
| Sanitizer overhead makes PoCs timeout | Medium | Low | 3x timeout for sanitized runs. Fallback to vanilla on timeout. |
| ASAN report format changes in new clang version | Medium | Medium | Parser is defensive — unknown format → `error_type="unknown"`, raw output preserved. Update parser in next release. |
| False positive from ASAN shuts down agent investigation | Very Low | High | Judge + human review before closing. False positive rate for ASAN is <0.01% in practice. |
| Sanitizer image not available in air-gapped environments | Low | Medium | Vanilla fallback. Sanitizer is additive capability, not required. |

---

## Decision Log

| Decision | Reasoning | Date |
|----------|-----------|------|
| Use compiler flags, not custom sanitizer code | ASAN/UBSAN/TSAN are mature (decade+), well-maintained by LLVM/GCC teams. Zero maintenance burden. | 2026-05-13 |
| Sanitizer images separate from vanilla images | Backward compatibility. Users who don't want sanitizer overhead use vanilla. Users who do pull ASAN image. No regression risk. | 2026-05-13 |
| Parse sanitizer output with regex, not a dedicated library | ASAN output format is stable. Regex is fast, zero-dependency. Dedicated parser library would add maintenance burden for marginal benefit. | 2026-05-13 |
| Sanitizer findings have strength 0.95 (not 1.0) | ASAN has an extremely low false positive rate but not zero (custom allocators, JIT code can confuse it). 0.95 reflects this conservative stance. | 2026-05-13 |
| Python image gets ASAN+UBSAN, C/C++ gets all 5 | Python's memory model makes MSAN/TSAN less useful. C/C++ benefits from all sanitizers. Prioritize what matters per language. | 2026-05-13 |

---

## Review Checklist

- [x] All 25 questions answered
- [x] Dependency tree verified — Phase 1 only, unblocks 4 downstream phases
- [ ] Gate test (Sanitizer Gauntlet) passes at 100%
- [ ] No downstream phase blocked by missing sanitizer data
- [ ] Docker images built and pushed to registry
- [ ] Documentation updated (code docs, user docs, ADR, changelog)
- [ ] 15 unit tests passing
- [ ] 5 integration tests passing

---

## Gate Receipt

```json
{
  "phase": 16,
  "name": "Sanitizer Instrumentation",
  "gate": "sanitizer_gauntlet",
  "timestamp": "TBD",
  "status": "PENDING",
  "total_tests": 10,
  "passed": 0,
  "failed": 0,
  "verdict": "PHASE 16 NOT YET EXECUTED"
}
```

---

## Changelog

| Date | Author | Change |
|------|--------|--------|
| 2026-05-13 | System | Initial plan created from `plan_standard.md` template |
