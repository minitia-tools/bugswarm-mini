# Phase 18: Persistent Learning & Bug Pattern Database

**Status**: NOT_STARTED
**Estimated Effort**: 16 hours
**Depends On**: Phase 2 (CPG), Phase 17 (Data Flow Analysis)
**Unblocks**: Phase 19 (Bug Probability ML), Phase 29 (Vulnerability Chaining)

---

## A1. What is being built?

A persistent learning layer that stores every confirmed bug finding as an embedded pattern in a Vector DB. The CPG of every new repository is matched against this database to identify code regions with high similarity to previously confirmed bugs. A CVE corpus with graph-isomorphism matching prioritizes known vulnerability patterns. Agent performance history and file hotspot tracking feed the allocation algorithm so every run is smarter than the last.

---

## A2. Which specific gap does it fill?

**Gap ID**: INV-006 (from `invincible.md`)
**Current behavior**: Every run is amnesia. The system doesn't remember which code patterns caused bugs, which agent strategies worked best, which CVE patterns are relevant, or which files had bugs in previous runs. Same repo scanned twice = same false positives twice.
**Target behavior**: Bug Pattern Vector DB persists across runs. Hotspot scores prioritize historically buggy files. CVE corpus matching flags known vulnerability patterns. Agent performance history optimizes allocation.

---

## A3. What is the success criteria?

| Metric | Target | Measurement |
|--------|--------|-------------|
| Pattern DB recall | >70% of re-introduced bugs matched to previous findings | Scan a repo with bugs fixed, re-introduce 2 bugs, verify both matched to prior findings |
| Hotspot accuracy | Top-20% hotspot files contain >60% of new bugs | Compare hotspot ranking vs actual bugs found in 5 repos |
| CVE pattern detection | >80% of known CVE patterns detected in the corpus | Test against 100 known CVE code snippets |
| Token efficiency gain | >30% reduction in tokens per verified bug after 5 runs on same repo | Compare run 1 vs run 5 token/verified-bug ratio |
| Query latency | <100ms for pattern DB similarity search | ChromaDB ANN query on 10K patterns |
| Storage | <1GB for 100K findings | Compressed embeddings + metadata |

---

## A4. What is the priority and why?

**Priority**: 3rd in the invincibility stack (Phase 18 of 30). Immediately after Data Flow Analysis.
**Justification**: This is the learning layer. Without it, every run starts from zero. Every other phase (ML prediction, fuzzer steering, allocation optimization) depends on having historical data to learn from. The system gets better with every run — but only if it remembers.

---

## A5. What is NOT being built?

- NOT a full ML pipeline (that's Phase 19 — this phase builds the data store that Phase 19 trains on)
- NOT real-time pattern matching during indexing (batch matching after CPG build)
- NOT a public vulnerability database (uses local corpus, not live NVD queries)
- NOT graph neural networks for pattern matching (cosine similarity on embeddings for Phase 18, GNNs deferred)
- NOT agent strategy optimization (stores history, Phase 15 allocation algorithm consumes it)

---

## B1. Integration point?

**Primary**: New Python module `swarm/learning.py` + ChromaDB collection
**New files**:
- `swarm/learning.py` — BugPatternDB, HotspotTracker, CveCorpus, AgentHistory
- `swarm/learning/store.py` — ChromaDB integration: embed, store, query
- `swarm/learning/embed.py` — Code-to-embedding: function signature + AST structure + taint metadata → vector

**Modified files**:
- `swarm/orchestrator.py` — After each run, push confirmed findings to PatternDB; update hotspots; record agent performance
- `agent/cli/wiring.py` — Assessment phase reads hotspot scores and pattern similarity before scout runs
- `agent/cli/config.py` — Add `--learning-db` flag pointing to ChromaDB path

---

## B2. Data flow?

```
RUN COMPLETES
  │
  ├─→ Every confirmed finding:
  │     ├─ Extract: code snippet (50 lines around bug) + CWE + language + severity + fix
  │     ├─ Embed: function signature + AST structure + taint metadata → 384-dim vector
  │     └─ Store: ChromaDB collection "bug_patterns" with metadata
  │
  ├─→ Hotspot tracker:
  │     ├─ File where bug was found → hotspot_score += severity × recency_weight
  │     └─ Persist to ~/.bugswarm/hotspots.yaml
  │
  ├─→ Agent history:
  │     ├─ Agent performance: bugs_found, FP_rate, tokens_per_verified
  │     └─ Persist to ~/.bugswarm/agent_history.yaml
  │
NEXT RUN BEGINS
  │
  ├─→ Assessment phase reads:
  │     ├─ Hotspot scores → prioritize high-score files
  │     ├─ Pattern similarity → for each function in new repo, query ChromaDB
  │     │   └─ Top-K similar patterns returned with similarity scores
  │     └─ Agent history → allocation algorithm weights agents that performed well
  │
  └─→ Scout agent receives: top-N hotspot files + top-M pattern matches
        → Investigates these first → faster bug discovery
```

---

## B3. New types/schemas?

### Python: BugPattern (in `swarm/learning.py`)

```python
@dataclass
class BugPattern:
    id: str                          # SHA256 of code snippet
    cwe: str                         # CWE-89, CWE-78, etc.
    language: str                    # python, javascript, etc.
    severity: int                    # 1-10
    code_snippet: str                # 50 lines around the bug
    function_signature: str          # "login(username, password) -> bool"
    taint_metadata: dict             # {sources: [...], sinks: [...], sanitized: bool}
    fix_diff: str | None             # The fix that resolved it
    embedding: list[float] | None    # 384-dim vector (computed by embed module)
    discovered_at: datetime
    repo_hash: str                   # Which repo this was found in
    run_id: str                      # Which run
```

### Python: HotspotEntry (in `swarm/learning.py`)

```python
@dataclass
class HotspotEntry:
    file_path: str
    score: float                     # Cumulative: sum(severity × recency_weight)
    bug_count: int                   # How many bugs found in this file
    last_bug_at: datetime
    last_scan_at: datetime
    recency_decay: float = 0.95      # Exponential decay per week
```

---

## B4. Modified modules?

| File | Change | Impact |
|------|--------|--------|
| `swarm/learning.py` | NEW — BugPatternDB, HotspotTracker, CveCorpus, AgentHistory | Core module |
| `swarm/learning/store.py` | NEW — ChromaDB client wrapper | Storage layer |
| `swarm/learning/embed.py` | NEW — Code embedding function | Embedding layer |
| `swarm/orchestrator.py:run()` | After run completes: push findings, update hotspots, record history | Integration point |
| `agent/cli/wiring.py` | Assessment phase reads patterns + hotspots before scout runs | Integration point |
| `agent/cli/config.py` | Add `--learning-db` flag | Config |

---

## B5. New dependencies?

| Dependency | Version | Purpose | Justification |
|-----------|---------|---------|---------------|
| `chromadb` | >=0.5.0 | Vector DB for pattern storage + ANN search | Already in Python deps |
| `sentence-transformers` | >=2.6.0 | Code-to-embedding (all-MiniLM-L6-v2) | 384-dim, fast, local, free |
| `pyyaml` | >=6.0 | Hotspot + agent history persistence | Already in deps |

---

## C1. Core algorithm?

### Pattern Storage

```
function store_finding(finding: ParsedFinding, repo_path: Path):
    snippet = extract_code_surrounding(finding.location, radius=25 lines)
    embedding = sentence_transformer.encode(
        f"{finding.claim} | {finding.mechanism} | {snippet[:500]}"
    )
    
    pattern = BugPattern(
        id = sha256(snippet),
        cwe = map_to_cwe(finding.claim),
        language = detect_language(finding.location),
        severity = finding.severity,
        code_snippet = snippet,
        function_signature = extract_function_at(finding.location),
        taint_metadata = extract_taint_context(finding),
        embedding = embedding.tolist(),
        ...
    )
    
    chroma_collection.add(
        ids=[pattern.id],
        embeddings=[embedding],
        metadatas=[pattern.to_metadata_dict()],
        documents=[snippet[:1000]]
    )
```

### Pattern Query

```
function query_similar_patterns(function_code: str, top_k: int = 10):
    embedding = sentence_transformer.encode(function_code[:1000])
    results = chroma_collection.query(
        query_embeddings=[embedding],
        n_results=top_k,
        include=["metadatas", "documents", "distances"]
    )
    return [
        SimilarPattern(
            similarity = 1.0 - distance,
            cwe = metadata["cwe"],
            severity = metadata["severity"],
            snippet = document
        )
        for distance, metadata, document in zip(results["distances"][0], ...)
    ]
```

### Hotspot Scoring

```
function update_hotspot(file_path: str, bug_severity: int, current_time: datetime):
    entry = hotspots.get(file_path) or HotspotEntry(file_path)
    
    # Apply recency decay to existing score
    days_since_last = (current_time - entry.last_scan_at).days
    entry.score *= entry.recency_decay ** days_since_last
    
    # Add new score
    entry.score += bug_severity
    entry.bug_count += 1
    entry.last_bug_at = current_time
    entry.last_scan_at = current_time
    
    hotspots[file_path] = entry
    save_hotspots()
```

**Complexity**: Embedding: O(N) where N = code length. ChromaDB query: O(log N) via HNSW index.

---

## C2. Failure modes?

| Failure | Handling | Recovery |
|---------|----------|----------|
| ChromaDB unavailable | Agent runs without pattern matching. Log WARN. | Retry connection every 5 min. Re-enable on reconnect. |
| Embedding model fails to load | Fall back to trigram BoW similarity (no ML embedding). | Log ERROR. Alert. |
| Hotspot file corrupted | Reset to empty. Log WARN. | Fresh start — no historical data lost (it's additive). |
| Pattern DB grows too large (>1M patterns) | Enable ChromaDB persistence with disk-backed index. | Monitor size. Alert at 80% disk. |
| Embedding produces NaN/Inf | Skip that pattern. Log ERROR. | Investigate input. Usually caused by empty/zero-length code. |

---

## C3. Edge cases?

| Edge Case | Behavior |
|-----------|----------|
| First run (empty DB) | No patterns returned. Hotspots start at zero. Agent history empty. Graceful — runs normally. |
| Duplicate finding (same bug found twice) | Pattern DB deduplicates by SHA256 of snippet. Second insertion is a no-op. |
| Very short function (<50 chars) | Embedding still works. Sentence transformer handles short text. |
| Multi-language repo | Patterns tagged with language. Queries filtered by language. |
| Hotspot decay over time | Score decays exponentially. File not touched in 6 months → score approaches 0. |

---

## C4. Concurrency?

**Locking strategy**: ChromaDB handles concurrent reads natively. Writes are batched at end of run (single writer). Hotspot YAML uses file-level lock (`fcntl`). Agent history uses atomic write (write to temp, rename).

**Race conditions**: None. Pattern DB writes happen after swarm terminates. Hotspot writes are additive — worst case is a slightly stale score, which is acceptable.

---

## C5. Performance budget?

| Metric | Target | Measurement |
|--------|--------|-------------|
| Embedding time | <50ms per function | `sentence_transformer.encode()` on 500-char input |
| ChromaDB query | <100ms for top-10 | ANN over 100K patterns |
| Hotspot save | <10ms | YAML dump of <1000 entries |
| Pattern DB insert | <50ms per pattern | Batch insert of <100 findings per run |

---

## C6. Algorithmic Peak Analysis

### C6.1 Algorithm Inventory

| # | Component | Current Approach | Naive/Peak | Peak Algorithm |
|---|-----------|-----------------|------------|----------------|
| 1 | Embedding generation | Sentence-transformer (all-MiniLM-L6-v2) | **PEAK** | Already peak — 384-dim, production-grade |
| 2 | Similarity search | ChromaDB HNSW ANN | **PEAK** | Already peak — O(log N) approximate nearest neighbor |
| 3 | Hotspot decay | Exponential decay | **PEAK** | Already peak — mathematically sound forgetting curve |
| 4 | CVE corpus matching | Embedding cosine similarity | NAIVE | → Graph edit distance on CPG subgraphs (C6.2.1) |

### C6.2.1 CVE Matching — Graph Edit Distance

**What**: Instead of embedding similarity (which only captures surface text), compute graph edit distance between the CPG subgraph at the target function and known-vulnerable CPG subgraphs from the CVE corpus.

**Quantitative improvement**: Text embedding catches ~60% of CVE patterns. Graph edit distance catches >90% because it matches STRUCTURE (call graph, data flow, control flow), not surface text.

**Edge cases**: Functions with same structure but different names match. Obfuscated code (renamed variables) still matches because graph structure is unchanged.

**Deferred**: Requires CPG serialization for CVE patterns (Phase 17b). Embedding approach is sufficient for Phase 18.

---

## D1. Aggressive unit tests? (15 tests)

| Test Name | Attack Vector | Expected Behavior |
|-----------|---------------|-------------------|
| `test_pattern_store_and_retrieve` | Store 1 pattern, query same code | Returns pattern with similarity >0.95 |
| `test_pattern_different_code` | Store SQL injection pattern, query race condition | Returns similarity <0.3 |
| `test_pattern_deduplication` | Store same pattern twice | Second insert is no-op. Collection has 1 entry. |
| `test_hotspot_scoring` | Bug at auth.py:42 severity 9 | Score increases by 9 |
| `test_hotspot_decay` | Score 100, 7 days pass | Score decays to ~70 |
| `test_hotspot_empty_start` | No hotspots | All files score 0. No crash. |
| `test_agent_history_record` | Agent A: 5 bugs, 1 FP | History stored correctly |
| `test_cve_corpus_match` | Know CVE-2024-12345 pattern in code | Matched with distance <0.3 |
| `test_empty_db_graceful` | Query before any patterns stored | Returns empty. No error. |
| `test_large_db_performance` | **AGGRESSIVE**: 50K patterns, query | <100ms. No OOM. |
| `test_corrupt_hotspot_file` | **AGGRESSIVE**: Invalid YAML | Resets to empty. Logs WARN. |
| `test_chromadb_unavailable` | **AGGRESSIVE**: ChromaDB down | Agent runs normally. Patterns skipped. |
| `test_multi_language_filtering` | Store Python + JS patterns. Query Python. | Only Python patterns returned. |
| `test_concurrent_reads` | **AGGRESSIVE**: 50 simultaneous queries | All return within 200ms. No corruption. |
| `test_embedding_nan_handling` | **AGGRESSIVE**: Empty code snippet | Skipped. Logged. No crash. |

---

## D2. Aggressive integration tests? (5 tests)

| Test Name | Attack Vector | Expected Behavior |
|-----------|---------------|-------------------|
| `test_full_learning_loop` | Run agent on repo with known bugs → store patterns → run again | Second run finds bugs faster (fewer tokens per verified bug) |
| `test_hotspot_prioritization` | Buggy file gets high hotspot → next run investigates it first | First agent turn targets high-hotspot file |
| `test_learning_persistence` | Run → store → restart process → query | Patterns survive process restart |
| `test_cross_repo_learning` | Learn pattern in repo A → query in repo B with similar code | Pattern matched across repos |
| `test_learning_with_50_agents` | **AGGRESSIVE**: 50 concurrent agents, each storing findings | No corruption. All patterns stored. All queries correct. |

---

## D3. Extreme gate test — The Memory Crucible? (8 attack vectors)

1. **Amnesia test**: Run, store 20 patterns, restart process, verify all 20 retrievable
2. **Decay test**: Insert score 100, simulate 30 days, verify score <5
3. **Scale test**: Insert 100K patterns, query top-10, verify <100ms
4. **Corruption test**: Corrupt hotspot YAML, verify graceful reset
5. **Multi-repo test**: Patterns from 5 different repos stored and queryable independently
6. **CVE detection test**: 20 known CVE snippets inserted, all 20 matched when queried with similar code
7. **Concurrent test**: 100 simultaneous queries against 50K pattern DB
8. **Empty test**: Fresh install, zero patterns, agent runs without error

---

## E1. Estimated cost?

| Cost Type | Estimate |
|-----------|----------|
| Development | 16 hours |
| ChromaDB storage | ~50MB per 10K patterns |
| Embedding compute | <1ms per function on CPU (all-MiniLM-L6-v2 is tiny) |
| Token cost impact | NEGATIVE — saves 30%+ tokens by prioritizing investigation |
| Infrastructure | $0 (ChromaDB runs locally, no cloud service needed) |

---

## E2. Observability?

**Logs**: pattern_stored, pattern_query, hotspot_updated, embedding_failed, chromadb_unavailable
**Metrics**: `bugswarm_pattern_db_size`, `bugswarm_pattern_query_latency_ms`, `bugswarm_hotspot_files_tracked`, `bugswarm_learning_token_savings_pct`
**Alerts**: ChromaDB unavailable >5min → WARN

---

## E3. Configuration?

| Parameter | Default | Env Var | CLI Flag |
|-----------|---------|---------|----------|
| `learning_enabled` | `true` | `BGSWARM_LEARNING_ENABLED` | `--[no-]learning` |
| `chromadb_path` | `~/.bugswarm/chroma` | `BGSWARM_CHROMADB_PATH` | `--chromadb-path` |
| `hotspot_decay_rate` | `0.95` | — | — |
| `pattern_top_k` | `10` | — | — |

---

## E4. Migration?

**Backward compatibility**: Full. Learning is additive. First run has empty DB. Second run has data from first run. No migration needed.

---

## E5. Documentation?

ADR-018: Persistent Bug Pattern Database. User docs: "Learning System" section.

---

## Gate Receipt

```json
{"phase": 18, "gate": "memory_crucible", "status": "PENDING", "verdict": "PHASE 18 NOT YET EXECUTED"}
```
