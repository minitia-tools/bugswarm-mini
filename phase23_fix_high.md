# Phase 23 Trigger Matrix — HIGH Finding Remediation Plan

**Status**: DRAFT — not yet implemented  
**Source**: Phase 23 Trigger Matrix Audit (12 HIGH findings, H1–H12)  
**Target codebase**: `bugswarm-evidence/src/` (Rust crate), `bugswarm-evidence/tests/`  
**Total estimated effort**: ~26 hrs  
**Implementation order**: H2 → H3 → H11 → H1 → H4 → H12 → H7 → H8 → H5 → H6 → H9 → H10

---

## Table of Contents

1. [Category A: Algorithm Completeness](#category-a-algorithm-completeness)
   - H1: Completeness Scoring Missing Bonuses
   - H4: No `investigation_priority()` Function
   - H7: No Stale-Condition Eviction
   - H8: No Staleness Metric
   - H12: No O(1) Exact-Match Hash Fast Path
2. [Category B: Data Completeness](#category-b-data-completeness)
   - H2: Missing Plan-Required Fields on `TriggerCondition`
   - H3: Missing Plan-Required Structs
   - H11: Missing `schema_version` for Forward Compatibility
3. [Category C: Operations](#category-c-operations)
   - H5: No Serialization / Persistence
   - H6: Zero Observability
   - H9: No Human Review Queue for Fuzzy Merges
   - H10: No Backfill Migration from Existing Bug Nodes
4. [Summary Table](#summary-table)
5. [Appendix: Immunity Mechanism Reference](#appendix-immunity-mechanism-reference)
6. [Appendix: Acceptance Test Suite Outline](#appendix-acceptance-test-suite-outline)

---

## Category A: Algorithm Completeness

### H1 — Completeness Scoring Missing Density Bonus + Layer Diversity Bonus

#### Root Cause Analysis

The current `recompute_completeness()` implementation at `trigger.rs:166-187` computes a flat weighted-coverage ratio:

```
score = covered_weight / total_weight
```

This was an MVP correctness baseline. The Phase 23 plan (section C6.2 completeness criteria) explicitly requires multiplicative bonuses to reward *richness* of trigger documentation, not just breadth of dimension coverage. Without these bonuses, a matrix with 8 conditions (one per dimension, each from a single layer with a single condition) scores identically to a matrix with 24 conditions (3 conditions per dimension across 3 layers). The bonuses differentiate "surface-level" from "deep" coverage.

Three factors were missed:
1. **Density bonus** — rewards having multiple distinct conditions within a single dimension. A single "Input: null pointer" tells much less than three distinct Input conditions that triangulate the trigger.
2. **Layer diversity bonus** — rewards multiple independent layers corroborating the same trigger surface, reducing single-layer hallucination risk.
3. **Min-floor penalty**: The plan specifies a minimum floor so partially-covered matrices are never zero'd out by low bonus scores. The current code has no such floor.

#### Permanent Fix

New constants in `spec.rs`:

```rust
/// Minimum number of conditions to reach full density bonus.
pub const DENSITY_BONUS_SATURATION: usize = 3;

/// Minimum number of layers to reach full layer diversity bonus.
pub const LAYER_BONUS_SATURATION: usize = 3;

/// Minimum floor for completeness score to prevent zeroing out partial coverage.
pub const COMPLETENESS_MIN_FLOOR: f32 = 0.1;
```

Replace `recompute_completeness()` in `trigger.rs`:

```rust
pub fn recompute_completeness(&mut self) {
    // Phase 1: base weighted coverage (preserved from original)
    let mut covered: HashSet<TriggerDimension> = HashSet::new();
    for c in &self.conditions {
        covered.insert(c.dimension);
    }

    let total_weight: f32 = TriggerDimension::all().iter()
        .filter(|d| **d != TriggerDimension::OsArch)
        .map(|d| d.weight())
        .sum();

    let covered_weight: f32 = covered.iter()
        .filter(|d| **d != TriggerDimension::OsArch)
        .map(|d| d.weight())
        .sum();

    let base_score = if total_weight > 0.0 {
        covered_weight / total_weight
    } else {
        0.0
    };

    // Phase 2: density bonus (conditions per dimension saturation)
    let density_bonus = {
        let cnt = self.conditions.len() as f32;
        (cnt / crate::spec::DENSITY_BONUS_SATURATION as f32).min(1.0)
    };

    // Phase 3: layer diversity bonus
    let layer_bonus = {
        let cnt = self.contributing_layers.len() as f32;
        (cnt / crate::spec::LAYER_BONUS_SATURATION as f32).min(1.0)
    };

    // Phase 4: multiplicative composition with min-floor clamp
    let effective_score = base_score * density_bonus * layer_bonus;
    let min_floor = (crate::spec::COMPLETENESS_MIN_FLOOR)
        .max(density_bonus * layer_bonus);

    self.completeness_score = effective_score.max(min_floor).min(1.0);
}
```

#### Immunity Mechanism

**Layer 1 — Property-based tests**: Generate random matrices and verify:
1. `completeness_score in [0.0, 1.0]` (always clamped)
2. Monotonicity: adding a condition never decreases score when introducing new dimension or layer

**Layer 2 — Spec constants enforcement**: CI grep check that `trigger.rs` never contains bare float literals used in score computation. All threshold values (`3.0`, `0.1`) must come from `spec.rs`.

**Layer 3 — Regression freeze**: Known configurations with frozen expected scores:
- 1 condition, 1 layer, 1 dimension (Input) → score ≈ 0.111 (base=0.3158 × low bonuses, clamped by min_floor)
- 7 dims × 3 conditions each × 3 layers → score = 1.0

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/spec.rs:23`: Add `DENSITY_BONUS_SATURATION`, `LAYER_BONUS_SATURATION`, `COMPLETENESS_MIN_FLOOR`
2. [ ] `bugswarm-evidence/src/trigger.rs:166-187`: Replace `recompute_completeness()` with bonus-aware version
3. [ ] `bugswarm-evidence/src/trigger.rs:166`: Remove standalone `min(1.0)` clamp on base_score (bonus clamping handles this)
4. [ ] `bugswarm-evidence/tests/trigger_audit.rs:after L252`: Add density bonus basic test
5. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `completeness_known_configurations_regression` test
6. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add property-based tests (proptest dependency)
7. [ ] `Makefile` or CI: Add `audit-magic-numbers` enforcement target
8. [ ] Run `cargo test -p bugswarm-evidence` — update expected values in existing completeness tests

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Empty matrix (0 conditions) | completeness_score == 0.0 | PASS / FAIL |
| 2 | 1 condition, 1 layer, 1 dimension | completeness_score >= 0.1 (min-floor) | PASS / FAIL |
| 3 | 7 dims covered, 3+ layers, 21+ conditions | completeness_score == 1.0 | PASS / FAIL |
| 4 | 2 layers vs 1 layer (same dims/conds) | Score with 2 layers > Score with 1 layer | PASS / FAIL |
| 5 | Property: score always in [0.0, 1.0] | No test failure across 10K random matrices | PASS / FAIL |
| 6 | Property: monotonic on adding novel dim/layer conds | No regression | PASS / FAIL |

---

### H4 — No `investigation_priority()` Function

#### Root Cause Analysis

`TriggerManager::get_incomplete_bugs()` at `trigger.rs:300-306` sorts purely by completeness score ascending. This ignores:
1. **Severity** — a severity=10 RCE with low completeness should outrank a severity=1 cosmetic issue
2. **Staleness** — conditions untouched for 90 days may need re-investigation
3. **Composite priority** — the plan specifies `0.7 * severity_weight + 0.3 * incompleteness`

The function was never written. The plan formula was documented but not assigned.

#### Permanent Fix

New constants in `spec.rs`:

```rust
/// Weight of severity in investigation priority calculation.
pub const PRIORITY_SEVERITY_WEIGHT: f32 = 0.7;
/// Weight of incompleteness in investigation priority calculation.
pub const PRIORITY_INCOMPLETENESS_WEIGHT: f32 = 0.3;
```

New method on `TriggerMatrix`:

```rust
/// Compute investigation priority score (0.0-1.0, higher = more urgent).
pub fn investigation_priority(&self, severity: Option<u8>) -> f32 {
    let severity_weight = match severity {
        Some(s) if s > 0 => (s as f32 / 10.0).clamp(0.1, 1.0),
        _ => 0.5, // default: unknown severity = medium urgency
    };
    let incompleteness = 1.0 - self.completeness_score;
    let priority = crate::spec::PRIORITY_SEVERITY_WEIGHT * severity_weight
                 + crate::spec::PRIORITY_INCOMPLETENESS_WEIGHT * incompleteness;
    priority.clamp(0.0, 1.0)
}
```

New prioritized sort method on `TriggerManager`:

```rust
pub fn get_incomplete_bugs_prioritized(
    &self,
    bug_severities: &HashMap<String, u8>,
) -> Vec<(&TriggerMatrix, f32)> {
    let mut incomplete: Vec<(&TriggerMatrix, f32)> = self.matrices.values()
        .filter(|m| !m.meets_phase30_gate())
        .map(|m| {
            let sev = bug_severities.get(&m.bug_id).copied();
            let priority = m.investigation_priority(sev);
            (m, priority)
        })
        .collect();
    incomplete.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
    });
    incomplete
}
```

#### Immunity Mechanism

**Compile-time trait**: Define `Prioritizable` trait that any struct requiring priority ordering must implement. CI enforces that no code calls `.sort()` on trigger collections without going through this trait.

**Known-answer test**: severity=10 + completeness=0.0 → priority=1.0; severity=1 + completeness=1.0 → priority=0.07.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/spec.rs:26`: Add `PRIORITY_SEVERITY_WEIGHT = 0.7`, `PRIORITY_INCOMPLETENESS_WEIGHT = 0.3`
2. [ ] `bugswarm-evidence/src/trigger.rs:193`: Add `investigation_priority()` to `impl TriggerMatrix`
3. [ ] `bugswarm-evidence/src/trigger.rs:300`: Add `get_incomplete_bugs_prioritized()` to `impl TriggerManager`
4. [ ] `bugswarm-evidence/src/trigger.rs:314`: Add `Prioritizable` trait
5. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `investigation_priority_respects_severity` test
6. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `investigation_priority_full_completeness` test
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `investigation_priority_default_severity` test

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | severity=10, completeness=0.0 | priority = 0.7×1.0 + 0.3×1.0 = 1.0 | PASS / FAIL |
| 2 | severity=1, completeness=1.0 | priority = 0.7×0.1 + 0.3×0.0 = 0.07 | PASS / FAIL |
| 3 | severity=None, completeness=0.5 | priority = 0.7×0.5 + 0.3×0.5 = 0.50 | PASS / FAIL |
| 4 | severity=10 bug > severity=1 bug (same completeness) | BUG-CRIT ranks first | PASS / FAIL |
| 5 | Priority always in [0.0, 1.0] | No out-of-range values | PASS / FAIL |

---

### H7 — No Stale-Condition Eviction

#### Root Cause Analysis

`TriggerMatrix::add_condition()` at `trigger.rs:140-163` blindly appends conditions without any cap. Over time, a single dimension could accumulate hundreds of unverified conditions, consuming unbounded memory and making the O(n) dedup scan increasingly expensive. The plan defines `max_conditions_per_dimension = 100` with an eviction policy: evict oldest-unverified, never evict verified conditions. This was simply never implemented.

#### Permanent Fix

New constant in `spec.rs`:

```rust
/// Maximum number of conditions allowed per dimension before eviction.
pub const MAX_CONDITIONS_PER_DIMENSION: usize = 100;
```

Replace `add_condition()` with eviction-aware version:

```rust
pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
    // [existing dedup logic preserved]

    // After inserting new (non-dedup) condition:
    self.conditions.push(condition);
    self.recompute_completeness();
    self.last_updated = chrono::Utc::now().to_rfc3339();

    // Eviction check: count conditions in this dimension
    let dim = condition.dimension;
    let conds_in_dim = self.conditions.iter()
        .filter(|c| c.dimension == dim)
        .count();

    if conds_in_dim > crate::spec::MAX_CONDITIONS_PER_DIMENSION {
        // Find oldest-unverified condition in this dimension
        if let Some(evict_idx) = self.conditions.iter()
            .enumerate()
            .filter(|(_, c)| c.dimension == dim && !c.verified)
            .min_by_key(|(_, c)| c.contributed_at.clone())
            .map(|(i, _)| i)
        {
            self.conditions.remove(evict_idx);

            // Clean up contributing_layers if evicted condition was sole
            // representative of a layer
            let remaining_layers: HashSet<ContributionLayer> = self.conditions
                .iter()
                .flat_map(|c| c.contributed_by.iter().copied())
                .collect();
            self.contributing_layers.retain(|l| remaining_layers.contains(l));
            self.recompute_completeness();
        } else {
            // All verified: log warning, no eviction
            warn!(
                bug_id = %self.bug_id,
                dimension = ?dim,
                count = conds_in_dim,
                "dimension exceeded MAX but all conditions verified"
            );
        }
    }

    // Debug-only invariant check
    #[cfg(debug_assertions)]
    {
        for dim in TriggerDimension::all() {
            let count = self.conditions.iter().filter(|c| c.dimension == dim).count();
            debug_assert!(
                count <= crate::spec::MAX_CONDITIONS_PER_DIMENSION + 1,
                "dimension {:?} exceeded cap", dim
            );
        }
    }

    true
}
```

#### Immunity Mechanism

**Hard cap via debug_assert**: Every `add_condition` call ends with a debug assertion that no dimension exceeds `MAX_CONDITIONS_PER_DIMENSION + 1`. This fires in CI but is stripped in release.

**Stress test**: Add 150 conditions to one dimension → assert exactly 100 remain, oldest 50 unverified evicted.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/spec.rs:29`: Add `MAX_CONDITIONS_PER_DIMENSION = 100`
2. [ ] `bugswarm-evidence/src/trigger.rs:140-163`: Replace `add_condition()` with eviction-aware version
3. [ ] `bugswarm-evidence/src/trigger.rs`: Add post-eviction `contributing_layers` cleanup logic
4. [ ] `bugswarm-evidence/src/trigger.rs`: Add `debug_assert!` dimension cap invariant
5. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `stale_eviction_dimension_cap` test
6. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `stale_eviction_verified_protection` test
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `stale_eviction_interleaved_dimensions` test

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Add 150 unverified conditions to Input dim | Exactly 100 remain, 50 oldest evicted | PASS / FAIL |
| 2 | Add 50 verified + 80 unverified (130 total) | All 50 verified + 50 newest unverified remain | PASS / FAIL |
| 3 | Add 150 verified conditions | No eviction, warning logged | PASS / FAIL |
| 4 | Add 100 to Input, 100 to Env, 100 to Timing | Each dim has 100, no cross-dim eviction | PASS / FAIL |
| 5 | debug_assert fires if bug bypasses cap | Test panics in debug mode | PASS / FAIL |

---

### H8 — No Staleness Metric

#### Root Cause Analysis

The plan defines staleness as `(now - last_updated) / 30_days` clamped to [0.0, 1.0], integrated into investigation priority. Without this, a bug last updated 6 months ago gets the same priority as one updated 5 minutes ago. The `TriggerMatrix` has `last_updated: String` but no method to compute staleness from it.

#### Permanent Fix

New constants in `spec.rs`:

```rust
/// Staleness window in days. Conditions older than this reach max staleness (1.0).
pub const STALENESS_WINDOW_DAYS: f32 = 30.0;
/// Weight of staleness in investigation priority (urgency decay factor).
pub const PRIORITY_STALENESS_WEIGHT: f32 = 0.1;
```

Adjust severity weight to rebalance: `PRIORITY_SEVERITY_WEIGHT` changes from 0.7 to 0.6.

New `staleness()` method:

```rust
pub fn staleness(&self) -> f32 {
    let now = chrono::Utc::now();
    let last = chrono::DateTime::parse_from_rfc3339(&self.last_updated)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or(now);
    let age_days = (now - last).num_days() as f32;
    let raw = age_days / crate::spec::STALENESS_WINDOW_DAYS;
    raw.clamp(0.0, 1.0)
}
```

Updated `investigation_priority()`:

```rust
pub fn investigation_priority(&self, severity: Option<u8>) -> f32 {
    let severity_weight = match severity {
        Some(s) if s > 0 => (s as f32 / 10.0).clamp(0.1, 1.0),
        _ => 0.5,
    };
    let incompleteness = 1.0 - self.completeness_score;
    let staleness_urgency = 1.0 - self.staleness(); // invert: 0.0 stale → 1.0 urgent

    let priority = 0.6 * severity_weight     // from spec
                 + 0.3 * incompleteness      // from spec
                 + 0.1 * staleness_urgency;  // from spec
    priority.clamp(0.0, 1.0)
}
```

#### Immunity Mechanism

**Static time injection**: CI check greps for `Utc::now()` in test files and fails. All timestamps in tests must come from explicit parameters.

**Fuzzed robustness test**: Feed random RFC 3339 strings (including garbage, empty, future) to `staleness()` and assert output always in [0.0, 1.0] with no panic.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/spec.rs:31`: Add `STALENESS_WINDOW_DAYS = 30.0`, `PRIORITY_STALENESS_WEIGHT = 0.1`
2. [ ] `bugswarm-evidence/src/spec.rs:27`: Adjust `PRIORITY_SEVERITY_WEIGHT` from 0.7 to 0.6
3. [ ] `bugswarm-evidence/src/trigger.rs:196`: Add `staleness()` method
4. [ ] `bugswarm-evidence/src/trigger.rs:203`: Update `investigation_priority()` with staleness
5. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `staleness_fresh` (0 days)
6. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `staleness_30_days` (= 1.0)
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `staleness_15_days` (= 0.5)
8. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `staleness_never_panics_on_bad_timestamps`

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | last_updated = right now | staleness = 0.0 | PASS / FAIL |
| 2 | last_updated = 30 days ago | staleness = 1.0 | PASS / FAIL |
| 3 | last_updated = 15 days ago | staleness ~0.5 | PASS / FAIL |
| 4 | 60 days ago | staleness = 1.0 (clamped) | PASS / FAIL |
| 5 | Future timestamp (2099) | staleness = 0.0 (clamped) | PASS / FAIL |
| 6 | Garbage timestamp | staleness = 0.0 (fallback), no panic | PASS / FAIL |
| 7 | H4 priority tests still pass with rebalanced weights | No regression | PASS / FAIL |

---

### H12 — No O(1) Exact-Match Hash Fast Path

#### Root Cause Analysis

Every `add_condition()` runs O(n) linear scan over all conditions with expensive Jaro-Winkler per element. For 700 conditions (7 dims x 100), every insert costs up to 700 Jaro-Winkler calls. The plan requires a `HashMap<String, Vec<usize>>` mapping `normalized_hash -> condition indices` for O(1) exact-match lookup, then fall back to per-dimension scan for fuzzy.

This optimization was deferred from MVP.

#### Permanent Fix

Add two internal indices to `TriggerMatrix`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerMatrix {
    // ... existing public fields ...

    /// O(1) exact-match lookup: key = "{dimension:?}:{hash_prefix_16}"
    #[serde(skip)]
    normalized_idx: HashMap<String, Vec<usize>>,

    /// O(k) per-dimension scan: dimension -> condition indices
    #[serde(skip)]
    by_dimension: HashMap<TriggerDimension, Vec<usize>>,
}
```

Three-phase dedup in `add_condition()`:

```rust
pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
    let hash_key = format!("{:?}:{}", condition.dimension,
        normalize_hash(&condition.normalized));

    // Phase 1: O(1) exact-match via normalized_idx
    if let Some(candidates) = self.normalized_idx.get(&hash_key) {
        for &idx in candidates {
            let existing = &self.conditions[idx];
            if existing.dimension == condition.dimension
                && existing.normalized == condition.normalized
            {
                return self.merge_condition(idx, &condition);
            }
        }
    }

    // Phase 2: O(k) fuzzy scan over same-dimension bucket
    if let Some(dim_bucket) = self.by_dimension.get(&condition.dimension) {
        if let Some(target_idx) = dim_bucket.iter()
            .find(|&&idx| {
                is_semantically_equivalent(
                    &self.conditions[idx].normalized,
                    &condition.normalized,
                )
            })
            .copied()
        {
            return self.merge_condition(target_idx, &condition);
        }
    }

    // Phase 3: new condition — insert and update indices
    let idx = self.conditions.len();
    self.conditions.push(condition);
    self.normalized_idx.entry(hash_key).or_default().push(idx);
    self.by_dimension
        .entry(self.conditions.last().unwrap().dimension)
        .or_default()
        .push(idx);

    self.contributing_layers.extend(
        self.conditions.last().unwrap().contributed_by.iter().copied()
    );
    self.recompute_completeness();
    self.last_updated = chrono::Utc::now().to_rfc3339();

    // Eviction check (H7) with index cleanup
    self.check_eviction(self.conditions.last().unwrap().dimension);

    true
}
```

`rebuild_indices()` for deserialization:

```rust
pub fn rebuild_indices(&mut self) {
    self.normalized_idx.clear();
    self.by_dimension.clear();
    for (i, tc) in self.conditions.iter().enumerate() {
        let key = format!("{:?}:{}", tc.dimension,
            normalize_hash(&tc.normalized));
        self.normalized_idx.entry(key).or_default().push(i);
        self.by_dimension.entry(tc.dimension).or_default().push(i);
    }
}
```

#### Immunity Mechanism

**Index consistency debug assertion**: After any mutation, verify all indices refer to valid conditions and every condition has entries in both indices.

**Performance regression test**: Insert 4000 conditions (500 per dim x 8) and assert avg insert time < 50us.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs:110-120`: Add `normalized_idx` and `by_dimension` fields
2. [ ] `bugswarm-evidence/src/trigger.rs:126-136`: Update `TriggerMatrix::new()` to init new fields
3. [ ] `bugswarm-evidence/src/trigger.rs`: Add `hash_key_for()` helper
4. [ ] `bugswarm-evidence/src/trigger.rs:140-163`: Replace `add_condition()` with 3-phase lookup
5. [ ] `bugswarm-evidence/src/trigger.rs`: Add `merge_condition()` helper
6. [ ] `bugswarm-evidence/src/trigger.rs`: Add `rebuild_indices()` method
7. [ ] `bugswarm-evidence/src/trigger.rs`: Add `assert_indices_consistent()` debug-only invariant
8. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `hash_fast_path_performance` test
9. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `hash_fast_path_correctness` test
10. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `hash_index_rebuild_roundtrip` test

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | 1000 exact-duplicate inserts | Each O(1), all return false (merged) | PASS / FAIL |
| 2 | 4000 unique conditions | avg insert < 50us | PASS / FAIL |
| 3 | Serialize -> deserialize -> rebuild_indices | Indices consistent, correctness preserved | PASS / FAIL |
| 4 | Exact duplicate: 2 conditions with identical normalized | Hash lookup finds match, Jaro-Winkler never called | PASS / FAIL |
| 5 | Fuzzy detection: 2 near-identical | Falls back to per-dim scan, finds match | PASS / FAIL |

---

## Category B: Data Completeness

### H2 — Missing Plan-Required Fields on `TriggerCondition`

#### Root Cause Analysis

The current `TriggerCondition` struct (`trigger.rs:63-86`) was implemented from an early draft. Five fields from the final plan were omitted:

1. **`canonical_value: String`** — best normalized form across merged layers (e.g., "null", "None", '''' all canonicalize to "empty")
2. **`semantic_hash: String`** — full SHA-256 hex (64 chars), not truncated 16. Used for cross-matrix similarity search
3. **`tags: Vec<String>`** — categorization tags like `["sql-injection", "cwe-89"]`
4. **`raw_contributions: Vec<RawTriggerCondition>`** — auditable record of every raw contribution before normalization
5. **`severity_specific`** — currently `Option<u8>` but plan requires `Option<SeveritySpecific>` struct with `min_severity: u8, max_severity: u8`

#### Permanent Fix

New `SeveritySpecific` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SeveritySpecific {
    pub min_severity: u8,
    pub max_severity: u8,
}

impl SeveritySpecific {
    pub fn new(min: u8, max: u8) -> Self {
        Self {
            min_severity: min.clamp(1, 10),
            max_severity: max.clamp(min, 10),
        }
    }
}
```

Complete revised `TriggerCondition`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerCondition {
    pub id: String,
    pub bug_id: String,
    pub dimension: TriggerDimension,
    pub description: String,
    pub normalized: String,

    /// Best representative normalized form across merged layers.
    pub canonical_value: String,

    /// Full SHA-256 hex digest of the semantic fingerprint (64 hex chars).
    pub semantic_hash: String,

    /// Arbitrary categorization tags.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Raw un-normalized contributions that were merged into this condition.
    #[serde(default)]
    pub raw_contributions: Vec<RawTriggerCondition>,

    pub layer: ContributionLayer,
    #[serde(default)]
    pub contributed_by: Vec<ContributionLayer>,

    /// Severity range for which this condition applies.
    pub severity_specific: Option<SeveritySpecific>,

    pub verified: bool,
    pub contributed_at: String,

    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

fn default_schema_version() -> u32 { 1 }
```

Updated `TriggerCondition::new()`:

```rust
pub fn new(bug_id: &str, dim: TriggerDimension, desc: &str, layer: ContributionLayer) -> Self {
    let normalized = normalize_description(desc);
    let sem_hash = full_semantic_hash(&normalized);
    Self {
        id: format!("tc-{}-{:?}-{}", bug_id, dim, normalize_hash(&normalized)),
        bug_id: bug_id.to_string(),
        dimension: dim,
        description: desc.to_string(),
        normalized: normalized.clone(),
        canonical_value: normalized.clone(),
        semantic_hash: sem_hash,
        tags: vec![],
        raw_contributions: vec![RawTriggerCondition::from_layer(desc, layer, dim, bug_id)],
        layer,
        contributed_by: vec![layer],
        severity_specific: None,
        verified: false,
        contributed_at: chrono::Utc::now().to_rfc3339(),
        schema_version: 1,
    }
}
```

#### Immunity Mechanism

**Serde backward-compatibility test**: Deserialize JSON from old format (missing all 5 new fields). Assert all defaults applied correctly and no deserialization error.

**Full-field roundtrip test**: Serialize with all 14 fields populated, deserialize, assert bit-exact equality.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs:62`: Add `SeveritySpecific` struct before `TriggerCondition`
2. [ ] `bugswarm-evidence/src/trigger.rs:64-87`: Replace `TriggerCondition` with full-field version
3. [ ] `bugswarm-evidence/src/trigger.rs:88-110`: Update `TriggerCondition::new()` to populate new fields
4. [ ] `bugswarm-evidence/src/trigger.rs`: Add `full_semantic_hash()` function (full 64-char SHA-256)
5. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Update `serialization_trigger_condition_roundtrip` for new fields
6. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `trigger_condition_forward_compatible_deserialization` test
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `trigger_condition_all_fields_present` test
8. [ ] `bugswarm-evidence/src/trigger.rs:140+`: Update merge logic to update `canonical_value` and push `raw_contributions`

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Serialize with all 14 fields -> deserialize | All fields roundtrip correctly | PASS / FAIL |
| 2 | Old-format JSON (missing 5 fields) -> deserialize | Defaults applied, no error | PASS / FAIL |
| 3 | TriggerCondition::new() creates valid raw_contributions[0] | 1 entry, layer preserved | PASS / FAIL |
| 4 | Two conditions merged -> raw_contributions grows | Merged condition has both layers' contributions | PASS / FAIL |
| 5 | semantic_hash is 64 chars (not 16) | len == 64 | PASS / FAIL |
| 6 | canonical_value updates on merge | Canonical = most descriptive normalized across layers | PASS / FAIL |

---

### H3 — Missing Plan-Required Structs

#### Root Cause Analysis

Six plan-required structs are completely missing:

1. **`RawTriggerCondition`** — snapshot of raw un-normalized layer output (audit trail)
2. **`TriggerConfig`** — runtime configuration with overridable thresholds
3. **`DimensionScore`** — per-dimension score breakdown (dashboard UI requirement)
4. **`TriggerMatrixResponse`** — standardized API response wrapper
5. **`DescribeTriggerRequest`** — typed request for the describe_trigger RPC
6. **`DescribeTriggerResponse`** — typed response for the describe_trigger RPC

These were deferred because the daemon uses a generic JSON-RPC dispatch with a flat `DaemonRequest`. The plan requires typed request/response structs.

#### Permanent Fix

All new structs in `trigger.rs`, after `ContributionLayer`:

**`RawTriggerCondition`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawTriggerCondition {
    pub raw_description: String,
    pub layer: ContributionLayer,
    pub dimension: TriggerDimension,
    pub bug_id: String,
    pub contributed_at: String,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

impl RawTriggerCondition {
    pub fn from_layer(raw_desc: &str, layer: ContributionLayer, dim: TriggerDimension, bug_id: &str) -> Self {
        Self {
            raw_description: raw_desc.to_string(),
            layer,
            dimension: dim,
            bug_id: bug_id.to_string(),
            contributed_at: chrono::Utc::now().to_rfc3339(),
            schema_version: 1,
        }
    }
}
```

**`TriggerConfig`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    #[serde(default = "default_dedup_threshold")]
    pub dedup_jaro_winkler_threshold: f64,
    #[serde(default = "default_dedup_min_length")]
    pub dedup_min_fuzzy_length: usize,
    #[serde(default = "default_dedup_max_chars")]
    pub dedup_max_description_chars: usize,
    #[serde(default = "default_max_conditions_per_dimension")]
    pub max_conditions_per_dimension: usize,
    #[serde(default = "default_phase30_min_rows")]
    pub phase30_min_rows: usize,
    #[serde(default = "default_phase30_min_layers")]
    pub phase30_min_layers: usize,
    #[serde(default = "default_true")]
    pub auto_merge_fuzzy: bool,
    #[serde(default = "default_true")]
    pub observability_enabled: bool,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            dedup_jaro_winkler_threshold: default_dedup_threshold(),
            dedup_min_fuzzy_length: default_dedup_min_length(),
            dedup_max_description_chars: default_dedup_max_chars(),
            max_conditions_per_dimension: default_max_conditions_per_dimension(),
            phase30_min_rows: default_phase30_min_rows(),
            phase30_min_layers: default_phase30_min_layers(),
            auto_merge_fuzzy: true,
            observability_enabled: true,
            schema_version: 1,
        }
    }
}

fn default_dedup_threshold() -> f64 { crate::spec::DEDUP_JARO_WINKLER_THRESHOLD }
fn default_dedup_min_length() -> usize { crate::spec::DEDUP_MIN_FUZZY_LENGTH }
fn default_dedup_max_chars() -> usize { crate::spec::DEDUP_MAX_DESCRIPTION_CHARS }
fn default_max_conditions_per_dimension() -> usize { crate::spec::MAX_CONDITIONS_PER_DIMENSION }
fn default_phase30_min_rows() -> usize { crate::spec::PHASE30_MIN_ROWS }
fn default_phase30_min_layers() -> usize { crate::spec::PHASE30_MIN_LAYERS }
fn default_true() -> bool { true }
```

**`DimensionScore`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionScore {
    pub dimension: TriggerDimension,
    pub condition_count: usize,
    pub weight: f32,
    pub covered: bool,
    pub density: f32,       // conditions / 3, capped at 1.0
    pub verified_ratio: f32, // verified_conds / condition_count
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}
```

**`TriggerMatrixResponse`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerMatrixResponse {
    pub bug_id: String,
    pub conditions: Vec<TriggerCondition>,
    pub contributing_layers: Vec<String>,
    pub dedup_count: usize,
    pub completeness_score: f32,
    pub last_updated: String,
    pub meets_phase30_gate: bool,
    pub dimension_scores: Vec<DimensionScore>,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

impl TriggerMatrixResponse {
    pub fn from_matrix(matrix: &TriggerMatrix) -> Self {
        let dim_scores: Vec<DimensionScore> = TriggerDimension::all()
            .iter()
            .map(|dim| {
                let conds: Vec<&TriggerCondition> = matrix.conditions
                    .iter().filter(|c| c.dimension == *dim).collect();
                let cnt = conds.len();
                let verified_cnt = conds.iter().filter(|c| c.verified).count();
                DimensionScore {
                    dimension: *dim,
                    condition_count: cnt,
                    weight: dim.weight(),
                    covered: cnt > 0,
                    density: (cnt as f32 / 3.0).min(1.0),
                    verified_ratio: if cnt > 0 { verified_cnt as f32 / cnt as f32 } else { 0.0 },
                    schema_version: 1,
                }
            })
            .collect();
        TriggerMatrixResponse {
            bug_id: matrix.bug_id.clone(),
            conditions: matrix.conditions.clone(),
            contributing_layers: matrix.contributing_layers
                .iter().map(|l| l.layer_name().to_string()).collect(),
            dedup_count: matrix.dedup_count,
            completeness_score: matrix.completeness_score,
            last_updated: matrix.last_updated.clone(),
            meets_phase30_gate: matrix.meets_phase30_gate(),
            dimension_scores: dim_scores,
            schema_version: 1,
        }
    }
}
```

**`DescribeTriggerRequest`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribeTriggerRequest {
    pub bug_id: String,
    #[serde(default)]
    pub dimension: Option<String>,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub verified_only: bool,
    #[serde(default)]
    pub include_raw: bool,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}
```

**`DescribeTriggerResponse`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribeTriggerResponse {
    pub bug_id: String,
    pub conditions: Vec<TriggerCondition>,
    pub total_count: usize,
    pub filtered_count: usize,
    pub completeness_score: f32,
    pub investigation_priority: f32,
    pub dimension_scores: Vec<DimensionScore>,
    pub summary: String,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

impl DescribeTriggerResponse {
    pub fn from_matrix(matrix: &TriggerMatrix, severity: Option<u8>) -> Self {
        let covered_dims = TriggerDimension::all().iter()
            .filter(|d| matrix.conditions.iter().any(|c| c.dimension == **d))
            .count();
        let summary = format!(
            "Bug {} has {} trigger conditions across {} dimensions ({} layers). \
             Completeness: {:.0}%. {} conditions verified.",
            matrix.bug_id, matrix.conditions.len(), covered_dims,
            matrix.contributing_layers.len(),
            matrix.completeness_score * 100.0,
            matrix.conditions.iter().filter(|c| c.verified).count(),
        );
        DescribeTriggerResponse {
            bug_id: matrix.bug_id.clone(),
            conditions: matrix.conditions.clone(),
            total_count: matrix.conditions.len(),
            filtered_count: matrix.conditions.len(),
            completeness_score: matrix.completeness_score,
            investigation_priority: matrix.investigation_priority(severity),
            dimension_scores: TriggerMatrixResponse::from_matrix(matrix).dimension_scores,
            summary,
            schema_version: 1,
        }
    }
}
```

#### Immunity Mechanism

**All-structs serialization test**: A single test serializes all 6 structs and asserts each produces a JSON object with no null fields. CI catches any struct that can't serialize or has uninitialized fields.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs:60`: Add `RawTriggerCondition` struct + impl
2. [ ] `bugswarm-evidence/src/trigger.rs:85`: Add `TriggerConfig` struct + impl + Default
3. [ ] `bugswarm-evidence/src/trigger.rs:130`: Add `DimensionScore` struct
4. [ ] `bugswarm-evidence/src/trigger.rs:150`: Add `TriggerMatrixResponse` + `from_matrix()`
5. [ ] `bugswarm-evidence/src/trigger.rs:190`: Add `DescribeTriggerRequest`
6. [ ] `bugswarm-evidence/src/trigger.rs:210`: Add `DescribeTriggerResponse` + `from_matrix()`
7. [ ] `bugswarm-evidence/src/trigger.rs`: Add module-level `default_schema_version()` fn
8. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `all_plan_structs_serialize_completely`
9. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add serde roundtrip per struct
10. [ ] `bugswarm-evidence/src/daemon.rs:225`: Add `describe_trigger` RPC handler

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | All 6 structs compile | cargo build passes | PASS / FAIL |
| 2 | TriggerConfig::default() matches spec.rs constants | Defaults = spec values | PASS / FAIL |
| 3 | TriggerMatrixResponse::from_matrix() computes correct dimension_scores | Per-dim counts match matrix state | PASS / FAIL |
| 4 | DescribeTriggerResponse.summary is non-empty | Contains bug_id and stats | PASS / FAIL |
| 5 | All structs have schema_version: u32 (H11) | grep confirms | PASS / FAIL |

---

### H11 — Missing `schema_version` for Forward Compatibility

#### Root Cause Analysis

`TriggerMatrix` and `TriggerCondition` both derive `Serialize/Deserialize` but neither includes `schema_version: u32`. Without it:
1. Future schema changes cannot be migrated — old data deserializes silently with default values
2. No version-branching logic possible in deserializer
3. Plan section C6.1 requires: "All serializable nodes MUST include schema_version: u32, defaulting to 1"

This field was simply omitted during initial development.

#### Permanent Fix

Add `schema_version` to `TriggerMatrix` (already added to `TriggerCondition` in H2):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerMatrix {
    pub bug_id: String,
    pub conditions: Vec<TriggerCondition>,
    pub contributing_layers: HashSet<ContributionLayer>,
    pub dedup_count: usize,
    pub completeness_score: f32,
    pub last_updated: String,

    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    #[serde(skip)]
    normalized_idx: HashMap<String, Vec<usize>>,
    #[serde(skip)]
    by_dimension: HashMap<TriggerDimension, Vec<usize>>,
}
```

Update `TriggerMatrix::new()` to set `schema_version: 1`.

#### Immunity Mechanism

**CI enforcement**: A script greps all struct definitions in `trigger.rs` + `types.rs` that derive `Serialize`. For each, verifies it contains a `schema_version` field.

**Backward-compat test**: Deserialize JSON without `schema_version` field, assert deserialized struct has `schema_version == 1`.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs:111`: Add `schema_version: u32` to `TriggerMatrix`
2. [ ] `bugswarm-evidence/src/trigger.rs:126`: Update `TriggerMatrix::new()` to set `schema_version = 1`
3. [ ] `bugswarm-evidence/src/trigger.rs:64`: Add to `TriggerCondition` (covered by H2)
4. [ ] `bugswarm-evidence/src/trigger.rs`: Add module-level `fn default_schema_version() -> u32` (shared)
5. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `schema_version_defaults_on_old_json`
6. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `schema_version_survives_roundtrip`
7. [ ] `Makefile`: Add `audit-schema-version` CI target

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Old TriggerMatrix JSON (no schema_version) -> deserialize | schema_version == 1 | PASS / FAIL |
| 2 | Old TriggerCondition JSON -> deserialize | schema_version == 1 | PASS / FAIL |
| 3 | Set schema_version=5 -> serialize -> deserialize | schema_version == 5 | PASS / FAIL |
| 4 | New TriggerMatrix::new() | schema_version == 1 | PASS / FAIL |
| 5 | All H3 structs have schema_version field | grep confirms | PASS / FAIL |

---

## Category C: Operations

### H5 — No Serialization / Persistence

#### Root Cause Analysis

`TriggerManager` (`trigger.rs:272-312`) holds all matrices in an in-memory `HashMap` with no `save()`/`load()`. On restart, all trigger state is lost. The plan requires two persistence paths:

1. **Graph-backed**: `TriggerManager` data backed by `EvidenceGraph` nodes/edges (partially implemented: `add_trigger_condition()` writes to graph but graph->manager sync is missing)
2. **Disk fallback**: `save(path)` serializes to JSON, `load(path)` restores

Persistence was deferred to Phase 24 but the Phase 23 plan explicitly requires "survives restarts."

#### Permanent Fix

**Path 1 — Graph-backed `rebuild_trigger_manager()`**:

```rust
// In EvidenceGraph impl:
pub fn rebuild_trigger_manager(&self) {
    let nodes = self.nodes.read();
    let mut tm = self.trigger_manager.write();
    for node in nodes.iter() {
        if node.kind != NodeKind::TriggerMatrix { continue; }
        let bug_id = &node.label;
        let matrix = tm.get_or_create(bug_id);
        let incoming = self.edges_to(node.id);
        for (edge, source_id) in &incoming {
            if edge.kind != EdgeKind::Triggers { continue; }
            if let Some(cond_node) = nodes.get(*source_id) {
                if let Some(tc_json) = cond_node.metadata.get("trigger_condition") {
                    if let Ok(tc) = serde_json::from_str::<TriggerCondition>(tc_json) {
                        matrix.add_condition(tc);
                    }
                }
            }
        }
    }
}
```

**Path 2 — Disk save/load on `TriggerManager`**:

```rust
pub fn save(&self, path: &std::path::Path) -> Result<(), anyhow::Error> {
    let matrices: Vec<&TriggerMatrix> = self.matrices.values().collect();
    let json = serde_json::to_string_pretty(&matrices)?;
    std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")))?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load(&mut self, path: &std::path::Path) -> Result<usize, anyhow::Error> {
    if !path.exists() { return Ok(0); }
    let json = std::fs::read_to_string(path)?;
    let matrices: Vec<TriggerMatrix> = serde_json::from_str(&json)?;
    let mut loaded = 0;
    for mut matrix in matrices {
        matrix.rebuild_indices();
        self.matrices
            .entry(matrix.bug_id.clone())
            .and_modify(|existing| {
                for cond in &matrix.conditions {
                    existing.add_condition(cond.clone());
                }
            })
            .or_insert_with(|| { loaded += 1; matrix });
    }
    Ok(loaded)
}
```

**Graceful shutdown with auto-save** (daemon.rs):

```rust
pub async fn run_daemon_with_persistence(
    socket_path: PathBuf,
    trigger_state_path: PathBuf,
) -> anyhow::Result<()> {
    // ... socket setup ...

    let graph = Arc::new(EvidenceGraph::new());

    // Restore from disk
    if trigger_state_path.exists() {
        match graph.load_triggers(&trigger_state_path) {
            Ok(n) => info!(count = n, "restored trigger matrices from disk"),
            Err(e) => warn!(error = %e, "failed to restore trigger state; starting fresh"),
        }
    }

    // Graceful shutdown
    let graph_clone = graph.clone();
    let state_path = trigger_state_path.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("shutting down - saving trigger state...");
        if let Err(e) = graph_clone.save_triggers(&state_path) {
            error!(error = %e, "failed to save trigger state");
        } else {
            info!("trigger state saved");
        }
        std::process::exit(0);
    });

    // ... accept loop ...
}
```

#### Immunity Mechanism

**Persistence roundtrip test**: Create non-trivial TriggerManager (3 bugs, varied conditions), save to temp file, load into fresh manager, assert bit-level equivalence on all matrices. Also test loading from nonexistent file returns Ok(0).

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs:280`: Add `save()` and `load()` to `impl TriggerManager`
2. [ ] `bugswarm-evidence/src/graph.rs:63`: Add `rebuild_trigger_manager()` to `EvidenceGraph`
3. [ ] `bugswarm-evidence/src/graph.rs:85`: Add `load_triggers()` / `save_triggers()` wrapper methods
4. [ ] `bugswarm-evidence/src/daemon.rs:68`: Add `run_daemon_with_persistence()` with SIGTERM handler
5. [ ] `bugswarm-evidence/src/main.rs`: Switch to persistence-aware daemon with CLI flag
6. [ ] `bugswarm-evidence/Cargo.toml`: Add `tempfile = "3"` dev-dependency
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `persistence_save_load_roundtrip`
8. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `persistence_load_nonexistent_file`

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Save 100-bug Manager -> restart -> load | All 100 bugs restored, identical scores | PASS / FAIL |
| 2 | Load nonexistent file | Returns Ok(0), manager empty | PASS / FAIL |
| 3 | Corrupt JSON -> load | Returns Err (serde error) | PASS / FAIL |
| 4 | SIGTERM graceful shutdown | Trigger state saved to disk before exit | PASS / FAIL |
| 5 | Graph-backed rebuild from nodes | Manager state matches graph node state | PASS / FAIL |

---

### H6 — Zero Observability

#### Root Cause Analysis

The plan E2 requires 9 structured log templates and 9 metrics counters. The current codebase has `tracing` as a dependency but zero log calls in `trigger.rs` and no metrics infrastructure. Every important operation (add_condition, dedup, eviction, completeness, priority) is silent. The dependency was added for future use but never wired in.

#### Permanent Fix

**Part 1 — 9 log templates** inserted at operation points:

| Target | Level | Fires When |
|--------|-------|-----------|
| `trigger.condition_added` | info | New condition inserted (not merged) |
| `trigger.condition_merged` | info | Condition dedup-merged into existing |
| `trigger.condition_evicted` | warn | Oldest-unverified condition evicted |
| `trigger.completeness_recomputed` | debug | Completeness score recalculated |
| `trigger.matrix_created` | debug | New TriggerMatrix created |
| `trigger.gate_passed` | debug | Phase 30 gate requirements met |
| `trigger.gate_failed` | trace | Phase 30 gate requirements not met |
| `trigger.priority_computed` | debug | Investigation priority calculated |
| `trigger.staleness_updated` | trace | Staleness metric computed |

Example log event:

```rust
info!(
    target: "trigger.condition_added",
    bug_id = %self.bug_id,
    dimension = ?condition.dimension,
    condition_id = %condition.id,
    layer = ?condition.layer,
    total_conditions = self.conditions.len(),
    "condition added: {} in {:?} from {:?}",
    condition.id, condition.dimension, condition.layer,
);
```

**Part 2 — 9 metrics** via atomic counters in a `TriggerMetrics` struct:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;

pub struct TriggerMetrics {
    pub trigger_rows_total: AtomicU64,     // total new conditions
    pub trigger_dedup_total: AtomicU64,    // total deduplications
    pub completeness_gauge: AtomicU64,     // max completeness (f32 * 10000 as u64)
    pub layer_diversity_gauge: AtomicU64,  // max layer count
    pub priority_gauge: AtomicU64,         // max priority (f32 * 10000 as u64)
    pub queries_total: AtomicU64,          // total trigger queries
    pub query_latency_ms: AtomicU64,       // avg query latency (us)
    pub evictions_total: AtomicU64,        // total evictions
    pub stale_conditions_gauge: AtomicU64, // current stale count
}

impl TriggerMetrics {
    pub fn new() -> Self {
        Self {
            trigger_rows_total: AtomicU64::new(0),
            trigger_dedup_total: AtomicU64::new(0),
            completeness_gauge: AtomicU64::new(0),
            layer_diversity_gauge: AtomicU64::new(0),
            priority_gauge: AtomicU64::new(0),
            queries_total: AtomicU64::new(0),
            query_latency_ms: AtomicU64::new(0),
            evictions_total: AtomicU64::new(0),
            stale_conditions_gauge: AtomicU64::new(0),
        }
    }

    pub fn inc_trigger_rows(&self) { self.trigger_rows_total.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_dedup(&self) { self.trigger_dedup_total.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_evictions(&self) { self.evictions_total.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_queries(&self) { self.queries_total.fetch_add(1, Ordering::Relaxed); }
    pub fn set_completeness(&self, score: f32) {
        self.completeness_gauge.store((score * 10000.0) as u64, Ordering::Relaxed);
    }
    pub fn set_priority(&self, priority: f32) {
        self.priority_gauge.store((priority * 10000.0) as u64, Ordering::Relaxed);
    }
    pub fn set_stale_count(&self, count: u64) {
        self.stale_conditions_gauge.store(count, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> HashMap<&'static str, f64> {
        let mut m = HashMap::new();
        m.insert("trigger_rows_total", self.trigger_rows_total.load(Ordering::Relaxed) as f64);
        m.insert("trigger_dedup_total", self.trigger_dedup_total.load(Ordering::Relaxed) as f64);
        m.insert("completeness_gauge", self.completeness_gauge.load(Ordering::Relaxed) as f64 / 10000.0);
        m.insert("layer_diversity_gauge", self.layer_diversity_gauge.load(Ordering::Relaxed) as f64);
        m.insert("priority_gauge", self.priority_gauge.load(Ordering::Relaxed) as f64 / 10000.0);
        m.insert("queries_total", self.queries_total.load(Ordering::Relaxed) as f64);
        m.insert("query_latency_ms", self.query_latency_ms.load(Ordering::Relaxed) as f64);
        m.insert("evictions_total", self.evictions_total.load(Ordering::Relaxed) as f64);
        m.insert("stale_conditions_gauge", self.stale_conditions_gauge.load(Ordering::Relaxed) as f64);
        m
    }
}
```

Update `TriggerManager` to hold `pub metrics: TriggerMetrics`, wire metric calls into each operation, and expose via `"metrics"` daemon RPC method.

#### Immunity Mechanism

**CI log template check**: A script verifies all 9 `target: "trigger.X"` strings exist in `trigger.rs`.

**Metrics assertion test**: Perform known operations, assert counter values match expected counts.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs:10`: Add `use std::sync::atomic::{AtomicU64, Ordering};`
2. [ ] `bugswarm-evidence/src/trigger.rs:275`: Add `TriggerMetrics` struct + impl
3. [ ] `bugswarm-evidence/src/trigger.rs:272`: Update `TriggerManager` to hold `pub metrics: TriggerMetrics`
4. [ ] `bugswarm-evidence/src/trigger.rs:140`: Insert 9 `tracing` log events at operation points
5. [ ] `bugswarm-evidence/src/trigger.rs`: Wire metrics calls into each operation
6. [ ] `bugswarm-evidence/src/daemon.rs:225`: Add `"metrics"` RPC method
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `metrics_increment_on_operations` test
8. [ ] `Makefile`: Add `audit-log-templates` enforcement target

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Insert new condition -> check logs | trigger.condition_added emitted | PASS / FAIL |
| 2 | Dedup condition -> logs + metrics | trigger.condition_merged + dedup_total=1 | PASS / FAIL |
| 3 | All 9 log templates grep in trigger.rs | 9 unique target strings | PASS / FAIL |
| 4 | All 9 metrics in snapshot() | HashMap has 9 entries | PASS / FAIL |
| 5 | metrics RPC returns valid JSON | All 9 fields present | PASS / FAIL |
| 6 | Counters monotonic (never decrease) | Property test | PASS / FAIL |

---

### H9 — No Human Review Queue for Borderline Fuzzy Merges

#### Root Cause Analysis

Current `is_semantically_equivalent()` uses binary decision: Jaro-Winkler >= 0.85 -> equivalent, else -> not equivalent. Plan C6.2.1.Q2 requires tri-state:
- >= 0.85: Auto-merge (high confidence)
- < 0.75: Reject (definitely different)
- [0.75, 0.85): Create ConditionEquivalentCandidate edge with "pending-review" tag -> human review

The borderline band catches "probably same but ambiguous" cases where human judgment is required. Without this, borderline matches are either auto-merged (losing distinction) or rejected (losing corroboration).

#### Permanent Fix

New constant in `spec.rs`:

```rust
pub const DEDUP_BORDERLINE_THRESHOLD: f64 = 0.75;
```

Tri-state enum:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum EquivalenceResult {
    Exact,         // >= threshold, auto-merge
    Borderline,    // [borderline, threshold), human review
    NotEquivalent, // < borderline, reject
}
```

New `check_equivalence()` function:

```rust
pub fn check_equivalence(a: &str, b: &str) -> EquivalenceResult {
    if a == b { return EquivalenceResult::Exact; }
    if a.len() < DEDUP_MIN_FUZZY_LENGTH || b.len() < DEDUP_MIN_FUZZY_LENGTH {
        return EquivalenceResult::NotEquivalent;
    }
    let sim = strsim::jaro_winkler(
        &a[..a.len().min(DEDUP_MAX_DESCRIPTION_CHARS)],
        &b[..b.len().min(DEDUP_MAX_DESCRIPTION_CHARS)],
    );
    if sim >= DEDUP_JARO_WINKLER_THRESHOLD { EquivalenceResult::Exact }
    else if sim >= DEDUP_BORDERLINE_THRESHOLD { EquivalenceResult::Borderline }
    else { EquivalenceResult::NotEquivalent }
}

// Legacy wrapper (backward compat)
pub fn is_semantically_equivalent(a: &str, b: &str) -> bool {
    check_equivalence(a, b) == EquivalenceResult::Exact
}
```

Review queue in `TriggerMatrix`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewCandidate {
    pub existing_condition_id: String,
    pub proposed_condition: TriggerCondition,
    pub similarity_score: f64,
    pub queued_at: String,
    pub status: String,        // "pending-review" | "approved" | "rejected"
    pub resolved_by: Option<String>,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

// Added to TriggerMatrix fields:
#[serde(default)]
pub review_queue: Vec<ReviewCandidate>,
```

Borderline handling in `add_condition()`: When fuzzy scan finds a borderline match, push a `ReviewCandidate` with status "pending-review" and do NOT auto-merge. The condition is still added as a new row until review resolves.

Review resolution methods:

```rust
pub fn approve_review(&mut self, review_index: usize) -> Result<(), String> {
    let review = self.review_queue.get_mut(review_index)
        .ok_or("review index out of bounds")?;
    if review.status != "pending-review" {
        return Err("review already resolved");
    }
    // Find and merge into existing, remove proposed, rebuild indices
    // ...
    review.status = "approved";
    Ok(())
}

pub fn reject_review(&mut self, review_index: usize) -> Result<(), String> {
    let review = self.review_queue.get_mut(review_index)
        .ok_or("review index out of bounds")?;
    if review.status != "pending-review" {
        return Err("review already resolved");
    }
    review.status = "rejected";
    // Keep both conditions as separate — no merge
    Ok(())
}
```

#### Immunity Mechanism

**Tri-state boundary test**: Construct strings at exactly 0.75, 0.80, 0.85 similarity and verify correct branch. Assert `is_semantically_equivalent` (legacy wrapper) only returns true for Exact.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/spec.rs:27`: Add `DEDUP_BORDERLINE_THRESHOLD = 0.75`
2. [ ] `bugswarm-evidence/src/trigger.rs:255`: Add `EquivalenceResult` enum
3. [ ] `bugswarm-evidence/src/trigger.rs:258`: Add `check_equivalence()` function
4. [ ] `bugswarm-evidence/src/trigger.rs:268`: Update `is_semantically_equivalent()` as legacy wrapper
5. [ ] `bugswarm-evidence/src/trigger.rs:110`: Add `ReviewCandidate` struct
6. [ ] `bugswarm-evidence/src/trigger.rs:125`: Add `review_queue` field to `TriggerMatrix`
7. [ ] `bugswarm-evidence/src/trigger.rs:140`: Update `add_condition()` borderline handling
8. [ ] `bugswarm-evidence/src/trigger.rs`: Add `approve_review()` and `reject_review()`
9. [ ] `bugswarm-evidence/src/trigger.rs`: Add `pending_reviews()` to `TriggerManager`
10. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `equivalence_tri_state_boundaries`
11. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `borderline_creates_review_queue_entry`
12. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `approve_review_merges_conditions`
13. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `reject_review_keeps_both`

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | check_equivalence on identical strings | Exact | PASS / FAIL |
| 2 | Jaro-Winkler in [0.75, 0.85) | Borderline | PASS / FAIL |
| 3 | Jaro-Winkler < 0.75 | NotEquivalent | PASS / FAIL |
| 4 | Borderline add -> review_queue grows | 1 entry, status "pending-review" | PASS / FAIL |
| 5 | Approve review -> conditions merged | Existing gets both layers, proposed removed | PASS / FAIL |
| 6 | Reject review -> both conditions remain | 2 separate conditions, review "rejected" | PASS / FAIL |
| 7 | Legacy is_semantically_equivalent unchanged | Exact=true, Borderline=false, NotEquivalent=false | PASS / FAIL |

---

### H10 — No Backfill Migration from Existing Bug Nodes

#### Root Cause Analysis

When Phase 23 deploys, existing `ConfirmedBug` nodes in the evidence graph may have no `TriggerMatrix` or `TriggerCondition` nodes. Plan E4 requires a migration that:

1. Scans all `ConfirmedBug` nodes
2. Creates an empty `TriggerMatrix` for each bug lacking one
3. Extracts trigger conditions from `SandboxRun` receipt JSONs
4. Logs every 100 bugs
5. Supports rollback

Migration was deferred because Phase 22 lacked trigger infrastructure. Now it must be provided before deployment.

#### Permanent Fix

```rust
// In graph.rs

pub struct MigrationResult {
    pub bugs_scanned: usize,
    pub matrices_created: usize,
    pub conditions_extracted: usize,
    pub errors: Vec<String>,
    pub created_node_ids: Vec<NodeId>,
}

pub fn migrate_existing_bugs(
    graph: &EvidenceGraph,
    extract_condition: fn(&serde_json::Value) -> Option<trigger::TriggerCondition>,
) -> MigrationResult {
    let mut result = MigrationResult {
        bugs_scanned: 0, matrices_created: 0,
        conditions_extracted: 0, errors: vec![], created_node_ids: vec![],
    };

    let nodes = graph.nodes.read();
    let confirmed_bugs: Vec<(NodeId, String)> = nodes.iter()
        .filter(|n| n.kind == NodeKind::ConfirmedBug)
        .map(|n| (n.id, n.label.clone()))
        .collect();
    drop(nodes);

    info!(total_bugs = confirmed_bugs.len(), "starting backfill migration");

    for (i, (bug_node_id, bug_label)) in confirmed_bugs.iter().enumerate() {
        // Skip if already has TriggerMatrix
        let has_matrix = {
            let nodes = graph.nodes.read();
            nodes.iter().any(|n| n.kind == NodeKind::TriggerMatrix && n.label == *bug_label)
        };
        if has_matrix { continue; }

        // Create TriggerMatrix node
        let matrix_node = EvidenceNode::new(0, NodeKind::TriggerMatrix, bug_label, "migration-backfill");
        let matrix_node_id = graph.add_node(matrix_node);
        result.matrices_created += 1;
        result.created_node_ids.push(matrix_node_id);

        // Link Bug -> Aggregates -> TriggerMatrix
        let edge = EvidenceEdge::new(EdgeKind::Aggregates, *bug_node_id, matrix_node_id, 1.0);
        graph.add_edge(edge);

        // Extract trigger conditions from SandboxRun receipts
        let sandbox_data = extract_sandbox_receipts(graph, *bug_node_id);
        for (run_id, receipt_json) in &sandbox_data {
            match serde_json::from_str::<serde_json::Value>(receipt_json) {
                Ok(receipt) => {
                    if let Some(tc) = extract_condition(&receipt) {
                        // Create TriggerCondition graph node
                        let mut cond_node = EvidenceNode::new(
                            0, NodeKind::TriggerCondition,
                            &format!("{}-{:?}", bug_label, tc.dimension),
                            "migration-backfill",
                        );
                        cond_node.metadata.insert(
                            "trigger_condition".to_string(),
                            serde_json::to_string(&tc).unwrap_or_default(),
                        );
                        cond_node.metadata.insert("source_run_id".to_string(), run_id.to_string());
                        let cond_node_id = graph.add_node(cond_node);
                        result.created_node_ids.push(cond_node_id);
                        result.conditions_extracted += 1;

                        // Edge: TriggerCondition -> Triggers -> TriggerMatrix
                        let tc_edge = EvidenceEdge::new(EdgeKind::Triggers, cond_node_id, matrix_node_id, 1.0);
                        graph.add_edge(tc_edge);

                        // Also add to in-memory TriggerManager
                        graph.trigger_manager.write().add_condition(tc);
                    }
                }
                Err(e) => {
                    result.errors.push(format!(
                        "receipt parse error for run {} (bug {}): {}", run_id, bug_label, e
                    ));
                }
            }
        }

        result.bugs_scanned += 1;
        if result.bugs_scanned % 100 == 0 {
            info!(
                checkpoint = result.bugs_scanned,
                total = confirmed_bugs.len(),
                matrices = result.matrices_created,
                conditions = result.conditions_extracted,
                "migration checkpoint: {}/{} bugs",
                result.bugs_scanned, confirmed_bugs.len(),
            );
        }
    }

    info!(
        bugs_scanned = result.bugs_scanned,
        matrices = result.matrices_created,
        conditions = result.conditions_extracted,
        errors = result.errors.len(),
        "backfill migration complete",
    );
    result
}

fn extract_sandbox_receipts(graph: &EvidenceGraph, bug_node_id: NodeId) -> Vec<(NodeId, String)> {
    let nodes = graph.nodes.read();
    let edges_from_bug = graph.edges_from(bug_node_id);
    edges_from_bug.iter().filter_map(|(e, target_id)| {
        if e.kind == EdgeKind::Confirms {
            nodes.get(*target_id).and_then(|n| {
                if n.kind == NodeKind::SandboxRun {
                    n.metadata.get("receipt").map(|r| (*target_id, r.clone()))
                } else { None }
            })
        } else { None }
    }).collect()
}

/// Default condition extractor for sandbox receipts.
pub fn default_extract_condition(receipt: &serde_json::Value) -> Option<trigger::TriggerCondition> {
    let raw_desc = receipt.get("trigger_condition")
        .or_else(|| receipt.get("input_hint"))
        .or_else(|| receipt.get("trigger_hint"))
        .or_else(|| receipt.get("input"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if raw_desc.is_empty() { return None; }

    let dimension = receipt.get("dimension")
        .and_then(|v| v.as_str())
        .map(parse_dimension_str)
        .unwrap_or(trigger::TriggerDimension::Input);

    let bug_id = receipt.get("bug_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown-migrated-bug");

    Some(trigger::TriggerCondition::new(
        bug_id, dimension, raw_desc,
        trigger::ContributionLayer::Manual,
    ))
}

fn parse_dimension_str(s: &str) -> trigger::TriggerDimension {
    match s.to_lowercase().as_str() {
        "input" => TriggerDimension::Input,
        "environment" => TriggerDimension::Environment,
        "timing" => TriggerDimension::Timing,
        "datastate" | "data_state" => TriggerDimension::DataState,
        "concurrency" => TriggerDimension::Concurrency,
        "configuration" | "config" => TriggerDimension::Configuration,
        "dependency" | "dependencyversion" => TriggerDimension::DependencyVersion,
        "os" | "arch" | "osarch" => TriggerDimension::OsArch,
        _ => TriggerDimension::Input,
    }
}
```

**Rollback**: Since EvidenceGraph is append-only (no node deletion), rollback marks created nodes with `metadata.rollback = "true"` and clears the TriggerManager.

#### Immunity Mechanism

**Idempotency test**: Run migration twice on the same graph. Second run must create zero new nodes.

**Dry-run mode**: Migration supports `--dry-run` flag that simulates without modifying graph, reporting what would be created.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/graph.rs:618`: Add `MigrationResult` struct and `migrate_existing_bugs()`
2. [ ] `bugswarm-evidence/src/graph.rs`: Add `extract_sandbox_receipts()` helper
3. [ ] `bugswarm-evidence/src/graph.rs`: Add `default_extract_condition()` extractor
4. [ ] `bugswarm-evidence/src/graph.rs`: Add `rollback_migration()` function
5. [ ] `bugswarm-evidence/src/main.rs`: Add `migrate` CLI subcommand with `--dry-run` flag
6. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `migration_is_idempotent` test
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `migration_creates_trigger_matrix` test
8. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `migration_empty_graph_noop` test

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Graph with 5 ConfirmedBugs, 0 TriggerMatrices -> migrate | 5 matrices created | PASS / FAIL |
| 2 | Migration idempotent (run twice) | Second run creates 0 matrices | PASS / FAIL |
| 3 | ConfirmedBug with SandboxRun receipt -> migrate | TriggerCondition extracted from receipt | PASS / FAIL |
| 4 | Dry-run mode | No nodes created, report shows expected counts | PASS / FAIL |
| 5 | Rollback after migration | Created nodes marked rolled_back, Manager cleared | PASS / FAIL |

---

## Summary Table

| # | Category | Finding | Effort (hrs) | Depends on | Key File(s) |
|---|----------|---------|-------------|------------|-------------|
| H1 | Algorithm | Density + layer bonuses | 2 | -- | spec.rs:23, trigger.rs:166 |
| H4 | Algorithm | investigation_priority() | 1 | H1 | spec.rs:26, trigger.rs:193 |
| H7 | Algorithm | Stale eviction | 2 | -- | spec.rs:29, trigger.rs:140 |
| H8 | Algorithm | Staleness metric | 1 | H7 | spec.rs:31, trigger.rs:196 |
| H12 | Algorithm | O(1) hash fast path | 2 | -- | trigger.rs:110, trigger.rs:140 |
| H2 | Data | Missing fields on TriggerCondition | 3 | -- | trigger.rs:64-110 |
| H3 | Data | Missing plan-required structs | 4 | H2 | trigger.rs:60-220 |
| H11 | Data | schema_version | 1 | -- | trigger.rs:111, trigger.rs:64 |
| H5 | Ops | Persistence | 3 | F1 | trigger.rs:280, graph.rs:63, daemon.rs:68 |
| H6 | Ops | Observability | 2 | -- | trigger.rs:275, trigger.rs:140 |
| H9 | Ops | Review queue | 2 | F2 | spec.rs:27, trigger.rs:110, trigger.rs:255 |
| H10 | Ops | Backfill migration | 3 | H5 | graph.rs:618 |

**Implementation order**: H2 -> H3 -> H11 -> H1 -> H4 -> H12 -> H7 -> H8 -> H5 -> H6 -> H9 -> H10

---

## Appendix: Immunity Mechanism Reference

| Layer | Mechanism | Enforced by | Scope |
|-------|-----------|------------|-------|
| 1 | Property-based tests (proptest) | CI: cargo test | Algorithm correctness (H1, H4, H7, H8) |
| 2 | Spec constants enforcement (no magic numbers) | CI: grep audit | All threshold values (H1, H7, H8, H9) |
| 3 | Regression freeze (known-answer tests) | CI: cargo test | All scoring functions (H1, H4, H8) |
| 4 | Compile-time traits (Prioritizable) | CI: grep for bare .sort() | Priority ordering (H4) |
| 5 | debug_assert! invariants | CI: cargo test --release false | Eviction cap (H7), index consistency (H12) |
| 6 | Serde backward compatibility tests | CI: cargo test | All structs (H2, H3, H11) |
| 7 | Schema version presence check | CI: grep struct definitions | Forward compatibility (H11) |
| 8 | Persistence roundtrip tests | CI: cargo test | Save/load correctness (H5) |
| 9 | Log template CI check | CI: grep for target strings | Observability (H6) |
| 10 | Migration idempotency tests | CI: cargo test | Backfill correctness (H10) |
| 11 | Static time injection (no Utc::now() in tests) | CI: grep for Utc::now in tests/ | Deterministic testing (H8) |
| 12 | Index integrity assertions | Debug-only: assert_indices_consistent() | Hash map correctness (H12) |

---

## Appendix: Acceptance Test Suite Outline

```
tests/trigger_audit.rs (extension)
├── completeness_tests
│   ├── completeness_density_bonus_basic
│   ├── completeness_layer_bonus_basic
│   ├── completeness_known_configurations_regression
│   └── completeness_min_floor_applies
├── priority_tests
│   ├── investigation_priority_respects_severity
│   ├── investigation_priority_full_completeness
│   ├── investigation_priority_default_severity
│   ├── investigation_priority_with_staleness
│   └── prioritizable_trait_compiles
├── eviction_tests
│   ├── stale_eviction_dimension_cap
│   ├── stale_eviction_verified_protection
│   ├── stale_eviction_interleaved_dimensions
│   └── stale_eviction_debug_assert_triggers
├── staleness_tests
│   ├── staleness_fresh
│   ├── staleness_30_days
│   ├── staleness_15_days
│   ├── staleness_clamped_at_max
│   └── staleness_never_panics_on_bad_timestamps
├── hash_fast_path_tests
│   ├── hash_fast_path_performance
│   ├── hash_fast_path_correctness
│   ├── hash_index_rebuild_roundtrip
│   └── hash_index_consistency_after_eviction
├── struct_field_tests
│   ├── trigger_condition_forward_compatible_deserialization
│   ├── trigger_condition_all_fields_present
│   ├── all_plan_structs_serialize_completely
│   └── severity_specific_range_clamp
├── schema_version_tests
│   ├── schema_version_defaults_on_old_json
│   └── schema_version_survives_roundtrip
├── persistence_tests
│   ├── persistence_save_load_roundtrip
│   ├── persistence_load_nonexistent_file
│   └── persistence_corrupt_json_error
├── metrics_tests
│   ├── metrics_increment_on_operations
│   └── metrics_snapshot_has_nine_entries
├── review_queue_tests
│   ├── equivalence_tri_state_boundaries
│   ├── borderline_creates_review_queue_entry
│   ├── approve_review_merges_conditions
│   └── reject_review_keeps_both
├── migration_tests
│   ├── migration_is_idempotent
│   ├── migration_creates_trigger_matrix
│   └── migration_empty_graph_noop
└── property_tests
    ├── completeness_score_bounded (proptest)
    └── completeness_density_penalty (proptest)
```
