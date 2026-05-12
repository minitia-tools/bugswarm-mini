# AGENTS.md — Enterprise Quality Standards

Every line of code in this repository must meet these standards. No exceptions, no shortcuts, no "fix later." This file is non-negotiable. It is read by every developer, every reviewer, and every automated gate before merge.

---

## 1. Immutable Invariants

These are the architectural absolutes. If a change violates any of these, it is rejected regardless of how well-written it is.

1. **Every confirmed bug must be traceable to a sandbox execution receipt containing a cryptographic hash of the PoC, the exit code, and the stderr digest.** No sandbox receipt → not a bug. Rhetoric is not evidence.

2. **No agent-generated code ever touches the host filesystem or the target repository.** PoCs and patches execute exclusively inside an isolated container with `--read-only --cap-drop=ALL --network=none`.

3. **No tool output enters an LLM context window without passing through the PII/Secrets Scanner.** This is mandatory for both external providers AND local models.

4. **No code chunk enters an LLM context window without passing through the Prompt Injection Scanner.** If the scanner flags it, the chunk is wrapped in sanitization tags or blocked.

5. **The orchestrator is the sole writer to the debate log.** Agents never write to the database directly. All writes go through the single-writer `asyncio.Queue`.

6. **The WAL is committed before the orchestrator acts on any state transition.** Crash recovery depends on this ordering. Write-first, act-second.

7. **Agent context is checkpointed at the end of every round.** In-memory state is ephemeral; the snapshot is authoritative.

8. **Sandbox execution receipts are immutable once written.** No update, no delete. Only insert.

9. **Every build phase is gated by an extreme stress test designed to break the implementation.** The phase is not "done" until the stress test passes at 100%. Phase N+1 does not begin until Phase N's stress test is green. There is no "fix later."

10. **Do whatever must be done to make each phase fully pass.** If a phase gate test fails — fix the code, rewrite the module, change the approach, install missing dependencies, update the configuration, debug the issue, read the logs, trace the syscalls, refactor the architecture, or escalate to a human. The only acceptable outcome is the phase gate passing at 100%. There is no "close enough." There is no "passes most tests." There is no "the failing test is a false positive." If the test fails, something is wrong. Find it. Fix it. Run the gate again. Repeat until every single test in the gate passes. Only then does the phase close and Phase N+1 begin.

11. **Before a phase is marked complete, every individual component within that phase must undergo its own extreme aggressive stress test.** The phase-level gate is the final exam — it is not the only test. Each component (every module, every struct, every function, every CLI subcommand, every error path, every configuration variant) must be attacked in isolation with inputs and conditions explicitly engineered to break it. A phase is not a monolith — it is a collection of components. If the phase gate passes but one component has an untested edge case, the phase is NOT complete. The component must be tested until it breaks or is proven unbreakable. Only when every component has survived its individual assault does the phase gate run. Only when both all component tests AND the phase gate pass at 100% is the phase marked complete.

12. **No God objects. Every file has exactly one responsibility.** If a file exceeds 400 lines, split it. If a class has more than 7 public methods, split it. If a function exceeds 50 lines, extract helpers. The single-responsibility principle is enforced by file size, not intuition.

13. **Every public function has a typed signature, a docstring, and a test.** `Any` is banned. `dict` without `TypedDict` or Pydantic model is banned. Every return type is explicit. Every `except Exception` must be narrowed to the specific exception types expected.

14. **No hardcoded paths, no hardcoded credentials, no hardcoded magic numbers.** All paths come from config. All secrets come from environment variables or a secret manager. Every magic number must be a named constant with a comment explaining the value choice.

15. **Parse, don't validate.** LLM output is parsed through a chain of strategies (native function calling → JSON fences → inline JSON → freeform heuristics). Raw string manipulation (`content.index('{"type"')`, manual brace counting) is banned in production paths. Use JSON Schema validation on every accepted finding.

16. **Every external dependency (CPG, Sandbox, LLM) is accessed through a client abstraction with connection pooling, health checks, retry, and circuit breaker.** No raw `subprocess.run()`. No raw `httpx.get()` without timeout. Every external call has a timeout, retry budget, and graceful degradation path.

17. **All state is persisted.** In-memory state is scaffolding. Production state lives in SQLite with WAL journaling, migration framework, and backup/restore. Process restart must recover the exact agent state from the last checkpoint.

18. **Backward compatibility is maintained during refactoring.** Every extracted module must be importable from its original location via a shim. `from agent.core import BugSwarmAgent` must still work after splitting `core.py` into 8 files. Existing gate tests must pass at every intermediate commit.

19. **One file at a time. Test after each file. Commit after each file.** No batch refactoring. Extract one module, run ALL existing tests, fix until green, commit. Only then move to the next file. Atomic commits with clear messages: `refactor: extract prompts.py from core.py`.

20. **The plan is the contract. Deviations require an ADR.** The refactoring plan in `agent_refactoring_plan.md` defines the target architecture. If implementation reveals a better approach, write a one-paragraph Architecture Decision Record explaining why, update the plan, then proceed.


---

## Phase Completion Gates — Extreme Stress Tests

The system is implemented one phase at a time, sequentially. At the end of each phase, an aggressive, adversarial stress test is executed. This test is explicitly designed to break the implementation — to find the edges where it fails. The phase is complete only when the test passes with zero failures, zero warnings, and zero unexplained anomalies.

These tests are not optional. They are the contract between phases. Phase N+1 is built on the assumption that Phase N is invincible. If that assumption is false, the entire architecture collapses.

### Phase 1 Gate: Sandbox Oracle

**Test: The Hostile PoC Gauntlet**

The sandbox daemon is bombarded with 1,000 malicious PoC scripts designed to escape, crash, or corrupt the host.

| # | Attack Vector | Expected Behavior | Actual Test |
|---|---------------|-------------------|-------------|
| 1 | `rm -rf / --no-preserve-root` inside container | Container writable layer destroyed. Host filesystem untouched. Container exits non-zero. | Verify host `/etc`, `/home`, `/var` identical before and after. Checksum all host files. |
| 2 | Fork bomb: `while(1) { fork(); }` | Container hits `--pids-limit=100`. OOM killer or PID cgroup terminates it. Host `pid_max` unaffected. | Run 10 fork bombs simultaneously across 10 containers. Host process count must stay within 5% of baseline. |
| 3 | Memory bomb: allocate 100 GB | Container hits `--memory=512m`. OOM killer terminates. Host RAM unaffected. | Monitor host `MemAvailable` throughout. Must not drop below 20% of total. |
| 4 | Disk bomb: `dd if=/dev/zero of=/tmp/bigfile bs=1M count=10000` | Container hits `--storage-opt size=1G`. Write fails with ENOSPC. Host disk unaffected. | Verify host disk usage change < 10 MB. |
| 5 | CPU bomb: `while(1) { sqrt(rand()); }` | Container limited to `--cpus=1`. Host CPU headroom preserved. | Run CPU bombs in 50 parallel containers. Host load average must stay below core count. |
| 6 | Network escape: `curl http://169.254.169.254/latest/meta-data/` (AWS metadata) | `--network=none` → connection refused. No bytes leave the container. | tcpdump on host interface. Zero packets from container IP range. |
| 7 | Container escape via `/proc/1/ns` manipulation | `nsenter`, `unshare` blocked by seccomp. Process killed. | `docker inspect` shows `OOMKilled: false` and `Error: "seccomp: ..."`. Agent banned. |
| 8 | Container escape via Docker socket: mount `/var/run/docker.sock` | Socket not present in container (no host mounts). `docker ps` fails. | Verify `/var/run/docker.sock` does not exist inside container. |
| 9 | Privilege escalation: `sudo`, `su`, `capsh` | `--cap-drop=ALL` → no capabilities. All privilege operations fail with EPERM. | Run Linux privesc checklist script. All 50 checks must fail. |
| 10 | Kernel module load: `insmod`, `modprobe` | Seccomp blocks `finit_module`, `init_module`. Process killed. | `lsmod` diff before/after = zero new modules. |
| 11 | eBPF abuse: load BPF program to exfiltrate data | Seccomp blocks `bpf`. Process killed. | Verify no new BPF programs via `bpftool prog list`. |
| 12 | Time-based escape: sleep for 10 years, then attack | Wall-clock timeout (120s) + CPU-time timeout (60s). Container killed at 120s regardless. | Container started, sleeps for 300s. Must be killed at 120s exactly. Verify exit code = 137 (SIGKILL). |
| 13 | Deadlock camouflage: create deadlock that looks like legitimate computation | Pre-kill diagnostic detects deadlock via thread dump. `timeout_reason: "likely_deadlock"` returned, not "infinite_loop". | Submit PoC that creates two threads waiting on each other's locks. Verify classification accuracy. |
| 14 | OOM during PoC with valid crash: memory leak + null pointer | Container OOM-killed. `OOMKilled: true` in receipt. Memory growth graph captured. Finding partially verified. | Submit PoC that allocates memory in a loop then crashes. Verify receipt contains BOTH OOM data AND crash data. |
| 15 | Cryptographic receipt tampering: agent modifies receipt after execution | Orchestrator validates receipt hash. Mismatch → `TAINTED`. Execution re-run independently. | Manually flip one byte in receipt JSON. Verify orchestrator rejects it and re-runs. |
| 16 | Timestamp manipulation: agent claims execution was faster than actual | Orchestrator records wall-clock time independently. Mismatch > 1s → `TAINTED`. | Submit PoC with fake timing report. Verify orchestrator's independent clock supersedes. |
| 17 | Environment variable injection: PoC tries to set `LD_PRELOAD`, `PATH`, `PYTHONPATH` | Orchestrator validates env var requests against whitelist. Dangerous vars rejected. | Submit `set_sandbox_env("LD_PRELOAD", "/tmp/evil.so")`. Must be rejected with reason. |
| 18 | Concurrent escape: 100 containers simultaneously trying different escape vectors | All contained. Zero host impact. Orchestrator manages fleet without resource exhaustion. | Run escape vector matrix (all 17 above) simultaneously in 100 containers. 3 repetitions. |

**Gate condition:** All 18 attack classes pass. Zero host modifications. Zero unexpected exits. Every `TAINTED` detection triggers correctly. Every escape attempt logged as `CRITICAL:SANDBOX_ESCAPE_ATTEMPT`.

---

### Phase 2 Gate: Code Property Graph

**Test: The Obfuscation Gauntlet**

The CPG builder must correctly parse, link, and taint-track across 25 deliberately hostile code patterns.

| # | Pattern | Expected Behavior |
|---|---------|-------------------|
| 1 | Dynamic dispatch: virtual method in C++ resolved correctly | CPG edge from call site to resolved vtable entry. Confidence < 1.0 but correct target ranked #1. |
| 2 | Python `getattr(obj, method_name)` where `method_name` comes from config | Soft edge to all possible targets with confidence scores. Uncertainty zone flagged. |
| 3 | Java DI container: `@Inject` annotation → concrete implementation | Speculative symbolic execution resolves the binding. Edge carries `resolved_by: concolic` metadata. |
| 4 | Cross-language call: JS `fetch("/api/users")` → Python `@app.get("/api/users")` | Cross-language edge created via URL path matching. Confidence logged. |
| 5 | Taint propagation through 10 function calls: `req.body` → validation → sanitizer → query builder → SQL execute | Taint path complete. Sink detected. Sanitizer detected. Path length = 10. |
| 6 | Taint broken by sanitizer: `req.body` → `html.escape()` → output | Taint path terminates at sanitizer. Sink NOT flagged. No false positive. |
| 7 | Indirect taint: `a = req.body; b = a; c = b; exec(c)` | Taint tracked through all assignments. Sink reached. |
| 8 | Taint through conditional: `if (flag) { exec(x) } else { exec(y) }` | Both branches tracked. Sink flagged regardless of `flag` value. |
| 9 | 10,000-line file with deeply nested AST (50 levels) | Parser does not stack overflow. AST constructed correctly. All nodes reachable. |
| 10 | Unicode identifiers, RTL override characters, zero-width spaces in function names | Parser handles all Unicode correctly. No crashes, no incorrect tokenization. |
| 11 | Code with syntax errors (deliberately broken file) | Parser reports error with line:column. Does NOT crash. Other files in repo still indexed. |
| 12 | Circular imports: `a.py` imports `b.py`, `b.py` imports `a.py` | Cycle detected. Graph contains both edges. No infinite loop. |
| 13 | Macro-expanded code (C `#define`, Rust `macro_rules!`) | Macro body is attributed to the expansion site. Call graph edges include macro source. |
| 14 | Decorator chain: `@auth @log @validate def handler(): ...` | All decorators in call chain. Order preserved. |
| 15 | Async/await: `async def a(): await b()` | Async call edge labeled correctly. Event loop trampoline not mistaken for direct call. |
| 16 | Generator functions: `yield from inner()` | Data flow traced through generator. Yields become data flow edges. |
| 17 | Lambda/closure capturing outer variable: `lambda: x` where `x = req.body` | Captured variable taint propagated. Lambda treated as data sink if executed unsafely. |
| 18 | Monkey-patched method: `MyClass.method = new_impl` at runtime | Static graph shows original. Dynamic trace from sandbox overwrites edge. CPG stores both with `static`/`dynamic` tags. |
| 19 | 100,000-file monorepo (simulated) | Indexing completes. Memory usage < 2 GB per 100K files. Query latency < 500ms for 3-hop traversal. |
| 20 | File with no extension, file with wrong extension (.js file containing Python) | tree-sitter language detection falls back to content-based heuristics. Errors logged, not fatal. |

**Gate condition:** All 20 patterns correctly handled. Zero crashes. Taint paths complete for #5, #7, #8. Taint correctly terminated for #6. Cross-language edges exist for #4. Memory under budget for #19.

---

### Phase 3 Gate: LLM Gateway

**Test: The Provider Chaos Test**

The gateway must survive every realistic API failure mode without data loss.

| # | Scenario | Expected Behavior |
|---|----------|-------------------|
| 1 | OpenAI returns 429 (rate limit) 5 times in a row | Exponential backoff: 1s → 2s → 4s → 8s → 16s. Succeeds on 5th try or falls back to next provider. |
| 2 | Anthropic returns 502 for every request for 30 seconds | All requests during outage routed to OpenAI/Google. Switches back when Anthropic recovers. |
| 3 | All providers return 5xx simultaneously for 60 seconds | Orchestrator pauses swarm (FREEZE). Retries every 10s. Resumes when any provider recovers. No data loss. |
| 4 | Provider returns malformed JSON (truncated response) | Adapter retries with schema-correction prompt. 2 retries max. On failure, agent turn marked `PROVIDER_ERROR`. |
| 5 | Provider returns valid JSON that doesn't match `ArgumentPayload` schema | Pydantic validation fails. Adapter retries with schema embedded in retry prompt. |
| 6 | Mid-debate hot-swap from GPT-4o to Claude 3.5 Sonnet via `SIGUSR1` | In-flight call completes on GPT-4o. Next call uses Claude. Prompt rendered correctly for Claude's format. |
| 7 | Token count mismatch between provider-reported and local tokenizer | Local tokenizer (tiktoken/claude-tokenizer) is authoritative for budget tracking. Discrepancy > 5% logged as WARN. |
| 8 | Provider adds new model version mid-session | Judge model version change detected. Previous verdict re-run for consistency check. Session continuity token validated. |
| 9 | API key expires mid-session (401) | Orchestrator logs ERROR. Swarm FREEZE'd. Does NOT retry (401 is not transient). Human intervention required. |
| 10 | Network latency spikes to 10 seconds | `asyncio.wait_for` with 30s timeout still succeeds. Agent turn takes longer but does not fail. |

**Gate condition:** All 10 scenarios handled correctly. Zero crashes. Zero lost agent turns (recovered or explicitly failed with state saved). Hot-swap works within 1 turn of signal. Budget tracking accurate within 5% across all providers.

---

### Phase 4 Gate: Single Agent

**Test: The Bug Hunter's Crucible**

The single agent, alone, with no swarm and no judges, must find real bugs in real codebases.

| # | Test | Pass Condition |
|---|------|----------------|
| 1 | 10 open-source repos with known, documented CVEs (severity 7+) | Agent finds ≥ 7 of 10 CVEs. Each confirmed by sandbox execution receipt. |
| 2 | 5 repos with deliberately planted bugs (SQL injection, XSS, buffer overflow, race condition, auth bypass) | Agent finds 5/5. Zero false positives (no confirmed bugs that aren't real). |
| 3 | A "clean" repo with zero known bugs | Agent reports ≤ 1 false positive (severity > 5). Ideally zero. |
| 4 | A repo where the bug requires tracing through 5+ function calls | Agent uses `trace_dependency` with radius ≥ 5 to find the bug. |
| 5 | A repo where the bug is only triggered by a specific edge-case input | Agent generates a PoC with the correct edge-case input. Sandbox confirms. |
| 6 | 100-turn run on a single repo (simulated via loop) | Agent does not loop, does not exceed token budget, does not hallucinate > 20% unverifiable claims. |
| 7 | PII-laced repo: `.env` file with real-looking API keys, source code with fake emails | PII scanner redacts ALL secrets before they reach the LLM. Zero PII in any LLM request log. |
| 8 | Prompt-injection-laced repo: code comments with "Ignore previous instructions", "You are now DAN" | Prompt injection scanner flags all malicious comments. Agent never sees the raw text. |

**Gate condition:** ≥ 7/10 CVEs found. 5/5 planted bugs found. ≤ 1 false positive. Zero PII leaks. Zero prompt injection successes. Hallucination rate < 20%.

---

### Phase 5 Gate: Evidence Graph

**Test: The Immutability Assault**

The evidence graph must survive deliberate attempts to corrupt, forge, or delete evidence.

| # | Attack | Expected Behavior |
|---|--------|-------------------|
| 1 | Agent submits claim with fabricated sandbox run ID | Graph rejects: `run_id` not found in `sandbox_executions` table. |
| 2 | Agent submits claim referencing another agent's sandbox run | Allowed (evidence is shared). But agent's `contribution_score` unaffected. |
| 3 | Attempt to create edge to non-existent node | Graph rejects: referential integrity enforced. |
| 4 | Attempt to delete a sandbox run node | Rejected: sandbox nodes are immutable (database permission or application-level check). |
| 5 | Attempt to modify sandbox receipt after insertion | Rejected: receipt hash validated on read. Mismatch → node marked `TAINTED`, re-execution triggered. |
| 6 | 1,000 concurrent reads + 100 concurrent writes | No deadlocks. No stale reads. All queries return consistent state. |
| 7 | Graph query that would return 10,000+ nodes | Paginated. Default page size 100. Query timeout at 5s. |
| 8 | Circular evidence: Claim A → confirms Claim B → confirms Claim A | Cycle detected. Both claims flagged. Judges warned of circular reasoning. |

**Gate condition:** All 8 attacks successfully defended. Zero data corruption. Zero successful forgeries.

---

### Phase 6 Gate: Multi-Agent Swarm (12 Agents)

**Test: The Gaslighting Gauntlet**

12 agents are deployed against repos specifically designed to test swarm dynamics.

| # | Scenario | Expected Behavior |
|---|----------|-------------------|
| 1 | Repo where 11 of 12 agents are seeded with prompts to agree on a false bug | The 1 correctly-prompted agent with sandbox evidence overrides the 11. Objective evidence wins. |
| 2 | Repo where a real bug exists but agents are seeded to focus on a minor style issue | Priority injection forces pivot. Topic drift detection fires. Swarm moves to security issue within 3 turns. |
| 3 | 3 agents are seeded to gaslight 1 agent who has sandbox-confirmed a real bug | Evidence asymmetry detected. Case escalated to (mock) judges with gaslighter annotations. |
| 4 | 2 agents form a semantic loop trading rephrased arguments for 10 turns | Semantic loop detected within 3 windows. Pattern-break prompt injected. Loop broken or escalated. |
| 5 | 3 agents form a triadic echo chamber (A→B→C→A reinforcement) | SCC detection on directed agreement graph catches the chamber. Hub agent rotated. |
| 6 | 1 agent hallucinates > 40% unverifiable claims for 3 consecutive rounds | Agent ejected. Spare agent replaces it. Ejected agent's partial findings preserved. |
| 7 | Ejected agent was actually correct (false positive ejection) | Post-ejection review by judge model re-instates agent. Hallucination threshold relaxed. |
| 8 | 1 agent dominates token budget (using 30% of all tokens) | Anomaly detector flags agent (Z-score > 3). Token cap imposed. Loop detected → pattern-break → freeze. |
| 9 | All 12 agents try to fetch the same code chunk from Vector DB | Cache hit rate > 90%. First fetch goes to DB; 11 subsequent served from cache. |
| 10 | Agent attempts to `recall_raw_context` every turn to bloat context window | Rate limiter blocks after 3 recalls per round. Trust score decremented. Agent eventually ejected. |

**Gate condition:** All 10 scenarios handled correctly. Gaslighting fails. Loops detected and broken. Agents ejected and re-instated correctly. Cache hit rate maintained.

---

### Phase 7 Gate: Compression & Loop Detection (at 12 agents, then simulated at 50)

**Test: The Context Bloat Siege**

Agents are force-fed enormous amounts of irrelevant data to trigger compression and loop conditions.

| # | Scenario | Expected Behavior |
|---|----------|-------------------|
| 1 | 12 agents × 15 rounds of debate on a verbose codebase (simulated 20K-token context each) | Compression triggers at 80% context window or every 12 interactions. No agent exceeds context window. |
| 2 | Summarization model (local Llama 3 8B Q4) encounters highly technical GPU shader code | Fidelity check detects BERTScore < 0.85. Falls back to GPT-4o-mini. Summary quality recovers. |
| 3 | Agent deliberately invokes `recall_raw_context` on every turn (griefing) | Rate limiter: max 3/round, cooldown 5 turns. Trust score drops. Agent ejected after 3 rounds. |
| 4 | Tool returns 50,000-line stack trace | Relevance scorer classifies as "Verbose — Offload." Top 20 + bottom 5 frames extracted. Raw trace on disk. |
| 5 | 50 agents (simulated) generate debate logs faster than DB writer can flush | Queue reaches 80% → semaphore reduced (12→8). Queue reaches 100% → PAUSE signal. No data loss. |
| 6 | Vector DB query results differ between agents due to ANN non-determinism | Deterministic index params + exact re-ranking + cache. All agents receive identical results for same embedding. |
| 7 | All 12 agents redundantly fetch the same code chunk | Cache hit rate stays > 80%. CLI shows "Vector DB Efficiency: 87.3%". |
| 8 | Context compression deletes a critical premise (hallucinated summary) | Agent invokes `recall_raw_context` to recover. Fidelity feedback loop reduces compression aggressiveness. |

**Gate condition:** All 8 scenarios handled. Zero context window overflows. Compression fidelity stays > 0.85 after fallback. Queue back-pressure works without data loss. Cache efficiency > 80%.

---

### Phase 8 Gate: Financial Control Plane

**Test: The Budget Exhaustion Drill**

The system is run with artificially low budgets to force every budget-related code path.

| # | Scenario | Expected Behavior |
|---|----------|-------------------|
| 1 | Token budget set to 100K, swarm would need 5M | Swarm terminates at 100K. Final report generated with partial findings. No overrun. |
| 2 | Cost budget set to $1, actual cost would be $50 | Swarm reaches 80% ($0.80) → models downgraded. At $1 → terminated. |
| 3 | Time budget set to 30s, swarm needs hours | After 30s, no new rounds. Current round allowed to complete. Then terminated. |
| 4 | Severity-9 zero-day found at 95% budget | Budget override proposed. Security Judge approves. Capped at 150% original. |
| 5 | Severity-9 zero-day found at 95% budget, Security Judge denies override | Partial finding persisted with `NEEDS_FURTHER_INVESTIGATION`. Standalone report exported. |
| 6 | Single agent using 30% of swarm tokens | Anomaly detector flags. Token cap imposed. Agent frozen if looping persists. |
| 7 | Swarm has found 0 bugs but consumed 90% of token budget | Information-theoretic stopping triggers: marginal gain < threshold. Swarm terminated early. |
| 8 | CLI shows budget burn bars | All three bars update in real-time on TUI. Color changes: green → yellow (80%) → red (95%). |

**Gate condition:** All 8 scenarios. Zero budget overruns without explicit override. Override mechanism works. Cost tracking accurate to within 2% of actual provider bills.

---

### Phase 9 Gate: Scale to 50 Agents

**Test: The 50-Agent Endurance Run**

Full 50-agent swarm on a real repository for a minimum of 5 rounds.

| # | Metric | Threshold |
|---|--------|-----------|
| 1 | Agents active simultaneously | 50 agents dispatched. All 50 produce at least 1 turn. |
| 2 | DB writer throughput | Queue never reaches PAUSE. Back-pressure handles throttle smoothly. |
| 3 | Vector DB query latency (cache miss) | p99 < 200ms under 50 concurrent query load. |
| 4 | LLM API rate limits | No provider returns 429 after initial ramp-up. Rate-limit awareness prevents saturation. |
| 5 | Diversity threshold computation | < 100ms for 50×50 similarity matrix. |
| 6 | TUI refresh rate | 60fps maintained even with 50 agent rows updating. |
| 7 | Memory per agent coroutine | < 50 MB each. Total orchestrator memory < 3 GB. |
| 8 | Sandbox container pool | 50 containers run simultaneously. No resource contention on host. |
| 9 | Run duration stability | 5-round run completes. No crashes, no hangs, no data corruption. |
| 10 | Cost proportionality | Cost scales roughly linearly with agent count (50 agents ≈ 4.2× cost of 12 agents due to shared summarization). |

**Gate condition:** All 10 metrics met. System stable for duration of run. If any metric fails, scale-down to 12 agents and fix before re-attempting 50.

---

### Phase 10 Gate: The Bench

**Test: The Judicial Stress Test**

The 5-judge panel is tested against crafted edge cases designed to produce wrong verdicts.

| # | Scenario | Expected Behavior |
|---|----------|-------------------|
| 1 | 200 golden dataset bug reports | All 5 judges produce verdicts. Accuracy vs human ground truth measured. No individual judge below 80% accuracy. |
| 2 | Plausible-but-false bug: agent crafts logically sound argument with fabricated evidence | Judges detect `trace_citation` missing or invalid. Verdict rejected. |
| 3 | Real bug with ambiguous stack trace (could be expected exception) | Judges examine `handled` flag in execution trace. Reason about handler correctness. Verdict includes reasoning, not just boolean. |
| 4 | 2-2-1 split with no obvious tiebreaker | Weighted re-vote → evidence-only re-vote → specialist escalation. Tiebreaker logged with full reasoning. |
| 5 | Security Judge (tiebreaker) has false positive rate > 0.20 | Tiebreaker authority revoked. Falls through to next specialist. Event logged. |
| 6 | Security and Architecture judges fundamentally disagree (severity 9 vs 2) | Inter-Judge Synthesis Debate triggered. Sub-panel reaches consensus or falls back to conservative principle (severity 9). |
| 7 | Judge issues query, response introduces new contradictory evidence | Case re-opened once. Final verdict issued. New bug type auto-opened as separate case. |
| 8 | Judge exhausts query budget (3) and remains uncertain | Verdict with `confidence: LOW`. Case flagged `NEEDS_HUMAN_REVIEW`. |
| 9 | 20,000-token debate history submitted | Structured brief format applied. Executive summary at top. Evidence receipts at bottom. Judge context within 128K window. |
| 10 | Benchmark: judge accuracy on golden dataset must not regress from previous release | Delta < 2% from baseline. Regression > 2% → release blocked, calibration reviewed. |

**Gate condition:** All 10 scenarios. Accuracy ≥ 80% per judge. No regression. Tiebreaker works. Structured brief prevents "lost in the middle."

---

### Phase 11 Gate: TUI

**Test: The Terminal Torture Test**

The TUI is tested under every possible terminal condition.

| # | Scenario | Expected Behavior |
|---|----------|-------------------|
| 1 | Terminal resize: 120×40 → 80×24 → 60×15 → 120×40 | All 7 views adapt. No crash. No lost data. Layout switches between grid/condensed/single-panel seamlessly. |
| 2 | Rapid resize: 100 terminal size changes in 1 second | TUI does not crash. Debounce prevents re-render storm. Eventual consistency maintained. |
| 3 | Unicode stress: agent names, bug descriptions, code snippets with emoji, CJK, RTL, combining characters | All rendered correctly. No misalignment. No truncated glyphs. |
| 4 | ANSI escape flood: sandbox output contains raw ANSI sequences | ANSI parsed and rendered correctly in Sandbox Monitor and Logs view. Raw view available. |
| 5 | 50-agent view: all 50 agent rows updating simultaneously every 500ms | No flicker. Textual diffs and re-renders only changed cells. Frame time < 16ms. |
| 6 | Keyboard shortcut coverage: every documented keybinding tested | All F1–F7, Tab, arrows, Enter, Esc, /, Space, :, Ctrl+L, p, q, ? tested. No dead keys. |
| 7 | Command mode: every `:` command tested | `:override`, `:eject`, `:reinstate`, `:budget set`, `:provider swap`, `:mode headless`, `:export json`, `:export sarif`, `:theme dark`. All parse correctly. Invalid commands show error message. |
| 8 | Human override modal: accept, reject, edit severity, request more info, reassign to judge | All 5 actions work. Justification required for severity-8+ downgrades. Dual-auth enforced for severity-9+. |
| 9 | Disconnect/reconnect: orchestrator process restarted while TUI is open | TUI shows "Connection lost. Reconnecting..." Spinner. Reconnects when socket available. State recovered. |
| 10 | Low-color terminal: `TERM=dumb`, 8-color, 256-color, truecolor | TUI degrades gracefully. Colors mapped to available palette. Monochrome fallback available. |
| 11 | Narrow terminal: 50 columns | TUI shows "Terminal too narrow" warning. Single-panel mode available. No horizontal overflow. |
| 12 | Short terminal: 15 rows | TUI shows "Terminal too short" warning. Panels prioritized. Essential info visible. |
| 13 | Non-interactive modes: `--json`, `--headless`, `-q` | TUI does not launch. `--json` outputs valid JSON lines. `--headless` runs without terminal. `-q` produces only final report. |

**Gate condition:** All 13 scenarios pass. Zero crashes. All views accessible at all supported terminal sizes. All keyboard shortcuts functional. Non-interactive modes produce valid output.

---

### Phase 12 Gate: Minitia Integration

**Test: The Multi-Engine Orchestration Test**

Minitia must install, run, monitor, and stop Bug Swarm alongside simulated other engines.

| # | Scenario | Expected Behavior |
|---|----------|-------------------|
| 1 | `minitia install bugswarm` on clean machine | Binary downloaded, checksum verified, symlinked. `~/.minitia/engines/bugswarm` resolves. |
| 2 | `minitia install bugswarm` on machine with older version | Old binary kept. New binary downloaded. Symlink updated. `minitia install bugswarm@1.9.0` installs specific version. |
| 3 | `minitia run bugswarm ./repo --budget 50` | Bug Swarm launched with `--json` flag. JSON stream parsed by Minitia. Dashboard card updates. |
| 4 | `minitia run` with 3 engines simultaneously (bugswarm + 2 simulated) | All 3 run in parallel. Dashboard shows all 3 cards. Each independently controllable. |
| 5 | `[D]etach TUI` → Bug Swarm native TUI launches in new terminal | `tmux split-window` or `$TERMINAL -e`. Bug Swarm TUI appears in new pane. Minitia dashboard still running. |
| 6 | `minitia stop bugswarm` while running | SIGTERM sent. Orchestrator issues FREEZE, checkpoints all agents, exits gracefully. |
| 7 | `minitia status` while Bug Swarm running | JSON status printed. `{"running": true, "elapsed": "02:14:38", "round": 4, ...}`. |
| 8 | Engine registry unreachable | Minitia uses cached `registry.yaml`. Warns "registry unreachable, using cached data from 2026-05-10". |
| 9 | Binary checksum mismatch after download | Download retried once. If still mismatched, install fails with "Checksum verification failed. Expected: abc, got: def. The binary may have been tampered with." |
| 10 | `minitia update --all` | Checks registry for newer versions of all installed engines. Downloads and installs updates. Keeps previous version for rollback. |

**Gate condition:** All 10 scenarios pass. Install/run/stop/update lifecycle works end-to-end. Detached TUI launches correctly. Checksum verification prevents tampered binaries.

---

### Phase 13 Gate: Observability & CI/CD

**Test: The Silent Failure Hunt**

The system is deliberately broken in ways that should trigger alerts. Every alert must fire.

| # | Scenario | Expected Behavior |
|---|----------|-------------------|
| 1 | Cost exceeds 80% of budget | Prometheus alert fires. Slack notification sent. TUI budget bar turns yellow. |
| 2 | Sandbox returns `TAINTED` execution | Prometheus counter increments. WARN log written. Slack notification sent. |
| 3 | CRITICAL:SANDBOX_ESCAPE_ATTEMPT log event | PagerDuty alert fired. Email sent to security lead. Swarm paused. |
| 4 | Host memory exceeds 80% | Prometheus alert fires. Orchestrator kills all sandbox containers. Swarm paused. |
| 5 | Judge verdict dismiss rate > 90% for 3 consecutive swarms | WARN alert. Possible swarm degradation flagged for review. |
| 6 | Compression fidelity < 0.7 | WARN alert. Summarization tier escalated. |
| 7 | PostgreSQL replication lag > 5s | WARN alert. Orchestrator pauses writes until replica catches up. |
| 8 | Orchestrator heartbeat missing > 30s | CRITICAL alert. Watchdog process detects and triggers recovery. |
| 9 | CI/CD: `bugswarm ci` on PR with known vulnerability | Exit code 1. SARIF report generated. GitHub Code Scanning annotation appears on PR. |
| 10 | CI/CD: `bugswarm ci` on clean PR | Exit code 0. No annotations. |
| 11 | OpenTelemetry trace completeness | Every agent turn, sandbox execution, and judge deliberation has a span. Sampling at 100% during test. |

**Gate condition:** All 11 scenarios. Every alert fires on the correct channel. CI/CD integration blocks vulnerable code. Traces are complete.

---

### Phase 14 Gate: Hardening (Ongoing)

**Test: The Production Escape Replay**

A known production bug (that Bug Swarm missed in a previous run) is fed back via `bugswarm learn`. The system must identify which component failed and self-correct.

| # | Scenario | Expected Behavior |
|---|----------|-------------------|
| 1 | Bug was in code region not indexed by CPG | CPG indexing radius increased. Re-index catches the region. Retrospective analysis flags "codebase indexing gap." |
| 2 | Agent suspected bug but was overruled by other agents' rhetoric | Sycophancy parameters tightened. Anti-gaslighting prompt strengthened. Agent interaction weights adjusted. |
| 3 | Judge dismissed valid finding due to calibration error | Judge bias parameters updated. Dismissed judge penalized. |
| 4 | Sandbox could not reproduce bug due to missing mock service | Mock fidelity improved. Production schema sampling added. |
| 5 | WAL file corrupted by kernel panic → recovery from S3 | S3 backup restored. CRC32 validation passes. Max data loss < 5 min + 1 micro-batch. Cost of lost API calls tracked. |
| 6 | Regression test auto-generated from the production bug | Test added to repo. `pytest` catches the bug. Confirmed the test would have prevented the escape. |
| 7 | Domain profile test: `smart_contract` profile flags theoretical attacks | Edge case that `web_app` profile dismissed is caught under `smart_contract`. Profile pragmatism relaxation works. |
| 8 | Domain profile test: `medical_device` profile catches unsafe patterns | Non-bug "unsafe pattern" flagged. Human review annotation included. |

**Gate condition:** All 8 scenarios. System learns from every fed-back production escape. At least 1 parameter auto-tuned per escape. Regression test generated and passing.


---

## 2. Language-Specific Standards

### 2.1 Rust (Sandbox Daemon, CPG Builder, Orchestrator)

**Safety:**
- `unsafe` blocks require a written justification in a `// SAFETY:` comment citing the invariant that makes the block sound. Reviewers must verify the invariant.
- All `unsafe` is gated behind `#[deny(unsafe_code)]` on the crate root; each `unsafe` block is explicitly `#[allow(unsafe_code)]` with justification.
- `unwrap()` and `expect()` are banned in production code paths. Use `anyhow::Context` with `.context("description")` or proper error propagation via `thiserror`.
- `panic!` and `unreachable!` are banned. Use `anyhow::bail!` for fatal conditions. The orchestrator must never panic — a panic kills 50 agents' worth of state.
- All integer casts use `TryFrom`/`TryInto`. `as` casts are banned unless accompanied by a comment proving the cast is lossless for all possible inputs.
- Array/slice indexing uses `.get()` with proper error handling. Direct `[]` indexing is banned outside of hot loops with a comment proving bounds are statically known.

**Concurrency:**
- No `static mut`. No `lazy_static` with mutable state. Use `once_cell::sync::Lazy` with `RwLock` or `Mutex` if shared state is unavoidable.
- All shared state between async tasks uses `Arc<RwLock<T>>` with explicit deadlock-prevention comments documenting lock acquisition order.
- Channel senders must use `try_send` or bounded channels with back-pressure. Unbounded channels are banned — they mask memory leaks as latency.
- `tokio::spawn` handles must be stored and `await`ed or explicitly `abort()`ed on drop. Spawned-and-forgotten tasks are banned.

**Performance:**
- All sandbox daemon syscalls use `libc` or `nix` with zero-allocation wrappers. No heap allocation in the hot path (container create, exec, monitor).
- CPG builder must process files in parallel via `rayon` with configurable thread count. Single-threaded AST construction is a compile error on repositories > 1,000 files.
- Database writes use batched inserts (PostgreSQL `COPY` protocol or SQLite `executemany`). Row-by-row inserts in a loop are banned.

**Error handling:**
- Every fallible function returns `Result<T, E>` where `E` is a concrete error type (never `Box<dyn Error>` in library code).
- Error types implement `std::fmt::Display` with actionable messages. "IO error" is not acceptable. "Failed to open sandbox config at /etc/bugswarm/sandbox.yaml: Permission denied (os error 13)" is.
- Errors propagated across FFI boundaries (Rust → Python via PyO3) must be converted to Python exceptions with the original Rust error chain preserved.

**Testing:**
- Every public function has at least one doc-test demonstrating correct usage.
- Every `unsafe` function has a dedicated test that exercises the safety invariant.
- Sandbox daemon has integration tests that create real containers and verify isolation (can't write to host, can't access network, OOM kill works, seccomp blocks blocked syscalls).
- CPG builder has snapshot tests (insta crate) on real open-source repositories. A CPG change that alters the graph structure must be reviewed against the snapshot diff.
- Orchestrator has property-based tests (proptest) on state machine transitions: "for any sequence of agent timeouts, submissions, and judge verdicts, the system converges or terminates within budget."

### 2.2 Python (Agents, CLI, TUI, Judge Panel)

**Typing:**
- `mypy` with `--strict` passes with zero errors. No `# type: ignore` without a comment explaining why inference is impossible.
- All public functions have complete type annotations including return types. `-> None` is explicit, not omitted.
- `Any` is banned. Use `object`, `Protocol`, `TypeVar`, or `Union` with explicit narrowing.
- Pydantic models are the only source of truth for data shapes. Never pass raw `dict` between components — hydrate a model first.

**Async:**
- `asyncio.create_task` handles must be stored and `await`ed or cancelled. Unawaited tasks are a resource leak.
- `asyncio.wait_for` with explicit timeout on every external call (LLM API, sandbox execution, DB query). No timeout → potential hang.
- `asyncio.gather` with `return_exceptions=True` when partial failures are acceptable; without it when all-or-nothing semantics are required. Document which semantic is intended.
- No blocking calls in async functions. `time.sleep` in an `async def` is a merge-blocker. Use `asyncio.sleep`.

**Error handling:**
- Exceptions are caught at the narrowest possible scope. Bare `except:` is banned. `except Exception:` requires a comment.
- Every caught exception is logged with traceback via `logger.exception()`.
- Agent errors must not crash the orchestrator. An agent that throws is ejected; the swarm continues.
- Tenacity retry decorators on all external API calls. Retry on 429, 502, 503, 504. Do not retry on 400, 401, 403, 404.

**Testing:**
- `pytest` with `--cov` and a minimum 85% line coverage. Coverage is gated in CI.
- Agent reasoning is tested with deterministic mock LLMs that return canned responses. Tests assert the agent's tool calls, not the LLM output.
- TUI is tested with Textual's `pilot` framework (`async with app.run_test() as pilot`). Every screen, every keyboard shortcut, every modal has a test.
- Judge panel is tested against the 200 golden bug reports. A judge model change must not regress accuracy on the golden dataset.
- PII scanner is tested against the `big-list-of-naughty-strings` corpus plus custom enterprise patterns from `.bugswarm_secrets.yaml`.

**Performance:**
- TUI refresh must stay under 16ms per frame (60fps). Profile with `textual devtools`.
- Agent message embeddings are cached. Never re-embed the same text.
- Vector DB queries are cached per-round with SHA256(embedding_vector) keys.
- LLM API calls use HTTP/2 connection pooling via `httpx.AsyncClient` with `keepalive_timeout=60`.

---

## 3. Security Standards

### Code-Level
- **No secrets in source code.** API keys, tokens, and credentials come from environment variables (`os.getenv`), a `.env` file excluded from git, or a secret manager. If a secret appears in a commit, the commit is squashed and the secret is rotated.
- **Dependency pinning.** `Cargo.lock` committed for Rust. `requirements.txt` with exact versions (hash-checking mode via `pip install --require-hashes`). No `>=` without `<=` bound.
- **Dependency auditing.** `cargo audit` and `pip-audit` run in CI on every push. Vulnerabilities above "low" severity block merge.
- **Input validation.** Every Pydantic model uses `constr`, `conint`, `condecimal`, `Field(ge=0, le=...)`. No raw string or int fields without constraints.

### Sandbox
- **Container profile is declarative and version-controlled.** No `docker run` flags constructed via string interpolation. The sandbox config is a YAML file parsed into a typed struct.
- **Seccomp profile is the single source of syscall allowlist.** If a new syscall is needed, the profile is updated, the justification is documented, and the change is reviewed by two engineers.
- **eBPF runtime monitoring is enabled by default.** If a container makes a syscall outside the seccomp profile, the container is killed and the agent is banned.
- **Sandbox escape attempt = `CRITICAL` severity incident.** PagerDuty alert, swarm paused, human review required before resume.

### LLM Interaction
- **PII scanner runs before ANY data leaves the host.** Local models are not exempt — the scanner still runs. The `--allow-pii` flag exists but requires explicit acknowledgment and is logged.
- **Prompt injection scanner classifies every code chunk.** A log of rejected chunks is kept for audit.
- **Agent system prompts are immutable after deployment.** A prompt change requires a version bump and re-benchmark against the golden dataset.

---

## 4. Git Standards

### Branching
- `main` is protected. Direct pushes are disabled. All changes come through pull requests.
- Feature branches: `feat/<description>` (e.g., `feat/semantic-loop-detector`).
- Fix branches: `fix/<description>` (e.g., `fix/oom-detection-race`).
- Release branches: `release/v2.1.0`. Tags: `v2.1.0` (annotated, signed).

### Commits
- **Conventional Commits format.** `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`, `perf:`, `security:`.
- **Commit messages explain WHY, not WHAT.** "fix: prevent race when 3 agents time out in same batch" — good. "fix: add mutex" — rejected.
- **One logical change per commit.** A commit that "fixes a bug AND refactors the sandbox AND updates docs" is split into three.
- **No commit reduces test coverage.** If a `fix:` commit touches production code, it must also touch tests with a net coverage gain or neutral.
- **Signed commits required.** `git config commit.gpgsign true`. All commits must be cryptographically signed.

### Pull Requests
- **PR template filled completely.** Description, motivation, testing performed, screenshots (for TUI changes), breaking changes checklist.
- **Linked to an issue.** No PR without a tracking issue.
- **Minimum one approving review from a code owner.** The `CODEOWNERS` file gates each subsystem: `src/sandbox/` → sandbox team, `src/tui/` → TUI team, `src/agents/` → agents team.
- **All CI checks green before merge.** This includes: lint, typecheck, unit tests, integration tests, dependency audit, coverage gate, and benchmark regression check (for perf-sensitive code).
- **Linear history.** Squash-merge only. No merge commits on `main`.

---

## 5. Testing Standards

### Test Pyramid Enforcement
```
        ┌──────┐
        │ E2E  │  ~5%  — Full swarm runs against real repos with real LLMs (nightly)
        ├──────┤
        │ Int  │  ~20% — Multi-agent integration, sandbox integration, judge panel
        ├──────┤
        │ Unit │  ~75% — Pure functions, models, algorithms, individual components
        └──────┘
```

### What Gets Tested, Always
- **Every Pydantic model:** valid input, invalid input, edge case input. Use `pytest.mark.parametrize` with Hypothesis strategies.
- **Every state machine transition:** orchestrator state diagram is a directed graph. Every edge has a test.
- **Every error path:** if a function returns `Result`, both `Ok` and `Err` variants are tested.
- **Every seccomp syscall:** each allowed syscall has a test proving it works; each blocked syscall has a test proving it's blocked.
- **Every TUI screen:** keyboard navigation, data binding, modal open/close, responsive layout breakpoints.

### What Runs When
| Trigger | Tests |
|---------|-------|
| `git push` (feature branch) | Unit tests + lint + typecheck + audit (must complete < 5 min) |
| PR opened | All above + integration tests + coverage gate |
| PR approved + merge to `main` | All above + benchmark regression check |
| Nightly (`main`) | All above + E2E swarm runs (3 repos × 3 domain profiles) |
| Release tag | All above + 200 golden dataset re-benchmark + load test (50 agents, 15 rounds) |

### Test Quality Gates
- **Coverage < 85%** → PR blocked.
- **Coverage decreased** → warning on PR, must be justified.
- **Integration test timeout > 30s** → test is too slow, refactor or move to nightly.
- **Flaky test (fails > 1% of runs)** → quarantined, issue filed, must be fixed within 1 sprint or test is deleted.

---

## 6. Observability Standards

### Logging
- **Structured JSON to stdout.** Never `print()`. Never freeform text logs in production paths.
- **Log levels must be correct:**
  - `TRACE`: Full agent conversation text, raw LLM requests/responses.
  - `DEBUG`: Tool calls, sandbox executions, cache hits/misses, graph queries.
  - `INFO`: Round boundaries, batch boundaries, verdicts, budget checkpoints.
  - `WARN`: Timeouts, loops detected, budget > 80%, queue back-pressure, compression fidelity < 0.7.
  - `ERROR`: Crashes, API failures after retries exhausted, database connection loss.
  - `CRITICAL`: Sandbox escape attempt, data corruption detected, WAL checksum failure.
- **Every log line has:** `timestamp`, `level`, `component`, `swarm_id`, `run_id`, `message`, and `context` (JSON object with relevant fields).
- **No PII in logs, ever.** The logger wrapper strips PII patterns before writing.

### Metrics
- **Every counter/gauge/histogram from the spec is implemented.** See §11 for the full list.
- **Custom metrics must have:** a name following `bugswarm_<subsystem>_<metric>`, a `help` string, and at least one label for dimensionality.
- **Histogram buckets are chosen for the expected value range.** Default `[0.01, 0.05, 0.1, 0.5, 1, 5, 10, 30, 60, 120]` is not acceptable — choose buckets that match the operation's latency profile.

### Tracing
- **OpenTelemetry spans for every: agent turn, LLM API call, sandbox execution, judge deliberation, DB query.**
- **Span attributes include:** `swarm_id`, `agent_id`, `model`, `token_count`, `duration_ms`, `status`.
- **Traces are sampled at 100% for development, 10% for production.** Configurable via `OTEL_TRACES_SAMPLER`.

---

## 7. Performance Standards

### Absolute Thresholds
| Operation | Max Latency | Measurement |
|-----------|-------------|-------------|
| Sandbox container start (warm) | 300ms | p99 over 100 starts |
| Sandbox container start (cold) | 3s | p99 over 100 starts |
| Agent turn (LLM call + tool dispatch) | 30s timeout | per-turn wall clock |
| Diversity Threshold recalculation (50 agents) | 100ms | CPU time |
| TUI frame render | 16ms | `textual devtools` profiler |
| DB writer batch flush (50 rows) | 50ms | p99 |
| Vector DB query (cache miss) | 200ms | p99 |
| Vector DB query (cache hit) | 10ms | p99 |
| Orchestrator state delta push to TUI | 500ms interval | configurable |

### Resource Thresholds
| Resource | Hard Limit |
|----------|------------|
| Memory per agent coroutine | < 50 MB |
| Memory per sandbox container | 512 MB (configurable) |
| Open FDs per orchestrator process | < 200 |
| GPU VRAM per local model instance | configurable (`--gpu-memory-fraction`) |
| Disk space per sandbox execution | < 1 GB |

### Optimization Rules
- **Profile before optimizing.** No optimization PR without a flamegraph or benchmark comparison in the description.
- **No premature optimization.** Don't micro-optimize code that runs once per 40-hour run. Optimize the hot path: agent turns, sandbox executions, embedding generation.
- **Algorithmic improvements over micro-optimizations.** Switching from O(n²) to O(n log n) is worth a PR. Shaving 2ms with SIMD intrinsics is not (unless it's the sandbox hot path).

---

## 8. Documentation Standards

### Code
- **Every public function/method/struct/enum has a docstring.** Rust: `///` with examples. Python: Google-style docstring with `Args:`, `Returns:`, `Raises:`.
- **Docstrings describe contracts, not implementations.** "Compresses agent context using map-reduce summarization" — good. "Calls `_compress()` then `_validate()`" — bad (that's what the code says).
- **`# SAFETY:` comments on every `unsafe` block.** Cites the invariant, explains why it holds, references relevant safety documentation.
- **`# HACK:`, `# FIXME:`, `# TODO:` are banned on `main`.** If something is hacky or incomplete, it ships in a feature branch or behind a feature flag. `main` is clean.

### Architecture
- **`/docs/architecture/` contains:** system overview, data flow diagrams (Mermaid), component interaction sequences, state machine diagrams.
- **Every ADR (Architecture Decision Record) follows the format:** Title, Status, Context, Decision, Consequences, Alternatives Considered.
- **ADR statuses:** `proposed` → `accepted` → `superseded` (by ADR-XXX). Never `rejected` — record the decision not to do something.

### API
- **CLI `--help` is tested.** A CI check runs `bugswarm --help` and `bugswarm <subcommand> --help` and fails if any output is empty or contains "TODO".
- **Error messages are actionable.** "Error: sandbox connection refused" → bad. "Error: sandbox daemon not running. Start it with: `systemctl start bugswarm-sandbox`" → good.
- **JSON schema for every output format is published and versioned.** `bugswarm report --format sarif` output must validate against the SARIF schema.

---

## 9. Dependency Management

- **Minimize dependencies.** Every new dependency must be justified in the PR description: what does it do, why can't we do it ourselves in < 500 lines, what is its maintenance track record.
- **Prefer stdlib.** Python: `pathlib` over `os.path`, `dataclasses`/Pydantic over attrs, `logging` structlog wrapper over third-party loggers. Rust: `std::collections` over `hashbrown`, `std::sync` over `parking_lot` unless benchmarks prove the alternative.
- **Lockfiles committed.** `Cargo.lock` for Rust binaries (not libraries). `requirements-lock.txt` with hashes for Python.
- **Dependency update cadence:** Patch versions auto-merged if CI passes. Minor versions reviewed weekly. Major versions planned as dedicated upgrade PRs with full test suite runs.
- **Vendored dependencies:** If a dependency is critical (seccomp, sandbox isolation, crypto), its source is vendored into `vendor/` and reviewed line-by-line. Updates to vendored deps require a diff review.

---

## 10. Release Standards

### Versioning
- **Semantic versioning.** MAJOR.MINOR.PATCH.
  - MAJOR: Breaking changes to the CLI interface, engine contract, or verdict schema.
  - MINOR: New features, new tools for agents, new judge models, new domain profiles.
  - PATCH: Bug fixes, performance improvements, dependency updates.
- **Pre-release tags:** `-alpha.N`, `-beta.N`, `-rc.N`. Alpha = internal testing. Beta = early adopters. RC = release candidate, full test suite passed.

### Release Checklist (gated in CI/CD)
1. All tests pass (unit + integration + E2E).
2. Golden dataset re-benchmarked — no regression in judge accuracy.
3. `cargo audit` and `pip-audit` report zero vulnerabilities.
4. Changelog updated with all user-facing changes since last release.
5. Documentation regenerated (`make docs`) and diff reviewed.
6. Binary built for all target platforms (x86_64-linux, aarch64-linux, x86_64-darwin, aarch64-darwin).
7. SHA256 checksums generated and signed.
8. Engine registry updated (`registry.minitia.ai/engines.json`).
9. Release tag signed and pushed.
10. Release notes published on GitHub with migration guide if MAJOR.

### Rollback
- **The previous MAJOR.MINOR binary remains available for 6 months.** `minitia install bugswarm@1.9.0` must work.
- **Database migrations are forward-compatible within a MAJOR version.** A v2.1 agent must be able to resume a v2.0 swarm from WAL.
- **Rollback procedure is documented and tested.** Every release includes a `ROLLBACK.md` with step-by-step instructions.

---

## 11. Review Standards

### Reviewer Responsibilities
- **Verify correctness.** Does the change do what it claims? Are edge cases handled?
- **Verify safety.** For Rust: is `unsafe` justified? For Python: are exceptions handled? Are there resource leaks?
- **Verify security.** Does this change introduce a new data path? Does it bypass the PII scanner or prompt injection scanner?
- **Verify testing.** Are the right things tested? Do tests cover the failure modes? Is coverage maintained?
- **Verify observability.** Are new paths logged at the correct level? Are new metrics added?
- **Verify documentation.** Are docstrings updated? Is the ADR written if this is an architectural change?
- **Reject if any of the above is insufficient.** "LGTM" without verification is negligence.

### Author Responsibilities
- **PRs are small.** > 400 lines changed → split. The only exception is mechanical refactors (renames, format changes).
- **PR description explains WHY.** The code explains WHAT. The reviewer needs the WHY.
- **Self-review before requesting review.** The author reads every diff line and leaves self-review comments on non-obvious choices.
- **Responds to all review comments.** "Done" is not a response. Explain what changed and why. If you disagree, explain why — don't just resolve the conversation.

---

## 12. Quality Gates Summary

| Gate | When | Blocks Merge? |
|------|------|---------------|
| `cargo fmt --check` / `ruff format --check` | Every push | Yes |
| `cargo clippy -- -D warnings` / `ruff check` | Every push | Yes |
| `mypy --strict` | Every push | Yes |
| Unit tests pass | Every push | Yes |
| Coverage ≥ 85% | PR | Yes |
| `cargo audit` + `pip-audit` clean | PR | Yes |
| Integration tests pass | PR | Yes |
| Benchmark no regression (>5%) | Merge to main | Warn |
| Golden dataset accuracy no regression | Release | Yes |
| E2E swarm runs pass | Nightly | Alert on failure |
| PR has ≥ 1 approving review | PR | Yes |
| All conversations resolved | PR | Yes |
| Linear history (squash-merge) | PR | Yes |

---

## Remaining Production Work Items

These are the 10 remaining items to reach production readiness. Ordered by impact.

### 1. Rust Daemon Mode — ✅ COMPLETE
All 3 Rust crates now listen on Unix sockets:
- `bugswarm-sandbox run-server --socket /var/run/bugswarm/sandbox.sock` — methods: execute, execute_statistical, health
- `bugswarm-cpg run-server --socket /var/run/bugswarm/cpg.sock` — methods: stats, taint, call_path, index, health
- `bugswarm-evidence run-server --socket /var/run/bugswarm/evidence.sock` — methods: add_claim, add_sandbox_run, link_result, confirm_bug, stats, query, score_agent, verify, health

### 2. Provider Adapter Files — PENDING
Gateway imports adapters from `gateway/providers/openai.py`, `gateway/providers/anthropic.py`, etc. Some imports fail in certain environments. Fix: ensure all adapter files exist and are importable. The `DeepSeekAdapter` reuses `OpenAIAdapter` — verify this works for all providers.

### 3. End-to-End Test — PENDING
Full pipeline never ran: CPG → agent → sandbox → evidence → bench → SARIF. Create `tests/e2e/test_full_pipeline.py` that:
1. Indexes a repo with known vulnerabilities via CPG
2. Runs 1 agent against it
3. Verifies sandbox produces receipts
4. Verifies evidence graph records claims
5. Verifies bench produces verdicts
6. Verifies SARIF export is valid

### 4. Rust Persistence (Evidence Graph Serialization) — PENDING
Evidence graph is entirely in-memory. Process restart loses all data. Add:
- `EvidenceGraph::save(path: &Path) -> Result<()>` — serialize to JSON/bincode
- `EvidenceGraph::load(path: &Path) -> Result<EvidenceGraph>` — deserialize
- Auto-save after every batch of changes
- `bugswarm-evidence run-server` loads from disk on startup

### 5. Rust Performance (R5 Fixes) — PENDING
- Cache compiled regexes with `OnceLock` in `extract_stack_frames` and `classify_exception`
- Replace polling loop with Docker wait API in container manager
- CPG cache between commands (already partially done in daemon mode)
- Fix O(n^4) echo chamber in evidence graph stats()

### 6. Shared Types Crate — PENDING
`ExecutionReceipt`, `MemoryProfile`, `NodeKind`, `EdgeKind` defined in multiple places. Create `bugswarm-types` crate with shared definitions. Import from single source of truth.

### 7. Minitia Real Binary Downloads — PENDING
Engine install writes a shell stub instead of downloading real binaries. Implement actual HTTP download with SHA-256 verification from `releases.bugswarm.ai`.

### 8. TUI Textual Integration — PENDING
TUI data model exists (`swarm/tui.py`) but has no terminal rendering. Integrate with Textual framework:
- Convert `TUIState` to Textual reactive widgets
- Implement all 7 screens with real-time updates
- Keyboard shortcuts and command mode

### 9. Prometheus HTTP Endpoint — PENDING
Metrics registry exists (`swarm/observability.py`) but no HTTP endpoint. Add:
- `GET /metrics` returning Prometheus text format
- `GET /health` for liveness probe
- `GET /ready` for readiness probe

### 10. Graceful Shutdown in Rust — PENDING
SIGTERM to Rust daemons leaves Docker containers dangling. Add:
- Signal handler in all 3 daemons
- Drain active connections before exit
- Clean up Docker containers on shutdown
- WAL checkpoint before exit

---

*This file is versioned. Changes require an ADR and approval from the engineering lead. The latest version is always at `AGENTS.md` in the repository root.*
