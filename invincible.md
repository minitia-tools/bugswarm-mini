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

The current system is a strong **shallow bug hunter** — it finds bugs a human would catch in code review. To be invincible, it needs the deep layers: data flow that traces through variables, symbolic execution that computes triggering inputs, fuzzing that generates crashes at scale, sanitizers that catch undefined behavior, pattern learning that improves with every run, and exploit chaining that connects individual bugs into attack paths. These 7 layers are Phase 16–22.
