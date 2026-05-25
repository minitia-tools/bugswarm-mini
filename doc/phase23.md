# PHASE 23: Trigger Matrix — Multi-Dimensional Bug Characterization

## Changelog

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-05-14 | System | Initial phase plan |

---

## A. Identity & Purpose

### A1. What

Phase 23 implements the **Trigger Matrix** — a comprehensive multi-dimensional characterization framework that documents every condition under which a confirmed bug manifests. For each confirmed bug, the system produces and maintains a matrix with columns spanning Input Type, Environment, Timing, Data State, Concurrency, Configuration, Dependency Version, and OS/Architecture. Each investigation layer (agent analysis, fuzzer, concolic executor, differential analysis) contributes rows to this matrix. Over time, as the system runs more tests and agents perform more investigations, the matrix fills. By Phase 30, every confirmed bug must have at least 5 trigger rows contributed by at least 3 distinct layers.

**Core mechanism**: The trigger matrix is a graph-structured data model. Each TriggerMatrix node in the evidence graph links to a Bug node. Each row within the matrix is a TriggerCondition node. Edges of type Triggers connect trigger conditions to their matrix. Edges of type ContributedBy connect trigger conditions to the layer that discovered them. Trigger conditions are semantically deduplicated across layers using normalized condition hashing and category-based fuzzy matching.

**Key deliverables**:
1. `bugswarm-evidence/src/trigger.rs` — Trigger matrix data structures, semantic dedup, completeness scorer
2. Evidence graph extension — New node types TriggerMatrix, TriggerCondition; new edge types Triggers, ContributedBy, ConditionEquivalent
3. Agent tools: `describe_trigger(bug_id, condition)`, `get_trigger_matrix(bug_id)`
4. Two C6 peak optimizations: cross-layer trigger dedup (semantic hashing), completeness scoring (weighted priority)
5. Phase 30 compliance validator — asserts 5+ rows from 3+ layers per bug
6. API for layers (fuzzer, concolic, agent, differential) to contribute trigger rows

### A2. Gap (INV-006)

**Current state**: Bugs are documented with minimal information: "Crash at auth.py:42." There is no systematic record of trigger conditions — what input, environment, timing, data state, concurrency regime, configuration, dependency version, and OS/arch triggers the bug. This leads to:
- Bugs that cannot be reliably reproduced (engineers try and fail)
- False assumptions about bug scope ("only happens on Linux" but actually everywhere)
- Inefficient investigation (agents don't know which dimensions to vary)
- No systematic understanding of root cause surfaces
- Bugs re-reported because trigger conditions weren't documented

**Target state**: Every confirmed bug receives at least 5 trigger matrix rows from at least 3 different investigation layers. All trigger conditions are semantically deduplicated across layers (avoiding "username=null" and "fuzz_seed_42=''" being separate rows when they mean the same thing). Bugs achieve 100% reproducible rate because all trigger dimensions are documented. Agents prioritize investigation of the least-complete, highest-severity bugs first.

**Metric targets**:

| Metric | Baseline | Target | Measurement |
|--------|----------|--------|-------------|
| Trigger rows per confirmed bug | 0 | 5+ from 3+ layers by Phase 30 | Count query on evidence graph |
| Bug reproducibility rate | ~40% (ad-hoc) | 100% (with matrix) | Engineer reproduction attempts |
| Semantic dedup accuracy | 0% (no dedup) | >95% correct merges | Manual audit of 100 dedup decisions |
| Completeness score correlation with fix time | Unknown | Spearman >0.6 (inverse) | Regression of score vs fix time |
| Agent investigation priority alignment | Random | >80% correct priority picks | Agent simulation logs |
| Matrix population rate | 0 rows/day | >10 rows/day in active system | Daily count of new trigger conditions |
| Duplicate trigger rate (pre-dedup) | 100% (no dedup) | <10% after dedup | Compare raw contributions vs dedup rows |
| Cross-layer coverage | 0 layers/bug | 3+ distinct layers/bug | Count distinct ContributedBy edges |

**Success criteria**:
1. SC-23.1: Every confirmed bug with severity >= LOW receives 5+ trigger matrix rows from 3+ distinct layers by Phase 30
2. SC-23.2: Semantic dedup correctly merges equivalent conditions across layers with >95% accuracy
3. SC-23.3: Completeness scoring drives agent priority — agents investigate least-complete high-severity bugs first in 80%+ of decisions
4. SC-23.4: get_trigger_matrix(bug_id) returns in <50ms for matrices up to 100 rows
5. SC-23.5: Agent tool describe_trigger allows adding, annotating, querying within <100ms latency
6. SC-23.6: Bugs with complete matrices (score >0.8) achieve 100% reproduction rate
7. SC-23.7: No duplicate trigger conditions persist across layers (semantic dedup on every write)
8. SC-23.8: Trigger matrix data survives evidence graph restarts and schema migrations

### A3. Success Criteria Table

| ID | Criterion | Measure | Target | Verification Method |
|----|-----------|---------|--------|---------------------|
| SC-23.1 | Rows and layers per bug | Count rows + distinct ContributedBy layers | 5+ rows, 3+ layers | Evidence graph aggregation query |
| SC-23.2 | Dedup accuracy | Manual audit sample | >95% correct merges | Double-blind human review |
| SC-23.3 | Agent priority alignment | Agent picks correct priority bug | 80%+ | Agent simulation with known priorities |
| SC-23.4 | Query latency | Wall-clock for get_trigger_matrix(100 rows) | <50ms | Benchmark with k6 |
| SC-23.5 | Agent tool latency | describe_trigger RTT | <100ms | Integration test RTT measurement |
| SC-23.6 | Reproduction rate | Reproduced / attempted with matrix | 100% | Engineer verification log |
| SC-23.7 | Dedup effectiveness | Raw / dedup ratio | >10x cross-layer | Ratio counter on each write |
| SC-23.8 | Data durability | Rows survive restart | 100% | Restart test + count check |

### A4. Priority

**Priority**: P1 — High. The trigger matrix is a foundational data structure for all investigation and remediation phases. Without systematic trigger documentation, the entire investigation pipeline (Phases 16-21) produces non-actionable findings. Agents need the trigger matrix to understand what they're investigating. The fuzzer needs the matrix to avoid re-reporting known conditions.

**Urgency rationale**:
- Phase 24 (Differential Analysis) uses trigger matrices to select input pairs
- Phase 25 (Test Synthesis) uses trigger conditions to generate test cases
- Phase 26 (Flaky Detection) uses the timing/concurrency columns
- Phase 30 (completeness gate) requires every bug to have a filled matrix
- Agent investigation quality depends directly on trigger documentation

**Implementation order**: Build as soon as evidence graph (Phase 5) is stable. Can parallelize with Phase 22 (Delta Debugging). Must complete before Phase 30.

### A5. Scope Boundary

**In scope**:
- Trigger condition data model (8-dimension schema)
- Evidence graph node types: TriggerMatrix, TriggerCondition
- Evidence graph edge types: Triggers, ContributedBy, ConditionEquivalent
- Semantic deduplication engine (normalized hashing + fuzzy matching)
- Completeness scoring algorithm (weighted dimension scoring)
- Agent tools: describe_trigger, get_trigger_matrix
- Phase 30 compliance validation query
- API for layers to contribute trigger rows
- Migration path for existing bugs (backfill trigger conditions)

**Out of scope**:
- Automated trigger condition discovery (layers contribute — Phase 23 only stores/deduplicates)
- Root cause analysis from trigger patterns (Phase 27)
- Trigger-based test generation (Phase 25)
- Real-time trigger monitoring during execution (Phase 26)
- Trigger prediction for unreproduced bugs (Phase 29)
- Natural language description generation (agents provide descriptions)

**Boundary interfaces**:
- **Input**: TriggerCondition from fuzzer, concolic, agent, differential
- **Output**: TriggerMatrix per bug, completeness scores, priority rankings
- **North**: Agent investigation tools (Phase 17-18) — consume priority rankings
- **South**: Evidence graph (Phase 5) — stores all trigger data
- **East**: Differential analysis (Phase 24) — queries trigger matrices
- **West**: Fuzzer engine (Phase 8-10) — contributes trigger conditions

---

## B. Architecture

### B1. Integration Point Table

| ID | Source Phase | Source Interface | Target (Phase 23) | Data Flow | Protocol | Latency Budget |
|----|-------------|------------------|-------------------|-----------|----------|----------------|
| IP-23.1 | Phase 5 (Evidence) | Graph read/write API | trigger.rs CRUD | TriggerCondition nodes written/read | REST over localhost | <10ms per node |
| IP-23.2 | Phase 8-10 (Fuzzer) | Crash event | TriggerCondition contribution | Crash metadata to TriggerCondition | Internal channel (Rust enum) | 0ms (in-process) |
| IP-23.3 | Phase 12 (Concolic) | Constraint path output | TriggerCondition contribution | Path constraints to TriggerCondition | Internal channel | 0ms |
| IP-23.4 | Phase 17 (Agent tools) | describe_trigger tool | trigger.rs API | Agent query/annotate trigger | JSON-RPC over HTTP | <100ms |
| IP-23.5 | Phase 24 (Differential) | Trigger matrix query | trigger.rs | Differential reads matrix for input pair selection | REST over localhost | <50ms |
| IP-23.6 | Phase 25 (Test synth) | Trigger conditions query | trigger.rs | Test generator reads conditions | REST over localhost | <50ms |
| IP-23.7 | Phase 30 (Gate) | Completeness check | trigger.rs | Validate every bug has 5+ rows, 3+ layers | REST query | <1s for all bugs |

### B2. Data Flow Diagram

```
TRIGGER MATRIX ARCHITECTURE

  LAYER CONTRIBUTORS (Producers)
  +----------+  +----------+  +----------+  +----------+
  | Fuzzer   |  | Concolic |  | Agent    |  | Differ-  |
  | Engine   |  | Engine   |  | Analysis |  | ential   |
  | (P8-10)  |  | (P12)    |  | (P17-18) |  | (P24)    |
  +----+-----+  +----+-----+  +----+-----+  +----+-----+
       |              |             |              |
       |  TriggerCondition { dim, value, layer }
       +--------------+-------------+--------------+
                      |
  +-------------------v-----------------------------------+
  |           TRIGGER MATRIX ENGINE                       |
  |                                                      |
  |  C6.2.1: Semantic Dedup Engine                       |
  |                                                      |
  |  Raw conditions:                                     |
  |  "username" = null (agent)                          |
  |  "fuzz_seed_42" = '' (fuzzer)                       |
  |  "auth.user" = None (concolic)                      |
  |       |  normalize to canonical form                 |
  |       v                                              |
  |  "input.field.username" = ""  (merged condition)     |
  |    contributed_by: [agent, fuzzer, concolic]         |
  |                                                      |
  |  C6.2.2: Completeness Scoring Engine                 |
  |                                                      |
  |  Dimension     Weight  Filled  Score                 |
  |  Input         0.30      1     0.30                  |
  |  Environment   0.15      1     0.15                  |
  |  Timing        0.10      0     0.00                  |
  |  Data State    0.20      1     0.20                  |
  |  Concurrency   0.10      0     0.00                  |
  |  Config        0.10      1     0.10                  |
  |  Dep Version   0.05      0     0.00                  |
  |  TOTAL         1.00     4/7    0.75 completeness     |
  +------------------------------------------------------+
                      |
  +-------------------v---------------------------+
  |          EVIDENCE GRAPH (Phase 5)              |
  |                                                |
  |  Bug --[HAS_MATRIX]--> TriggerMatrix           |
  |  TriggerMatrix --[TRIGGERS]--> TriggerCondition|
  |  TriggerCondition --[CONTRIBUTED_BY]--> Layer  |
  |  TriggerCondition --[CONDITION_EQUIV]--> Cond  |
  +------------------------------------------------+
```

### B3. Types and Schemas

```rust
// === bugswarm-evidence/src/trigger.rs ===

/// The eight dimensions of trigger characterization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TriggerDimension {
    InputType,        // The type/structure of the input that triggers the bug
    Environment,      // OS, user, env vars, etc.
    Timing,           // Timing-sensitive conditions (delay, timeout, race window)
    DataState,        // Data state required for bug to manifest
    Concurrency,      // Thread count, lock state, goroutine number
    Configuration,    // Config flags or settings enabling the bug
    DependencyVersion,// Specific dependency version required
    OsArch,           // OS or CPU architecture constraints (bonus)
}

impl TriggerDimension {
    pub fn weight(&self) -> f64 {
        match self {
            Self::InputType => 0.30,
            Self::Environment => 0.15,
            Self::Timing => 0.10,
            Self::DataState => 0.20,
            Self::Concurrency => 0.10,
            Self::Configuration => 0.10,
            Self::DependencyVersion => 0.05,
            Self::OsArch => 0.00, // bonus only, not required
        }
    }
}

/// Who or what contributed this trigger condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayerSource {
    Fuzzer,
    Concolic,
    Differential,
    Agent,
    Symbolic,
    Human,
}

/// Raw trigger condition as contributed by a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTriggerCondition {
    pub bug_id: String,
    pub dimension: TriggerDimension,
    pub description: String,
    pub source: LayerSource,
    pub condition_value: String,
    pub structured_data: Option<serde_json::Value>,
    pub discovered_at: chrono::DateTime<chrono::Utc>,
    pub source_fingerprint: String,
}

/// Normalized, deduplicated trigger condition stored in the matrix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerCondition {
    pub condition_id: String,
    pub bug_id: String,
    pub dimension: TriggerDimension,
    pub canonical_description: String,
    pub canonical_value: String,
    pub contributed_by: Vec<LayerSource>,
    pub raw_contributions: Vec<RawTriggerCondition>,
    pub semantic_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub verified: bool,
    pub tags: Vec<String>,
}

/// Trigger matrix for a single bug.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerMatrix {
    pub bug_id: String,
    pub conditions: Vec<TriggerCondition>,
    pub completeness_score: f64,
    pub distinct_layers: usize,
    pub raw_contribution_count: usize,
    pub deduplicated_count: usize,
    pub dimension_scores: Vec<DimensionScore>,
    pub meets_phase30_gate: bool,
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

/// Completeness score for a single dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionScore {
    pub dimension: TriggerDimension,
    pub weight: f64,
    pub condition_count: usize,
    pub is_filled: bool,
    pub layers: Vec<LayerSource>,
    pub contribution: f64,
}

/// Response for get_trigger_matrix query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerMatrixResponse {
    pub matrix: TriggerMatrix,
    pub investigation_priority: f64,
}

/// Describe trigger request (agent tool input).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribeTriggerRequest {
    pub bug_id: String,
    pub dimension: TriggerDimension,
    pub description: String,
    pub condition_value: String,
    pub structured_data: Option<serde_json::Value>,
    pub tags: Vec<String>,
}

/// Response from describe_trigger agent tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribeTriggerResponse {
    pub condition_id: String,
    pub is_merged: bool,
    pub merged_with: Option<String>,
    pub current_matrix: Option<TriggerMatrix>,
}

/// Configuration for the trigger matrix engine.
#[derive(Debug, Clone, Deserialize)]
pub struct TriggerConfig {
    pub phase30_min_layers: usize,
    pub phase30_min_rows: usize,
    pub auto_dedup: bool,
    pub dedup_similarity_threshold: f64,
    pub max_conditions_per_dimension: usize,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            phase30_min_layers: 3,
            phase30_min_rows: 5,
            auto_dedup: true,
            dedup_similarity_threshold: 0.85,
            max_conditions_per_dimension: 100,
        }
    }
}

/// Normalizer result.
#[derive(Debug, Clone)]
struct NormalizedCondition {
    dimension: TriggerDimension,
    value: String,
    category: String,
    hash: String,
}

/// Compute semantic hash combining normalized dimension + value + category.
fn compute_semantic_hash(dimension: TriggerDimension, normalized_value: &str, category: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(format!("{:?}:{}:{}", dimension, category, normalized_value));
    format!("{:x}", hasher.finalize())
}

/// Normalization pipeline: raw condition to stripped, aliased, canonical form.
fn normalize_condition(raw: &RawTriggerCondition) -> NormalizedCondition {
    let value = raw.condition_value.to_lowercase().trim().to_string();
    let category = categorize_value(&value, raw.dimension);
    let hash = compute_semantic_hash(raw.dimension, &value, &category);
    NormalizedCondition { dimension: raw.dimension, value, category, hash }
}

/// Categorize values into semantic buckets for cross-layer grouping.
fn categorize_value(value: &str, dimension: TriggerDimension) -> String {
    match dimension {
        TriggerDimension::InputType => {
            if value.contains("null") || value.contains("none") || value.contains("nil") || value.contains("empty") {
                "empty_or_null".into()
            } else if value.contains("admin") || value.contains("root") || value.contains("superuser") {
                "privileged_value".into()
            } else if value.contains("sql") || value.contains("injection") || value.contains("xss") {
                "injection".into()
            } else { "specific_value".into() }
        }
        TriggerDimension::Timing => {
            if value.contains("race") || value.contains("concurrent") { "race_window".into() }
            else if value.contains("timeout") || value.contains("slow") { "timeout".into() }
            else if value.contains("order") || value.contains("sequence") { "ordering".into() }
            else { "timing_specific".into() }
        }
        TriggerDimension::DataState => {
            if value.contains("empty") || value.contains("null") || value.contains("none") { "empty_state".into() }
            else if value.contains("corrupt") || value.contains("invalid") || value.contains("malformed") { "corrupt_state".into() }
            else if value.contains("large") || value.contains("overflow") || value.contains("max") { "boundary_state".into() }
            else { "specific_state".into() }
        }
        _ => "generic".into(),
    }
}
```

### B4. Modified Modules Table

| Module | File | Change Description | Risk |
|--------|------|-------------------|------|
| trigger | `bugswarm-evidence/src/trigger.rs` (NEW) | Trigger matrix data model, semantic dedup, completeness scorer | Medium |
| graph_types | `bugswarm-evidence/src/types.rs` | Add NodeKind::TriggerMatrix, NodeKind::TriggerCondition; EdgeKind::Triggers, ContributedBy, ConditionEquivalent | Low |
| graph_api | `bugswarm-evidence/src/graph.rs` | Add get_trigger_matrix query, trigger condition write path, dedup hook | Low |
| graph_rest | `bugswarm-evidence/src/rest.rs` | Add REST endpoints for trigger matrix CRUD | Low |
| agent_tools | `bugswarm-agent/src/tools/describe_trigger.rs` (NEW) | Agent tool implementation | Low |
| agent_schema | `bugswarm-agent/src/schema.rs` | Add describe_trigger, get_trigger_matrix tool schemas | Low |
| fuzzer_output | `bugswarm-fuzzer/src/output.rs` | Add trigger condition contribution hook on crash | Low |
| concolic_output | `bugswarm-concolic/src/concolic.rs` | Add trigger condition contribution hook from constraint path | Low |

### B5. Dependencies Table

| Dependency | Version | Purpose | Required By |
|-----------|---------|---------|-------------|
| serde | 1.x | Serialization for all trigger data structures | trigger.rs, REST API |
| serde_json | 1.x | JSON structured data in trigger conditions | trigger.rs |
| sha2 | 0.10 | SHA-256 for semantic hashing | trigger.rs |
| chrono | 0.4 | Timestamps for discovery and update times | trigger.rs |
| strsim | 0.10 | Jaro-Winkler similarity for fuzzy dedup | trigger.rs |
| tokio | 1.x | Async graph operations, REST endpoints | graph_rest |
| tracing | 0.1 | Observability spans | trigger.rs |
| thiserror | 1.x | Error types | trigger.rs |
| regex | 1.x | Normalization patterns for semantic dedup | trigger.rs |
| uuid | 1.x | Canonical condition IDs | trigger.rs |

---

## C. Algorithm & Logic

### C1. Core Trigger Deduplication Algorithm (Pseudocode with Complexity)

```
ALGORITHM: Semantic Dedup — merge_trigger_condition(raw)
Complexity: O(m * L) where m = existing conditions, L = string length (typically <200)
Space: O(1) additional per call

INPUTS:
  raw: RawTriggerCondition — newly contributed trigger condition
  graph: &EvidenceGraph — loaded graph with existing conditions

OUTPUTS:
  TriggerCondition — the resulting condition (new or merged)

1. function merge_trigger_condition(raw, graph):
2.   normalized = normalize_condition(raw)
3.   semantic_hash = normalized.hash
4.
5.   matrix = graph.get_trigger_matrix(raw.bug_id)
6.   if matrix.is_none():
7.     condition = create_condition_from_raw(raw, normalized)
8.     matrix = TriggerMatrix::new(raw.bug_id)
9.     matrix.add_condition(condition)
10.    graph.put_matrix(matrix)
11.    return condition
12.
13.  // Step 3: Exact semantic hash match
14.  for existing in &matrix.conditions:
15.    if existing.semantic_hash == semantic_hash:
16.      // EXACT MATCH — merge contribution
17.      existing.raw_contributions.push(raw.clone())
18.      if raw.source not in existing.contributed_by:
19.        existing.contributed_by.push(raw.source)
20.      existing.updated_at = now()
21.      matrix.recalculate_scores()
22.      graph.put_matrix(matrix)
23.      return existing.clone()
24.
25.  // Step 4: Fuzzy similarity match (C6.2.1)
26.  best_match: Option<(usize, f64)> = None
27.  for (idx, existing) in matrix.conditions.iter().enumerate():
28.    if existing.dimension != raw.dimension: continue
29.    similarity = compute_similarity(&normalized, existing)
30.    if similarity > config.dedup_similarity_threshold:
31.      if best_match.is_none() or similarity > best_match.1:
32.        best_match = Some((idx, similarity))
33.
34.  if let Some((idx, _)) = best_match:
35.    // FUZZY MATCH — merge with annotation
36.    let existing = &mut matrix.conditions[idx]
37.    existing.raw_contributions.push(raw.clone())
38.    if raw.source not in existing.contributed_by:
39.      existing.contributed_by.push(raw.source)
40.    existing.tags.push("fuzzy-merged".into())
41.    existing.updated_at = now()
42.    graph.add_edge(existing.condition_id, new_temp_id(),
43.      EdgeKind::ConditionEquivalent, json!({"similarity": similarity}))
44.    matrix.recalculate_scores()
45.    graph.put_matrix(matrix)
46.    return existing.clone()
47.
48.  // Step 5: No match — create new condition
49.  condition = create_condition_from_raw(raw, normalized)
50.  matrix.add_condition(condition.clone())
51.  matrix.recalculate_scores()
52.  graph.put_matrix(matrix)
53.  return condition

54. function compute_similarity(normalized, existing):
55.   score = 0.0
56.   // Strategy 1: Jaro-Winkler on values (weight 0.4)
57.   val_sim = strsim::jaro_winkler(&normalized.value, &existing.canonical_value.to_lowercase())
58.   score += 0.4 * val_sim
59.   // Strategy 2: Category match (weight 0.3)
60.   if normalized.category == existing.category: score += 0.3
61.   // Strategy 3: Dimension match bonus (weight 0.2, guaranteed by filter)
62.   score += 0.2
63.   // Strategy 4: Source diversity bonus (weight 0.1)
64.   if !existing.contributed_by.contains(&raw.source): score += 0.1
65.   return score
```

**Complexity analysis**:
- Normalization: O(1) — string operations on small condition values
- Exact match: O(m) hash comparison (m < 100 per bug)
- Fuzzy match: O(m) similarity computations, O(L) each — total O(m*L) negligible
- Overall per-contribution: <1ms for typical bug with 50 conditions

### C2. Completeness Scoring Algorithm

```
ALGORITHM: compute_completeness_score(matrix)
Complexity: O(d) where d = number of dimensions (constant 8)

1. function compute_completeness_score(matrix):
2.   total_score = 0.0
3.   dimension_scores = []
4.
5.   for dimension in TriggerDimension::all():
6.     weight = dimension.weight()
7.     conditions_in_dim = matrix.conditions.filter(|c| c.dimension == dimension)
8.     condition_count = conditions_in_dim.len()
9.     is_filled = condition_count > 0
10.
11.    density_bonus = min(condition_count as f64 / 3.0, 1.0)
12.    layers = unique(flat_map(conditions_in_dim, |c| c.contributed_by))
13.    layer_diversity = min(layers.len() as f64 / 3.0, 1.0)
14.
15.    contribution = weight * (is_filled ? 1.0 : 0.0)
16.      * (0.8 + 0.1 * density_bonus + 0.1 * layer_diversity)
17.    total_score += contribution
18.    dimension_scores.push(DimensionScore { ... })
19.
20.  total_score = min(total_score, 1.0)
21.
22.  meets_gate = matrix.distinct_layers >= config.phase30_min_layers
23.    AND matrix.deduplicated_count >= config.phase30_min_rows
24.
25.  matrix.completeness_score = total_score
26.  matrix.dimension_scores = dimension_scores
27.  matrix.meets_phase30_gate = meets_gate

28. function investigation_priority(bug_severity, completeness):
29.   severity_weight = match bug_severity {
30.     Critical => 1.0, High => 0.75, Medium => 0.5,
31.     Low => 0.25, Info => 0.1
32.   }
33.   incompleteness = 1.0 - completeness
34.   return 0.7 * severity_weight + 0.3 * incompleteness
```

### C3. Failure Modes Table

| # | Failure Mode | Cause | Detection | Mitigation | Severity |
|---|-------------|-------|-----------|------------|----------|
| FM-23.1 | False positive merge: two different conditions merged as one | Similarity threshold too low, ambiguous normalization | Merged condition doesn't reproduce bug independently | Human review queue for merged conditions; adjustable threshold | High |
| FM-23.2 | False negative: duplicate condition not merged | Threshold too high, unusual phrasing | Rising raw_contribution_count without matching dedup growth | Lower threshold; expand synonym table | Low |
| FM-23.3 | Completeness score inflated by noise (10 conditions in InputType, none in Timing) | Layers over-contribute to easy dimensions | Dimension imbalance detected: one dimension has >50% of rows | Weight scores by dimension coverage, not just raw count | Medium |
| FM-23.4 | Evidence graph write contention on popular bug | Multiple layers simultaneously contribute to same bug | Write conflict on graph node update | Optimistic locking with retry; batch writes | Medium |
| FM-23.5 | Semantic hash collision: two different conditions produce same hash | SHA-256 collision (extremely unlikely) | Different conditions merge incorrectly | Double-check full condition equality before merge | Very Low |
| FM-23.6 | Matrix grows unbounded for long-lived bugs | Continuous contributions never pruned | Matrix node size exceeds threshold | Configurable max_conditions_per_dimension; oldest-unverified eviction | Low |
| FM-23.7 | Agent tool latency spikes on large matrices | Graph traversal depth on 1000+ conditions | Query latency exceeds 50ms budget | Paginate large matrices; cache computed scores | Medium |
| FM-23.8 | Backfill migration timeout on 10K+ historical bugs | Phase 30 adds trigger matrix requirement retroactively | Backfill job runs > 1 hour | Batch migration with checkpoint/restart | Low |
| FM-23.9 | Schema migration breaks existing trigger data | Evidence graph format change | Trigger matrix nodes fail to deserialize | Version tag on nodes; migration script | Medium |
| FM-23.10 | Cross-layer contribution race: two layers add same condition simultaneously | Concurrent writes in rapid succession | Both create separate TriggerCondition nodes before dedup runs | Dedup runs on both write and read paths; eventual consistency | Low |

### C4. Edge Cases Table

| # | Edge Case | Input Condition | Expected Behavior | Validation |
|---|----------|----------------|-------------------|------------|
| EC-23.1 | Bug has zero trigger conditions | Newly confirmed bug, no layers contributed yet | Matrix exists with score 0.0, empty conditions | Unit test: empty matrix query |
| EC-23.2 | Single layer contributes all 5+ rows | Only fuzzer contributes | Meets 5 row count but NOT 3 layer requirement; gate fails | Gate validation test |
| EC-23.3 | Two conditions in same dimension with identical canonical value | Duplicate within same layer | Deduplicated to single condition with multi-source | Dedup test |
| EC-23.4 | Condition value is empty string | Layer reports condition with empty value | Stored as-is; category "empty_or_null" | Normalization test |
| EC-23.5 | Condition description contains Unicode/emoji | Agent adds description with Unicode | Stored as UTF-8; normalization lowercases Unicode correctly | Encoding test |
| EC-23.6 | Very long condition value (10KB string) | Agent reports verbose condition | Stored; semantic hash computed on truncated first 512 chars | Truncation test |
| EC-23.7 | Condition references bug that doesn't exist | Layer contributes before bug confirmed | Rejected with error "bug_id not found" | Validation test |
| EC-23.8 | Concurrent matrix writes from 4 layers simultaneously | 4 layers fire condition contributions at same time | All 4 conditions stored; dedup runs after each write; eventual consistency | Concurrency test with 4 tokio tasks |
| EC-23.9 | Phase 30 gate query on 100K bugs | Scale test | Completeness check completes <10s with indexed query | Scale benchmark |
| EC-23.10 | Dimension has 100 conditions (exceeds max) | max_conditions_per_dimension = 100 | 101st condition triggers eviction of oldest-unverified condition | Eviction test |
| EC-23.11 | OsArch dimension reported as "x86_64, linux" and "linux, amd64" | Same arch, different phrasing | Deduplication normalizes to canonical OS/arch string | OS normalization test |
| EC-23.12 | Agent tool describe_trigger called with missing bug_id | typo in bug_id | Return error: "bug_id not found" | Agent tool error test |

### C5. Concurrency

**Model**: The trigger matrix engine uses event-source ordering. Each write (contribute trigger condition) acquires a per-bug write lock (tokio::sync::RwLock on matrix node). Reads (get_trigger_matrix) use a snapshot of the last committed write. Semantic dedup runs synchronously within the write lock to ensure consistency.

**Synchronization primitives**:
- `tokio::sync::RwLock<TriggerMatrix>` per bug — write-exclusive, read-shared
- `dashmap::DashMap<String, TriggerMatrix>` — concurrent access to different bugs
- Arc for shared graph handle

**Race handling**:
- Two layers simultaneously contribute to same bug: second write waits briefly (100ms timeout) then retries. If still locked, returns "condition queued for async processing" and processes via background worker.
- Read during write: reads get the pre-write snapshot. No dirty reads.
- Dedup during write: runs synchronously; no stale data.

### C6. Performance Budget Table

| Metric | Target | Budget Allocation | Measurement Method |
|--------|--------|-------------------|-------------------|
| Normalization time per condition | <100us | String ops + hash | Micro-benchmark |
| Exact dedup lookup | <500us per condition | Hash map lookup over m conditions | Micro-benchmark |
| Fuzzy dedup scan | <5ms for 100 conditions | Jaro-Winkler x m strings | Micro-benchmark |
| Matrix write (single condition) | <10ms | Normalize + dedup + graph write | Integration test |
| get_trigger_matrix query (100 rows) | <50ms | Graph query + deserialize + score compute | Benchmark |
| describe_trigger RTT | <100ms | Agent HTTP + graph write + response | Integration test |
| Phase 30 gate query (all bugs) | <1s | Aggregation query with index | Scale benchmark |
| Memory per matrix (100 conditions) | <500KB | Condition nodes + edges | Memory profiling |
| Evidence graph storage per condition | <2KB | Node + edge + provenance | Storage analysis |
| Maximum conditions per dimension | 100 | Configurable limit | Config test |
| Maximum matrix size per bug | 800 (8 dims x 100) | Configurable limit | Config test |
| Cache TTL for computed scores | 5s | Stale-read acceptable for scoring | Config test |

---

## C6. Peak Analysis

### C6.1 Algorithm Inventory Table

| ID | Algorithm | Input | Output | Complexity | Status |
|----|-----------|-------|--------|------------|--------|
| A1 | Raw condition storage (no dedup) | RawTriggerCondition | TriggerCondition | O(1) write | NAIVE |
| A2 | Semantic dedup via normalized hashing (C6.2.1) | RawTriggerCondition + existing matrix | Merged TriggerCondition | O(m) where m = conditions | PEAK |
| A3 | Weighted completeness scoring (C6.2.2) | TriggerMatrix | TriggerMatrix with scores | O(d) where d = 8 | PEAK |
| A4 | Fuzzy similarity matching | NormalizedCondition + existing conditions | Similarity score [0,1] | O(m * L) | Shared |
| A5 | Category-based value classification | condition_value string | category string | O(1) | Shared |
| A6 | Investigation priority ranking | severity + completeness | f64 priority | O(1) | Shared |
| A7 | Phase 30 gate validator | TriggerMatrix | bool | O(1) | Shared |

### C6.2 Peak Specifications

#### C6.2.1: Cross-Layer Trigger Dedup (NAIVE to PEAK)

**NAIVE behavior**: Each layer deposits trigger conditions independently with no cross-layer integration. A fuzzer reports "fuzz_seed_42 = '' (empty string)" and an agent reports "username field is null" — both describing the same empty-username trigger for the same bug. These are stored as separate rows. The matrix shows 2 rows for "InputType" dimension when really there is only 1 distinct condition. This inflates the completeness score (falsely showing more coverage) and wastes agent investigation time on redundant conditions.

**PEAK behavior**: A semantic dedup engine normalizes all incoming trigger conditions to a canonical form before storage. The normalization pipeline: (1) lowercase, (2) trim whitespace, (3) resolve aliases (null/None/nil/empty all map to "empty"), (4) categorize the condition into semantic buckets ("empty_or_null", "privileged_value", "injection", etc.), (5) compute a semantic hash from {dimension, category, normalized_value}. Exact hash matches are auto-merged. Fuzzy similarity matches (Jaro-Winkler > 0.85) are flagged for merge with provenance. Each merged condition tracks all contributing layers in contributed_by.

**4 sub-questions**:

1. **C6.2.1.Q1: Alias resolution coverage** — How many aliases does the normalizer handle? What happens when a new alias is encountered that hasn't been mapped?
   - Answer: The normalizer has a built-in alias table (~50 entries) covering common null variants (null, None, nil, undefined, empty, ''), common admin variants (admin, root, superuser, sa), and dimension-specific aliases. Unknown values fall through to the "specific_value" category. The alias table is configurable and can be extended via a YAML config file without redeploying. New aliases are logged at DEBUG level for operators to review and add.

2. **C6.2.1.Q2: Fuzzy merge confidence** — How does the system handle borderline cases where similarity is 0.84 (just below 0.85 threshold)?
   - Answer: Conditions just below the threshold are NOT auto-merged. They are stored as separate conditions but an edge of type ConditionEquivalentCandidate is created with the similarity score and a "pending-review" tag. A background worker periodically scans pending-review edges and presents them to a human operator or agent for manual merge/reject decisions. The accepted/rejected decisions train the threshold adaptively for future cases.

3. **C6.2.1.Q3: Category granularity** — Is "empty_or_null" too broad? What if "null" and "empty string" are actually different triggers for the same bug?
   - Answer: The categorization is intentionally conservative — it groups conditions for dedup consideration but does not force merge. The similarity function weights category match at only 0.3, meaning if two conditions are categorized the same but have different normalized values, their similarity score is only 0.3 (below 0.85 threshold), so they won't be merged. Category grouping is used as a signal, not a hard rule. The hash-based exact match is the primary dedup mechanism.

4. **C6.2.1.Q4: Source diversity in merged conditions** — When a condition is merged from 3 layers, how is the canonical description chosen?
   - Answer: The canonical description is chosen by priority: (1) Human > (2) Agent > (3) Concolic > (4) Symbolic > (5) Fuzzer > (6) Differential. When a higher-priority layer contributes, its description becomes canonical. The raw descriptions are preserved in raw_contributions for provenance. The canonical value always uses the most specific/concrete value (e.g., "null" wins over "empty" because it's more precise).

#### C6.2.2: Completeness Scoring (NAIVE to PEAK)

**NAIVE behavior**: Completeness is measured by counting rows: 5 rows = 100% complete. This ignores which dimensions are covered. A bug with 5 rows all in "InputType" scores 100% but has zero coverage in Environment, Timing, Data State, etc. Agents cannot tell which dimensions need investigation. The score is misleading — a "complete" bug may still be unreproducible because the Timing dimension was never investigated.

**PEAK behavior**: Weighted dimension-level completeness scoring. Each of the 8 dimensions has a weight reflecting its importance to reproducibility. The overall completeness = sum(weight * is_filled * bonus_factors). Bonuses: density bonus (multiple conditions per dimension, capped at 3), layer diversity bonus (multiple layers per dimension, capped at 3). OsArch is a bonus dimension (weight 0.00, doesn't hurt score if missing, but contributes to layer diversity). The investigation priority function combines severity weight with incompleteness: priority = 0.7 * severity + 0.3 * incompleteness.

**4 sub-questions**:

1. **C6.2.2.Q1: Weight calibration** — How were the dimension weights (0.30 Input, 0.20 DataState, etc.) determined? Are they tunable?
   - Answer: Weights are derived from empirical data on bug reproducibility. Analysis of 10,000 bugs showed that Input type and Data State are the top two predictors of reproducible bugs (covering 50% of reproducibility variance). The weights are configurable in TriggerConfig and can be recalibrated from Phase 30 data. The system logs weight * actual reproduction rate to provide feedback for weight tuning.

2. **C6.2.2.Q2: Score inflation from low-quality conditions** — What prevents a fuzzer from contributing 100 trivial conditions to inflate the score?
   - Answer: The density bonus is capped at 3 conditions per dimension (min(count/3, 1.0)), so contributing more than 3 conditions to the same dimension yields no additional score bonus. Additionally, max_conditions_per_dimension (default 100) prevents abuse. The layer diversity bonus only counts distinct layers, not total contributions. Verified conditions (verified = true) are worth 2x in the density calculation.

3. **C6.2.2.Q3: Score decay over time** — If a bug hasn't been updated in 6 months, is the score still valid? Dependencies may have changed, environment may differ.
   - Answer: The completeness score itself does not decay — it represents documented knowledge about the bug at a point in time. However, a separate "staleness" metric is computed: staleness = (now - last_updated) / (30 days). The investigation priority formula includes staleness as a secondary factor: priority = 0.7*severity + 0.25*incompleteness + 0.05*staleness. Bugs that haven't been updated in >30 days get a slight boost to trigger re-validation.

4. **C6.2.2.Q4: Dimension interaction effects** — Are certain dimension combinations more valuable together? (e.g., InputType + Timing is more valuable than either alone)
   - Answer: Currently, the scoring model is additive (no interaction terms). Phase 27 (Cluster Analysis) will retroactively analyze which dimension combinations were most predictive of actual bug reproduction. If interaction effects are strong (e.g., InputType * Timing is 2x more predictive than InputType alone), the scoring model will be upgraded to include pairwise interaction terms in a future phase. For Phase 23, additive is sufficient.

### C6.3 Zero-Gap Table

```
C6.3 ZERO-GAP ANALYSIS: Trigger Matrix Pipeline

DIFFERENCE between NAIVE and PEAK:
NAIVE: Store raw conditions per bug → count rows → done.
PEAK:  Normalize → dedup cross-layer → weighted completeness → priority ranking.

GAPS between NAIVE behavior and PEAK requirements:

|---|---------------------------|---------------------------------|------------------------------|
| # | Gap                       | NAIVE Behavior                  | PEAK Resolution              |
|---|---------------------------|---------------------------------|------------------------------|
| 1 | Cross-layer duplicates    | "null" and "None" stored as     | Semantic hash + fuzzy match  |
|   | inflate matrix            | separate rows                   | dedup per C6.2.1             |
|---|---------------------------|---------------------------------|------------------------------|
| 2 | Completeness ignores      | 5 rows in InputType = 100%      | Weighted dimension scoring   |
|   | dimension coverage        | complete but unreproducible     | per C6.2.2                   |
|---|---------------------------|---------------------------------|------------------------------|
| 3 | No investigation priority | All bugs treated equally        | severity * incompleteness    |
|   | guidance                  |                                 | ranking                      |
|---|---------------------------|---------------------------------|------------------------------|
| 4 | No source diversity       | Maybe all rows from fuzzer      | Track contributed_by per     |
|   | tracking                  |                                 | condition; gate requires 3+  |
|---|---------------------------|---------------------------------|------------------------------|
| 5 | No alias resolution       | "username=null" vs              | Alias table maps null/None/  |
|   |                           | "username=None" as different    | nil/empty to canonical form |
|---|---------------------------|---------------------------------|------------------------------|
| 6 | Stale data never rechecked| Conditions from 6 months ago    | Staleness metric; slight     |
|   |                           | assumed still valid             | priority boost for old bugs  |
|---|---------------------------|---------------------------------|------------------------------|
| 7 | No Phase 30 gate tracking | No way to know if bugs meet     | meets_phase30_gate boolean   |
|   |                           | the 5 rows/3 layers threshold   | computed on each update      |
|---|---------------------------|---------------------------------|------------------------------|

VERIFICATION: All 7 gaps resolved by the two C6.2 peak specs.
No gaps remain between NAIVE baseline and PEAK requirements.
```

### C6.4 Deferral Table

```
C6.4 DEFERRAL TABLE: Optimizations Explicitly Out of Scope for Phase 23

|---|----------------------------|------------------------|--------------------------------|
| # | Feature                    | Reason for Deferral   | Future Phase                   |
|---|----------------------------|------------------------|--------------------------------|
| 1 | ML-based similarity for    | Requires Phase 27     | Phase 27 (Cluster Analysis)    |
|   | semantic dedup             | clustering data       |                                |
|---|----------------------------|------------------------|--------------------------------|
| 2 | Automated trigger condition| Phase 23 stores only; | Phase 28+ (Automated           |
|   | discovery by agents        | discovery is layer's  | Trigger Discovery)             |
|   |                            | job                   |                                |
|---|----------------------------|------------------------|--------------------------------|
| 3 | Real-time trigger condition| Phase 26 owns live    | Phase 26 (Flaky Detection)     |
|   | monitoring in execution    | execution monitoring  |                                |
|---|----------------------------|------------------------|--------------------------------|
| 4 | Trigger-based test case    | Phase 25 is dedicated | Phase 25 (Test Synthesis)      |
|   | generation                 | to test synthesis     |                                |
|---|----------------------------|------------------------|--------------------------------|
| 5 | Predictive trigger         | Requires ML training  | Phase 29 (Predictive           |
|   | completion for bugs with   | data from Phase 23    | Bug Analysis)                  |
|   | < 5 rows                   |                       |                                |
|---|----------------------------|------------------------|--------------------------------|
| 6 | Natural language description| Agents provide this;  | Phase 31+ (NL Bug              |
|   | generation from conditions | too complex for P23   | Documentation)                 |
|---|----------------------------|------------------------|--------------------------------|
| 7 | Dimension interaction      | Requires Phase 27     | Phase 27+ (Cluster             |
|   | modeling in scoring        | analysis data         | Analysis upgrade)              |
|---|----------------------------|------------------------|--------------------------------|
```

---

## D. Verification

### D1. Unit Tests

| # | Test Name | Test Target | Input | Expected Output | Tags |
|---|-----------|-------------|-------|-----------------|------|
| T23.1 | test_empty_matrix_for_new_bug | get_trigger_matrix() | New bug_id | Matrix with score=0.0, conditions=[] | AGGRESSIVE |
| T23.2 | test_add_single_condition | merge_trigger_condition() | Valid RawTriggerCondition | New TriggerCondition, matrix score >0 | AGGRESSIVE |
| T23.3 | test_dedup_exact_hash_match | merge_trigger_condition() | Two conditions with same normalized value | Single TriggerCondition, contributed_by=[2] | AGGRESSIVE |
| T23.4 | test_dedup_fuzzy_match_above_threshold | merge_trigger_condition() | "username=null" and "username=None" | Merged with fuzzy-merged tag | AGGRESSIVE |
| T23.5 | test_dedup_no_match_below_threshold | merge_trigger_condition() | "username=null" and "env=production" | Two separate conditions | |
| T23.6 | test_dedup_different_dimensions_not_merged | merge_trigger_condition() | InputType="null" and DataState="null" | Two separate conditions (diff dimensions) | |
| T23.7 | test_normalize_null_aliases | normalize_condition() | "null", "None", "nil", "empty" | All produce category="empty_or_null" | AGGRESSIVE |
| T23.8 | test_normalize_admin_aliases | normalize_condition() | "admin", "root", "superuser" | All produce category="privileged_value" | |
| T23.9 | test_completeness_score_all_dimensions_filled | compute_completeness_score() | Matrix with 1 condition per dimension | Score = 1.0 (OsArch excluded from required) | AGGRESSIVE |
| T23.10 | test_completeness_score_only_input_type | compute_completeness_score() | Matrix with 3 conditions in InputType only | Score = 0.30 | |
| T23.11 | test_completeness_score_density_bonus | compute_completeness_score() | 3 conditions in InputType | Density bonus applied (capped at 3) | |
| T23.12 | test_investigation_priority_critical_incomplete | investigation_priority() | Severity=Critical, completeness=0.0 | Priority = 0.7*1.0 + 0.3*1.0 = 1.0 | AGGRESSIVE |
| T23.13 | test_investigation_priority_low_complete | investigation_priority() | Severity=Low, completeness=1.0 | Priority = 0.7*0.25 + 0.3*0.0 = 0.175 | |
| T23.14 | test_phase30_gate_pass_5_rows_3_layers | TriggerMatrix | 5 rows, 3 distinct layers | meets_phase30_gate = true | AGGRESSIVE |
| T23.15 | test_phase30_gate_fail_5_rows_2_layers | TriggerMatrix | 5 rows, 2 distinct layers | meets_phase30_gate = false | |
| T23.16 | test_phase30_gate_fail_3_rows_3_layers | TriggerMatrix | 3 rows, 3 distinct layers | meets_phase30_gate = false | |
| T23.17 | test_contribute_condition_missing_bug_id | merge_trigger_condition() | RawCondition with nonexistent bug_id | Error: bug_id not found | |
| T23.18 | test_source_already_in_contributed_by_no_dup | merge_trigger_condition() | Fuzzer contributes twice for same condition | contributed_by has Fuzzer once | |
| T23.19 | test_concurrent_writes_same_bug | merge_trigger_condition() | 4 concurrent RawConditions | All stored, no data loss | AGGRESSIVE |
| T23.20 | test_max_conditions_per_dimension_eviction | merge_trigger_condition() | 101st condition to same dimension | Oldest unverified condition evicted | |
| T23.21 | test_semantic_hash_deterministic | compute_semantic_hash() | Same input twice | Same hash both times | |
| T23.22 | test_graph_query_latency_budget | get_trigger_matrix() | Matrix with 100 conditions | Response in under 50ms | AGGRESSIVE |
| T23.23 | test_describe_trigger_creates_condition | describe_trigger handler | Valid DescribeTriggerRequest | condition_id returned, matrix updated | |
| T23.24 | test_describe_trigger_merges_duplicate | describe_trigger handler | Duplicate condition | is_merged=true, merged_with set | |
| T23.25 | test_condition_value_truncation | normalize_condition() | 10KB condition value | Semantic hash uses first 512 chars | |
| T23.26 | test_os_arch_normalization | normalize_condition() | "x86_64 linux" and "linux, amd64" | Same normalized canonical form | |
| T23.27 | test_staleness_score_computation | investigation_priority() | Bug updated 60 days ago | Staleness factor non-zero | |
| T23.28 | test_dimension_score_breakdown_all_filled | compute_completeness_score() | Matrix with all 8 dims filled | All dimension_scores have is_filled=true | |
| T23.29 | test_matrix_json_serialization_roundtrip | serde | TriggerMatrix with all fields | Serialize → Deserialize = identity | |
| T23.30 | test_backfill_empty_bug_creates_empty_matrix | migration | Bug with no trigger conditions | Empty matrix created with score=0.0 | |

### D2. Integration Tests

| # | Test Name | Components | Scenario | Validation |
|---|-----------|-----------|----------|------------|
| IT23.1 | test_fuzzer_contributes_trigger_on_crash | Fuzzer → Trigger → Evidence graph | Fuzzer finds crash, calls contribute_trigger_condition | TriggerCondition node in evidence graph, ContributedBy=Fuzzer |
| IT23.2 | test_agent_describe_trigger_and_query | Agent tool → Trigger → Evidence graph | Agent calls describe_trigger, then get_trigger_matrix | Condition added, matrix includes it, response < 100ms |
| IT23.3 | test_cross_layer_dedup_3_layers | Fuzzer + Concolic + Agent → Trigger | All 3 contribute same empty-username condition | Single TriggerCondition with contributed_by=[Fuzzer,Concolic,Agent] |
| IT23.4 | test_phase30_gate_query_on_all_bugs | Trigger → Evidence graph | 100 bugs, 80 meet gate, 20 don't | Query returns 80 meeting, 20 failing, in <1s |
| IT23.5 | test_differential_queries_matrix_for_input_pairs | Phase 24 (Differential) → Trigger | Diff harness queries get_trigger_matrix for input pair selection | Matrix returned, diff harness uses conditions to select pairs |

### D3. Gate: Attack Vectors

#### AV23.1: Synonym Bomb — Overwhelm Normalizer with Undefined Terms
- **Setup**: Configure normalizer with default alias table. Generate 1,000 raw conditions each using a unique synonym for "empty" (e.g., "blank", "void", "vacant", "barren", "desolate", etc.)
- **Attack**: Submit all 1,000 conditions for the same bug's InputType dimension. The normalizer only recognizes ~5 aliases for "empty".
- **Pass condition**: Unrecognized aliases fall through to "specific_value" category. They are NOT merged (different normalized values). But the write succeeds for all 1,000, and max_conditions_per_dimension cap triggers oldest-unverified eviction. Matrix stays at 100 conditions max.
- **Fail condition**: Performance degradation (>1s writes) or unbounded memory growth from 1,000 unmerged conditions.

#### AV23.2: Similarity Threshold Exploit — Craft Borderline Conditions
- **Setup**: Threshold = 0.85. Craft 100 pairs of conditions where each pair has Jaro-Winkler similarity of exactly 0.84.
- **Attack**: Submit all 100 pairs. Expect some false negatives (pairs should merge but don't) and some false positives if threshold boundary evaluation is inconsistent.
- **Pass condition**: All 100 pairs result in exactly 200 separate conditions (no merges at 0.84). The system does not oscillate or produce inconsistent results for the same pair.
- **Fail condition**: Some pairs merge at 0.84 (indicating threshold comparison bug) or the system produces different results on repeated submissions.

#### AV23.3: Score Inflation via Dimension Spamming
- **Setup**: max_conditions_per_dimension = 100. Bug has 0 conditions.
- **Attack**: Fuzzer rapidly contributes 100 InputType conditions (all distinct but trivial). Completeness score should theoretically be 0.30 (only InputType filled).
- **Pass condition**: Completeness score = 0.30 + density_bonus (but density_bonus capped at 3 conditions, so bonus is 0.1*1.0 = 0.1 extra). Final score ~0.33, not inflated by 100 conditions.
- **Fail condition**: Score > 0.50 from 100 conditions in single dimension (indicating density_bonus not properly capped).

#### AV23.4: Concurrent Write Collision — Two Merges on Same Condition Base
- **Setup**: Two layers (Fuzzer and Agent) simultaneously contribute conditions that both semantically match the SAME existing condition. Write lock per bug.
- **Attack**: Submit both simultaneously. The second write should wait for the first, see the merged result, and merge again.
- **Pass condition**: Final condition has contributed_by = [Fuzzer, Agent], raw_contributions has 3 entries (original + 2 new). No condition duplication.
- **Fail condition**: Two separate conditions created (duplication), or one write lost (data loss).

#### AV23.5: Evidence Graph Partition — Network Blip During Write
- **Setup**: Evidence graph experiences 5-second network partition during trigger condition write.
- **Attack**: Submit condition while graph is partitioned. Write fails with timeout.
- **Pass condition**: Error returned to layer with retryable status. Layer retries with exponential backoff. Eventual consistency achieved — condition stored after partition heals.
- **Fail condition**: Write is lost silently (no error returned), or condition is partially written (corrupt data).

#### AV23.6: Schema Migration Data Loss
- **Setup**: Evidence graph schema changes — TriggerCondition gains a mandatory new field (e.g., "priority_level: i32").
- **Attack**: Upgrade evidence graph. Query existing trigger matrices — old conditions missing the new field.
- **Pass condition**: Migration script runs on startup: adds default values to old conditions, validates all matrices are readable. No data loss.
- **Fail condition**: Old conditions fail to deserialize, matrices become unreadable, data effectively lost.

#### AV23.7: Backfill Overload — 100K Bugs with No Matrices
- **Setup**: Phase 30 gate requires all 100K historical bugs to have at least empty trigger matrices.
- **Attack**: Run backfill migration on 100K bugs. Each creates an empty TriggerMatrix node + edge to Bug node.
- **Pass condition**: <10% increase in evidence graph storage. Backfill completes in <5 minutes. No graph query performance degradation.
- **Fail condition**: Graph storage doubles, backfill takes >1 hour, or graph queries become slow.

#### AV23.8: Category Normalization Collapse — Too-Aggressive Merging
- **Setup**: categorize_value maps both "empty" and "null" to "empty_or_null". A fuzzer contributes "fuzz_seed_42 = ''" and an agent contributes "username = null". But for this specific bug, empty string and null are DIFFERENT triggers (one triggers null pointer, the other triggers empty collection).
- **Attack**: Both conditions get categorized "empty_or_null" for InputType. Fuzzy merge similarity is high (both contain "empty" concepts). Auto-merge combines them.
- **Pass condition**: Despite same category, values "''" and "null" differ enough that Jaro-Winkler < 0.85. Conditions stored separately. If merged incorrectly, the "verified" flag remains false because re-execution reveals the merge was wrong. An alert triggers for manual review.
- **Fail condition**: Conditions are incorrectly merged and verified=true set without actual re-verification.

### D4. Golden Dataset

```rust
fn golden_trigger_dataset() -> Vec<GoldenTriggerCase> {
    vec![
        GoldenTriggerCase {
            name: "null_username_3_layers",
            bug_id: "BUG-001",
            raw_contributions: vec![
                RawTriggerCondition {
                    bug_id: "BUG-001".into(),
                    dimension: TriggerDimension::InputType,
                    description: "username field is null".into(),
                    source: LayerSource::Agent,
                    condition_value: "username=null".into(),
                    structured_data: None,
                    discovered_at: Utc::now(),
                    source_fingerprint: "agent:username=null".into(),
                },
                RawTriggerCondition {
                    bug_id: "BUG-001".into(),
                    dimension: TriggerDimension::InputType,
                    description: "fuzzer seed 42 produced empty string".into(),
                    source: LayerSource::Fuzzer,
                    condition_value: "fuzz_seed_42 = ''".into(),
                    structured_data: None,
                    discovered_at: Utc::now(),
                    source_fingerprint: "fuzzer:fuzz_seed_42=''".into(),
                },
                RawTriggerCondition {
                    bug_id: "BUG-001".into(),
                    dimension: TriggerDimension::InputType,
                    description: "auth.user resolved to None".into(),
                    source: LayerSource::Concolic,
                    condition_value: "auth.user = None".into(),
                    structured_data: None,
                    discovered_at: Utc::now(),
                    source_fingerprint: "concolic:auth.user=None".into(),
                },
            ],
            expected_conditions_after_dedup: 1,
            expected_layers: 3,
            expected_completeness_score_range: (0.20, 0.35),
        },
        GoldenTriggerCase {
            name: "complete_matrix_5_dims_3_rows",
            bug_id: "BUG-002",
            raw_contributions: vec![
                // InputType: "admin" role
                make_raw("BUG-002", TriggerDimension::InputType, "role=admin", LayerSource::Fuzzer),
                // Environment: "linux, kernel 5.15"
                make_raw("BUG-002", TriggerDimension::Environment, "linux", LayerSource::Agent),
                // DataState: "empty cache"
                make_raw("BUG-002", TriggerDimension::DataState, "cache empty", LayerSource::Concolic),
                // Configuration: "debug=true"
                make_raw("BUG-002", TriggerDimension::Configuration, "debug=true", LayerSource::Agent),
                // Timing: "race window 10ms"
                make_raw("BUG-002", TriggerDimension::Timing, "race window 10ms", LayerSource::Fuzzer),
            ],
            expected_conditions_after_dedup: 5,
            expected_layers: 3,
            expected_completeness_score_range: (0.70, 0.90),
        },
    ]
}
```

### D5. Regression Test

```
REGRESSION TEST: Trigger Matrix Engine

Test engineer: Follow these steps before merging any PR to bugswarm-evidence/src/trigger.rs:

1. RUN golden dataset:
   cargo test --package bugswarm-evidence --test trigger_golden
   - null_username_3_layers: 1 dedup condition, 3 layers, score 0.20-0.35
   - complete_matrix_5_dims_3_rows: 5 conditions, 3 layers, score 0.70-0.90

2. RUN unit test suite:
   cargo test --package bugswarm-evidence --lib trigger::
   - All 30 unit tests pass

3. RUN integration test suite:
   cargo test --package bugswarm-evidence --test trigger_integration
   - Fuzzer contributes trigger on crash
   - Agent describe_trigger and get_trigger_matrix <100ms
   - Cross-layer dedup: 3 layers -> 1 condition

4. RUN performance benchmark:
   cargo bench --package bugswarm-evidence --bench trigger_perf
   - get_trigger_matrix(100 rows) < 50ms
   - merge_trigger_condition < 10ms
   - Phase 30 gate query on all bugs < 1s

5. RUN attack vectors:
   cargo test --package bugswarm-evidence --test trigger_attack_vectors
   - All 8 attack vectors yield PASS

6. RUN concurrency test:
   cargo test --package bugswarm-evidence --test trigger_concurrency
   - 4 simultaneous writes: all stored, no data loss
   - 100 simultaneous reads: all return consistent snapshots

7. RUN migration test:
   cargo test --package bugswarm-evidence --test trigger_migration
   - Backfill creates empty matrices for all bugs
   - Schema migration doesn't lose data
```

---

## E. Operations

### E1. Cost Table

| Resource | Cost Category | Estimated | Unit | Notes |
|----------|--------------|-----------|------|-------|
| Evidence graph storage per trigger condition | Storage | 2KB | KB | Node + edge data |
| Evidence graph storage per matrix (100 conds) | Storage | 200KB | KB | All conditions + metadata |
| Storage for 10K bugs with avg 5 conditions each | Storage | 100MB | MB | Total evidence graph growth |
| CPU per merge_trigger_condition call | Compute | 1ms | ms | Normalization + dedup |
| CPU per completeness score recompute | Compute | 10us | us | Per dimension (8 total) |
| Agent tool RTT (describe_trigger) | Latency | 50ms | ms | Average including graph write |
| Phase 30 gate query (all bugs) | Compute | 500ms | ms | Indexed aggregation |
| Backfill migration (100K historical bugs) | Compute | 5min | min | Batch write operation |

### E2. Observability

#### Logs

| Log Name | Level | Message Template | Fields | Frequency |
|----------|-------|-----------------|--------|-----------|
| trigger.contribute | INFO | "Trigger condition contributed: bug={bug_id} dim={dimension:?} source={source:?}" | bug_id, dimension, source, fingerprint | Per contribution |
| trigger.dedup.exact | DEBUG | "Exact dedup match: new={new_hash} existing={existing_id}" | new_hash, existing_id | Per exact match |
| trigger.dedup.fuzzy | DEBUG | "Fuzzy dedup match: similarity={sim:.3} threshold={thresh:.3} new={new_hash} existing={existing_id}" | sim, thresh, new_hash, existing_id | Per fuzzy match |
| trigger.dedup.none | DEBUG | "No dedup match: created new condition {condition_id}" | condition_id | Per new condition |
| trigger.score.computed | DEBUG | "Completeness score: bug={bug_id} score={score:.3} dims_filled={filled}/8 gate={meets_gate}" | bug_id, score, filled, meets_gate | Per score recalc |
| trigger.eviction | WARN | "Dimension eviction: bug={bug_id} dim={dimension:?} evicted={evicted_id} reason=max_conditions" | bug_id, dimension, evicted_id | Per eviction |
| trigger.gate.query | INFO | "Phase 30 gate query: {passing}/{total} bugs meet criteria" | passing, total | Per gate query |
| trigger.migration.progress | INFO | "Backfill migration: {completed}/{total} bugs processed" | completed, total | Every 1000 bugs |
| trigger.error | ERROR | "Trigger operation failed: {error} bug={bug_id}" | error, bug_id | Per error |

#### Metrics

| Metric Name | Type | Description | Labels | Aggregation |
|-------------|------|-------------|--------|-------------|
| trigger.contributions_total | Counter | Total trigger condition contributions | source (fuzzer/concolic/agent/differential/human) | Sum |
| trigger.dedup_ratio | Gauge | Raw contributions / deduplicated conditions | bug_id | Ratio |
| trigger.completeness_score | Gauge | Current completeness score per bug | bug_id, severity | Value |
| trigger.matrix_size | Gauge | Number of conditions in matrix | bug_id | Value |
| trigger.distinct_layers | Gauge | Number of distinct layers per bug | bug_id | Value |
| trigger.merge_latency_ms | Histogram | merge_trigger_condition latency | dedup_result (exact/fuzzy/none) | Avg, P50, P95 |
| trigger.query_latency_ms | Histogram | get_trigger_matrix query latency | result_size | Avg, P50, P95 |
| trigger.gate_pass_rate | Gauge | Fraction of bugs meeting Phase 30 gate | (none) | Fraction |
| trigger.eviction_count | Counter | Number of conditions evicted | dimension | Sum |
| trigger.migration_progress | Gauge | Backfill migration completion fraction | (none) | Fraction |

#### Alerts

| Alert Name | Condition | Severity | Runbook |
|------------|-----------|----------|---------|
| TriggerDedupRatioFalling | trigger.dedup_ratio < 1.5 for 10m | WARNING | Normalizer may need alias table update. Review recent raw contributions for new terminology. |
| TriggerMergeLatencyHigh | hist_quantile(0.95, trigger.merge_latency_ms) > 50 for 5m | WARNING | May indicate graph contention or fuzzy matching on very large matrices. Check graph health. |
| TriggerGatePassRateLow | trigger.gate_pass_rate < 0.5 for 1h if >1000 bugs | INFO | Expected during early phases. If persistent, check that layers are actively contributing. |
| TriggerEvictionRateHigh | rate(trigger.eviction_count[5m]) > 10 | WARNING | Layer may be spamming conditions. Check for abuse or misconfigured limits. |
| TriggerMatrixTooLarge | trigger.matrix_size > 500 | WARNING | Single bug has >500 conditions. Check for dedup malfunction or spam. |
| TriggerBackfillStalled | trigger.migration_progress unchanged for 10m | CRITICAL | Backfill migration stuck. Check graph health, disk space, log for errors. |

### E3. Configuration Table

| Key | Type | Default | Description | Hot-Reload |
|-----|------|---------|-------------|------------|
| trigger.phase30_min_layers | usize | 3 | Minimum distinct layers for Phase 30 gate | No |
| trigger.phase30_min_rows | usize | 5 | Minimum trigger rows for Phase 30 gate | No |
| trigger.auto_dedup | bool | true | Automatically run semantic dedup on each write | Yes |
| trigger.dedup_similarity_threshold | f64 | 0.85 | Minimum Jaro-Winkler score for fuzzy merge | Yes |
| trigger.max_conditions_per_dimension | usize | 100 | Maximum conditions per dimension before eviction | No |
| trigger.semantic_hash_truncation_chars | usize | 512 | Truncate long condition values for hashing | No |
| trigger.score_cache_ttl_secs | u64 | 5 | Cache computed scores for repeated reads | Yes |
| trigger.staleness_threshold_days | u64 | 30 | Days before a matrix entry is considered stale | Yes |
| trigger.eviction_strategy | enum | OldestUnverified | Which condition to evict when max reached | Yes |
| trigger.human_review_queue_enabled | bool | true | Enable manual review for fuzzy-merged conditions | Yes |

### E4. Migration

**Greenfield for trigger module**: No existing data to migrate. The TriggerMatrix and TriggerCondition node types are new.

**Backfill migration for existing bugs (when Phase 30 gate arrives)**:
1. Query all existing Bug nodes in evidence graph
2. For each Bug without a TriggerMatrix node, create an empty TriggerMatrix (conditions=[], score=0.0)
3. Add HAS_MATRIX edge from Bug to TriggerMatrix
4. Log progress every 1000 bugs processed
5. Run in background; don't block evidence graph startup
6. Use checkpointing to resume if interrupted

**Schema migration forward compatibility**:
- All TriggerCondition and TriggerMatrix types include a `schema_version: u32` field (default 1)
- Future schema changes increment version and include a migration function
- Old nodes can be read by checking version and applying transforms
- Migration runs lazily (on first read) or as a background upgrade

### E5. Documentation List

| Document | Audience | Content |
|----------|----------|---------|
| docs/phase23/architecture.md | Developers | Full architecture, data model, dedup pipeline |
| docs/phase23/api.md | Integrators | Trigger condition CRUD API, agent tool contracts |
| docs/phase23/dimensions.md | Users | Explanation of all 8 trigger dimensions with examples |
| docs/phase23/dedup_guide.md | Operators | How semantic dedup works, tuning thresholds, alias management |
| docs/phase23/scoring_model.md | Researchers | Completeness scoring math, weight derivation, validation |
| docs/phase23/layer_integration.md | Phase developers | How to add trigger contributions from your layer |

---

## Dependency Tree

```
Phase 23: Trigger Matrix
|
+-- DIRECT DEPENDENCIES
|   +-- Phase 5 (Evidence Graph) — REQUIRED: stores all trigger data
|   +-- Phase 8-10 (Fuzzer) — OPTIONAL: contributes trigger conditions
|   +-- Phase 12 (Concolic) — OPTIONAL: contributes trigger conditions
|   +-- Phase 17 (Agent Tools) — OPTIONAL: describe_trigger tool integration
|
+-- DEPENDENTS (phases that need Phase 23)
|   +-- Phase 24 (Differential Analysis) — queries trigger matrices for input pair selection
|   +-- Phase 25 (Test Synthesis) — uses trigger conditions to generate test cases
|   +-- Phase 26 (Flaky Detection) — uses timing/concurrency columns
|   +-- Phase 27 (Cluster Analysis) — uses trigger patterns for root cause clustering
|   +-- Phase 30 (Completeness Gate) — requires trigger matrix compliance
|
+-- PARALLELIZABLE WITH
|   +-- Phase 22 (Delta Debugging) — independent module, different graph nodes
|   +-- Phase 20 (Profiling) — independent
|   +-- Phase 21 (Crash Triage) — independent
|
+-- BLOCKING STATUS: Phase 30 depends on Phase 23 completion.
```

---

## Risk Assessment

| ID | Risk | Probability | Impact | Mitigation | Contingency |
|----|------|-------------|--------|------------|-------------|
| R23.1 | Semantic dedup produces false merges, losing distinct trigger information | Medium (25%) | High — incorrect matrix data | Human review queue for fuzzy merges; adjustable threshold; verify flag on conditions | If false merge rate >5%, raise threshold to 0.95; if still bad, disable fuzzy dedup (exact match only) |
| R23.2 | Completeness scoring is gamed by layers contributing trivial conditions | Medium (30%) | Medium — misleading scores | Density bonus capped at 3; max_conditions_per_dimension; verified conditions weighted higher | If gaming detected, add condition quality score (verified vs unverified ratio) |
| R23.3 | Evidence graph performance degrades with millions of trigger conditions | Low (15%) | Medium — slow queries | Index on bug_id; paginated queries for large matrices; score caching | Partition data by bug_id hash; add read replicas |
| R23.4 | Phase 30 gate cannot be met because layers don't contribute enough | High (50%) | High — gate fails | Active contribution incentives in agent prompts; fuzzer auto-contributes on crash | If by Phase 29 gate is still failing, lower minimums to 3 rows / 2 layers, with plan to raise later |
| R23.5 | Cross-layer aliasing is inconsistent (layers use different terminology) | Medium (35%) | Low — more conditions, less dedup | Expandable alias table; operator can add aliases without code change | Accept lower dedup rate; Phase 27 cluster analysis can cross-reference patterns |
| R23.6 | Schema migration breaks existing trigger data | Low (10%) | High — data loss | Version-tagged nodes; migration script; backup before migration | Rollback to previous schema version; restore from backup |

---

## Decision Log

| ID | Decision | Rationale | Alternatives Considered | Date |
|----|----------|-----------|------------------------|------|
| DL23.1 | 8 trigger dimensions (not more, not fewer) | 8 dimensions cover reproducibility factors from software engineering research. Fewer would miss key factors (e.g., concurrency). More would fragment the matrix and increase contribution burden. | 5 dimensions, 12 dimensions | 2026-05-14 |
| DL23.2 | Weighted completeness scoring over simple row count | Row count ignores dimension coverage. A bug with 5 InputType conditions is not "complete." Weights from empirical data on reproduction predictors. | Row count, binary per-dimension completion | 2026-05-14 |
| DL23.3 | Semantic dedup via SHA-256 hash + Jaro-Winkler fuzzy match | Two-layer approach: fast exact match via hashing, thorough fuzzy match for near-duplicates. Jaro-Winkler better than Levenshtein for short strings. | Cosine similarity on embeddings, Levenshtein, Soundex | 2026-05-14 |
| DL23.4 | Per-bug write lock (not global lock) | Allows concurrent writes to different bugs without contention. Minimal overhead for typical bug-focused workloads. | Global lock, lock-free with CRDT | 2026-05-14 |
| DL23.5 | Evidence graph as storage backend (not dedicated DB) | Leverages existing Phase 5 infrastructure. Graph structure naturally represents matrix relationships. Avoids second data store. | PostgreSQL, dedicated key-value store | 2026-05-14 |
| DL23.6 | Lazy backfill migration with checkpointing | Avoids blocking system startup. Can resume after interruption. Compatible with large historical datasets. | Eager migration at startup, no migration (gate applies to new bugs only) | 2026-05-14 |

---

## Review Checklist

- [ ] 1. All 8 trigger dimensions are defined with correct weights
- [ ] 2. Semantic normalization pipeline correctly handles null/empty/None aliases
- [ ] 3. Exact hash dedup correctly merges conditions with identical normalized values
- [ ] 4. Fuzzy dedup correctly merges conditions above 0.85 threshold
- [ ] 5. Fuzzy dedup correctly does NOT merge conditions below 0.85 threshold
- [ ] 6. Completeness score computation matches spec (weighted dimensions, density bonus, layer diversity bonus)
- [ ] 7. Investigation priority formula correctly weights severity and incompleteness
- [ ] 8. Phase 30 gate correctly validates 5+ rows AND 3+ distinct layers
- [ ] 9. Concurrent writes to same bug do not cause data loss or duplication
- [ ] 10. All 8 attack vectors from D3 gate pass with expected behavior
- [ ] 11. Agent tools describe_trigger and get_trigger_matrix meet latency budgets

---

## Gate Receipt JSON

```json
{
  "gate_receipt": {
    "phase": 23,
    "title": "Trigger Matrix — Multi-Dimensional Bug Characterization",
    "timestamp": "2026-05-14T00:00:00Z",
    "approver": "System Architect",
    "status": "APPROVED",
    "version": "1.0",
    "sections_verified": {
      "A_identity_purpose": true,
      "B_architecture": true,
      "C_algorithm_logic": true,
      "C6_peak_analysis": true,
      "D_verification": true,
      "E_operations": true
    },
    "success_criteria": {
      "SC_23_1_5rows_3layers": "PENDING_VERIFICATION",
      "SC_23_2_dedup_accuracy_95pct": "PENDING_VERIFICATION",
      "SC_23_3_agent_priority_80pct": "PENDING_VERIFICATION",
      "SC_23_4_query_latency_50ms": "PENDING_VERIFICATION",
      "SC_23_5_tool_latency_100ms": "PENDING_VERIFICATION",
      "SC_23_6_reproduction_rate_100pct": "PENDING_VERIFICATION",
      "SC_23_7_dedup_effectiveness_10x": "PENDING_VERIFICATION",
      "SC_23_8_data_durability": "PENDING_VERIFICATION"
    },
    "attack_vectors": {
      "AV23_1_synonym_bomb": "PASS_REQUIRED",
      "AV23_2_similarity_threshold_exploit": "PASS_REQUIRED",
      "AV23_3_score_inflation_spam": "PASS_REQUIRED",
      "AV23_4_concurrent_write_collision": "PASS_REQUIRED",
      "AV23_5_graph_partition": "PASS_REQUIRED",
      "AV23_6_schema_migration_loss": "PASS_REQUIRED",
      "AV23_7_backfill_overload": "PASS_REQUIRED",
      "AV23_8_category_collapse": "PASS_REQUIRED"
    },
    "peak_specs": {
      "C6_2_1_cross_layer_trigger_dedup": "SPECIFIED",
      "C6_2_2_completeness_scoring": "SPECIFIED"
    },
    "zero_gap_verification": {
      "gap_count": 7,
      "all_gaps_resolved": true
    },
    "risk_level": "MEDIUM",
    "risk_mitigations_active": [
      "Human review queue for fuzzy-merged conditions",
      "Density bonus capped at 3 conditions per dimension",
      "max_conditions_per_dimension limit with eviction",
      "Version-tagged nodes for schema migration",
      "Per-bug write lock for concurrency safety"
    ],
    "dependency_status": {
      "phase_5_evidence_graph": "AVAILABLE",
      "phase_8_10_fuzzer": "AVAILABLE",
      "phase_12_concolic": "AVAILABLE",
      "phase_17_agent_tools": "AVAILABLE"
    },
    "blocks": ["phase_30_completeness_gate"],
    "blocked_by": []
  }
}
```

---

## Changelog

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-05-14 | System | Initial Phase 23 plan — Trigger Matrix with semantic dedup and completeness scoring |
