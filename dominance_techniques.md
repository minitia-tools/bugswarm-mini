# Bug Swarm Dominance Techniques — Complete Architecture

## The 15-Layer Invincibility Stack

Each layer plugs into an existing phase. No rewrites. Each layer finds what the previous can't. Combined: zero blind spots.

---

## LAYER 1: Real Data Flow Analysis

**Phase**: CPG (Phase 2)

**What it finds**: Deep taint through variables, functions, fields, and collections. The current system does surface substring matching on AST nodes. It finds `os.system(user_input)` but misses:

```python
x = request.form.get('cmd')     # Source
y = normalize(x)                 # Transform
z = build_command(y)             # Wrap
os.system(z)                     # Sink — MISSED because 'z' is not literally 'request'
```

**How it works**: Build a Static Single Assignment (SSA) pass over the AST. Every variable assignment gets a unique version number. Taint propagates through version chains:

```
Taint Rules:
1. Source → variable:        x₁ = source()          → x₁ is tainted
2. Assignment:                y₁ = x₁                → y₁ is tainted
3. Function call:             foo(x₁)                → foo's param₁ is tainted
4. Return value:              return x₁              → caller's receiving var is tainted
5. Field access:              obj.field = x₁         → obj.field is tainted on read
6. Collection:                list.append(x₁)        → list[i] is tainted for all i
7. Sanitizer:                 y₁ = html.escape(x₁)   → y₁ is clean
```

**Implementation**:
- Add `DataFlowGraph` to CPG as a separate graph layer alongside the call graph
- Each node is `(variable_name, version_number, taint_status)`
- Edges represent assignments, calls, returns, field accesses
- BFS from sources propagates taint until reaching sinks or sanitizers
- Integrate with existing `bfs_taint()` in `graph.rs`

**Tools**: tree-sitter (already used), custom SSA transform (new).

**Output**: Taint paths with `confidence: 0.95` (static certainty) vs current `confidence: 0.5` (substring guess).

---

## LAYER 2: Symbolic Execution

**Phase**: CPG (Phase 2) + Sandbox (Phase 1)

**What it finds**: The EXACT input value that reaches a buggy code path. No guessing.

```
Bug location: if (x > 1000 && y == "admin" && z.startswith("DROP"))
Symbolic solver produces: x = 1001, y = "admin", z = "DROP TABLE users"
Sandbox confirms: CRASH at line 42
```

**How it works**: Instrument the code path from entry point to suspected bug. Collect path constraints as symbolic expressions. When the path reaches the buggy line, the accumulated constraints form a system of equations. The SMT solver finds a satisfying assignment — that's the input.

```
Path constraints collected:
  C1: x > 1000
  C2: y == "admin"
  C3: z.startswith("DROP")

SMT Solver (Z3) solves:
  x = 1001  (any value > 1000)
  y = "admin"
  z = "DROP" + any_suffix

Concrete solution: {x: 1001, y: "admin", z: "DROP TABLE"}
```

**Integration**:
- CPG identifies branches near sinks
- Symbolic engine (new Rust binary: `bugswarm-symbolic`) instruments those branches
- Z3 SMT solver finds satisfying assignments
- Sandbox executes the solved input
- Result → evidence graph as `InputSource::Symbolic` (higher confidence)

**Tools**: Z3 SMT solver, angr (binary-level) or custom Python concolic engine.

**Hard metric**: Symbolically-generated inputs must have >90% bug reproduction rate vs <40% for agent-guessed inputs.

---

## LAYER 3: Coverage-Guided Fuzzing

**Phase**: Sandbox (Phase 1)

**What it finds**: Crashes at 1000s of inputs per second. The agent writes one PoC per turn. The fuzzer generates thousands.

**How it works**: Loop inside the sandbox:
```
1. Take seed input (from agent PoC or random)
2. Mutate (bit flip, byte insert, arithmetic change, dictionary substitution)
3. Execute target with mutated input
4. If new code path discovered (coverage gain) → keep input, mutate further
5. If crash → triage, deduplicate, inject into evidence graph
6. Repeat 1000x/second
```

**Integration**: The sandbox daemon gains a fuzzing loop mode. Agent submits: "fuzz this function with these seeds." Sandbox runs AFL++/libFuzzer inside the container. Any crash → auto-finding in evidence graph with `FindingSource::Fuzzer`.

**Steering**: Combined with Layer 13 (taint-guided) for 10x efficiency.

**Tools**: AFL++, libFuzzer, Honggfuzz. Compiled into sandbox container images.

**Hard metric**: Fuzzer must produce at least 1 unique crash per 10,000 executions for high-complexity repos.

---

## LAYER 4: Differential Analysis

**Phase**: Sandbox (Phase 1) + CPG (Phase 2)

**What it finds**: Behavioral changes between versions, deviations from reference implementations, inconsistency bugs.

**Four modes**:

1. **Version-to-version**: Run same inputs against current code and previous release. Any output difference → potential regression.

2. **Reference comparison**: Compare target function against known-good implementation of same algorithm. Deviations → potential bugs.

3. **Input-pair differential**: Run function with two inputs that SHOULD produce the same output. Different outputs → inconsistency.

4. **Oracle differential**: Run against a different implementation of the same spec. Different outputs → at least one is buggy.

**Integration**: Sandbox runs pairs of executions. Differential engine compares outputs byte-by-byte. Differences flagged as findings with `FindingSource::Differential`. Agents investigate and explain the divergence.

**Hard metric**: Differential mode catches 100% of semantic regressions that pass existing test suites.

---

## LAYER 5: Runtime Sanitizer Instrumentation

**Phase**: Sandbox (Phase 1)

**What it finds**: Bugs that produce NO crash and NO stack trace. Memory corruption, undefined behavior, data races — completely silent in normal execution.

**Sanitizers compiled into sandbox images**:

| Sanitizer | Detects | Without It |
|-----------|---------|------------|
| AddressSanitizer (ASAN) | Buffer overflow, heap use-after-free, stack use-after-return | Runs normally, corrupts memory silently |
| UndefinedBehaviorSanitizer (UBSAN) | Integer overflow, null deref, misaligned access, signed overflow | Wraps silently, produces wrong results |
| ThreadSanitizer (TSAN) | Data races, mutex misuses, lock order violations | Race happens silently, corrupts data non-deterministically |
| MemorySanitizer (MSAN) | Reads of uninitialized memory | Reads garbage values, produces wrong results |
| LeakSanitizer (LSAN) | Memory leaks | Memory grows until OOM, no crash attribution |

**Integration**: Sandbox Docker images built with `-fsanitize=address,undefined,thread`. Every PoC execution automatically gets sanitizer coverage. Sanitizer report captured as structured evidence. Agents don't need to guess — the sanitizer tells them exactly what happened.

**Hard metric**: 100% of sandbox executions run with at minimum ASAN+UBSAN. TSAN for concurrency tests.

---

## LAYER 6: Persistent Learning & Bug Pattern Database

**Phase**: Assessment (Phase 15) + CPG (Phase 2)

**What it finds**: Bugs that resemble previously confirmed bugs. Prioritizes investigation based on statistical likelihood.

**Components**:

1. **Bug Pattern Vector DB**: Every confirmed finding embedded. Code snippet + CWE + language + severity + fix. New repo → CPG matched against this DB. High-similarity regions → priority investigation.

2. **CVE Corpus with Graph Isomorphism**: CPG built from CVE-patched code. Graph edit distance between target code and known-vulnerable patterns. Subgraph isomorphism with edit distance < threshold → high-priority.

3. **Agent Performance History**: Per persona, per model, per repo type: bugs found, false positive rate, tokens/verified. Feeds allocation algorithm.

4. **File Hotspot Tracking**: Files that produced bugs in previous runs get `hotspot_score`. Assessment phase reads and prioritizes.

**Integration**: New Vector DB collection in ChromaDB. Embeddings from CPG function nodes. Cosine similarity retrieval during indexing. Hotspot scores persisted to `~/.bugswarm/hotspots.yaml`.

**Hard metric**: Pattern DB must improve `bugs_found / tokens_consumed` by 30%+ within 10 runs on the same repo.

---

## LAYER 7: Vulnerability Chaining

**Phase**: Evidence Graph (Phase 5)

**What it finds**: Multi-step exploit paths. Individual low-severity bugs that combine into critical attacks.

```
Bug A (severity 3): Info leak via out-of-bounds read → reveals ASLR base address
Bug B (severity 4): Stack buffer overflow → overwrites return pointer
Bug C (severity 10): Arbitrary code execution via chained A+B

Without chaining: 2 low-severity bugs, dismissed.
With chaining: 1 critical RCE, confirmed.
```

**New edge type**: `EdgeKind::Enables` — "Bug A enables Bug B" when A's output (leaked address, corrupted state) is an input or precondition for B.

**Chain detection**: BFS through evidence graph following `Enables` edges. If a path exists from low-severity to critical, flag the entire chain. Severity of chain = max(severity of all members).

**Integration**: New evidence graph query: `find_chains()`. Returns list of exploit chains. Each chain gets a combined finding with all member bugs linked. Bench reviews chains as a single finding.

**Hard metric**: Chain detection must identify 100% of theoretically chainable bugs within 10 hops.

---

## LAYER 8: Specification Mining + Invariant Detection

**Phase**: Sandbox (Phase 1). Plugs into `execute_statistical`.

**What it finds**: Silent data corruption. Bugs with NO crash. Functions that return wrong values. Invariant violations that no tool catches.

**How it works**:

```
1. Property-based input generator produces 10,000 inputs
2. Sandbox runs target function with each input
3. Output recorder captures return values, exceptions, side effects
4. Invariant miner analyzes patterns:
   - "balance is always >= 0 after withdraw()"
   - "authenticate() always returns bool, never None"
   - "parse_json() raises ValueError on invalid input, never TypeError"
   - "sorted(list) is always non-descending"
5. Any violation → BUG FOUND with exact input that triggered it
```

**Specification types mined**:

| Invariant Type | Example | Tool |
|---------------|---------|------|
| Range invariants | `return_value >= 0` | Hypothesis + custom |
| Type invariants | `return_value is not None` | Python type annotations |
| Ordering invariants | `output is sorted` | Daikon |
| Relationship invariants | `output.length == input.length` | Custom |
| Exception invariants | `raises ValueError for negative input` | Hypothesis |
| State invariants | `balance_before - amount == balance_after` | Custom |

**Integration**: New sandbox command: `invariant-check --function withdraw --inputs 10000`. Sandbox daemon runs the loop. Violations auto-injected into evidence graph as `FindingSource::InvariantViolation`.

**Tools**: Hypothesis (Python property-based testing), Daikon (invariant detector), custom output recorder.

**Hard metric**: Must find at least 1 unique bug per 100 functions tested that no other layer would find.

---

## LAYER 9: Mutation Testing as Bug Oracle

**Phase**: CPG (Phase 2) + Sandbox (Phase 1). Plugs into `execute_statistical`.

**What it finds**: Bugs-in-waiting. Code that works today but has zero test coverage at its boundary. A one-character change would break it.

**How it works**:

```
1. CPG identifies mutation targets: comparison operators, arithmetic, conditionals
2. Mutation engine generates mutants:
   - if (x >= 0)  →  if (x > 0)       (off-by-one)
   - if (x == y)  →  if (x != y)       (inverted condition)
   - return a + b →  return a - b       (wrong operator)
   - x = obj.method()  →  x = None      (null return)
3. Sandbox runs test suite against each mutant
4. If ALL tests pass → mutant SURVIVED → untested boundary FLAGGED
5. Agent investigates: "What would happen in production if this changed?"
```

**Survivor analysis**: For each surviving mutant, the system records:
- The mutation applied (operator changed, value changed)
- The mutated location (file:line)
- Which tests ran (coverage data)
- Why the mutant survived (no test covered this condition)

**Integration**: New sandbox command: `mutate --function login --operators comparison,arithmetic`. Sandbox runs test suite. Survivors → evidence graph as `FindingSource::MutationSurvivor`. Agents investigate highest-severity survivors.

**Tools**: mutmut (Python), Stryker (JS), PIT (Java). Integrated into sandbox images.

**Hard metric**: Zero false positives on mutation survivors — every survivor must represent a genuinely untested code path.

---

## LAYER 10: Concolic Execution (Concrete + Symbolic)

**Phase**: CPG (Phase 2) + Sandbox (Phase 1). New binary: `bugswarm-concolic`.

**What it finds**: Systematic path exploration around every suspected bug. The EXACT condition for every branch.

**How it works**:

```
Step 1: Run with concrete input
  Input: x = 1, y = "test"
  Path taken: if (x > 0) → true branch → if (y == "admin") → false branch → exit
  Constraints collected: [x > 0, y != "admin"]

Step 2: Negate last constraint
  Negate: y == "admin" (instead of y != "admin")
  Solve: x = 1, y = "admin"
  Execute with new input
  New path: if (x > 0) → true → if (y == "admin") → TRUE → reached sink!

Step 3: Negate earlier constraint
  Negate: x <= 0 (instead of x > 0)
  Solve: x = 0, y = "test"
  New path: if (x > 0) → false branch → exit

All 3 paths explored. Sink reached with precise input.
```

**Why better than pure symbolic**: Pure symbolic execution hits path explosion (2^N branches for N conditions). Concolic explores one path at a time, systematically, without exponential blowup.

**Integration**: New Rust binary `bugswarm-concolic`. Takes a function + target line. Instruments via LLVM pass or Python AST transform. Generates inputs. Feeds to sandbox. Returns reachability verdict.

**Hard metric**: Must reach 95%+ of branches within 100 solver queries for functions under 200 lines.

---

## LAYER 11: Bug Probability Prediction (ML)

**Phase**: Assessment (Phase 15). Feeds the difficulty assessment and allocation algorithm.

**What it finds**: Nothing directly. It TELLS the system where to look. Saves 80% of token budget by prioritizing high-probability code.

**Features extracted per function from CPG + git**:

```
Cyclomatic complexity:        12        (if/for/while count)
Nesting depth:                4         (max indentation)
Parameter count:              5         (inputs to validate)
Lines of code:                87        (attack surface size)
Taint path density:           0.6       (sources → sinks ratio)
CVE pattern similarity:       0.8       (max cosine to known CVE)
Author commit frequency:      3/week   (active development = more bugs)
File churn rate:              45%       (% of file changed recently)
Historical bug density:       0.03      (bugs per line in past runs)
Language complexity score:    0.7       (C++ > Rust > Go > Python)
```

**Model**: XGBoost or RandomForest. Trained on every confirmed bug. Updated after every run. Features extracted automatically by CPG + git log.

**Output per function**: `{"function": "auth.py:login()", "bug_probability": 0.94, "top_reason": "taint_density=0.8"}`

**Integration**: Assessment module calls `BugProbabilityModel.predict_all(repo_path)`. Ranking feeds scout agent's investigation order. SwarmAllocation distributes agents toward top-N functions.

**Hard metric**: Top-10% probability functions must contain >70% of confirmed bugs.

---

## LAYER 12: Fix-Induced Bug Prediction

**Phase**: Evidence Graph (Phase 5) + Bench (Phase 10). Fires when a judge confirms a bug.

**What it finds**: New bugs that would be introduced by the proposed fix. Prevents the most dangerous action in software: fixing a bug.

**How it works**:

```
1. Agent proposes fix: "Change if (x) to if (x is not None) at auth.py:42"
2. CPG traces ALL callers of the changed function (14 callers found)
3. For each caller:
   a. What values do they pass to the changed function?
   b. Does the fix change behavior for those values?
   c. If yes → FLAGGED
4. Probability computed: (# affected callers) / (# total callers)
5. Severity of predicted bugs: based on affected caller's role
```

**Analysis output**:

```
Fix: auth.py:42  if (x) → if (x is not None)

Caller analysis:
  ✓ api.py:30     passes x=User object    → no impact
  ✓ api.py:55     passes x=Session dict   → no impact
  ✗ handler.py:12 passes x=0 (int)        → WAS falsy, now truthy → BROKEN
  ✗ handler.py:89 passes x="" (str)       → WAS falsy, now truthy → BROKEN
  ⚠ middleware:45 passes x=None            → correctly handled

Impact: 2/14 callers broken. New null path opened at handler.py:12.
Probability: 23% chance of introducing 1-2 new bugs.
Recommendation: Add explicit check for falsy values before merging.
```

**Integration**: When `confirm_bug()` is called on the evidence graph AND a fix proposal exists, auto-trigger FixImpact analysis. Result attached to the finding. Judges review before accepting.

**Hard metric**: Fix-induced predictions must have <10% false positive rate (predicted breakage that doesn't actually break).

---

## LAYER 13: Coverage-Guided Greybox Fuzzing + Taint Steering

**Phase**: Sandbox (Phase 1) + CPG (Phase 2). Enhancement to Layer 3.

**What it finds**: Same as Layer 3 (crashes at scale), but 10x more efficient on dangerous code.

**How it works**: Standard fuzzer explores uniformly. This one is steered by CPG taint analysis:

```
CPG exports a "danger map": list of (code_address, danger_score)
  auth.py:42 → danger 0.9  (SQL execute sink)
  auth.py:47 → danger 0.3  (string format)
  auth.py:50 → danger 0.0  (return statement)

Fuzzer mutates inputs. After each execution:
  - Check which code addresses were reached
  - Compute reward: sum(danger_score for all reached addresses)
  - High reward inputs → keep and mutate 10x more
  - Low reward inputs → discard faster

Result: Fuzzer spends 90% of CPU on the 5% of code near sinks.
```

**Integration**: CPG daemon serves `/danger_map` endpoint. Fuzzer queries it at startup. Custom AFL++ mutator reads danger scores from shared memory. Same architecture as Layer 3, just with smarter input selection.

**Hard metric**: Taint-steered fuzzing must reach sinks 10x faster than uniform fuzzing on the same codebase.

---

## LAYER 14: Trigger Matrix

**Phase**: Evidence Graph (Phase 5). New node type attached to every confirmed bug.

**What it finds**: Nothing new — it DOCUMENTS everything. Makes every bug 100% reproducible.

**How it works**: Every layer that interacts with a bug contributes rows:

```
Bug: NPE at auth.py:42

Rows contributed by:
  Agent:        Input = null username → crash ✓
  Fuzzer:       Input = "" (empty string) → crash ✓
  Fuzzer:       Input = "a" * 1000000 → no crash (memory OK)
  Concolic:     Input = any value where user row missing → crash
  Sanitizer:    ASAN: heap-buffer-overflow at auth.py:42, size 0
  Differential: Behavior unchanged from v1.2 → not a regression
  Sandbox:      Reproduced in 12/100 runs → 12% flaky
  Fuzzer:       Triggered 47 times across 1M inputs → 0.0047% rate
```

**Integration**: Evidence graph node type: `TriggerMatrix`. Every sandbox execution, fuzzer crash, agent analysis appends a row. Matrix fills over the bug's lifetime. Judges require minimum completeness before closing.

**Hard metric**: Every confirmed bug must have at minimum 5 trigger matrix rows from at least 3 different layers before being marked closed.

---

## LAYER 15: Delta Debugging

**Phase**: Sandbox (Phase 1). Post-processing step after crash detection.

**What it finds**: The MINIMAL reproduction of any crash. Reduces 500-byte inputs to the 3 essential bytes.

**How it works**: ddmin algorithm:

```
Input: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\x00\xff\x00GET / HTTP..."
       ↓
Step 1: Split into two halves
       "aaaaaaaaaaaaaaaaaaaa" + "aaaaaaaa\x00\xff\x00GET / HTTP..."
       ↓ Test each half
       First half: no crash → discard
       Second half: crash → keep
       ↓
Step 2: Split second half
       "aaaaaaaa" + "\x00\xff\x00GET / HTTP..."
       ↓
       "aaaaaaaa": no crash → discard
       "\x00\xff\x00GET / HTTP...": crash → keep
       ↓
Step 3: Continue until no smaller subset crashes
       ↓
Final: "\x00\xff\x00" → CRASH (3 bytes)
       ↑↑↑ The 3 bytes that actually matter
```

**Integration**: Sandbox daemon post-processing. After any crash, the crash input is automatically fed to the delta debugger. The minimized input is stored alongside the bug in the evidence graph's Trigger Matrix. The agent receives the minimized version: "Your 500-byte input crashes. The essential 3 bytes are \x00\xff. Investigate null byte handling."

**Tools**: python-afl (ddmin reference), custom implementation for string/binary inputs.

**Hard metric**: Delta debugging must reduce inputs by >90% within 100 iterations for string/binary inputs under 10KB.

---

## Integration Map — Every Layer Plugs Into An Existing Phase

```
Phase 1: Sandbox
  ← Layer 3: Fuzzing (AFL++ in container)
  ← Layer 4: Differential (pair execution + comparison)
  ← Layer 5: Sanitizers (ASAN/UBSAN/TSAN compiled in)
  ← Layer 8: Specification Mining (property-based input + invariant detection)
  ← Layer 9: Mutation Testing (mutant generation + test execution)
  ← Layer 13: Taint-Guided Fuzzing (danger map from CPG)
  ← Layer 15: Delta Debugging (post-crash minimization)

Phase 2: CPG
  ← Layer 1: Data Flow Analysis (SSA + use-def chains)
  ← Layer 2: Symbolic Execution (path constraint collection)
  ← Layer 6: Pattern Database (embedding + similarity search)
  ← Layer 10: Concolic Execution (concrete + symbolic path exploration)

Phase 5: Evidence Graph
  ← Layer 7: Vulnerability Chaining (Enables edges + BFS chain detection)
  ← Layer 12: Fix-Induced Prediction (caller impact analysis)
  ← Layer 14: Trigger Matrix (multi-source trigger documentation)

Phase 10: Bench
  ← Layer 12: Fix-Induced Prediction (judge reviews fix impact before accepting)

Phase 15: Assessment
  ← Layer 6: Pattern Database (hotspot scoring)
  ← Layer 11: Bug Probability ML (function scoring + investigation priority)
```

---

## Execution Pipeline — Full Run

```
bugswarm run ./repo

  Phase 0: Provider Discovery (~2s)
  Phase 1: Difficulty Assessment (~15s)
    ← Layer 6: reads hotspot scores
    ← Layer 11: reads bug probability rankings

  Phase 2: Allocation (~1ms)
    ← Agent count, model split, personas, rounds computed

  Phase 3: Swarm Execution
    Agent 1: queries CPG
      ← Layer 1: Data flow taint paths returned
      ← Layer 6: Pattern DB suggests similar bugs
    Agent 1: reads file, finds suspicious code
    Agent 1: submits PoC to sandbox
      ← Layer 5: Sanitizer catches UAF → finding
    Agent 2: submits PoC
      ← Layer 8: Invariant violation detected → finding
      ← Layer 9: Mutation survivor flagged → finding
    Agent 3: requests concolic exploration
      ← Layer 10: Generates precise input → finding
    Fuzzer (background):
      ← Layer 3: 1000s of inputs → 3 unique crashes
      ← Layer 13: Taint-steered → hits SQL sink 10x faster
    Post-crash:
      ← Layer 15: Delta debugged to minimal input for each crash

  Phase 4: Evidence Graph Assembly
    ← Layer 7: Chains detected between findings
    ← Layer 14: Trigger matrix populated from all sources

  Phase 5: Bench Adjudication
    Judge reviews finding → confirms
    Agent proposes fix → Layer 12 predicts fix impact
    Judge reviews fix impact → accepts or requests revision

  Phase 6: Report
    All findings with trigger matrices
    All chains with combined severities
    All fix predictions with caller impact analysis
```

---

## Hard Metrics Summary

| Layer | Metric | Target |
|-------|--------|--------|
| 1 | Data Flow | Taint paths with confidence ≥ 0.95 |
| 2 | Symbolic | >90% bug reproduction rate |
| 3 | Fuzzing | ≥1 unique crash per 10K inputs |
| 4 | Differential | 100% semantic regression catch rate |
| 5 | Sanitizers | 100% of executions run with ASAN+UBSAN |
| 6 | Pattern DB | 30%+ token efficiency improvement within 10 runs |
| 7 | Chaining | 100% chainable bugs detected within 10 hops |
| 8 | Spec Mining | ≥1 unique bug per 100 functions tested |
| 9 | Mutation | Zero false positives on survivors |
| 10 | Concolic | 95%+ branch coverage within 100 solver queries |
| 11 | ML Probability | Top-10% functions contain >70% of bugs |
| 12 | Fix Prediction | <10% false positive rate |
| 13 | Taint Fuzzing | 10x faster sink reach than uniform |
| 14 | Trigger Matrix | ≥5 rows from ≥3 layers per bug |
| 15 | Delta Debug | >90% input reduction within 100 iterations |
