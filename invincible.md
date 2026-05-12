# The 7 Missing Layers — What Makes Bug Swarm Truly Invincible

Even with the self-configuring swarm (Phase 15), the system has fundamental blind spots. These 7 layers are what separate a shallow bug hunter from an invincible vulnerability detection system.

---

## Layer 1: Real Data Flow Analysis

**Current**: CPG does AST walking with substring matching. It can find `os.system(user_input)` but misses:
```python
x = user_input
y = transform(x)
exec(y)
```

Without **use-def chains, SSA form, and points-to analysis**, taint tracking is surface-level. A sanitizer that renames a variable defeats it. A wrapper function hides the sink. An intermediate assignment breaks the chain.

**Needed**: Inter-procedural data flow with def-use chains. Variable `x` assigned from a taint source → every use of `x` is tainted until explicitly sanitized by a recognized sanitizer function. Taint propagates through:
- Variable assignments (`a = b`)
- Function calls (`foo(a)` where `a` is tainted → `foo`'s parameter is tainted)
- Return values (`return tainted_value` → caller receives tainted)
- Field accesses (`obj.field = tainted` → `obj.field` is tainted on read)
- Collection operations (`list.append(tainted)` → list elements are tainted)

Build this as a **static single assignment (SSA)** pass over the AST. Every variable gets a unique version. Taint flows through version chains.

---

## Layer 2: Symbolic/Concolic Execution

**Current**: Agents write PoCs manually. They guess inputs. An agent might know `if (x > 1000 && y == "admin")` is a buggy path but has no way to compute the exact input that reaches it.

**Needed**: A symbolic execution engine that takes a code path and solves for the input that triggers it:

```
Path constraint: x > 1000 AND y == "admin" AND z.startswith("DROP")
Solution: x = 1001, y = "admin", z = "DROP TABLE"
```

The solver produces a concrete input. That input is fed to the sandbox. The sandbox executes. If the bug triggers, the finding is confirmed with a PRECISE input — not a guess.

**Implementation**: Use Z3 or a similar SMT solver. Instrument the target code at the CPG-identified bug location. Run concolic execution (concrete + symbolic) to explore the path space around the bug. Generate the constraint set for reaching the buggy line. Solve. Feed solution to sandbox.

The spec mentioned this in QD5 (speculative symbolic executor for dynamic dispatch). Never built.

---

## Layer 3: Coverage-Guided Fuzzing

**Current**: PoCs are handwritten by agents. One at a time. An agent might test `x = 0` and `x = 1` but never find the crash at `x = 2^31`.

**Needed**: A fuzzing tier alongside the agent tier. The fuzzer runs continuously inside the sandbox, generating thousands of inputs per second:

1. **Fuzzer loop**: Start with seed inputs from agent PoCs. Mutate (bit flips, byte inserts, arithmetic). Execute in sandbox with coverage instrumentation. Keep inputs that discover new code paths.

2. **Crash triage**: Any crash → deduplicate (same stack trace = same bug) → inject into evidence graph as an auto-discovered finding.

3. **Agent investigation**: When the fuzzer finds a crash, the swarm's agents investigate: "What input caused this? What code path was exercised? Is this exploitable?"

**Tools**: AFL++, libFuzzer, Honggfuzz. Run inside the sandbox container with coverage instrumentation compiled into the target binary.

---

## Layer 4: Differential Analysis

**Current**: The system has no baseline. It can't tell if behavior X is "correct" or "buggy." A function returning `-1` might be a bug... or the intended behavior.

**Needed**: A differential testing harness that compares behaviors:

1. **Version-to-version**: Run same inputs against current code and previous release. Any output difference → potential regression.
2. **Reference comparison**: Compare against known-good implementations of the same algorithm from trusted libraries. Deviations → potential bugs.
3. **Input-pair differential**: Run function `f(in1)` and `f(in2)` where in1 and in2 SHOULD produce the same output. Different outputs → inconsistency bug.
4. **Oracle differential**: Run against a different implementation of the same spec (e.g., two JSON parsers with the same input). Different outputs → at least one is buggy.

The differential engine runs continuously. Any behavioral divergence is auto-flagged and injected into the evidence graph.

---

## Layer 5: Runtime Sanitizer Instrumentation

**Current**: The sandbox runs PoCs in a vanilla container. Memory errors, undefined behavior, and data races happen silently — no crash, no stack trace, no detection.

**Needed**: Sandbox images compiled with sanitizers. Every PoC execution gets free coverage:

| Sanitizer | Detects | Example |
|-----------|---------|---------|
| **AddressSanitizer (ASAN)** | Buffer overflows, heap use-after-free, stack use-after-return, double-free | `arr[100] = x` when `arr` has 50 elements |
| **UndefinedBehaviorSanitizer (UBSAN)** | Integer overflow, null pointer deref, misaligned access, division by zero, signed overflow | `int_max + 1` wrapping silently |
| **ThreadSanitizer (TSAN)** | Data races, mutex misuses, lock order violations | Two threads writing `counter += 1` without a lock |
| **MemorySanitizer (MSAN)** | Reads of uninitialized memory | Using `malloc`'d memory before writing to it |
| **LeakSanitizer (LSAN)** | Memory leaks | `malloc` without corresponding `free` |

Each sanitizer produces a crash or report. The sandbox captures it as structured evidence. Agents don't need to guess at memory bugs — the sanitizer detects them at runtime.

---

## Layer 6: Persistent Learning & Bug Pattern Database

**Current**: Every run is amnesia. The system doesn't remember:
- Which code patterns caused bugs in previous runs on this repo
- Which agent strategies (persona + model + temperature) worked best
- Which CVE patterns are most relevant to this codebase
- That a specific file had bugs 3 runs ago — it should be prioritized now

**Needed**: A persistent learning layer:

1. **Bug Pattern Vector DB**: Every confirmed finding from every run is embedded and stored. Code snippet, CWE category, language, severity, fix. When indexing a new repo, the CPG is matched against this database. Code regions with high similarity to known-buggy patterns get priority investigation.

2. **CVE Corpus with Graph Isomorphism**: Not substring matching. The CPG is compared against a database of CPGs built from CVE-patched code. Graph edit distance between the target code and known-vulnerable patterns. Subgraph isomorphism with edit distance < threshold → high-priority investigation zone.

3. **Agent Performance History**: Per-agent, per-persona, per-model, per-repo-type: bugs found, false positive rate, tokens per verified finding. This data feeds the allocation algorithm so future runs assign more agents to what works.

4. **File Hotspot Tracking**: Files that produced bugs in previous runs get `hotspot_score` increments. The scout agent reads hotspot scores and prioritizes those files in the difficulty assessment.

---

## Layer 7: Vulnerability Chaining

**Current**: The system finds individual bugs. It can't chain them. A real-world exploit often requires multiple steps:

```
1. Information leak (read memory via out-of-bounds) → discover ASLR base address
2. Buffer overflow (write to known address) → overwrite return pointer
3. Control flow hijack → execute shellcode
4. Privilege escalation → root
```

Bug Swarm finds each individually but never connects them. The evidence graph has no "enables" edge type. An agent that finds Bug A and an agent that finds Bug B never realize they form an exploit chain.

**Needed**: Exploit chain synthesis in the evidence graph:

1. **New edge type**: `EdgeKind::Enables` — "Bug A enables Bug B" when Bug A's output (leaked address, corrupted state, gained capability) is an input or precondition for Bug B.

2. **Chain detection algorithm**: BFS through the evidence graph, following Enables edges. If a path exists from a low-severity bug through intermediate bugs to a critical-severity bug, flag the entire chain.

3. **Chain-aware prioritization**: A bug that is part of a chain is UPGRADED in severity because it's a stepping stone to something worse. A "minor info leak" that enables an "RCE" becomes part of a critical finding.

4. **Auto-generated chain PoC**: If the system has sandbox-verified PoCs for Bug A and Bug B, and they form a chain, auto-generate a combined PoC that executes both in sequence and verify the chain works end-to-end.

---

## What Makes It Truly Invincible

| Layer | Finds | Status |
|-------|-------|--------|
| CPG (static analysis) | Surface taint paths, call graphs | Built but shallow (substring matching) |
| Agent reasoning (LLM) | Logic flaws, business logic bugs | Built, working with DeepSeek V4 |
| Sandbox (dynamic) | Runtime crashes, memory errors | Built, working |
| **#1: Data Flow Analysis** | Deep taint through variables, functions, fields | **Missing** |
| **#2: Symbolic Execution** | Precise inputs for deep code paths | **Missing** |
| **#3: Fuzzing** | Crash discovery at scale (1000s/sec) | **Missing** |
| **#4: Differential Testing** | Regression, behavioral deviation, inconsistency | **Missing** |
| **#5: Sanitizer Instrumentation** | Undefined behavior, races, leaks at runtime | **Missing** |
| **#6: Pattern Database** | Learning from history, CVE graph matching | **Missing** |
| **#7: Vulnerability Chaining** | Multi-step exploit synthesis | **Missing** |

---

## The Dominance Stack — 8 Techniques No Single Tool Combines

These go beyond finding bugs. They provide mathematical certainty about what was found, what was missed, and what will break if you fix it. Combined with the 7 layers above, they form a complete bug-finding system with zero blind spots.

---

### Technique 1: Specification Mining + Invariant Detection

**What**: Run the application with thousands of inputs inside the sandbox. Observe outputs. Learn implicit invariants. Flag violations.

```
Observed: function withdraw(amount) → balance always >= 0
Invariant mined: "withdraw returns non-negative balance"
If one input produces balance < 0 → BUG FOUND
```

This finds bugs with NO crash, NO stack trace, NO visible error. Silent data corruption. No static analyzer can find this. No fuzzer can catch this. Only runtime invariant checking.

**Integration point**: Sandbox daemon (Phase 1). Property-based input generator feeds thousands of inputs. Output recorder mines invariants via pattern analysis. Violation → auto-injected into evidence graph.

**Tools required**: Hypothesis (Python), QuickCheck (Haskell), Daikon (invariant detector).

---

### Technique 2: Mutation Testing as Bug Oracle

**What**: Inject artificial bugs into the code. Run existing tests. If tests don't catch the mutation → the system flags "there is an untested assumption at this line — a real bug here would go undetected."

```
Original:  if (balance >= amount) { withdraw(); }
Mutation:  if (balance > amount) { withdraw(); }   // >= became >
Tests:     All pass (no test for exact balance case)
Result:    FLAGGED — untested boundary condition at line 42
```

**Why dominant**: Finds bugs-in-waiting. Code that works today but a one-character change breaks it. The LLM explains WHY the mutation matters and what a real-world exploit would look like.

**Integration point**: CPG + Sandbox. CPG identifies mutation targets (operators, conditionals). Sandbox runs test suite against mutated code. Survivors → flagged. Agents investigate and generate exploit scenario.

**Tools required**: mutmut (Python), Stryker (JS), PIT (Java).

---

### Technique 3: Concolic Execution (Concrete + Symbolic)

**What**: Runs a CONCRETE input through the code, collects symbolic constraints along the path taken, then NEGATES one constraint and solves for a new input that takes a DIFFERENT path.

```
Run 1: input = "hello" → path A (true branch of if)
       Constraint collected: x == "hello"
       Negate: x != "hello"
       Solve: x = "anything_else"
Run 2: input = "anything_else" → path B (false branch)
       Both branches now covered
```

Systematically explores ALL paths around a suspected bug. Finds the EXACT condition that triggers it. Generates the PoC input automatically. No guessing by agents.

**Integration point**: CPG + Sandbox. CPG identifies branching points near sinks. Concolic engine instruments the code. Z3 SMT solver generates path-reaching inputs. Sandbox executes. Results → evidence graph.

**Tools required**: Z3 (SMT solver), angr or Triton (binary analysis with concolic), PyExZ3 (Python symbolic executor).

---

### Technique 4: Bug Probability Prediction (ML on Historical Bugs)

**What**: Train a model on every confirmed bug the system has ever found.

```
Features extracted per function:
  - Cyclomatic complexity
  - Nesting depth
  - Number of parameters
  - Taint path presence (from CPG)
  - Author commit frequency (from git)
  - File churn rate (from git)
  - Historical bug density in this file
  - CVE pattern similarity score
  - Lines changed in last N commits

Output: auth.py:login() → 94% probability of containing a bug
        utils.py:format_date() → 3% probability
```

Agents investigate the 94% function first. Not randomly. Not based on CPG taint alone. Based on statistical likelihood from every bug ever found across all runs.

**Integration point**: Assessment module (Phase 15). Before the scout agent runs, the probability model scores every function. Top-N functions feed into the scout's investigation priority. Allocation algorithm weights agent distribution toward high-probability files.

**Tools required**: scikit-learn (RandomForest or XGBoost), feature extractor from CPG + git log.

---

### Technique 5: Fix-Induced Bug Prediction

**What**: When a fix is proposed, compute: "If we apply this fix, how many new bugs will it introduce?"

```
Fix: Change `if (x)` to `if (x is not None)`
Impact analysis via call graph:
  - 14 callers pass x as integer 0 → now correctly handled ✓
  - 3 callers rely on falsy behavior → BROKEN by this fix ✗
  - 2 new null pointer paths opened → new bug risk ⚠
Probability: 23% chance of introducing 1-2 new bugs
```

**Why dominant**: The most dangerous action in software is fixing a bug. The fix itself introduces new ones. This system PREDICTS that before the fix is applied. No tool on the market does this.

**Integration point**: Evidence Graph + Bench. When a judge confirms a bug and an agent proposes a fix, the system traces all callers of the changed function via CPG. Simulates behavior change. Flags callers whose assumptions are violated. Reports probability. The judge reviews before accepting.

**Tools required**: CPG call graph (existing), behavior simulation (new), caller contract checker (new).

---

### Technique 6: Coverage-Guided Greybox Fuzzing + Taint Steering

**What**: Fuzzer generates inputs. Coverage instrumentation tracks which code paths are hit. Inputs that discover NEW paths are kept and mutated further. BUT — inputs that reach TAINTED sinks get 10x priority mutation.

```
Random input → hits 50% coverage → kept, normal priority
Random input → hits 50.1% coverage (new path) → kept, mutated
Random input → reaches SQL execute() sink → HIGH PRIORITY, mutated 10x more
Random input → reaches exec() sink → CRITICAL PRIORITY, mutated 100x more
```

Standard fuzzers explore uniformly. This one is steered by the CPG's taint analysis. It spends 10x more effort on the 5% of code that's actually dangerous.

**Integration point**: Sandbox + CPG. CPG identifies sinks and generates a "danger map" of code addresses. Fuzzer running in sandbox uses danger map as reward function. Crash triage auto-injects findings into evidence graph. Agents investigate and explain.

**Tools required**: AFL++ with custom LLVM pass for taint-aware coverage, libFuzzer with custom mutator.

---

### Technique 7: Trigger Matrix

**What**: For every confirmed bug, produce a complete multi-dimensional trigger analysis:

```
Bug: NPE at auth.py:42
┌─────────────────────┬────────────────────────────────────────┐
│ Trigger Type        │ Condition                               │
├─────────────────────┼────────────────────────────────────────┤
│ Input               │ username = null (or "")                 │
│ Environment         │ Database unreachable (network failure)  │
│ Timing              │ Request arrives during server restart   │
│ Data State          │ User row missing from database          │
│ Concurrency         │ Two simultaneous login() calls          │
│ Configuration       │ DEBUG=False (production only path)      │
│ Dependency Version  │ sqlite3 < 3.35 (old C extension bug)    │
│ OS / Arch           │ 32-bit only (pointer truncation)        │
└─────────────────────┴────────────────────────────────────────┘
```

The Trigger Matrix makes bugs 100% reproducible by documenting every condition. The agent, fuzzer, and concolic executor each contribute rows to this matrix. Over time, the matrix fills until the bug is fully characterized.

**Integration point**: Evidence Graph. Each confirmed bug gets a TriggerMatrix node. Every sandbox execution, fuzzer run, and agent analysis adds rows. Judges review the matrix for completeness before closing.

---

### Technique 8: Delta Debugging

**What**: When a bug is found with a complex input, automatically minimize to the smallest reproduction.

```
Original crash input:  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\x00\xff\x00GET / HTTP/1.1..."
Delta debugged input:  "\x00\xff"
                       ↑↑↑↑↑ The 3 bytes that actually matter
```

The agent finds a crash with a 500-byte input. Delta debugging reduces it to the 3 bytes that matter. The agent now knows EXACTLY what triggers it. The fix is targeted to those 3 bytes' handling. The investigation time drops from hours to seconds.

**Integration point**: Sandbox post-processing. After any crash, the crash input is fed to the delta debugger. The minimized input is stored alongside the bug in the evidence graph. The agent receives both: "Your 500-byte input crashes. The essential 3 bytes are \x00\xff. Investigate null byte handling."

**Tools required**: python-afl (delta debugging), custom ddmin implementation for string/binary inputs.

---

## Summary: All 15 Layers

| # | Layer | Finds | Status |
|---|-------|-------|--------|
| **Base** | | | |
| — | CPG (static) | Surface taint paths | Built (shallow) |
| — | LLM Agents | Logic flaws, business logic | Built (DeepSeek V4) |
| — | Sandbox (dynamic) | Runtime crashes | Built |
| **7 Deep Layers** | | | |
| 1 | Data Flow Analysis | Deep taint through variables | Missing |
| 2 | Symbolic Execution | Precise triggering inputs | Missing |
| 3 | Fuzzing | Crashes at scale (1000s/sec) | Missing |
| 4 | Differential Testing | Regressions, deviations | Missing |
| 5 | Sanitizer Instrumentation | Undefined behavior, races, leaks | Missing |
| 6 | Pattern Database | Learning from history | Missing |
| 7 | Vulnerability Chaining | Multi-step exploit synthesis | Missing |
| **8 Dominance Techniques** | | | |
| 8 | Specification Mining | Silent data corruption | Missing |
| 9 | Mutation Testing | Bugs-in-waiting (untested edges) | Missing |
| 10 | Concolic Execution | Systematic path exploration | Missing |
| 11 | Bug Probability ML | Prioritized investigation | Missing |
| 12 | Fix-Induced Prediction | Prevents fix regressions | Missing |
| 13 | Taint-Guided Fuzzing | 10x efficiency on dangerous code | Missing |
| 14 | Trigger Matrix | 100% bug reproducibility | Missing |
| 15 | Delta Debugging | Precise trigger identification | Missing |

These 15 layers together form a complete bug-finding system with no blind spots. Each layer feeds the next. Each layer finds what the previous one misses.
