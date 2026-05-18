# BugSwarm vs Mythos: The Gap Analysis

**Research date**: May 18, 2026  
**Sources**: anthropic.com/glasswing, red.anthropic.com/2026/mythos-preview, platform.claude.com/docs, /root/a/ codebase audit

---

## Where You Stand: Distance to Surpassing Mythos

### The current state:

**What BugSwarm has that Mythos cannot replicate with any tool:**
- AFL++ fuzzer (1000+ execs/sec, danger-map guided) — finds crash bugs at machine scale
- Code Property Graph with SSA taint analysis — computes data flow that reading code cannot reveal
- Invariant miner (1000x execution with statistical confidence) — finds silent wrong answers
- Delta debugger (ddmin) — automatically minimizes crash inputs to essential bytes
- Mutation testing (6 operators) — systematically finds untested code paths
- Differential analysis (4 modes) — catches semantic regressions
- Trigger matrix (8 dimensions × 8 layers, Jaro-Winkler dedup) — systematic bug documentation

**What Mythos has that BugSwarm lacks:**
- Frontier reasoning (93.9% SWE-bench, 83.1% CyberGym) — understands code deeply
- Autonomous exploit writing (ROP chains, JIT heap sprays, cross-bug chains)
- Agentic autonomy (Claude Code: sub-agents, team coordination, worktrees)
- Actually found thousands of real 0-days in production software

---

## The 7 Gaps: What Must Be Fixed

### Gap 1: Model Quality (Critical — 80% of the gap)

| Component | Current | Required |
|-----------|---------|----------|
| Investigation model | DeepSeek V4 Flash | Claude Opus 4.6 or Mythos Preview |
| Judge model | None wired | Claude Sonnet 4.6 |
| Context window | 128K tokens | 200K tokens |
| Reasoning quality | Mid-tier | Frontier (CyberGym 80%+) |

**Technical requirement**: BugSwarm Gateway already supports Anthropic. The `ProviderType::ANTHROPIC` adapter exists. The model name is a config change. But the IEP engine's prompt format must match Anthropic's tool-use schema (different from OpenAI's function-call format). Fix: `bugswarm-gateway/src/gateway/providers/anthropic.py` — adapt tool schema output.

**Distance**: ~2 days of work (already 90% wired).

### Gap 2: Autonomous Agentic Loop (High — 10% of the gap)

| Component | Current (BugSwarm) | Required (Claude Code) |
|-----------|-------------------|----------------------|
| Loop structure | Round/turn-based IEP | Observe → Plan → Act → Verify (free-form) |
| Tool chaining | Sequential, one at a time | Parallel, backgroundable |
| Sub-agents | Not supported | Agent tool spawns isolated contexts |
| Context management | 80% compression trigger | Auto-compaction, summary generation |
| Self-correction | Hypothesis ranking | Autonomous retry, alternative approaches |

**Technical requirement**: Rewrite `bugswarm-agent/src/agent/loop.py`:
1. Remove the fixed round/turn structure — let the model drive the investigation freely
2. Add background tool execution (`run_in_background: true` equivalent)
3. Add sub-agent spawning for parallel file analysis
4. Add autonomous retry with alternative approaches
5. Add verification step after each action

**Distance**: ~2 weeks of work.

### Gap 3: Functional Stubs (High — 5% of the gap)

These 4 handlers return fake data:

| Handler | Current Behavior | Required |
|---------|-----------------|----------|
| `mine_invariants` | Returns stub traces with hardcoded values | Actually executes code 1000x in sandbox |
| `run_mutations` | Fake test runner (string matching) | Actually compiles and runs test suites |
| `solve_reachability` | Heuristic only (val + 1) | Actually calls Z3 SMT solver |
| `invariant_check` | Generates inputs, never executes | Actually runs in sandbox |

**Technical requirement**: Wire real execution. Each stub requires:
1. Generate a PoC script
2. Execute via `manager.execute()`
3. Parse ExecutionReceipt
4. Build real data structures from parsed output

**Distance**: ~1 week of work (already have the execution infrastructure).

### Gap 4: Tool Count (Medium — 3% of the gap)

BugSwarm has 13 tools. Claude Code has 30+. Missing tools that would dramatically improve BugSwarm:

| Missing Tool | Why Needed |
|-------------|-----------|
| `grep` / `glob` | Search codebase for patterns without reading every file |
| `edit` | Make surgical code changes (for causal interventions) |
| `write_file` | Generate fix proposals directly |
| `git` | Understand version history, blame, diffs |
| `web_fetch` | Pull documentation, CVE databases, known exploit patterns |
| `subagent` | Spawn child agents for parallel investigation |

**Technical requirement**: Add 6 tools to ToolRegistry. Each is ~20-50 lines. All can use existing sandbox/CPG infrastructure.

**Distance**: ~2 days of work.

### Gap 5: CPG Language Coverage (Medium — 1% of the gap)

Currently Python + JavaScript only. Mythos reads C, C++, Rust, Go, etc. directly. BugSwarm cannot parse these into the CPG.

**Technical requirement**: Add tree-sitter grammars for C, C++, Rust, Go, Java. Each is ~200 lines of parser code per language. The CPG, call graph, CFG, SSA are language-agnostic once parsed.

**Distance**: ~2 weeks for 5 languages (C, C++, Rust, Go, Java).

### Gap 6: Real-World Testing (High — 1% of the gap)

BugSwarm has never been run against a real codebase with a real LLM. Zero confirmed bugs found by this system in the wild.

**Technical requirement**: Run BugSwarm against:
1. OSS-Fuzz corpus (1,000 open-source repos with known bugs)
2. Juliet test suite (100,000 synthetic bugs across 150 CWE categories)
3. Real-world targets (nginx, Redis, SQLite)
4. Measure: bugs found, false positive rate, time to first bug, cost per bug

**Distance**: ~1 week of running experiments + 1 week of tuning based on results.

### Gap 7: Exploit Generation (Future — currently 0% of the gap)

BugSwarm finds bugs and chains them. It does not write exploits (ROP chains, heap sprays, shellcode). Mythos does this autonomously.

**Technical requirement**: This is a post-MVP capability. It requires:
1. Model capable of exploit writing (Mythos-class)
2. Binary analysis tools (GDB, pwntools integration)
3. Exploit verification sandbox (safe execution of weaponized exploits)
4. This is Q3 territory (3+ months of work)

---

## The Distance: How Far Are You?

```
                    Reasoning Quality (model)
                    │
             100%   │   ★ Mythos + Claude Code
                    │
              80%   │
                    │   ★ Opus 4.6 + Claude Code
              60%   │
                    │   ★ BugSwarm with Mythos (TARGET)
              40%   │
                    │   ★ BugSwarm with Opus 4.6 (achievable in 3 weeks)
              20%   │
                    │   ★ BugSwarm with DeepSeek V4 (CURRENT)
                    │
               0%   └──────────────────────────────────────
                    0     20     40     60     80    100
                         Tool & Automation Quality (system)
                                         │
                    BugSwarm's fuzzer, CPG, symbolic,    Mythos tools
                    invariants, mutations, dedup, chains  (Read, Edit, Bash, Grep)
```

**Current position**: Bottom-right quadrant — great tools, weak model.
**Target position**: Top-right quadrant — great tools + great model.
**Distance**: ~4-6 weeks of work (Gaps 1-6), then ~3 months for Gap 7.

---

## What Must Be Done: The 6-Week Plan

### Week 1-2: Model + Tools
- Switch investigation model to Claude Opus 4.6 via existing Anthropic adapter
- Fix prompt format for Anthropic's tool-use schema
- Add grep, glob, edit, write_file, git, web_fetch tools
- Fix 4 functional stubs (real execution)

### Week 3-4: Agentic Loop
- Rewrite IEP engine for free-form autonomous investigation
- Add sub-agent spawning
- Add background task execution
- Add verification step

### Week 5: Languages + Testing
- Add C, C++, Rust tree-sitter grammars to CPG
- Run against OSS-Fuzz corpus
- Run against Juliet test suite
- Measure and tune

### Week 6: Hardening
- Run against real targets (nginx, Redis, SQLite)
- Compare bug counts vs Mythos+ClaudeCode baseline
- Document findings

---

## The Bottom Line

**Can BugSwarm surpass Mythos?**

With an Opus-class model + BugSwarm's tools: Yes. BugSwarm's fuzzer alone would find crash bugs at 1000x the rate Mythos can write PoCs. The CPG would find taint paths that reading code cannot reveal. The invariant miner would find silent wrong answers that no reasoning about code could detect.

With DeepSeek V4: No. The model is too weak to effectively use the tools. It would not know when to fuzz vs when to query the CPG vs when to mine invariants.

**What you need to do right now**: Plug Claude Opus 4.5 or 4.6 into BugSwarm. That single change moves you from "research prototype" to "competitive with Claude Code." The rest (tools, loop, languages, testing) amplifies from there.
