# Phase 18: Persistent Learning & Bug Pattern Database

**Status**: NOT_STARTED
**Estimated Effort**: 16 hours
**Depends On**: Phase 2 (CPG), Phase 17 (Data Flow Analysis)
**Unblocks**: Phase 19 (Bug Probability ML), Phase 29 (Vulnerability Chaining)

---

## A1. What is being built?

A persistent learning layer storing every confirmed finding as an embedded pattern in ChromaDB. CPG of new repos matched against historical patterns to identify high-similarity code regions. CVE corpus with graph-isomorphism matching. File hotspot tracking with exponential decay. Agent performance history feeding allocation algorithm. Every run makes the next run smarter.

## A2. Which specific gap does it fill?

**Gap ID**: INV-006. Current: every run is amnesia. Target: Pattern DB persists across runs, hotspots prioritize historically buggy files, CVE matching flags known vulnerabilities, agent history optimizes allocation.

## A3. Success criteria?

| Metric | Target |
|--------|--------|
| Pattern recall | >70% of re-introduced bugs matched to prior findings |
| Hotspot accuracy | Top-20% hotspot files contain >60% of new bugs |
| CVE detection | >80% of known CVE patterns matched |
| Token efficiency | >30% reduction in tokens/verified-bug after 5 runs |
| Query latency | <100ms for 10K patterns |
| Storage | <1GB for 100K findings |

## A4. Priority?

3rd in invincibility stack. Learning layer — everything above it (ML, fuzzer steering, allocation) depends on having historical data. Without it, every run starts from zero.

## A5. Scope boundary?

NOT: full ML pipeline (Phase 19), real-time matching (batch), public vulnerability DB, GNN matching (cosine only for now), agent strategy optimization (stores history, Phase 15 consumes it).

---

## B1. Integration point?

**New files**: `swarm/learning.py`, `swarm/learning/store.py`, `swarm/learning/embed.py`
**Modified**: `swarm/orchestrator.py` (post-run: push findings, update hotspots), `agent/cli/wiring.py` (pre-run: read hotspots + patterns), `agent/cli/config.py` (+`--learning-db` flag)

## B2. Data flow?

```
RUN COMPLETES → every confirmed finding embedded + stored in ChromaDB
              → hotspot scores updated with exponential decay
              → agent performance recorded

NEXT RUN → assessment reads hotspots + queries ChromaDB for similar patterns
         → scout agent receives top-N prioritized files
```

## B3. New types?

`BugPattern` (id, cwe, language, severity, code_snippet, function_signature, taint_metadata, fix_diff, embedding, discovered_at, repo_hash, run_id)
`HotspotEntry` (file_path, score, bug_count, last_bug_at, recency_decay=0.95)
`AgentPerformanceRecord` (agent_id, persona, model, bugs_found, fp_rate, tokens_per_verified, runs_completed)

## B4. Modified modules?

| File | Change |
|------|--------|
| `swarm/learning.py` | NEW — BugPatternDB, HotspotTracker, CveCorpus, AgentHistory |
| `swarm/learning/store.py` | NEW — ChromaDB client wrapper, embed + store + query |
| `swarm/learning/embed.py` | NEW — Code→embedding: sentence-transformer all-MiniLM-L6-v2 |
| `swarm/orchestrator.py` | After each run: push findings, update hotspots, record history |
| `agent/cli/wiring.py` | Assessment reads hotspots + patterns before scout |
| `agent/cli/config.py` | Add `--learning-db` flag |

## B5. Dependencies?

| Dep | Version | Purpose |
|-----|---------|---------|
| `chromadb` | >=0.5.0 | Vector DB for pattern storage + ANN search |
| `sentence-transformers` | >=2.6.0 | Code→384-dim embedding (all-MiniLM-L6-v2) |
| `pyyaml` | >=6.0 | Hotspot + agent history persistence |

---

## C1. Core algorithm?

### Pattern Storage
```
store_finding(finding, repo_path):
    snippet = extract_code_surrounding(location, radius=25)
    embedding = sentence_transformer.encode(claim + mechanism + snippet[:500])
    chroma_collection.add(ids=[sha256(snippet)], embeddings=[embedding], metadatas=[...])
```

### Pattern Query
```
query_similar(function_code, top_k=10):
    embedding = sentence_transformer.encode(function_code[:1000])
    results = chroma_collection.query(query_embeddings=[embedding], n_results=top_k)
    return [(1.0 - distance, cwe, severity, snippet) for ... in results]
```

### Hotspot Scoring
```
update_hotspot(file_path, severity, now):
    entry = hotspots.get(file_path) or HotspotEntry(file_path)
    entry.score *= decay_rate ** days_since_last_scan
    entry.score += severity
    entry.bug_count += 1
    save_hotspots()
```

### CVE Corpus Matching
```
match_cve_patterns(function_code):
    embedding = sentence_transformer.encode(function_code)
    results = cve_collection.query(query_embeddings=[embedding], n_results=5)
    return [cve for cve in results if distance < 0.3]
```

**Complexity**: Embedding O(N), ChromaDB query O(log N) via HNSW, hotspot O(1).

## C2. Failure modes?

| Failure | Handling | Recovery |
|---------|----------|----------|
| ChromaDB unavailable | Agent runs without patterns. Log WARN. | Retry every 5min. Re-enable on reconnect. |
| Embedding model fails | Fall back to trigram BoW similarity. Log ERROR. | Alert. Investigate model file. |
| Hotspot file corrupted | Reset to empty. Log WARN. | Fresh start — data is additive, loss is non-critical. |
| Pattern DB >1M entries | Enable disk-backed ChromaDB persistence. | Monitor size. Alert at 80% disk. |
| Embedding NaN/Inf | Skip pattern. Log ERROR. | Usually caused by empty code input. |

## C3. Edge cases?

| Edge Case | Behavior |
|-----------|----------|
| First run (empty DB) | No patterns. Hotspots zero. Agent history empty. Runs normally. |
| Duplicate finding | SHA256 dedup. Second insert is no-op. |
| Very short function (<50 chars) | Embedding still works on short text. |
| Multi-language repo | Patterns tagged by language. Queries filtered. |
| Hotspot decay over 6 months | Score approaches 0. File effectively forgotten. |

## C4. Concurrency?

ChromaDB handles concurrent reads natively. Writes batched at end of run (single writer). Hotspot YAML uses atomic write (temp file + rename). Agent history same. No race conditions — data written after swarm terminates.

## C5. Performance budget?

| Metric | Target |
|--------|--------|
| Embedding time | <50ms per function |
| ChromaDB query | <100ms for top-10 over 100K patterns |
| Hotspot save | <10ms for <1000 entries |
| Pattern insert | <50ms per pattern (batch) |

---

## C6. Algorithmic Peak Analysis

### C6.1 Algorithm Inventory

| # | Component | Approach | Status | Peak Reference |
|---|-----------|----------|--------|----------------|
| 1 | Embedding | sentence-transformer all-MiniLM-L6-v2 (384-dim) | **PEAK** | Already optimal for code similarity |
| 2 | Similarity search | ChromaDB HNSW ANN O(log N) | **PEAK** | Production-grade approximate nearest neighbor |
| 3 | Hotspot decay | Exponential decay (0.95/week) | **PEAK** | Mathematically sound forgetting curve |
| 4 | CVE matching | Embedding cosine similarity | NAIVE → Graph edit distance | C6.2.1 |

### C6.2.1 CVE Matching — Graph Edit Distance (Deferred)

**Peak algorithm**: Compute graph edit distance between CPG subgraph at target function and known-vulnerable CPG subgraphs from CVE corpus. Catches >90% vs 60% for text embedding.

**Quantitative**: Graph structure matches even when variable names differ (obfuscation-resistant).

**Edge cases**: Same structure, different names → matched. Obfuscated → still matched. Different structure, similar text → NOT matched (embedding false positive eliminated).

**Verification**: 100 known CVE snippets. Assert graph edit distance finds ≥90, embedding finds ≥60.

**Deferral reason**: Requires CPG serialization for CVE patterns (dependency-blocked, Phase 17b). Embedding is sufficient for Phase 18.

### C6.3 Zero-Gap Guarantee

```
Component: Embedding — [x] PEAK achieved via sentence-transformer
Component: Similarity — [x] PEAK achieved via ChromaDB HNSW
Component: Hotspot — [x] PEAK achieved via exponential decay
Component: CVE — [x] NAIVE accepted with valid deferral (dependency-blocked: Phase 17b)
```

### C6.4 Peak Deferral

CVE graph matching deferred. Reason: dependency-blocked (requires CPG serialization, not yet built). Embedding approach is sufficient with documented limitation.

---

## D1. Aggressive unit tests? (15 tests, 9 aggressive)

| # | Test | Attack | Expected |
|---|------|-------|----------|
| 1 | `test_pattern_store_retrieve` | Store + query same code | Similarity >0.95 |
| 2 | `test_pattern_different_code` | SQLi pattern, query race condition | Similarity <0.3 |
| 3 | `test_pattern_dedup` | Store same pattern twice | Collection has 1 entry |
| 4 | `test_hotspot_scoring` | Bug at auth.py:42 severity 9 | Score increases by 9 |
| 5 | `test_hotspot_decay` | Score 100, 7 days pass | Score ~70 |
| 6 | `test_hotspot_empty` | No hotspots | All files 0. No crash. |
| 7 | `test_agent_history` | Agent A: 5 bugs, 1 FP | Correctly stored |
| 8 | `test_cve_corpus` | Known CVE pattern in code | Matched |
| 9 | `test_empty_db` | **AGGRESSIVE**: Query with 0 patterns | Empty result. No crash. |
| 10 | `test_large_db` | **AGGRESSIVE**: 50K patterns, query | <100ms, no OOM |
| 11 | `test_corrupt_hotspot` | **AGGRESSIVE**: Invalid YAML | Reset to empty, log WARN |
| 12 | `test_chromadb_down` | **AGGRESSIVE**: ChromaDB unavailable | Agent runs. Patterns skipped. No crash. |
| 13 | `test_multi_language` | **AGGRESSIVE**: Python+JS, query Python | Only Python returned |
| 14 | `test_concurrent_reads` | **AGGRESSIVE**: 50 simultaneous queries | All <200ms. No corruption. |
| 15 | `test_embedding_nan` | **AGGRESSIVE**: Empty code input | Skipped. Logged. No crash. |

## D2. Aggressive integration tests? (5 tests, 3 aggressive)

| # | Test | Attack | Expected |
|---|------|-------|----------|
| 1 | `test_full_learning_loop` | Run → store → run again | Second run fewer tokens/bug |
| 2 | `test_hotspot_prioritization` | Buggy file high score → next run first | Agent targets hotspot first |
| 3 | `test_learning_persistence` | **AGGRESSIVE**: Run → store → restart → query | Patterns survive restart |
| 4 | `test_cross_repo` | **AGGRESSIVE**: Learn repo A → query repo B | Cross-repo pattern matching |
| 5 | `test_50_agent_concurrent` | **AGGRESSIVE**: 50 agents storing findings | No corruption. All stored. |

## D3. Extreme gate test — The Memory Crucible? (8 attack vectors)

1. **Amnesia**: Store 20 patterns, restart, all 20 retrievable
2. **Decay**: Score 100, simulate 30 days, score <5
3. **Scale**: 100K patterns, top-10 query <100ms
4. **Corruption**: Corrupt hotspot YAML, graceful reset
5. **Multi-repo**: 5 repos, patterns independently queryable
6. **CVE detection**: 20 CVE snippets, all 20 matched
7. **Concurrent**: 100 simultaneous queries, 50K DB
8. **Empty**: Fresh install, zero patterns, agent runs

## D4. Golden dataset?

Applicable: 200 golden dataset bugs stored as patterns. After Phase 18, re-query golden dataset against itself. Assert >95% self-match rate (each bug matches itself).

## D5. Regression test?

`test_learning_regression`: Memory Crucible gate runs on every CI push. Any of 8 vectors fail → build fails.

---

## E1. Estimated cost?

| Cost | Estimate |
|------|----------|
| Development | 16 hours |
| Storage | ~50MB per 10K patterns |
| Embedding compute | <1ms per function (CPU) |
| Token savings | 30%+ reduction |
| Infrastructure | $0 (local ChromaDB) |

## E2. Observability?

**Logs**: pattern_stored, pattern_query, hotspot_updated, embedding_failed, chromadb_unavailable
**Metrics**: `bugswarm_pattern_db_size`, `bugswarm_pattern_query_latency_ms`, `bugswarm_hotspot_files_tracked`, `bugswarm_learning_token_savings_pct`
**Alerts**: ChromaDB unavailable >5min → WARN

## E3. Configuration?

| Parameter | Default | Env | Flag |
|-----------|---------|-----|------|
| `learning_enabled` | `true` | `BGSWARM_LEARNING` | `--[no-]learning` |
| `chromadb_path` | `~/.bugswarm/chroma` | `BGSWARM_CHROMADB_PATH` | `--chromadb-path` |
| `hotspot_decay_rate` | `0.95` | — | — |
| `pattern_top_k` | `10` | — | — |
| `cve_similarity_threshold` | `0.3` | — | — |

## E4. Migration?

Full backward compat. Learning is additive. First run: empty DB. Second run: data from first. No migration needed.

## E5. Documentation?

ADR-018: Persistent Bug Pattern Database. User docs: "Learning System" section. Changelog entry.

---

## Gate Receipt

```json
{"phase":18,"gate":"memory_crucible","attack_vectors":8,"passed":0,"failed":0,"verdict":"PHASE 18 NOT YET EXECUTED"}
```
