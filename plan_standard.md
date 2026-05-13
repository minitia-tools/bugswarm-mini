# Enterprise Plan Standard — The 25 Questions Every Phase Document Must Answer

A plan is not complete until it answers all 25. If a question has no answer, the plan has a blind spot. Blind spots in plans become bugs in production.

---

## Section A: Identity & Purpose (5 questions)

### A1. What exactly is being built?
One sentence. One paragraph. No ambiguity.

**Bad**: "Improve the sandbox."
**Good**: "Compile sandbox Docker images with AddressSanitizer, UndefinedBehaviorSanitizer, and ThreadSanitizer instrumentation so every PoC execution automatically detects memory corruption, undefined behavior, and data races."

### A2. Which specific gap does it fill?
Reference the exact gap from the gap matrix. Link to the document where the gap was identified.

**Required**: Gap ID, gap description, current behavior, target behavior.

### A3. What is the success criteria?
Measurable. Binary. Not "better" — "metric X must be ≥ value Y."

**Bad**: "Find more bugs."
**Good**: "Sanitizer-enabled executions must detect ≥1 previously-undetected memory bug per 100 PoCs submitted. Zero false positives from sanitizer reports (100% precision)."

### A4. What is the priority and why?
Justify its position in the build order with dependency analysis.

**Required**: Dependency graph showing what blocks this and what this unblocks.

### A5. What is NOT being built?
Scope boundary. Explicitly list what is out of scope to prevent scope creep.

---

## Section B: Architecture (5 questions)

### B1. Where does it plug into the existing system?
Exact phase, exact module, exact file path. Not "the sandbox" — `bugswarm-sandbox/src/container.rs:execute()`.

### B2. What is the data flow?
Input → Processing → Output. Diagram or step-by-step.

```
Input: PoC script submitted by agent
  → Sandbox receives PoC
  → Container created with sanitizer-enabled binary
  → PoC executed
  → Sanitizer detects violation → produces report
  → Sandbox captures report as structured evidence
  → Evidence graph receives Finding with FindingSource::Sanitizer
Output: ExecutionReceipt with sanitizer_report field
```

### B3. What new types, schemas, or data structures are needed?
Every new struct, enum, class, table, collection. With field descriptions.

```rust
struct SanitizerReport {
    sanitizer_type: Sanitizer,       // ASAN, UBSAN, TSAN, MSAN, LSAN
    error_type: String,              // "heap-buffer-overflow"
    address: u64,                    // Memory address
    size: u64,                       // Allocation/access size
    stack_trace: Vec<StackFrame>,    // Where it happened
    allocation_site: Option<StackFrame>,  // Where memory was allocated (ASAN)
    thread_id: Option<u64>,          // Which thread (TSAN)
}
```

### B4. What existing modules are modified and how?
Every file touched. Every function signature changed. Every API affected.

### B5. What new dependencies are introduced?
Every crate, package, library, binary, or service. With version and justification.

---

## Section C: Algorithm & Logic (5 questions)

### C1. What is the core algorithm?
Pseudocode or step-by-step. Edge cases handled. Complexity stated.

### C2. What are the failure modes?
Every way this can fail. What happens when it fails. How it recovers.

```
Failure Mode 1: ASAN binary not available for target language
  → Fall back to non-instrumented execution. Log warning. Continue.

Failure Mode 2: Sanitizer report parsing fails (unexpected format)
  → Capture raw output. Flag as TAINTED. Retry with different sanitizer version.

Failure Mode 3: Sanitizer overhead makes PoC too slow (>120s timeout)
  → Increase timeout for sanitized runs. Flag if consistently over budget.
```

### C3. What are the edge cases?
Empty inputs. Maximum inputs. Boundary conditions. Concurrency scenarios.

### C4. How does it interact with concurrent execution?
12 agents submitting PoCs simultaneously. 50 agents. Locking strategy. Race conditions.

### C5. What is the performance budget?
CPU, memory, latency, throughput. With p50/p95/p99 targets.

---

## Section D: Verification — AGGRESSIVE TESTING MANDATORY

**Rule**: Every component must be attacked with inputs explicitly engineered to break it. If a test doesn't try to BREAK the implementation, it's not a test. Per AGENTS.md invariant #11: "Each component must be tested until it breaks or is proven unbreakable."

### D1. What are the aggressive unit tests?
Every function. Every error path. Every edge case. Every boundary condition. Listed by test name.

Each test must include:
- **Happy path**: Expected input → expected output
- **Null/empty/boundary**: null, zero, empty string, max value, min value
- **Malformed**: Invalid JSON, wrong types, missing fields, corrupted data
- **Concurrent**: Race conditions, simultaneous access, deadlock scenarios
- **Resource exhaustion**: Memory limits, timeout, disk full, FD exhaustion
- **Deliberate attack**: Input specifically crafted to break the parser/handler/algorithm

**Format per test**:
```
test_name: "test_parse_asan_corrupted_input"
attack_vector: "Stderr with null bytes, invalid UTF-8, and truncated ASAN output"
expected_behavior: "Parser returns error_type='unknown', raw_output preserved. No panic."
```

### D2. What are the aggressive integration tests?
Every module × module interaction under load, under failure, under attack.

Each integration test must include:
- **Happy path**: Module A calls Module B with valid data → correct response
- **Module B down**: Module B returns error/unreachable → Module A handles gracefully
- **Module B slow**: Module B takes 10x normal time → Module A times out cleanly
- **Concurrent storm**: 50 simultaneous calls from Module A to Module B → no corruption
- **Data corruption**: Module B returns malformed data → Module A detects and rejects

### D3. What is the extreme gate test?
**MANDATORY**: This test is explicitly designed to BREAK the implementation. It must exercise every failure mode, every edge case, every boundary condition simultaneously. The phase is NOT complete until this test passes at 100% with zero failures, zero warnings, and zero unexplained anomalies.

The gate test is NOT a verification test. It is an ATTACK on the implementation. Write it as if you are trying to prove the implementation wrong.

**Gate test requirements**:
- Must test ≥10 distinct attack vectors
- Each attack vector must have a documented expected behavior
- Must run in <5 minutes (for fast feedback)
- Must produce a JSON gate receipt with pass/fail per attack vector
- Must check host integrity after all attacks (no side effects, no resource leaks)

### D4. How is it verified against the golden dataset?
If applicable: 200 pre-adjudicated bugs. Before/after comparison.

### D5. What is the regression test?
After this is built, what test runs forever to ensure it never breaks?

---

## Section E: Operations (5 questions)

### E1. What is the estimated cost?
Token cost. Infrastructure cost. Time cost. Per-run and per-month.

### E2. What are the observability requirements?
Logs (level, format). Metrics (name, type, labels). Alerts (condition, channel). Traces (spans).

### E3. What is the configuration surface?
Every configurable parameter. Default value. Valid range. Environment variable. CLI flag.

### E4. What is the migration path?
How existing users/data/state are migrated. Backward compatibility guarantees.

### E5. What documentation is produced?
Code docs. User docs. Architecture Decision Record. Changelog entry.

---

## The Meta-Standard — What Every Plan Document Itself Must Have

| Requirement | Description |
|-------------|-------------|
| **Phase number** | Sequential. Unique. Never reused. |
| **One-sentence summary** | At the very top. Anyone reading should understand what this is in 5 seconds. |
| **Status badge** | NOT_STARTED / IN_PROGRESS / COMPLETE / DEFERRED |
| **Dependency tree** | What must be done before this. What this unblocks. |
| **Estimated effort** | In hours. Broken down by subtask. |
| **Risk assessment** | What could go wrong. Probability. Mitigation. |
| **Decision log** | Every non-obvious choice documented with reasoning. |
| **Review checklist** | What the reviewer must verify before marking complete. |
| **Gate receipt** | JSON with: phase number, gate name, timestamp, passed count, failed count, verdict. |
| **Changelog** | Every edit to this plan document tracked with date and author. |

---

## Template — Copy This for Every New Phase Plan

```markdown
# Phase N: [Name] — [One-Sentence Summary]

**Status**: NOT_STARTED | IN_PROGRESS | COMPLETE | DEFERRED
**Estimated Effort**: X hours
**Depends On**: Phase A, Phase B
**Unblocks**: Phase C, Phase D

---

## A1. What is being built?
[One sentence.]

## A2. Which gap does it fill?
[Gap ID from gap matrix.]

## A3. Success criteria?
[Measurable. Binary.]

## A4. Why this priority?
[Dependency justification.]

## A5. What is out of scope?
[Scope boundary.]

---

## B1. Integration point?
[Exact file:line.]

## B2. Data flow?
[Input → Process → Output.]

## B3. New types/schemas?
[Every struct/enum/class/table.]

## B4. Modified modules?
[Every file touched. Every API change.]

## B5. New dependencies?
[Every crate/package/library with version.]

---

## C1. Core algorithm?
[Pseudocode. Complexity.]

## C2. Failure modes?
[Every failure. Handling. Recovery.]

## C3. Edge cases?
[Empty. Max. Boundary. Concurrent.]

## C4. Concurrency?
[Locking. Race conditions. Thread safety.]

## C5. Performance budget?
[CPU. Memory. Latency. Throughput.]

---

## D1. Unit tests?
[List by name.]

## D2. Integration tests?
[Module × Module interaction.]

## D3. Gate test?
[Designed to break this. Must pass 100%.]

## D4. Golden dataset?
[Before/after comparison.]

## D5. Regression test?
[Forever test.]

---

## E1. Estimated cost?
[Tokens. Infra. Time.]

## E2. Observability?
[Logs. Metrics. Alerts. Traces.]

## E3. Configuration?
[Every parameter. Default. Range. Env var. Flag.]

## E4. Migration?
[Backward compat. Data migration.]

## E5. Documentation?
[Code docs. User docs. ADR. Changelog.]

---

## Dependency Tree
```
Before: [list]
This: Phase N
After: [list]
```

## Risk Assessment
| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| ... | ... | ... | ... |

## Decision Log
| Decision | Reasoning | Date |
|----------|-----------|------|
| ... | ... | ... |

## Review Checklist
- [ ] All 25 questions answered
- [ ] Aggressive testing mandate met — D1 unit tests include attack vectors, D2 integration tests include failure modes, D3 gate test designed to BREAK implementation
- [ ] Every component individually stress-tested (parser, handler, algorithm, config — each with null/empty/corrupt/concurrent/maximum inputs)
- [ ] Dependency tree verified
- [ ] Gate test passes at 100% — zero failures, zero warnings, zero anomalies
- [ ] No downstream phase blocked
- [ ] Documentation updated

## Gate Receipt
```json
{"phase": N, "gate": "...", "passed": X, "failed": 0, "verdict": "PHASE N COMPLETE"}
```

## Changelog
| Date | Author | Change |
|------|--------|--------|
| ... | ... | ... |
```
