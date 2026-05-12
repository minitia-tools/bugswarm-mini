# Bug Swarm Enterprise Production Audit

## 176 gate tests pass, but deeply simulated — major gaps before production.

---

## CRITICAL (5 items — blocks production deployment)

| # | Gap | Phases | Impact |
|---|-----|--------|--------|
| 1 | **Provider adapter files missing** — `providers/openai.py`, `providers/anthropic.py`, etc. don't exist in the gateway package. Every LLM call crashes at import. | 3, 4, 6, 10 | Gateway is non-functional |
| 2 | **Semantic embedding is SHA-256 hashing, not ML embeddings** — `simple_embed()` uses `hashlib.sha256()` which is cryptographically designed to produce maximally different outputs for similar inputs. Cosine similarity between SHA hashes is meaningless. Used everywhere in Phases 6, 7, 8. | 6, 7, 8 | Diversity, loop detection, MMR routing all broken |
| 3 | **All LLM calls are simulated with `random.random()`** — Agent hypothesis generation, judge verdicts, bench deliberation, abstractive compression — all use random or hardcoded values instead of actual LLM API calls. | 4, 6, 7, 10 | No real AI reasoning anywhere |
| 4 | **No inter-phase integration** — swarm orchestrator doesn't call actual agent. Bench doesn't call actual LLM. Sandbox doesn't receive requests from agent. Each phase tested in isolation with stubs. | All | Full pipeline has never run end-to-end |
| 5 | **No persistent storage except Agent SQLite** — evidence graph, financial state, orchestrator state, bench calibrations, TUI state all in-memory. Process restart = total data loss. | 5, 6, 8, 10, 11, 12 | No crash recovery |

---

## HIGH (15 items — must fix before production)

### Phase 1 — Sandbox
| # | Gap | File |
|---|-----|------|
| 6 | Daemon mode unimplemented — `RunServer` prints error and exits | `main.rs:290` |
| 7 | Statistical re-execution is dead code — all spawned tasks return error immediately | `container.rs:562` |
| 8 | Memory profiling always reports zeros — `peak_mb: 0` on every path | `container.rs:267-360` |
| 9 | Escape detection uses substring matching, not AST — trivially bypassed | `scanner.rs:27` |
| 10 | Seccomp `include_str!` references non-existent file — compile-time panic | `seccomp.rs:8` |

### Phase 2 — CPG
| # | Gap | File |
|---|-----|------|
| 11 | Only 2 of 9 languages have parsers — Go/Java/Rust/C/C++/Ruby all silently skipped | `parser.rs:38` |
| 12 | Sink/source detection uses substring — `"load"` matches every function with "load" | `parser.rs:903` |
| 13 | No data flow analysis — only AST walking, no use-def chains | `parser.rs` |
| 14 | No cross-file call resolution — all external calls go to non-existent stubs | `parser.rs:243` |
| 15 | No graph serialization — must re-index entire codebase every time | `main.rs:92` |

### Phase 3 — Gateway
| # | Gap | File |
|---|-----|------|
| 16 | Streaming is faked — `yield response.content` after full response, defeats purpose | `client.py:253` |
| 17 | No secrets management — API keys raw from env vars only | `types.py:119` |
| 18 | No circuit breaker for failing providers — `disable_provider` exists but never called | `client.py:128` |
| 19 | No audit log for LLM calls — compliance and cost audit impossible | `client.py:186` |

### Phase 4 — Agent
| # | Gap | File |
|---|-----|------|
| 20 | CPG/sandbox invoked via hardcoded `/root/a/...` paths — dev-machine specific | `core.py:121` |

### Phase 6 — Swarm
| # | Gap | File |
|---|-----|------|
| 21 | Agents simulated with hardcoded strings + `random.random()` — no LLM calls | `orchestrator.py:342` |

---

## MEDIUM (28 items — should fix before production)

| # | Gap | Phase | File |
|---|-----|-------|------|
| 22 | CPU time never tracked — `cpu_time_secs` always null | 1 | `container.rs:174` |
| 23 | No authentication anywhere — API keys, mTLS, socket ACL all absent | 1,3,6,12 | multiple |
| 24 | Default Docker image uses mutable `:latest` tag | 1 | `config.rs:121` |
| 25 | Sandbox container names use UUID — no correlation to swarm/round/agent | 1 | `container.rs:214` |
| 26 | External calls always `confidence: 0.5` — no stdlib resolution | 2 | `parser.rs:255` |
| 27 | No `.gitignore` respect — indexes node_modules, __pycache__, vendor | 2 | `parser.rs:107` |
| 28 | Taint BFS uses single global visited set — blocks multi-path traversal | 2 | `graph.rs:328` |
| 29 | No response caching — identical requests re-call LLM | 3 | `client.py:186` |
| 30 | Budget enforcement advisory only — caller must honor, no gateway enforcement | 3,8 | `client.py:302` |
| 31 | No rate limiting on concurrent LLM requests | 3 | `client.py:186` |
| 32 | Agent tool dispatch blocks event loop for up to 130s | 4 | `core.py:204` |
| 33 | SQLite default `:memory:` — all state lost on crash | 4 | `core.py:336` |
| 34 | Finding parsing uses fragile string matching — breaks on any edge case | 4 | `core.py:568` |
| 35 | No native tool-use/function-calling schema — relies entirely on freeform parsing | 4 | `core.py:516` |
| 36 | System prompt has no versioning — no A/B testing, no audit trail | 4 | `core.py:32` |
| 37 | Evidence graph entirely in-memory — no serialization | 5 | `graph.rs:17` |
| 38 | Content hash doesn't cover metadata/severity — tampered node can verify | 5 | `types.rs:173` |
| 39 | No node/edge limits — can grow until OOM | 5 | `graph.rs:36` |
| 40 | Orchestrator state in-memory — no crash recovery | 6 | `orchestrator.rs:281` |
| 41 | Abstractive compression calls no LLM — pure Python stub | 7 | `compression.py:217` |
| 42 | Relevance scorer never actually offloads to disk | 7 | `compression.py:436` |
| 43 | Financial state entirely in-memory — restart resets budgets | 8 | `finance.py:78` |
| 44 | Token counts unverified — malicious agent can report zero | 8 | `finance.py:168` |
| 45 | Scale test is all simulated — no real Docker/LLM measurement | 9 | `scale_test.py:18` |
| 46 | Judge verdicts simulated with random — no LLM adjudication | 10 | `bench.py:231` |
| 47 | Calibration simulated — doesn't call judges | 10 | `bench.py:183` |
| 48 | TUI is a data model only — no actual terminal rendering | 11 | `tui.py` |
| 49 | Engine download is simulated stub — no real binary | 12 | `core.py:185` |

---

## LOW (15 items — address after production)

| # | Gap | Phase |
|---|-----|-------|
| 50 | No graceful sandbox shutdown — killing process leaves dangling containers | 1 |
| 51 | `compute_memory_growth` dead stub returning (0,0) | 1 |
| 52 | Taint reason only checks Docker error strings | 1 |
| 53 | No provenance tracking on graph nodes | 2 |
| 54 | Call-path BFS silently returns incomplete results at limit | 2 |
| 55 | O(n) recursive call collection on deeply nested ASTs | 2 |
| 56 | `add_prediction` returns duplicate tuple `(id, id)` | 5 |
| 57 | `reaching_to` is O(n×e) slow | 5 |
| 58 | Query pagination not implemented | 5 |
| 59 | Spare pool never replenished | 6 |
| 60 | Ejection doesn't consider token waste | 6 |
| 61 | No compression of sandbox receipt JSON | 7 |
| 62 | Query cache uses exact embedding match — no ANN | 7 |
| 63 | Override budget multiplier hardcoded to 1.5x | 8 |
| 64 | Audit chain verifies but doesn't identify corrupted entry | 14 |

---

## Priority Build Order for Production Readiness

### Sprint 1: Wire the foundation
1. Create provider adapter files (Phase 3 gap #1)
2. Replace SHA-256 "embeddings" with real sentence-transformers (Phases 6, 7, 8 gap #2)
3. Add persistent storage to evidence graph, finance, orchestrator (Phases 5, 8, 6 gap #5)

### Sprint 2: Connect the pipeline
4. Wire agent ↔ sandbox ↔ CPG ↔ gateway together (gap #4)
5. Replace simulated LLM calls with real API calls (Phases 4, 6, 7, 10 gap #3)
6. Add PostgreSQL persistence to Evidence Graph (Phase 5 gap #37)

### Sprint 3: Harden the edges
7. Implement sandbox daemon mode (Phase 1 gap #6)
8. Add real data flow analysis to CPG (Phase 2 gap #13)
9. Add authentication everywhere (gap #23)
10. Add structured output / function calling (Phase 4 gap #35)

### Sprint 4: Production polish
11. Add Prometheus HTTP endpoint + OpenTelemetry (Phase 13)
12. Add real TUI rendering with Textual (Phase 11)
13. Add real binary downloads to Minitia (Phase 12)
14. End-to-end integration test (gap #59)

---

*Audit completed. 176 unit/gate tests pass. 64 gaps identified: 5 critical, 15 high, 28 medium, 15 low.*
