# Phase 23 Trigger Matrix — HIGH Finding Remediation Plan (v2, Critiqued & Hardened)

**Status**: DRAFT — not yet implemented  
**Source**: Phase 23 Trigger Matrix Audit (12 HIGH findings, H1–H12)  
**Target codebase**: `bugswarm-evidence/src/` (Rust crate), `bugswarm-evidence/tests/`  
**Total estimated effort**: ~30 hrs (revised from 26 after hardening)  
**Critique basis**: 68 issues across 5 categories (16 kill-switch, 9 algorithmic, 12 inconsistencies, 13 error-prone, 18 missing practices) — ALL fixed in this revision.

---

## Revised Implementation Sequence

```
H11(schema_version,default_schema_version in spec.rs)
 → H2(fields + full_semantic_hash + EP1 fix: contributed_at: DateTime<Utc>)
 → H3(structs: TriggerConfig wired, DimensionScore helper, request/response structs)
 → H1(completeness: corrected density bonus per-dimension, flat min_floor, base_coverage helper)
 → H4 + H8(scoring: ALL three priority weights in spec.rs, staleness with DateTime<Utc>, formula uses spec constants)
 → H9(tri-state dedup: check_equivalence with qualified imports, review queue with full approve/reject)
 → H12(O(1) index: 32-char hash, Option<HashMap> lazy indices, index-aware eviction, rebuild on mutation)
 → H7(eviction: single-pass fold, DateTime<Utc> comparison, index cleanup via rebuild_indices, vacuum after bulk evict)
 → H5(persistence: restore_condition, atomic tmp+fsync+rename, streaming write, grace shutdown, entry match fix)
 → H6(observability: TriggerConfig wired, fetch_max for gauges, stale gauge wired, MetricsSnapshot struct)
 → H10(migration: journal-based, edges_to for Confirms, no double node creation, transactional resume)
```

**Rationale** (fixing all ordering contradictions from original plan):
- H11 first so `schema_version` exists in `spec.rs` as `default_schema_version()`, DRY.
- H2 early so all fields are present before any algorithm references them.
- H3 early so `TriggerConfig` is available to be wired into later stages.
- H4+H8 merged: all three priority weights defined ONCE in spec.rs at final values, never changed.
- H9 before H12: `check_equivalence` exists before `add_condition` uses it (fixes IC6).
- H7 after H12: eviction uses stable 32-char hash keys and index cleanup.
- H5 after H7: persistence uses `restore_condition` (bypasses dedup/eviction pipeline).
- H10 last: migration depends on all TriggerManager APIs being stable.

**Atomic commit rule**: Each finding (H1–H12) is one git commit. Every commit passes `cargo test --no-fail-fast`.

**Test-first mandate**: All 65+ acceptance tests (Appendix B) are written, committed, and FAILING on `main` BEFORE any implementation code is written. Red-green-refactor cycle per finding.

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
   - H9: No Human Review Queue for Borderline Fuzzy Merges
   - H10: No Backfill Migration from Existing Bug Nodes
4. [Summary Table](#summary-table)
5. [Appendix A: Immunity Mechanism Reference](#appendix-a-immunity-mechanism-reference)
6. [Appendix B: Acceptance Test Suite Outline](#appendix-b-acceptance-test-suite-outline)
7. [Appendix C: Implementation Practices (MP1–MP18)](#appendix-c-implementation-practices-mp1-mp18)
8. [Appendix D: Hash Policy, Format Stability & Memory Budget](#appendix-d-hash-policy-format-stability--memory-budget)
9. [Appendix E: Migration & Rollback Protocol](#appendix-e-migration--rollback-protocol)

---

## Prelude: Shared Module-Level Infrastructure

All items below are placed in `bugswarm-evidence/src/lib.rs` or `bugswarm-evidence/src/spec.rs` and are available to all subsequent sections.

### `spec.rs` — Single Source of Truth for All Constants

```rust
// bugswarm-evidence/src/spec.rs
// All threshold constants, weights, and format specifiers.

// ------ H1: Completeness scoring ------
/// Minimum number of conditions in a single dimension to reach full density bonus.
pub const DENSITY_BONUS_SATURATION: usize = 3;
/// Minimum number of layers to reach full layer diversity bonus.
pub const LAYER_BONUS_SATURATION: usize = 3;
/// Hard floor for completeness score. Never lower than this, regardless of coverage.
pub const COMPLETENESS_MIN_FLOOR: f64 = 0.1;
/// The trigger dimension excluded from weighted coverage computation (bonus-only dim).
pub const EXCLUDED_DIMENSION: TriggerDimension = TriggerDimension::OsArch;

// ------ H4+H8: Priority weights (ALL defined together, sum = 1.0) ------
pub const PRIORITY_SEVERITY_WEIGHT: f64 = 0.6;
pub const PRIORITY_INCOMPLETENESS_WEIGHT: f64 = 0.3;
pub const PRIORITY_STALENESS_WEIGHT: f64 = 0.1;
// Compile-time assertion: weights sum to 1.0
// Verified by test: priority_weights_sum_to_one

// ------ H7: Eviction ------
/// Maximum number of conditions allowed per dimension before eviction triggers.
pub const MAX_CONDITIONS_PER_DIMENSION: usize = 100;

// ------ H8: Staleness ------
/// Staleness window in days. Conditions older than this reach max staleness (1.0).
pub const STALENESS_WINDOW_DAYS: f64 = 30.0;

// ------ H9: Dedup tri-state ------
pub const DEDUP_JARO_WINKLER_THRESHOLD: f64 = 0.85;
pub const DEDUP_BORDERLINE_THRESHOLD: f64 = 0.75;
pub const DEDUP_MIN_FUZZY_LENGTH: usize = 4;
pub const DEDUP_MAX_DESCRIPTION_CHARS: usize = 200;

// ------ H12: Hash policy ------
/// Number of hex chars for the hash_key used in normalized_idx (128 bits).
pub const HASH_KEY_PREFIX_LEN: usize = 32;
/// Maximum hash_map entries before triggering a collision-warning log.
pub const HASH_COLLISION_WARN_THRESHOLD: u64 = 1_000_000_000;

// ------ H5/H6: Persistence & metrics ------
/// Maximum serialized JSON size before switching to per-bug file storage.
pub const MAX_SERIALIZED_SIZE_BYTES: usize = 10 * 1024 * 1024; // 10 MB
/// Timestamp format for all RFC 3339 serialization.
pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";

// ------ H5: Phase 30 gate ------
pub const PHASE30_MIN_ROWS: usize = 2;
pub const PHASE30_MIN_LAYERS: usize = 2;

// ------ Shared: schema version default ------
pub fn default_schema_version() -> u32 { 1 }
```

### `lib.rs` — Crate-Level Declarations

```rust
// bugswarm-evidence/src/lib.rs
pub mod spec;
pub mod trigger;
pub mod graph;
pub mod types;
pub mod daemon;

// Crate-level quality gates
#![deny(missing_docs)]
#![deny(unsafe_code)]

// Re-export key types for convenience
pub use trigger::{
    TriggerCondition, TriggerMatrix, TriggerManager, TriggerConfig,
    TriggerDimension, ContributionLayer, SeveritySpecific,
    RawTriggerCondition, DimensionScore,
    TriggerMatrixResponse, DescribeTriggerRequest, DescribeTriggerResponse,
    EquivalenceResult, ReviewCandidate,
    TriggerMetrics, MetricsSnapshot,
    Prioritizable, full_semantic_hash, dimension_key,
};
```

### `trigger.rs` — Required Imports (used throughout this plan)

```rust
// bugswarm-evidence/src/trigger.rs
use std::collections::{HashMap, HashSet, BTreeMap};
use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use tracing::{info, warn, debug, trace};

use crate::spec;
```

### `graph.rs` — New Public API Signatures Required Before Migration (fixes KS6)

The following graph APIs must exist before H10 migration can be implemented:

```rust
// bugswarm-evidence/src/graph.rs

/// Get all edges whose `to` field equals `node_id` (incoming edges).
/// Returns Vec<(edge, source_node_id)>.
/// REQUIRED for: H10 migration (find Confirms edges to ConfirmedBug),
///               H5 rebuild_trigger_manager (find Triggers edges to TriggerMatrix).
pub fn edges_to(&self, node_id: NodeId) -> Vec<(EvidenceEdge, NodeId)>;

/// Get all edges whose `from` field equals `node_id` (outgoing edges).
/// REQUIRED for: general graph traversal.
pub fn edges_from(&self, node_id: NodeId) -> Vec<(EvidenceEdge, NodeId)>;

/// Add a node and return its assigned ID.
/// REQUIRED for: H10 migration (create TriggerMatrix and TriggerCondition nodes).
pub fn add_node(&self, node: EvidenceNode) -> NodeId;

/// Add an edge between two existing nodes.
/// REQUIRED for: H10 migration (create Aggregates and Triggers edges).
pub fn add_edge(&self, edge: EvidenceEdge);
```

**Migration dependency**: H10 CANNOT be implemented until `edges_to()`, `add_node()`, and `add_edge()` are public methods on `EvidenceGraph`. These may already exist — verify and document exact signatures before writing H10 code.

---

## Category A: Algorithm Completeness

### H1 — Completeness Scoring Missing Density Bonus + Layer Diversity Bonus

#### Root Cause Analysis

The current `recompute_completeness()` at `trigger.rs:166-187` computes a flat weighted-coverage ratio:
```
score = covered_weight / total_weight
```
This treats "1 condition per dimension" identically to "3 conditions per dimension" — both get the same base score. The Phase 23 plan (section C6.2) requires multiplicative bonuses to reward depth of trigger documentation, not just breadth. Three factors were omitted:

1. **Density bonus** — rewards multiple distinct conditions *within a single dimension*. The original plan's code used `self.conditions.len()` (global count across ALL dimensions), which makes density indistinguishable from cardinality (KS1). **Fix**: compute per-dimension density as average across dimensions.
2. **Layer diversity bonus** — rewards multiple independent layers corroborating the same trigger surface.
3. **Min-floor penalty** — prevents low coverage from being zeroed out. The original plan incorrectly scaled the floor by the bonuses themselves (KS7). **Fix**: flat constant floor.

#### Permanent Fix

**New constants in `spec.rs`** (already defined in Prelude):
- `DENSITY_BONUS_SATURATION = 3`
- `LAYER_BONUS_SATURATION = 3`
- `COMPLETENESS_MIN_FLOOR = 0.1`
- `EXCLUDED_DIMENSION = TriggerDimension::OsArch`

**Extract `compute_base_coverage()` as a private helper** (fixes IC2):

```rust
/// Compute base weighted-coverage ratio across all non-excluded dimensions.
/// Returns (covered_weight, total_weight, base_score).
fn compute_base_coverage(conditions: &[TriggerCondition]) -> (f32, f32, f32) {
    let mut covered: HashSet<TriggerDimension> = HashSet::new();
    for c in conditions {
        covered.insert(c.dimension);
    }

    let total_weight: f32 = TriggerDimension::all().iter()
        .filter(|d| **d != crate::spec::EXCLUDED_DIMENSION)
        .map(|d| d.weight())
        .sum();

    let covered_weight: f32 = covered.iter()
        .filter(|d| **d != crate::spec::EXCLUDED_DIMENSION)
        .map(|d| d.weight())
        .sum();

    let base_score = if total_weight > 0.0 {
        (covered_weight / total_weight).min(1.0)
    } else {
        0.0
    };

    (covered_weight, total_weight, base_score)
}
```

**Replace `recompute_completeness()` with per-dimension density bonus** (fixes KS1, KS7):

```rust
pub fn recompute_completeness(&mut self) {
    use crate::spec::{
        DENSITY_BONUS_SATURATION, LAYER_BONUS_SATURATION, COMPLETENESS_MIN_FLOOR,
    };

    // Phase 1: base weighted coverage
    let (_covered_weight, _total_weight, base_score) =
        compute_base_coverage(&self.conditions);

    // Phase 2: per-dimension density bonus (fixes KS1)
    // Compute avg density: for each dim, density = min(conds_in_dim / SATURATION, 1.0)
    // Then average across ALL dimensions (including uncovered ones, which contribute 0.0).
    let density_bonus: f64 = {
        let dim_counts: Vec<f64> = TriggerDimension::all().iter()
            .map(|dim| {
                let cnt = self.conditions.iter()
                    .filter(|c| c.dimension == *dim)
                    .count();
                (cnt as f64 / DENSITY_BONUS_SATURATION as f64).min(1.0)
            })
            .collect();
        if dim_counts.is_empty() {
            0.0
        } else {
            dim_counts.iter().sum::<f64>() / dim_counts.len() as f64
        }
    };

    // Phase 3: layer diversity bonus
    let layer_bonus: f64 = {
        let cnt = self.contributing_layers.len() as f64;
        (cnt / LAYER_BONUS_SATURATION as f64).min(1.0)
    };

    // Phase 4: multiplicative composition with FLAT min-floor clamp (fixes KS7)
    let effective_score = base_score as f64 * density_bonus * layer_bonus;
    self.completeness_score = effective_score
        .max(COMPLETENESS_MIN_FLOOR)
        .min(1.0) as f32;
}
```

**Key corrections from original plan**:
- **KS1 fixed**: Density is `avg(per_dim_density)` not `global_count / SATURATION`. A matrix with 7 conditions all in `Input` gets density = (7/3 capped at 1.0 + six dims at 0.0) / 8 = 0.125. A matrix with 1 condition in each of 7 dims gets density = (1/3 × 7 + 0/3 × 1) / 8 = 0.292. Depth vs breadth is properly differentiated.
- **KS7 fixed**: `min_floor` is the flat constant `COMPLETENESS_MIN_FLOOR = 0.1`, not a bonus-scaled value. `effective_score.max(0.1).min(1.0)` ensures partial coverage is never zeroed but perfect coverage never gets inflated.
- **IC2 fixed**: `compute_base_coverage()` extracted to avoid duplicating logic.
- **EP9 fixed**: `EXCLUDED_DIMENSION` constant replaces hardcoded `OsArch` filter.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/spec.rs`: Add `DENSITY_BONUS_SATURATION`, `LAYER_BONUS_SATURATION`, `COMPLETENESS_MIN_FLOOR`, `EXCLUDED_DIMENSION`
2. [ ] `bugswarm-evidence/src/trigger.rs`: Add private `compute_base_coverage()` helper
3. [ ] `bugswarm-evidence/src/trigger.rs:166-187`: Replace `recompute_completeness()` with per-dimension density version
4. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `completeness_density_per_dimension` test (verifies KS1 fix)
5. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `completeness_flat_min_floor` test (verifies KS7 fix)
6. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `completeness_known_configurations_regression`
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add property-based tests (proptest dependency)
8. [ ] `Makefile` or CI: Add `audit-magic-numbers` enforcement target
9. [ ] Run `cargo test -p bugswarm-evidence` — update expected values in existing completeness tests



> **Note on struct re-definitions (fixes IC8)**: Every code block in this plan that re-defines a struct already shown earlier (e.g., `TriggerMatrix` appears in H11, H12, H3 sections) includes ALL fields from all previous definitions. The canonical, complete struct definition is the one in H12 (§H12 — Updated TriggerMatrix). Implementers should use that as the authoritative source, with fields added incrementally per implementation order.


#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Empty matrix (0 conditions) | completeness_score == 0.0 | PASS / FAIL |
| 2 | 1 condition, 1 layer, 1 dimension | completeness_score >= COMPLETENESS_MIN_FLOOR | PASS / FAIL |
| 3 | 7 dims covered, 3+ layers, 3+ conds per dim | completeness_score >= 0.95 | PASS / FAIL |
| 4 | 1 cond per 7 dims vs 3 conds per 1 dim (same total=7) | Density scores differ; breadth has higher baseline | PASS / FAIL |
| 5 | Property: score always in [0.0, 1.0] | No test failure across 10K random matrices | PASS / FAIL |
| 6 | Property: monotonic on adding novel dim/layer conds | No score regression | PASS / FAIL |

---

### H4 — No `investigation_priority()` Function (Merged with H8)

**H4 and H8 are implemented together to avoid weight collisions.** All three priority weights are defined in `spec.rs` at their final values from the start (fixes KS4, IC3, IC7). The `investigation_priority()` formula uses only `crate::spec::` constants — never hardcoded numbers.

#### Root Cause Analysis

`TriggerManager::get_incomplete_bugs()` sorts purely by completeness score ascending, ignoring severity, incompleteness, and staleness. The plan formula requires: `0.6 * severity + 0.3 * incompleteness + 0.1 * staleness_urgency`. No implementation existed.

#### Permanent Fix

**Constants in `spec.rs`** (already defined in Prelude):
```rust
pub const PRIORITY_SEVERITY_WEIGHT: f64 = 0.6;
pub const PRIORITY_INCOMPLETENESS_WEIGHT: f64 = 0.3;
pub const PRIORITY_STALENESS_WEIGHT: f64 = 0.1;
```

**`staleness()` method on `TriggerMatrix`** (uses `DateTime<Utc>` not String, fixes KS2, EP1):

```rust
/// Compute staleness as fraction of the staleness window.
/// 0.0 = just updated, 1.0 = at or beyond STALENESS_WINDOW_DAYS.
/// Always returns a value in [0.0, 1.0]. Malformed or future timestamps return 0.0.
pub fn staleness(&self) -> f64 {
    let now = Utc::now();
    if let Some(ref last) = self.last_updated {
        let dur = now.signed_duration_since(*last);
        if dur.num_seconds() < 0 {
            return 0.0; // future timestamp: clamp to zero staleness
        }
        let age_days = dur.num_days() as f64;
        let raw = age_days / crate::spec::STALENESS_WINDOW_DAYS;
        raw.clamp(0.0, 1.0)
    } else {
        0.0 // uninitialized timestamp = fresh
    }
}
```

**Note**: `last_updated` field type changed from `String` to `Option<DateTime<Utc>>` to fix EP1. Alternatively, keep as `DateTime<Utc>` with `Utc::now()` default on construction (no Option needed). See H2 for the final field definition.

**`investigation_priority()` method** (uses spec constants — fixes KS4):

```rust
/// Compute investigation priority score (0.0-1.0, higher = more urgent).
/// Uses all three spec weights with fully qualified paths.
pub fn investigation_priority(&self, severity: Option<u8>) -> f64 {
    let severity_weight = match severity {
        Some(s) if s > 0 => (s as f64 / 10.0).clamp(0.1, 1.0),
        _ => 0.5, // default: unknown severity = medium urgency
    };

    let incompleteness = 1.0 - self.completeness_score as f64;

    // staleness_urgency: stale = 1.0 means low urgency, so we invert
    let staleness_urgency = 1.0 - self.staleness();

    let priority =
          crate::spec::PRIORITY_SEVERITY_WEIGHT * severity_weight
        + crate::spec::PRIORITY_INCOMPLETENESS_WEIGHT * incompleteness
        + crate::spec::PRIORITY_STALENESS_WEIGHT * staleness_urgency;

    priority.clamp(0.0, 1.0)
}
```

**`Prioritizable` trait** (fully defined, fixes KS16):

```rust
/// Trait for types that can be sorted by priority.
pub trait Prioritizable {
    /// Return a priority score in [0.0, 1.0]; higher = more urgent.
    fn priority(&self) -> f64;
}

impl Prioritizable for TriggerMatrix {
    fn priority(&self) -> f64 {
        self.investigation_priority(None)
    }
}
```

**Prioritized sort method on `TriggerManager`**:

```rust
pub fn get_incomplete_bugs_prioritized(
    &self,
    bug_severities: &HashMap<String, u8>,
) -> Vec<(&TriggerMatrix, f64)> {
    let mut incomplete: Vec<(&TriggerMatrix, f64)> = self.matrices.values()
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

**Key corrections from original plan**:
- **KS4/IC3/IC7 fixed**: All three weights defined together at final H8 values from the start.
- **KS2/EP1 fixed**: `last_updated` is `DateTime<Utc>`. `staleness()` uses `signed_duration_since` which is type-safe.
- **KS4 fixed**: `investigation_priority()` uses `crate::spec::PRIORITY_*` not hardcoded `0.6`, `0.3`, `0.1`.
- **KS16 fixed**: `Prioritizable` trait fully defined with `priority() -> f64` method.
- Timeline protection: CI greps for `Utc::now()` in test files and rejects.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/spec.rs`: Add PRIORITY_* three weights
2. [ ] `bugswarm-evidence/src/trigger.rs`: Add `Prioritizable` trait definition
3. [ ] `bugswarm-evidence/src/trigger.rs`: Add `staleness()` method to `impl TriggerMatrix`
4. [ ] `bugswarm-evidence/src/trigger.rs`: Add `investigation_priority()` to `impl TriggerMatrix`
5. [ ] `bugswarm-evidence/src/trigger.rs`: Implement `Prioritizable` for `TriggerMatrix`
6. [ ] `bugswarm-evidence/src/trigger.rs`: Add `get_incomplete_bugs_prioritized()` to `TriggerManager`
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `priority_weights_sum_to_one` test
8. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add 6 priority+staleness tests
9. [ ] CI: Add grep check for bare `Utc::now()` in test files

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | PRIORITY_SEVERITY + INCOMPLETENESS + STALENESS | Sum == 1.0 | PASS / FAIL |
| 2 | severity=10, completeness=0.0, fresh (staleness=0) | priority = 0.6*1.0 + 0.3*1.0 + 0.1*1.0 = 1.0 | PASS / FAIL |
| 3 | severity=1, completeness=1.0, fresh | priority = 0.6*0.1 + 0.3*0.0 + 0.1*1.0 = 0.16 | PASS / FAIL |
| 4 | severity=None, completeness=0.5, fresh | priority = 0.6*0.5 + 0.3*0.5 + 0.1*1.0 = 0.55 | PASS / FAIL |
| 5 | staleness = 1.0 (max stale) | staleness_urgency = 0.0 | PASS / FAIL |
| 6 | Priority always in [0.0, 1.0] across 10K random inputs | No out-of-range values | PASS / FAIL |

---

### H12 — No O(1) Exact-Match Hash Fast Path

**Moved before H7** so eviction can use stable indices. Uses full 32-char hash prefix (128 bits, fixes KS8). Indices are `Option<HashMap>` with lazy initialization via `ensure_indices()` (fixes EP11).

#### Root Cause Analysis

Every `add_condition()` runs O(n) linear scan over all conditions with Jaro-Winkler per element. For 700 conditions (7 dims × 100), every insert costs up to 700 Jaro-Winkler calls. The plan requires `HashMap<String, Vec<usize>>` for O(1) exact-match lookup.

#### Permanent Fix

**`dimension_key()` — format-stable dimension serialization** (fixes AW6):

```rust
/// Return a format-stable string key for a trigger dimension.
/// NEVER use Debug formatting ({:?}) for hash keys — it's not stable across refactors.
pub fn dimension_key(d: TriggerDimension) -> &'static str {
    match d {
        TriggerDimension::Input => "input",
        TriggerDimension::Environment => "env",
        TriggerDimension::Timing => "timing",
        TriggerDimension::DataState => "datastate",
        TriggerDimension::Concurrency => "concurrency",
        TriggerDimension::Configuration => "config",
        TriggerDimension::DependencyVersion => "depver",
        TriggerDimension::OsArch => "osarch",
    }
}
```

**`full_semantic_hash()` — full 64-char SHA-256** (fixes KS11, AW8):

```rust
/// Compute the full SHA-256 hex digest (64 hex chars) for cross-matrix similarity search.
pub fn full_semantic_hash(s: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}
```

**`hash_prefix()` — 32-char prefix for index lookup** (fixes KS8):

```rust
/// Return the first HASH_KEY_PREFIX_LEN hex chars of SHA-256 (128 bits).
/// Safe for HashMap lookup: collision probability < 2^-128 per pair.
fn hash_prefix(s: &str) -> String {
    full_semantic_hash(s)[..crate::spec::HASH_KEY_PREFIX_LEN].to_string()
}
```

**Updated `TriggerMatrix` with lazy indices** (fixes EP11):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerMatrix {
    pub bug_id: String,
    pub conditions: Vec<TriggerCondition>,
    pub contributing_layers: HashSet<ContributionLayer>,
    pub dedup_count: usize,
    pub completeness_score: f32,
    pub last_updated: DateTime<Utc>,

    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,

    /// O(1) exact-match lookup: key = "dimkey:{hash_prefix}"
    /// Lazily initialized via ensure_indices(). None means needs rebuild. (fixes EP11)
    #[serde(skip, default)]
    normalized_idx: Option<HashMap<String, Vec<usize>>>,

    /// O(k) per-dimension scan: dimension -> condition indices
    #[serde(skip, default)]
    by_dimension: Option<HashMap<TriggerDimension, Vec<usize>>>,

    #[serde(default)]
    pub review_queue: Vec<ReviewCandidate>,


**Thread-safety of indices** (fixes AW1):

The `normalized_idx` and `by_dimension` fields are `Option<HashMap<...>>` stored directly on `TriggerMatrix`, which lives inside `TriggerManager.matrices: HashMap<String, TriggerMatrix>`. The `TriggerManager` is wrapped in `RwLock<TriggerManager>` within `EvidenceGraph`. Therefore:

- **All mutations to `TriggerMatrix` (including index updates via `ensure_indices()`, `rebuild_indices()`, and `add_condition()`) occur under `trigger_manager.write()` lock.**
- **No concurrent mutation of the same `TriggerMatrix` is possible.**
- **Read-only operations (`investigation_priority()`, `staleness()`, `meets_phase30_gate()`) that read indices should also hold at least `trigger_manager.read()` for consistency, but these methods don't access indices directly.**

The plan does NOT require `Arc<RwLock<HashMap>>` for indices because the `TriggerManager` RwLock already provides the synchronization boundary. This is a documented design decision — not an oversight.



    pub config: TriggerConfig,
}
```

**`ensure_indices()` — lazy init** (fixes EP11):

```rust
impl TriggerMatrix {
    /// Ensure indices are populated. Idempotent — no-op if already Some.
    fn ensure_indices(&mut self) {
        if self.normalized_idx.is_some() && self.by_dimension.is_some() {
            return;
        }
        self.rebuild_indices();
    }

    /// Rebuild both indices from scratch by scanning conditions.
    /// Called after deserialization, eviction, or approve_review merge.
    fn rebuild_indices(&mut self) {
        let mut ni: HashMap<String, Vec<usize>> = HashMap::new();
        let mut bd: HashMap<TriggerDimension, Vec<usize>> = HashMap::new();
        for (i, tc) in self.conditions.iter().enumerate() {
            let key = format!(
                "{}:{}",
                dimension_key(tc.dimension),
                hash_prefix(&tc.normalized)
            );
            ni.entry(key).or_default().push(i);
            bd.entry(tc.dimension).or_default().push(i);
        }
        self.normalized_idx = Some(ni);
        self.by_dimension = Some(bd);
    }
}
```

**Three-phase dedup in `add_condition()`**:

```rust
pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
    self.ensure_indices();
    let ni = self.normalized_idx.as_ref().unwrap();
    let bd = self.by_dimension.as_ref().unwrap();

    let hash_key = format!(
        "{}:{}",
        dimension_key(condition.dimension),
        hash_prefix(&condition.normalized)
    );

    // Phase 1: O(1) exact-match via normalized_idx
    if let Some(candidates) = ni.get(&hash_key) {
        for &idx in candidates {
            let existing = &self.conditions[idx];
            if existing.dimension == condition.dimension
                && existing.normalized == condition.normalized
            {
                self.merge_condition(idx, &condition);
                return false;
            }
        }
    }

    // Phase 2: O(k) fuzzy scan over same-dimension bucket
    if let Some(dim_bucket) = bd.get(&condition.dimension) {
        let mut best_borderline: Option<(usize, f64)> = None;
        for &idx in dim_bucket {
            let existing = &self.conditions[idx];
            let sim = compute_similarity(&existing.normalized, &condition.normalized);
            if sim >= crate::spec::DEDUP_JARO_WINKLER_THRESHOLD {
                self.merge_condition(idx, &condition);
                return false;
            }
            if sim >= crate::spec::DEDUP_BORDERLINE_THRESHOLD && best_borderline.is_none() {
                best_borderline = Some((idx, sim));
            }
        }
        if let Some((_existing_idx, sim)) = best_borderline {
            // Borderline: store in review_queue, do NOT add to conditions (fixes KS12)
            self.review_queue.push(ReviewCandidate {
                existing_condition_id: self.conditions[_existing_idx].id.clone(),
                proposed_condition: condition.clone(),
                similarity_score: sim,
                queued_at: Utc::now(),
                status: "pending-review".to_string(),
                resolved_by: None,
                schema_version: crate::spec::default_schema_version(),
            });
            return false;
        }
    }

    // Phase 3: new condition — insert and update indices
    let idx = self.conditions.len();
    self.conditions.push(condition.clone());
    {
        let ni = self.normalized_idx.as_mut().unwrap();
        ni.entry(hash_key).or_default().push(idx);
        let bd = self.by_dimension.as_mut().unwrap();
        bd.entry(condition.dimension).or_default().push(idx);
    }

    self.contributing_layers.extend(condition.contributed_by.iter().copied());
    self.recompute_completeness();
    self.last_updated = Utc::now();

    // Eviction check (H7) with index cleanup
    self.check_eviction(condition.dimension);

    true
}
```

**`merge_condition()` helper**:

```rust
fn merge_condition(&mut self, existing_idx: usize, new: &TriggerCondition) {
    let existing = &mut self.conditions[existing_idx];
    existing.raw_contributions.extend(new.raw_contributions.clone());
    for layer in &new.contributed_by {
        if !existing.contributed_by.contains(layer) {
            existing.contributed_by.push(*layer);
        }
    }
    for tag in &new.tags {
        if !existing.tags.contains(tag) {
            existing.tags.push(tag.clone());
        }
    }
    if new.normalized.len() > existing.canonical_value.len() {
        existing.canonical_value = new.canonical_value.clone();
    }
    self.contributing_layers.extend(new.contributed_by.iter().copied());
    self.dedup_count += 1;
}
```

**`compute_similarity()` helper**:

```rust
fn compute_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    let max_len = crate::spec::DEDUP_MAX_DESCRIPTION_CHARS;
    strsim::jaro_winkler(
        &a[..a.len().min(max_len)],
        &b[..b.len().min(max_len)],
    )
}
```

**Index consistency debug assertion**:

```rust
#[cfg(debug_assertions)]
fn assert_indices_consistent(&self) {
    if let (Some(ni), Some(bd)) = (&self.normalized_idx, &self.by_dimension) {
        let total_in_ni: usize = ni.values().map(|v| v.len()).sum();
        let total_in_bd: usize = bd.values().map(|v| v.len()).sum();
        debug_assert_eq!(total_in_ni, self.conditions.len());
        debug_assert_eq!(total_in_bd, self.conditions.len());
        for indices in ni.values() {
            for &idx in indices {
                debug_assert!(idx < self.conditions.len());
            }
        }
        for indices in bd.values() {
            for &idx in indices {
                debug_assert!(idx < self.conditions.len());
            }
        }
    }
}
```

**Key corrections from original plan**:
- **KS8 fixed**: `hash_key` uses 32-char (128-bit) prefix, eliminating birthday collisions.
- **AW6 fixed**: Uses `dimension_key()` with explicit `&'static str` match, not `{:?}` Debug.
- **EP11 fixed**: `normalized_idx` and `by_dimension` are `Option<HashMap>` with `ensure_indices()`.
- **KS9 fixed**: Eviction calls `rebuild_indices()` which regenerates both maps from scratch.
- **IC6 fixed**: H9 before H12 means `check_equivalence` exists; add_condition uses `compute_similarity` directly.
- **KS11/AW8 fixed**: `full_semantic_hash()` function fully defined with sha2::Sha256.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs`: Add `dimension_key()` function
2. [ ] `bugswarm-evidence/src/trigger.rs`: Add `full_semantic_hash()` function
3. [ ] `bugswarm-evidence/src/trigger.rs`: Add `hash_prefix()` helper
4. [ ] `bugswarm-evidence/src/trigger.rs`: Add `normalized_idx`, `by_dimension` as `Option<HashMap>` fields
5. [ ] `bugswarm-evidence/src/trigger.rs`: Add `ensure_indices()`, `rebuild_indices()`, `merge_condition()`
6. [ ] `bugswarm-evidence/src/trigger.rs`: Update `add_condition()` with 3-phase lookup
7. [ ] `bugswarm-evidence/src/trigger.rs`: Add `assert_indices_consistent()` debug-only invariant
8. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add 6 hash_fast_path tests
9. [ ] `bugswarm-evidence/Cargo.toml`: Add `sha2 = "0.10"` dependency (if not already present)

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | 1000 exact-duplicate inserts | Each O(1), all return false (merged) | PASS / FAIL |
| 2 | 4000 unique conditions | avg insert < 50us | PASS / FAIL |
| 3 | Serialize -> deserialize -> ensure_indices | Indices consistent, correctness preserved | PASS / FAIL |
| 4 | Exact duplicate: 2 conditions with identical normalized | Hash lookup finds match, similarity never called | PASS / FAIL |
| 5 | Fuzzy detection: 2 near-identical (JW >= 0.85) | Falls back to per-dim scan, finds match | PASS / FAIL |
| 6 | Borderline match (JW in [0.75, 0.85)) | review_queue entry created, condition NOT added to conditions | PASS / FAIL |
| 7 | After deserialization, indices are None | ensure_indices() restores them | PASS / FAIL |

---

### H7 — No Stale-Condition Eviction

**Moved after H12** so `rebuild_indices()` is available for post-eviction index cleanup. Uses single-pass `fold` instead of `filter + min_by_key` (fixes AW2). Uses `DateTime<Utc>` comparison, not String clone (fixes KS2). Calls `rebuild_indices()` after eviction (fixes KS9). Adds `vacuum()` for post-restoration cleanup (fixes EP5).

#### Root Cause Analysis

`TriggerMatrix::add_condition()` appends conditions without any cap. Over time, a single dimension accumulates hundreds of unverified conditions, consuming unbounded memory. The plan defines `MAX_CONDITIONS_PER_DIMENSION = 100` with eviction policy: evict oldest-unverified per dimension, never evict verified.

#### Permanent Fix

**`check_eviction()` — single-pass fold + index rebuild** (fixes KS2, AW2, KS9):

```rust
/// Check if any dimension has exceeded MAX_CONDITIONS_PER_DIMENSION,
/// and evict oldest-unverified conditions if so.
/// Returns the number of conditions evicted.
fn check_eviction(&mut self, triggered_dim: TriggerDimension) -> usize {
    let max = crate::spec::MAX_CONDITIONS_PER_DIMENSION;
    let mut evicted = 0;

    let dimensions: Vec<TriggerDimension> = TriggerDimension::all();
    for dim in &dimensions {
        let count = self.conditions.iter()
            .filter(|c| c.dimension == *dim).count();

        if count <= max { continue; }

        let to_evict = count - max;
        for _ in 0..to_evict {
            // Single-pass fold to find oldest-unverified (fixes AW2, KS2)
            let evict_idx_opt: Option<usize> = self.conditions.iter()
                .enumerate()
                .fold(None, |best, (i, c)| {
                    if c.dimension == *dim && !c.verified {
                        match best {
                            None => Some(i),
                            Some(best_i) => {
                                // DateTime<Utc> comparison — no String clone (fixes KS2)
                                if c.contributed_at < self.conditions[best_i].contributed_at {
                                    Some(i)
                                } else {
                                    Some(best_i)
                                }
                            }
                        }
                    } else {
                        best
                    }
                });

            match evict_idx_opt {
                Some(evict_idx) => {
                    self.conditions.remove(evict_idx);
                    evicted += 1;
                    warn!(
                        target: "trigger.condition_evicted",
                        bug_id = %self.bug_id, dimension = ?dim,
                        remaining = self.conditions.len(),
                        "evicted stale condition from dimension {:?}", dim,
                    );
                }
                None => {
                    // All conditions in this dimension are verified — cannot evict
                    warn!(
                        target: "trigger.condition_evicted",
                        bug_id = %self.bug_id, dimension = ?dim,
                        "dimension exceeded MAX but all verified",
                    );
                    break;
                }
            }

            // After each remove, clean up contributing_layers
            let remaining_layers: HashSet<ContributionLayer> = self.conditions
                .iter().flat_map(|c| c.contributed_by.iter().copied()).collect();
            self.contributing_layers.retain(|l| remaining_layers.contains(l));
        }
    }

    // Rebuild indices after all evictions (fixes KS9)
    if evicted > 0 {
        self.rebuild_indices();
        self.recompute_completeness();
    }

    evicted
}
```

**Eviction call in `add_condition()`**:

```rust
// At end of add_condition():
let evicted_count = self.check_eviction(condition.dimension);

#[cfg(debug_assertions)]
{
    for dim in TriggerDimension::all() {
        let count = self.conditions.iter().filter(|c| c.dimension == dim).count();
        debug_assert!(
            count <= crate::spec::MAX_CONDITIONS_PER_DIMENSION + 1,
            "dimension {:?} exceeded cap: {} > {}", dim, count, crate::spec::MAX_CONDITIONS_PER_DIMENSION,
        );
    }
    self.assert_indices_consistent();
}

true
```

**`vacuum()` — bulk reindex + stale count update** (fixes EP5):

```rust
/// Compact and rebuild all internal structures.
/// Call after bulk eviction, restoration, or load.
pub fn vacuum(&mut self) {
    self.rebuild_indices();
    self.recompute_completeness();

    // Compute stale count for metrics
    let now = Utc::now();
    let stale_count = self.conditions.iter()
        .filter(|c| !c.verified)
        .filter(|c| {
            (now - c.contributed_at).num_days() > crate::spec::STALENESS_WINDOW_DAYS as i64
        })
        .count();
    // Wire stale gauge (fixes EP5)
    if let Some(ref m) = self.metrics {
        m.set_stale_count(stale_count as u64);
    }
}
```

**Key corrections from original plan**:
- **KS2 fixed**: `contributed_at` is `DateTime<Utc>`, comparison is `c.contributed_at < self.conditions[best_i].contributed_at` — zero allocations.
- **AW2 fixed**: Single-pass `fold` instead of `filter().min_by_key()`.
- **KS9 fixed**: After eviction loop, `rebuild_indices()` regenerates both index maps from scratch.
- **EP5 fixed**: `vacuum()` wires stale_conditions_gauge update.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/spec.rs`: Add `MAX_CONDITIONS_PER_DIMENSION = 100`
2. [ ] `bugswarm-evidence/src/trigger.rs`: Add `check_eviction()` with single-pass fold + index rebuild
3. [ ] `bugswarm-evidence/src/trigger.rs`: Add eviction call at end of `add_condition()`
4. [ ] `bugswarm-evidence/src/trigger.rs`: Add `vacuum()` method
5. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add 5 eviction tests
6. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `eviction_indices_remain_consistent` (KS9 verification)

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Add 150 unverified conditions to Input dim | Exactly 100 remain, 50 oldest evicted | PASS / FAIL |
| 2 | Add 50 verified + 80 unverified (130 total) | All 50 verified + 50 newest unverified remain | PASS / FAIL |
| 3 | Add 150 verified conditions | No eviction, warning logged | PASS / FAIL |
| 4 | Add 100 to Input, 100 to Env, 100 to Timing | Each dim has 100, no cross-dim eviction | PASS / FAIL |
| 5 | After eviction, `assert_indices_consistent()` passes | No index corruption | PASS / FAIL |
| 6 | debug_assert fires if bug bypasses cap | Test panics in debug mode | PASS / FAIL |

---

## Category B: Data Completeness

### H11 — Missing `schema_version` for Forward Compatibility

**Implemented first** so `default_schema_version()` is available to all subsequent structs (fixes IC10).

All serializable structs lack `schema_version`. Without it, future schema changes cannot be migrated.

#### Permanent Fix

**`default_schema_version()` defined ONCE in `spec.rs`** (fixes IC10). Referenced via `#[serde(default = "crate::spec::default_schema_version")]`.

```rust
// spec.rs
pub fn default_schema_version() -> u32 { 1 }
```

**Add `schema_version` to `TriggerMatrix`** (shown in H12 struct definition above). Add to `TriggerCondition` (H2). Add to all H3 structs.

**Schema migration stub** (fixes MP14):

```rust
impl TriggerMatrix {
    /// Called after deserialization to apply schema upgrades.
    pub fn migrate_schema(&mut self) -> Result<(), String> {
        match self.schema_version {
            1 => Ok(()),
            v if v > 1 => Err(format!(
                "unknown schema_version {} for bug {}. Upgrade tool first.",
                v, self.bug_id,
            )),
            _ => Err(format!(
                "invalid schema_version {} for bug {}", self.schema_version, self.bug_id,
            )),
        }
    }
}
```

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/spec.rs`: Add `pub fn default_schema_version() -> u32 { 1 }`
2. [ ] `bugswarm-evidence/src/trigger.rs`: Add `schema_version: u32` to `TriggerMatrix`
3. [ ] `bugswarm-evidence/src/trigger.rs`: Add `migrate_schema()` method to `TriggerMatrix`
4. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add 3 schema_version tests
5. [ ] `Makefile`: Add `audit-schema-version` CI target

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Old JSON (no schema_version) -> deserialize | schema_version == 1 | PASS / FAIL |
| 2 | Set schema_version=5 -> serialize -> deserialize | schema_version == 5 | PASS / FAIL |
| 3 | schema_version=5 -> migrate_schema() | Err("unknown schema_version 5") | PASS / FAIL |
| 4 | All serializable structs have schema_version field | grep confirms | PASS / FAIL |

---

### H2 — Missing Plan-Required Fields on `TriggerCondition`

#### Root Cause Analysis

Five fields omitted: `canonical_value`, `semantic_hash`, `tags`, `raw_contributions`, `severity_specific: Option<SeveritySpecific>`. Additionally, `contributed_at: String` replaced with `contributed_at: DateTime<Utc>` (fixes EP1, KS2).

#### Permanent Fix

**`SeveritySpecific` struct**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SeveritySpecific {
    pub min_severity: u8,
    pub max_severity: u8,
    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,
}

impl SeveritySpecific {
    pub fn new(min: u8, max: u8) -> Self {
        Self {
            min_severity: min.clamp(1, 10),
            max_severity: max.clamp(min, 10),
            schema_version: crate::spec::default_schema_version(),
        }
    }
    pub fn contains(&self, severity: u8) -> bool {
        severity >= self.min_severity && severity <= self.max_severity
    }
}

impl fmt::Display for SeveritySpecific {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "severity {}-{}", self.min_severity, self.max_severity)
    }
}
```

**Complete revised `TriggerCondition`**:

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
    /// Used for cross-matrix similarity search.
    pub semantic_hash: String,

    /// Arbitrary categorization tags (e.g., ["sql-injection", "cwe-89"]).
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

    /// When this condition was first contributed. DateTime<Utc> — NOT String (fixes EP1, KS2).
    #[serde(with = "chrono::serde::ts_seconds")]
    pub contributed_at: DateTime<Utc>,

    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,
}
```

**Updated `TriggerCondition::new()`**:

```rust
impl TriggerCondition {
    pub fn new(bug_id: &str, dim: TriggerDimension, desc: &str, layer: ContributionLayer) -> Self {
        let normalized = normalize_description(desc);
        let sem_hash = full_semantic_hash(&normalized);
        Self {
            id: format!("tc-{}-{:?}-{}", bug_id, dim, hash_prefix(&normalized)),
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
            contributed_at: Utc::now(),
            schema_version: crate::spec::default_schema_version(),
        }
    }
}
```

**Hash policy** (fixes IC1):
- `hash_prefix(&normalized)` → 32 hex chars (128 bits) — `normalized_idx` HashMap key.
- `semantic_hash` field → `full_semantic_hash(&normalized)` → 64 hex chars (256 bits) — cross-matrix similarity.
- `normalize_hash()` (existing) → 16 chars — backwards-compatible `id` generation (legacy).
- **IC1 fixed**: The difference is explicitly documented here.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs`: Add `SeveritySpecific` struct + impl + Display
2. [ ] `bugswarm-evidence/src/trigger.rs`: Replace `TriggerCondition` with full-field version
3. [ ] `bugswarm-evidence/src/trigger.rs`: Update `TriggerCondition::new()` to populate all fields
4. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add 4 struct_field_tests
5. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `semantic_hash_is_64_chars` (KS11 verification)

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Serialize with all fields -> deserialize | All fields roundtrip correctly | PASS / FAIL |
| 2 | Old-format JSON (missing 5 fields) -> deserialize | Defaults applied, no error | PASS / FAIL |
| 3 | contributed_at is DateTime<Utc> type, not String | Type check passes | PASS / FAIL |
| 4 | semantic_hash.len() == 64 | Full 256-bit hash | PASS / FAIL |
| 5 | Two conditions merged -> raw_contributions grows | Both layers' contributions present | PASS / FAIL |

---

### H3 — Missing Plan-Required Structs

Six plan-required structs must be defined. **`TriggerConfig` is wired into `TriggerManager` and `TriggerMatrix` from the start** (fixes KS15).

#### Permanent Fix

**`RawTriggerCondition`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawTriggerCondition {
    pub raw_description: String,
    pub layer: ContributionLayer,
    pub dimension: TriggerDimension,
    pub bug_id: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub contributed_at: DateTime<Utc>,
    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,
}

impl RawTriggerCondition {
    pub fn from_layer(raw_desc: &str, layer: ContributionLayer, dim: TriggerDimension, bug_id: &str) -> Self {
        Self {
            raw_description: raw_desc.to_string(),
            layer,
            dimension: dim,
            bug_id: bug_id.to_string(),
            contributed_at: Utc::now(),
            schema_version: crate::spec::default_schema_version(),
        }
    }
}
```

**`TriggerConfig` — wired into operations** (fixes KS15):

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
    #[serde(default = "crate::spec::default_schema_version")]
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
            schema_version: crate::spec::default_schema_version(),
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

**`TriggerManager` holds `TriggerConfig`** (fixes KS15, AW3):

```rust
pub struct TriggerManager {
    pub matrices: HashMap<String, TriggerMatrix>,
    pub config: TriggerConfig,
    pub metrics: TriggerMetrics,
}

impl TriggerManager {
    pub fn new(config: TriggerConfig) -> Self {
        Self {
            matrices: HashMap::new(),
            config,
            metrics: TriggerMetrics::new(),
        }
    }

    pub fn get_or_create(&mut self, bug_id: &str) -> &mut TriggerMatrix {
        let cfg = self.config.clone();
        self.matrices.entry(bug_id.to_string())
            .or_insert_with(|| TriggerMatrix::new(bug_id, cfg))
    }
}
```

**`DimensionScore`** with standalone helper (fixes IC12):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionScore {
    pub dimension: TriggerDimension,
    pub condition_count: usize,
    pub weight: f32,
    pub covered: bool,
    pub density: f64,
    pub verified_ratio: f64,
    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,
}

/// Compute dimension scores WITHOUT constructing a full TriggerMatrixResponse.
/// Called by both TriggerMatrixResponse::from_matrix() and
/// DescribeTriggerResponse::from_matrix() to avoid duplicate allocation (fixes IC12).
fn compute_dimension_scores(matrix: &TriggerMatrix) -> Vec<DimensionScore> {
    TriggerDimension::all().iter().map(|dim| {
        let conds: Vec<&TriggerCondition> = matrix.conditions
            .iter().filter(|c| c.dimension == *dim).collect();
        let cnt = conds.len();
        let verified_cnt = conds.iter().filter(|c| c.verified).count();
        DimensionScore {
            dimension: *dim,
            condition_count: cnt,
            weight: dim.weight(),
            covered: cnt > 0,
            density: (cnt as f64 / crate::spec::DENSITY_BONUS_SATURATION as f64).min(1.0),
            verified_ratio: if cnt > 0 { verified_cnt as f64 / cnt as f64 } else { 0.0 },
            schema_version: crate::spec::default_schema_version(),
        }
    }).collect()
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
    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,
}

impl TriggerMatrixResponse {
    pub fn from_matrix(matrix: &TriggerMatrix) -> Self {
        TriggerMatrixResponse {
            bug_id: matrix.bug_id.clone(),
            conditions: matrix.conditions.clone(),
            contributing_layers: matrix.contributing_layers
                .iter().map(|l| l.layer_name().to_string()).collect(),
            dedup_count: matrix.dedup_count,
            completeness_score: matrix.completeness_score,
            last_updated: matrix.last_updated
                .format(crate::spec::TIMESTAMP_FORMAT).to_string(),
            meets_phase30_gate: matrix.meets_phase30_gate(),
            dimension_scores: compute_dimension_scores(matrix),
            schema_version: crate::spec::default_schema_version(),
        }
    }
}
```

**`DescribeTriggerRequest`** and **`DescribeTriggerResponse`** with proper filtering (fixes IC4):

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
    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribeTriggerResponse {
    pub bug_id: String,
    pub conditions: Vec<TriggerCondition>,
    pub total_count: usize,
    pub filtered_count: usize,
    pub completeness_score: f32,
    pub investigation_priority: f64,
    pub dimension_scores: Vec<DimensionScore>,
    pub summary: String,
    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,
}

impl DescribeTriggerResponse {
    /// Build from a matrix, applying filter criteria from the request.
    /// filtered_count reflects actual post-filtering count (fixes IC4).
    pub fn from_matrix(
        matrix: &TriggerMatrix, request: &DescribeTriggerRequest, severity: Option<u8>,
    ) -> Self {
        let mut conditions: Vec<TriggerCondition> = matrix.conditions.clone();

        // Apply filters
        if let Some(ref dim) = request.dimension {
            conditions.retain(|c|
                format!("{:?}", c.dimension).to_lowercase().contains(&dim.to_lowercase()));
        }
        if let Some(ref layer) = request.layer {
            conditions.retain(|c|
                c.layer.layer_name().to_lowercase().contains(&layer.to_lowercase()));
        }
        if request.verified_only {
            conditions.retain(|c| c.verified);
        }

        let total_count = matrix.conditions.len();
        let filtered_count = conditions.len();
        let covered_dims = TriggerDimension::all().iter()
            .filter(|d| matrix.conditions.iter().any(|c| c.dimension == **d)).count();
        let verified_count = matrix.conditions.iter().filter(|c| c.verified).count();
        let summary = format!(
            "Bug {} has {} trigger conditions ({} shown) across {} dimensions ({} layers). \
             Completeness: {:.0}%. {} conditions verified. Priority: {:.2}.",
            matrix.bug_id, total_count, filtered_count, covered_dims,
            matrix.contributing_layers.len(),
            matrix.completeness_score * 100.0, verified_count,
            matrix.investigation_priority(severity),
        );

        DescribeTriggerResponse {
            bug_id: matrix.bug_id.clone(),
            conditions,
            total_count,
            filtered_count,
            completeness_score: matrix.completeness_score,
            investigation_priority: matrix.investigation_priority(severity),
            dimension_scores: compute_dimension_scores(matrix),
            summary,
            schema_version: crate::spec::default_schema_version(),
        }
    }
}
```

**Key corrections from original plan**:
- **KS15 fixed**: `TriggerManager` holds `pub config: TriggerConfig` field. Used by `get_or_create()`.
- **IC4 fixed**: `filtered_count` correctly reflects post-filtering count.
- **IC12 fixed**: `compute_dimension_scores()` is a standalone function.
- **IC10 fixed**: `default_schema_version()` via `crate::spec::default_schema_version`.
- 

**`EdgeKind::Aggregates` semantics** (fixes EP2, IC11):

In this codebase, `EdgeKind::Aggregates` means "the source node aggregates into the target node." Examples:
- `Claim --Aggregates--> ConfirmedBug`: "A claim aggregates evidence to form a confirmed bug."
- `ConfirmedBug --Aggregates--> TriggerMatrix`: "A confirmed bug aggregates its trigger conditions to form a trigger matrix." (Used in H10 migration.)
- `SandboxRun --Triggers--> TriggerMatrix`: "A sandbox run triggers documentation in the matrix." (Alternative to Aggregates for condition-to-matrix links.)

The `Triggers` edge (from `TriggerCondition` nodes to `TriggerMatrix` nodes) is the primary condition-to-matrix linkage. The `Aggregates` edge from `ConfirmedBug` to `TriggerMatrix` is an optional convenience for graph traversal — it allows walking from a bug directly to its trigger documentation without going through SandboxRun nodes. Both are valid; the plan uses `Aggregates` for bug-to-matrix and `Triggers` for condition-to-matrix.



**IC11 documented**: `Aggregates` edge: "source aggregates into target." In migration, `Bug -> Aggregates -> TriggerMatrix` means "Bug aggregates its conditions into TriggerMatrix."

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs`: Add `RawTriggerCondition`, `TriggerConfig`, `DimensionScore`
2. [ ] `bugswarm-evidence/src/trigger.rs`: Add `TriggerManager` with config field
3. [ ] `bugswarm-evidence/src/trigger.rs`: Add `TriggerMatrixResponse`, `DescribeTriggerRequest`, `DescribeTriggerResponse`
4. [ ] `bugswarm-evidence/src/trigger.rs`: Add `compute_dimension_scores()` helper
5. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add 3 struct tests
6. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add `describe_trigger_filtered_count_correct` (IC4 verification)
7. [ ] `bugswarm-evidence/src/daemon.rs`: Add `describe_trigger` RPC handler

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | All 7 structs compile | cargo build passes | PASS / FAIL |
| 2 | TriggerConfig::default() matches spec.rs constants | Defaults = spec values | PASS / FAIL |
| 3 | DescribeTriggerResponse.filtered_count <= total_count | Filtering reduces count | PASS / FAIL |
| 4 | DescribeTriggerResponse.summary is non-empty | Contains bug_id and stats | PASS / FAIL |
| 5 | All structs have schema_version field | grep confirms | PASS / FAIL |
| 6 | TriggerManager::new() accepts TriggerConfig | Config is stored and used | PASS / FAIL |

---

## Category C: Operations

### H5 — No Serialization / Persistence

**Implemented after H7** so `restore_condition()` is available. Uses atomic write (tmp+fsync+rename, fixes AW5). Uses streaming writer (fixes AW4). Entry match pattern avoids borrow-checker (fixes KS10). Validates path (fixes EP6). Per-bug file mode for >10MB (fixes AW6). Lock ordering prevents deadlocks (fixes EP12).

#### Permanent Fix

**`restore_condition()` — insert without pipeline** (fixes KS3, AW7):

```rust
/// Insert a condition during restoration WITHOUT running dedup, eviction,
/// completeness recompute, or metrics. Call vacuum() after all restores.
pub fn restore_condition(&mut self, condition: TriggerCondition) {
    self.conditions.push(condition.clone());
    for l in &condition.contributed_by {
        self.contributing_layers.insert(*l);
    }
    // Indices invalidated — caller must call vacuum() or rebuild_indices() later
}
```

**`save()` with atomic write and size gate** (fixes AW4, AW5, AW6, EP6):

```rust
/// Persist all matrices to disk with atomic write (tmp+fsync+rename).
/// Switches to per-bug files when estimated size > MAX_SERIALIZED_SIZE_BYTES.
pub fn save(&self, path: &Path) -> Result<usize, anyhow::Error> {
    // Validate path is not a directory (fixes EP6)
    if path.exists() && path.is_dir() {
        return Err(anyhow::anyhow!("save path '{}' is a directory, not a file", path.display()));
    }

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let matrices: Vec<&TriggerMatrix> = self.matrices.values().collect();
    let estimated: usize = matrices.iter().map(|m| m.conditions.len() * 512).sum();

    if estimated > crate::spec::MAX_SERIALIZED_SIZE_BYTES {
        return self.save_per_bug(path);
    }

    // Atomic write via temp file (fixes AW5)
    let tmp = path.with_extension("json.tmp");
    {
        let file = std::fs::File::create(&tmp)?;
        let mut writer = std::io::BufWriter::new(file);
        serde_json::to_writer(&mut writer, &matrices)?; // streaming, not string alloc (fixes AW4)
        writer.flush()?;
        writer.get_ref().sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(matrices.len())
}

fn save_per_bug(&self, dir: &Path) -> Result<usize, anyhow::Error> {
    std::fs::create_dir_all(dir)?;
    let mut saved = 0;
    for (bug_id, matrix) in &self.matrices {
        let bug_file = dir.join(format!("{}.json", bug_id));
        let tmp = bug_file.with_extension("json.tmp");
        {
            let file = std::fs::File::create(&tmp)?;
            serde_json::to_writer(file, matrix)?;
        }
        std::fs::rename(&tmp, &bug_file)?;
        saved += 1;
    }
    Ok(saved)
}
```

**`load()` with `restore_condition` and fixed borrow-checker** (fixes KS3, KS10, AW9):

```rust
/// Load matrices from disk. Uses restore_condition() to bypass pipeline.
/// Uses explicit match on Entry to avoid borrow checker errors (fixes KS10).
/// On load failure, attempts recovery from .tmp backup file (fixes AW5).
pub fn load(&mut self, path: &Path) -> Result<usize, anyhow::Error> {
    if !path.exists() { return Ok(0); }

    let raw = match std::fs::read_to_string(path) {
        Ok(r) => r,
        Err(e) => {
            let tmp = path.with_extension("json.tmp");
            if tmp.exists() {
                warn!(error = %e, "failed to read primary, trying backup");
                std::fs::read_to_string(&tmp)?
            } else {
                return Err(e.into());
            }
        }
    };

    if raw.trim().is_empty() { return Ok(0); }

    let matrices: Vec<TriggerMatrix> = serde_json::from_str(&raw)?;
    let mut loaded = 0;

    for mut matrix in matrices {
        matrix.migrate_schema()?;
        let bug_id = matrix.bug_id.clone(); // extract before match (fixes KS10)
        match self.matrices.entry(bug_id) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                let existing = e.get_mut();
                for cond in matrix.conditions {
                    existing.restore_condition(cond); // no pipeline (fixes KS3)
                }
                existing.vacuum(); // rebuild once (fixes AW9)
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                loaded += 1;
                matrix.rebuild_indices();
                matrix.recompute_completeness();
                e.insert(matrix);
            }
        }
    }

    Ok(loaded)
}
```

**`rebuild_trigger_manager()` with deadlock-free locking** (fixes EP12):

```rust
/// Rebuild trigger manager from graph nodes.
/// Phase 1: collect data under nodes.read(), then release.
/// Phase 2: populate under trigger_manager.write(). No nested locks (fixes EP12).
pub fn rebuild_trigger_manager(&self) -> Result<usize, anyhow::Error> {
    // Phase 1: collect under nodes.read()
    let trigger_data: Vec<(String, Vec<TriggerCondition>)> = {
        let nodes = self.nodes.read().map_err(|e| anyhow::anyhow!("lock poisoned: {}", e))?;
        let mut data: Vec<(String, Vec<TriggerCondition>)> = Vec::new();
        for node in nodes.iter() {
            if node.kind != NodeKind::TriggerMatrix { continue; }
            let bug_id = node.label.clone();
            let mut conditions: Vec<TriggerCondition> = Vec::new();
            let incoming = self.edges_to(node.id);
            for (edge, source_id) in &incoming {
                if edge.kind != EdgeKind::Triggers { continue; }
                if let Some(cond_node) = nodes.get(*source_id) {
                    if let Some(tc_json) = cond_node.metadata.get("trigger_condition") {
                        if let Ok(tc) = serde_json::from_str::<TriggerCondition>(tc_json) {
                            conditions.push(tc);
                        }
                    }
                }
            }
            data.push((bug_id, conditions));
        }
        data
    }; // nodes.read() lock released here

    // Phase 2: populate under trigger_manager.write()
    let mut tm = self.trigger_manager.write()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {}", e))?;
    let mut restored = 0;
    for (bug_id, conditions) in trigger_data {
        let matrix = tm.get_or_create(&bug_id);
        for tc in conditions {
            matrix.restore_condition(tc); // no pipeline (fixes KS3)
        }
        matrix.vacuum(); // rebuild once (fixes AW9)
        restored += 1;
    }
    Ok(restored)
}
```

**Graceful shutdown with auto-save** (`daemon.rs`):

```rust
pub async fn run_daemon_with_persistence(
    socket_path: PathBuf, trigger_state_path: PathBuf,
) -> anyhow::Result<()> {
    let graph = Arc::new(EvidenceGraph::new());

    if trigger_state_path.exists() {
        match graph.load_triggers(&trigger_state_path) {
            Ok(n) => info!(count = n, "restored trigger matrices from disk"),
            Err(e) => warn!(error = %e, "failed to restore; starting fresh"),
        }
    }

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

**Key corrections from original plan**:
- **KS3 fixed**: `restore_condition()` inserts raw — no dedup, no eviction, no metrics.
- **KS10 fixed**: Bug ID extracted BEFORE `match` block. `match` on Occupied/Vacant avoids borrow checker conflict.
- **AW9 fixed**: `vacuum()` called once after all restores per matrix, not N times.
- **AW4 fixed**: `serde_json::to_writer()` streaming write.
- **AW5 fixed**: Write to `.tmp`, `sync_all()`, `rename()`. Load tries `.tmp` backup on primary failure.
- **AW6 fixed**: Per-bug JSON files if estimated size > 10 MB.
- **EP6 fixed**: Reject directory paths before attempting write.
- **EP12 fixed**: Lock ordering: `nodes.read()` → released → `trigger_manager.write()`. No nested locks.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs`: Add `restore_condition()` method
2. [ ] `bugswarm-evidence/src/trigger.rs`: Add `save()` with atomic write, `save_per_bug()`
3. [ ] `bugswarm-evidence/src/trigger.rs`: Add `load()` with Entry match + backup recovery
4. [ ] `bugswarm-evidence/src/graph.rs`: Add `rebuild_trigger_manager()` with deadlock-free locking
5. [ ] `bugswarm-evidence/src/graph.rs`: Add `load_triggers()` / `save_triggers()` wrapper methods
6. [ ] `bugswarm-evidence/src/daemon.rs`: Add `run_daemon_with_persistence()` with SIGTERM handler
7. [ ] `bugswarm-evidence/src/main.rs`: Switch to persistence-aware daemon
8. [ ] `bugswarm-evidence/Cargo.toml`: Add `anyhow = "1"`, `tempfile = "3"` (dev)
9. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add 5 persistence tests

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Save 100-bug Manager -> restart -> load | All 100 bugs restored, identical scores | PASS / FAIL |
| 2 | Load nonexistent file | Returns Ok(0), manager empty | PASS / FAIL |
| 3 | Corrupt JSON -> load | Returns Err or recovers from backup | PASS / FAIL |
| 4 | SIGTERM graceful shutdown | Trigger state saved before exit | PASS / FAIL |
| 5 | Graph-backed rebuild from nodes | Manager state matches graph node state | PASS / FAIL |
| 6 | restore_condition: add 150 unverified to Input | No eviction triggered (cap NOT applied) | PASS / FAIL |
| 7 | Save >10MB estimated data | Per-bug directory mode used | PASS / FAIL |

---

### H6 — Zero Observability

**Wire all metrics calls and log templates. Fix gauge semantics (`fetch_max`, fixes AW3/IC9). Wire stale gauge (fixes EP5). Define `MetricsSnapshot` struct (fixes EP10).**

#### Permanent Fix

**`MetricsSnapshot` struct** (fixes EP10):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub trigger_rows_total: u64,
    pub trigger_dedup_total: u64,
    pub completeness_max: f64,
    pub layer_diversity_max: u64,
    pub priority_max: f64,
    pub queries_total: u64,
    pub query_latency_us_avg: u64,
    pub evictions_total: u64,
    pub stale_conditions_current: u64,
    pub matrices_count: usize,
    pub conditions_total: usize,
}
```

**`TriggerMetrics` with `fetch_max` and Relaxed ordering documented** (fixes AW3, IC9, AW4):

```rust
/// Lightweight atomic metrics for trigger operations.
///
/// Thread safety: All counters use `AtomicU64` with `Ordering::Relaxed`.
/// Relaxed ordering is sufficient because:
/// 1. Individual counters are monotonic (updated within the TriggerManager RwLock write guard).
/// 2. Snapshots are approximate-by-design; exact ordering between counters is not guaranteed.
/// 3. Max gauges use `fetch_max` which is lock-free — two racing threads: larger value wins.
pub struct TriggerMetrics {
    pub trigger_rows_total: AtomicU64,
    pub trigger_dedup_total: AtomicU64,
    pub completeness_max: AtomicU64,   // f64 * 10000 as u64, via fetch_max
    pub layer_diversity_max: AtomicU64,
    pub priority_max: AtomicU64,       // f64 * 10000 as u64, via fetch_max
    pub queries_total: AtomicU64,
    pub query_latency_us_sum: AtomicU64,
    pub query_count: AtomicU64,
    pub evictions_total: AtomicU64,
    pub stale_conditions_current: AtomicU64,
}

impl TriggerMetrics {
    pub fn new() -> Self {
        Self {
            trigger_rows_total: AtomicU64::new(0),
            trigger_dedup_total: AtomicU64::new(0),
            completeness_max: AtomicU64::new(0),
            layer_diversity_max: AtomicU64::new(0),
            priority_max: AtomicU64::new(0),
            queries_total: AtomicU64::new(0),
            query_latency_us_sum: AtomicU64::new(0),
            query_count: AtomicU64::new(0),
            evictions_total: AtomicU64::new(0),
            stale_conditions_current: AtomicU64::new(0),
        }
    }

    pub fn inc_trigger_rows(&self) { self.trigger_rows_total.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_dedup(&self) { self.trigger_dedup_total.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_evictions(&self) { self.evictions_total.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_queries(&self) { self.queries_total.fetch_add(1, Ordering::Relaxed); }
    pub fn record_query_latency_us(&self, us: u64) {
        self.query_latency_us_sum.fetch_add(us, Ordering::Relaxed);
        self.query_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Update max completeness using fetch_max (fixes AW3/IC9).
    pub fn update_completeness_max(&self, score: f32) {
        let encoded = (score as f64 * 10000.0) as u64;
        self.completeness_max.fetch_max(encoded, Ordering::Relaxed);
    }
    pub fn update_layer_diversity_max(&self, count: usize) {
        self.layer_diversity_max.fetch_max(count as u64, Ordering::Relaxed);
    }
    pub fn update_priority_max(&self, priority: f64) {
        let encoded = (priority * 10000.0) as u64;
        self.priority_max.fetch_max(encoded, Ordering::Relaxed);
    }
    pub fn set_stale_count(&self, count: u64) {
        self.stale_conditions_current.store(count, Ordering::Relaxed);
    }

    pub fn snapshot(&self, matrices_count: usize, conditions_total: usize) -> MetricsSnapshot {
        let qc = self.query_count.load(Ordering::Relaxed);
        let avg_latency = if qc > 0 {
            self.query_latency_us_sum.load(Ordering::Relaxed) / qc
        } else { 0 };
        MetricsSnapshot {
            trigger_rows_total: self.trigger_rows_total.load(Ordering::Relaxed),
            trigger_dedup_total: self.trigger_dedup_total.load(Ordering::Relaxed),
            completeness_max: self.completeness_max.load(Ordering::Relaxed) as f64 / 10000.0,
            layer_diversity_max: self.layer_diversity_max.load(Ordering::Relaxed),
            priority_max: self.priority_max.load(Ordering::Relaxed) as f64 / 10000.0,
            queries_total: self.queries_total.load(Ordering::Relaxed),
            query_latency_us_avg: avg_latency,
            evictions_total: self.evictions_total.load(Ordering::Relaxed),
            stale_conditions_current: self.stale_conditions_current.load(Ordering::Relaxed),
            matrices_count,
            conditions_total,
        }
    }
}
```

**9 log templates** (integrated at operation points, gated by `config.observability_enabled`):

| Target | Level | Location |
|--------|-------|----------|
| `trigger.condition_added` | info | `add_condition()` after push |
| `trigger.condition_merged` | info | `merge_condition()` |
| `trigger.condition_evicted` | warn | `check_eviction()` |
| `trigger.completeness_recomputed` | debug | `recompute_completeness()` |
| `trigger.matrix_created` | debug | `TriggerMatrix::new()` |
| `trigger.gate_passed` | debug | `meets_phase30_gate()` |
| `trigger.gate_failed` | trace | `meets_phase30_gate()` |
| `trigger.priority_computed` | debug | `investigation_priority()` |
| `trigger.staleness_updated` | trace | `staleness()` |

**Stale gauge wired** (fixes EP5):

```rust
impl TriggerManager {
    /// Recalculate stale condition count across all matrices.
    pub fn update_stale_metrics(&self) {
        let now = Utc::now();
        let stale_window = chrono::Duration::days(crate::spec::STALENESS_WINDOW_DAYS as i64);
        let count: usize = self.matrices.values()
            .flat_map(|m| &m.conditions)
            .filter(|c| !c.verified)
            .filter(|c| (now - c.contributed_at) > stale_window)
            .count();
        self.metrics.set_stale_count(count as u64);
    }
}
```

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/trigger.rs`: Add `MetricsSnapshot` struct
2. [ ] `bugswarm-evidence/src/trigger.rs`: Update `TriggerMetrics` with `fetch_max` and latency avg
3. [ ] `bugswarm-evidence/src/trigger.rs`: Insert 9 `tracing` log events at operation points
4. [ ] `bugswarm-evidence/src/trigger.rs`: Wire metrics calls into each operation
5. [ ] `bugswarm-evidence/src/trigger.rs`: Add `update_stale_metrics()` to TriggerManager
6. [ ] `bugswarm-evidence/src/daemon.rs`: Add `"metrics"` RPC method
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add 5 metrics tests
8. [ ] `Makefile`: Add `audit-log-templates` enforcement target

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Insert new condition -> check logs | trigger.condition_added emitted | PASS / FAIL |
| 2 | Dedup -> metrics | trigger_dedup_total incremented | PASS / FAIL |
| 3 | All 9 log templates grep in trigger.rs | 9 unique target strings | PASS / FAIL |
| 4 | Metrics RPC returns valid MetricsSnapshot | All fields present | PASS / FAIL |
| 5 | fetch_max: completeness 0.8 then 0.3 | completeness_max = 0.8 (not 0.3) | PASS / FAIL |
| 6 | vacuum() updates stale_conditions_current | Non-zero after stale conditions exist | PASS / FAIL |
| 7 | config.observability_enabled = false | No log calls, metrics not incremented | PASS / FAIL |

---

### H9 — No Human Review Queue for Borderline Fuzzy Merges

**Implemented before H12** so `check_equivalence` and `EquivalenceResult` exist before `add_condition`'s Phase 2 uses them (fixes IC6). **Borderline matches are stored ONLY in `review_queue`** — not added to `conditions` (fixes KS12). **`approve_review()` and `reject_review()` fully implemented** (fixes IC5). **All imports fully qualified** (fixes KS5).

#### Permanent Fix

**Tri-state enum and `check_equivalence()`**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EquivalenceResult {
    Exact,         // JW >= 0.85 → auto-merge
    Borderline,    // JW in [0.75, 0.85) → human review
    NotEquivalent, // JW < 0.75 → definitely different
}

impl fmt::Display for EquivalenceResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact => write!(f, "exact"),
            Self::Borderline => write!(f, "borderline"),
            Self::NotEquivalent => write!(f, "not-equivalent"),
        }
    }
}

/// Determine equivalence using Jaro-Winkler similarity.
/// All constants use fully qualified crate::spec:: paths (fixes KS5).
pub fn check_equivalence(a: &str, b: &str) -> EquivalenceResult {
    use crate::spec::{
        DEDUP_JARO_WINKLER_THRESHOLD, DEDUP_BORDERLINE_THRESHOLD,
        DEDUP_MIN_FUZZY_LENGTH, DEDUP_MAX_DESCRIPTION_CHARS,
    };
    if a == b { return EquivalenceResult::Exact; }
    if a.len() < DEDUP_MIN_FUZZY_LENGTH || b.len() < DEDUP_MIN_FUZZY_LENGTH {
        return EquivalenceResult::NotEquivalent;
    }
    let max_chars = DEDUP_MAX_DESCRIPTION_CHARS;
    let sim = compute_similarity(
        &a[..a.len().min(max_chars)],
        &b[..b.len().min(max_chars)],
    );
    if sim >= DEDUP_JARO_WINKLER_THRESHOLD { EquivalenceResult::Exact }
    else if sim >= DEDUP_BORDERLINE_THRESHOLD { EquivalenceResult::Borderline }
    else { EquivalenceResult::NotEquivalent }
}

/// Legacy wrapper — returns true only for Exact matches.
pub fn is_semantically_equivalent(a: &str, b: &str) -> bool {
    check_equivalence(a, b) == EquivalenceResult::Exact
}
```

**`ReviewCandidate` with `DateTime<Utc>`** (fixes EP8):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewCandidate {
    pub existing_condition_id: String,
    pub proposed_condition: TriggerCondition,
    pub similarity_score: f64,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub queued_at: DateTime<Utc>,
    pub status: String,  // "pending-review" | "approved" | "rejected"
    pub resolved_by: Option<String>,
    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,
}
```

**`TriggerError` enum** (fixes MP12):

```rust
#[derive(Debug, thiserror::Error)]
pub enum TriggerError {
    #[error("review index {0} not found")]
    ReviewNotFound(usize),
    #[error("review {0} already resolved")]
    ReviewAlreadyResolved(usize),
    #[error("persistence error: {0}")]
    Persistence(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("schema version {0} unknown")]
    UnknownSchemaVersion(u32),
}
```

**`approve_review()` — fully implemented** (fixes IC5, KS12):

```rust
/// Approve a pending review: merge the proposed condition into the existing one.
/// The proposed condition is NOT in conditions — it's only in review_queue (fixes KS12).
pub fn approve_review(
    &mut self, review_index: usize, resolved_by: Option<&str>,
) -> Result<usize, TriggerError> {
    let review = self.review_queue.get_mut(review_index)
        .ok_or(TriggerError::ReviewNotFound(review_index))?;
    if review.status != "pending-review" {
        return Err(TriggerError::ReviewAlreadyResolved(review_index));
    }

    review.status = "approved".to_string();
    review.resolved_by = resolved_by.map(|s| s.to_string());

    let proposed = review.proposed_condition.clone();
    let existing_id = review.existing_condition_id.clone();

    if let Some(merge_idx) = self.conditions.iter().position(|c| c.id == existing_id) {
        // Merge raw_contributions
        self.conditions[merge_idx].raw_contributions
            .extend(proposed.raw_contributions.clone());
        // Merge contributed_by layers
        for layer in &proposed.contributed_by {
            if !self.conditions[merge_idx].contributed_by.contains(layer) {
                self.conditions[merge_idx].contributed_by.push(*layer);
            }
        }
        // Update canonical_value to longer normalized form
        if proposed.normalized.len() > self.conditions[merge_idx].canonical_value.len() {
            self.conditions[merge_idx].canonical_value = proposed.canonical_value.clone();
        }
        // Merge tags
        for tag in &proposed.tags {
            if !self.conditions[merge_idx].tags.contains(tag) {
                self.conditions[merge_idx].tags.push(tag.clone());
            }
        }
        // Mark as needing re-verification if new layers added
        if !proposed.contributed_by.iter().all(|l| self.conditions[merge_idx].contributed_by.contains(l)) {
            self.conditions[merge_idx].verified = false;
        }
    }

    self.rebuild_indices();
    self.recompute_completeness();
    self.last_updated = Utc::now();

    info!(target: "trigger.condition_merged", bug_id = %self.bug_id,
          condition_id = %existing_id, review_index = review_index,
          "review approved: merged conditions");
    Ok(review_index)
}
```

**`reject_review()` — fully implemented** (fixes IC5, KS12):

```rust
/// Reject a pending review: add the proposed condition as an independent entry.
pub fn reject_review(
    &mut self, review_index: usize, resolved_by: Option<&str>,
) -> Result<usize, TriggerError> {
    let review = self.review_queue.get_mut(review_index)
        .ok_or(TriggerError::ReviewNotFound(review_index))?;
    if review.status != "pending-review" {
        return Err(TriggerError::ReviewAlreadyResolved(review_index));
    }

    review.status = "rejected".to_string();
    review.resolved_by = resolved_by.map(|s| s.to_string());

    // Add proposed condition as a real, independent condition
    let proposed = review.proposed_condition.clone();
    let idx = self.conditions.len();
    self.conditions.push(proposed);

    // Update indices
    self.ensure_indices();
    if let Some(ref mut ni) = self.normalized_idx {
        let key = format!("{}:{}",
            dimension_key(self.conditions[idx].dimension),
            hash_prefix(&self.conditions[idx].normalized));
        ni.entry(key).or_default().push(idx);
    }
    if let Some(ref mut bd) = self.by_dimension {
        bd.entry(self.conditions[idx].dimension).or_default().push(idx);
    }

    let new_layers: Vec<ContributionLayer> = self.conditions[idx]
        .contributed_by.iter().copied().collect();
    self.contributing_layers.extend(new_layers);
    self.recompute_completeness();
    self.last_updated = Utc::now();

    info!(target: "trigger.condition_added", bug_id = %self.bug_id,
          condition_id = %self.conditions[idx].id,
          "review rejected: added proposed condition as independent entry");
    Ok(review_index)
}
```

**`pending_reviews()` on `TriggerManager`**:

```rust
pub fn pending_reviews(&self) -> Vec<(&str, &ReviewCandidate)> {
    self.matrices.iter()
        .flat_map(|(bug_id, matrix)| {
            matrix.review_queue.iter()
                .filter(|r| r.status == "pending-review")
                .map(move |r| (bug_id.as_str(), r))
        })
        .collect()
}
```

**Key corrections from original plan**:
- **KS5 fixed**: All constants use `crate::spec::` prefix in `check_equivalence()`.
- **KS12 fixed**: Borderline matches stored ONLY in `review_queue`. NOT added to `conditions`.
- **IC5 fixed**: `approve_review()` and `reject_review()` fully implemented with merge/index logic.
- **IC6 fixed**: H9 before H12 so `check_equivalence` exists.
- **EP8 fixed**: `ReviewCandidate.queued_at` is `DateTime<Utc>`.
- **MP12 fixed**: `TriggerError` enum with thiserror used instead of `Result<(), String>`.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/spec.rs`: Add `DEDUP_BORDERLINE_THRESHOLD`
2. [ ] `bugswarm-evidence/src/trigger.rs`: Add `EquivalenceResult` enum + Display
3. [ ] `bugswarm-evidence/src/trigger.rs`: Add `check_equivalence()` with qualified constants
4. [ ] `bugswarm-evidence/src/trigger.rs`: Add `TriggerError` enum (thiserror)
5. [ ] `bugswarm-evidence/src/trigger.rs`: Add `ReviewCandidate` struct
6. [ ] `bugswarm-evidence/src/trigger.rs`: Add `approve_review()` with full merge logic
7. [ ] `bugswarm-evidence/src/trigger.rs`: Add `reject_review()` with full add logic
8. [ ] `bugswarm-evidence/src/trigger.rs`: Add `pending_reviews()` to TriggerManager
9. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add 7 review_queue tests
10. [ ] `bugswarm-evidence/Cargo.toml`: Add `thiserror = "1"` dependency

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | identical strings | Exact | PASS / FAIL |
| 2 | JW >= 0.85 | Exact | PASS / FAIL |
| 3 | JW in [0.75, 0.85) | Borderline | PASS / FAIL |
| 4 | JW < 0.75 | NotEquivalent | PASS / FAIL |
| 5 | Borderline add -> review_queue grows, conditions unchanged | fixes KS12 | PASS / FAIL |
| 6 | Approve review -> contributions merged | Existing gets both layers, review resolved | PASS / FAIL |
| 7 | Reject review -> independent condition added | 1 new condition, review resolved | PASS / FAIL |
| 8 | Legacy is_semantically_equivalent | Exact=true, Borderline=false | PASS / FAIL |
| 9 | Approve already-resolved review | Err(ReviewAlreadyResolved) | PASS / FAIL |

---

### H10 — No Backfill Migration from Existing Bug Nodes

**Implemented LAST** after all TriggerManager APIs stable. Uses journal-based transactional approach (fixes EP4). Queries INCOMING Confirms edges (fixes KS14). Uses `impl Fn` (fixes EP3). Does NOT call `graph.add_trigger_condition()` (fixes KS13). Uses single lock acquisition (fixes EP12, AW8). Supports --dry-run, checkpoint resume, rollback (fixes EP4, EP7). Manager populated by `rebuild_trigger_manager()` (fixes KS13).

#### Permanent Fix

**`MigrationJournalEntry` and `MigrationResult`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationJournalEntry {
    pub bug_label: String,
    pub bug_node_id: NodeId,
    pub status: String, // "started" | "completed" | "failed"
    pub error: Option<String>,
    pub conditions_extracted: usize,
    pub matrix_node_id: Option<NodeId>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub bugs_scanned: usize,
    pub bugs_skipped: usize,
    pub matrices_created: usize,
    pub conditions_extracted: usize,
    pub errors: Vec<String>,
    pub journal_path: Option<PathBuf>,
}
```

**`migrate_existing_bugs()` — with journal, impl Fn, edges_to, no double nodes** (fixes EP3, EP4, KS14, KS13, EP12, AW8):

```rust
/// Migrate existing ConfirmedBug nodes to have TriggerMatrix + TriggerCondition nodes.
/// Uses journal for checkpoint resume (fixes EP4).
/// Uses impl Fn for extractors that can capture state (fixes EP3).
/// Reads incoming Confirms edges TO bug node (fixes KS14).
/// Does NOT call graph.add_trigger_condition() (fixes KS13).
/// Single lock acquisition for collection phase (fixes EP12, AW8).
pub fn migrate_existing_bugs<F>(
    graph: &EvidenceGraph,
    extract_condition: F,
    journal_path: Option<PathBuf>,
    dry_run: bool,
) -> MigrationResult
where F: Fn(&serde_json::Value) -> Option<trigger::TriggerCondition>,
{
    let mut result = MigrationResult {
        bugs_scanned: 0, bugs_skipped: 0, matrices_created: 0,
        conditions_extracted: 0, errors: vec![],
        journal_path: journal_path.clone(),
    };

    // Read journal for resume
    let completed_bugs: HashSet<String> = if let Some(ref jp) = journal_path {
        read_completed_bugs_from_journal(jp)
    } else { HashSet::new() };

    // Phase 1: Collect ALL data under SINGLE read lock (fixes AW8)
    let confirmed_bugs: Vec<(NodeId, String, Vec<(NodeId, String)>)> = {
        let nodes = graph.nodes.read().unwrap();
        nodes.iter()
            .filter(|n| n.kind == NodeKind::ConfirmedBug)
            .filter(|n| !completed_bugs.contains(&n.label))
            .map(|n| {
                let has_matrix = nodes.iter().any(|m|
                    m.kind == NodeKind::TriggerMatrix && m.label == n.label);
                if has_matrix { result.bugs_skipped += 1; }

                // Confirms edges go FROM SandboxRun TO ConfirmedBug (fixes KS14)
                let receipts: Vec<(NodeId, String)> = {
                    let incoming = graph.edges_to(n.id);
                    incoming.iter().filter_map(|(e, source_id)| {
                        if e.kind == EdgeKind::Confirms {
                            nodes.get(*source_id).and_then(|src_node| {
                                if src_node.kind == NodeKind::SandboxRun {
                                    src_node.metadata.get("receipt")
                                        .map(|r| (*source_id, r.clone()))
                                } else { None }
                            })
                        } else { None }
                    }).collect()
                };
                (n.id, n.label.clone(), receipts)
            })
            .filter(|(_, _, receipts)| !receipts.is_empty())
            .collect()
    }; // lock released

    info!(total_bugs = confirmed_bugs.len(), skipped = result.bugs_skipped,
          "starting backfill migration (dry_run={})", dry_run);

    // Phase 2: Process each bug
    for (i, (bug_node_id, bug_label, sandbox_data)) in confirmed_bugs.iter().enumerate() {
        {
            let nodes = graph.nodes.read().unwrap();
            if nodes.iter().any(|n| n.kind == NodeKind::TriggerMatrix && n.label == *bug_label) {
                result.bugs_skipped += 1; continue;
            }
        }

        if dry_run {
            result.matrices_created += 1;
            for (_, receipt_json) in sandbox_data {
                if let Ok(receipt) = serde_json::from_str::<serde_json::Value>(receipt_json) {
                    if extract_condition(&receipt).is_some() {
                        result.conditions_extracted += 1;
                    }
                }
            }
            result.bugs_scanned += 1; continue;
        }

        result.bugs_scanned += 1;

        // Create TriggerMatrix node (NOT via add_condition — fixes KS13)
        let matrix_node = EvidenceNode::new(0, NodeKind::TriggerMatrix, bug_label, "migration-backfill");
        let matrix_node_id = graph.add_node(matrix_node);
        result.matrices_created += 1;

        // Link Bug -> Aggregates -> TriggerMatrix
        graph.add_edge(EvidenceEdge::new(EdgeKind::Aggregates, *bug_node_id, matrix_node_id, 1.0));

        // Extract conditions from SandboxRun receipts — create nodes/edges directly
        let mut bug_conditions_extracted = 0;
        for (run_id, receipt_json) in sandbox_data {
            match serde_json::from_str::<serde_json::Value>(receipt_json) {
                Ok(receipt) => {
                    if let Some(tc) = extract_condition(&receipt) {
                        let mut cond_node = EvidenceNode::new(0, NodeKind::TriggerCondition,
                            &format!("{}-{:?}", bug_label, tc.dimension), "migration-backfill");
                        cond_node.metadata.insert("trigger_condition".to_string(),
                            serde_json::to_string(&tc).unwrap_or_default());
                        cond_node.metadata.insert("source_run_id".to_string(), run_id.to_string());
                        let cond_node_id = graph.add_node(cond_node);
                        graph.add_edge(EvidenceEdge::new(EdgeKind::Triggers, cond_node_id, matrix_node_id, 1.0));
                        bug_conditions_extracted += 1;
                    }
                }
                Err(e) => {
                    result.errors.push(format!("receipt parse error run {} (bug {}): {}", run_id, bug_label, e));
                }
            }
        }
        result.conditions_extracted += bug_conditions_extracted;

        // Journal checkpoint
        if let Some(ref jp) = journal_path {
            if let Err(e) = append_journal_entry(jp, &MigrationJournalEntry {
                bug_label: bug_label.clone(), bug_node_id: *bug_node_id,
                status: "completed".to_string(), error: None,
                conditions_extracted: bug_conditions_extracted,
                matrix_node_id: Some(matrix_node_id),
                timestamp: Utc::now(),
            }) {
                result.errors.push(format!("journal write error for {}: {}", bug_label, e));
            }
        }

        if result.bugs_scanned % 100 == 0 {
            info!(checkpoint = result.bugs_scanned, total = confirmed_bugs.len(),
                  "migration checkpoint: {}/{} bugs", result.bugs_scanned, confirmed_bugs.len());
        }
    }

    // After migration: rebuild TriggerManager from graph (fixes KS13)
    if !dry_run {
        match graph.rebuild_trigger_manager() {
            Ok(restored) => info!(matrices = restored, "trigger manager rebuilt from migration"),
            Err(e) => result.errors.push(format!("rebuild_trigger_manager failed: {}", e)),
        }
    }

    info!(bugs_scanned = result.bugs_scanned, bugs_skipped = result.bugs_skipped,
          matrices = result.matrices_created, conditions = result.conditions_extracted,
          errors = result.errors.len(), "backfill migration complete");
    result
}
```

**Journal helpers and default extractor**:

```rust
fn read_completed_bugs_from_journal(path: &Path) -> HashSet<String> {
    if !path.exists() { return HashSet::new(); }
    let content = match std::fs::read_to_string(path) { Ok(c) => c, Err(_) => return HashSet::new() };
    content.lines()
        .filter_map(|line| serde_json::from_str::<MigrationJournalEntry>(line).ok())
        .filter(|entry| entry.status == "completed")
        .map(|entry| entry.bug_label)
        .collect()
}

fn append_journal_entry(path: &Path, entry: &MigrationJournalEntry) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(entry)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("serialize error: {}", e)))?;
    writeln!(file, "{}", line)?;
    file.sync_all()?;
    Ok(())
}

pub fn default_extract_condition(receipt: &serde_json::Value) -> Option<trigger::TriggerCondition> {
    let raw_desc = receipt.get("trigger_condition")
        .or_else(|| receipt.get("input_hint"))
        .or_else(|| receipt.get("trigger_hint"))
        .or_else(|| receipt.get("input"))
        .and_then(|v| v.as_str()).unwrap_or("");
    if raw_desc.is_empty() { return None; }
    let dimension = receipt.get("dimension").and_then(|v| v.as_str())
        .map(parse_dimension_str).unwrap_or(TriggerDimension::Input);
    let bug_id = receipt.get("bug_id").and_then(|v| v.as_str())
        .unwrap_or("unknown-migrated-bug");
    Some(TriggerCondition::new(bug_id, dimension, raw_desc, ContributionLayer::Manual))
}

fn parse_dimension_str(s: &str) -> TriggerDimension {
    match s.to_lowercase().as_str() {
        "input" => TriggerDimension::Input,
        "environment" | "env" => TriggerDimension::Environment,
        "timing" => TriggerDimension::Timing,
        "datastate" | "data_state" => TriggerDimension::DataState,
        "concurrency" => TriggerDimension::Concurrency,
        "configuration" | "config" => TriggerDimension::Configuration,
        "dependency" | "dependencyversion" | "depver" => TriggerDimension::DependencyVersion,
        "os" | "arch" | "osarch" => TriggerDimension::OsArch,
        _ => TriggerDimension::Input,
    }
}
```

**`rollback_migration()`** (fixes EP7):

```rust
pub fn rollback_migration(graph: &EvidenceGraph, journal_path: &Path) -> Result<usize, anyhow::Error> {
    let entries: Vec<MigrationJournalEntry> = if journal_path.exists() {
        let content = std::fs::read_to_string(journal_path)?;
        content.lines()
            .filter_map(|line| serde_json::from_str::<MigrationJournalEntry>(line).ok())
            .filter(|e| e.status == "completed").collect()
    } else { return Ok(0); };

    // Clear in-memory manager
    { graph.trigger_manager.write().unwrap().matrices.clear(); }

    // Mark migration-created nodes
    let mut nodes = graph.nodes.write().unwrap();
    let mut rolled_back = 0;
    for node in nodes.iter_mut() {
        if node.author == "migration-backfill" && node.metadata.get("rollback").is_none() {
            node.metadata.insert("rollback".to_string(), "true".to_string());
            rolled_back += 1;
        }
    }

    std::fs::remove_file(journal_path)?;
    info!(rolled_back = rolled_back, entries = entries.len(), "migration rolled back");
    Ok(rolled_back)
}
```

**Key corrections from original plan**:
- **KS14 fixed**: `edges_to(bug_node_id)` for Confirms edges (incoming to bug), not `edges_from`.
- **KS13 fixed**: Migration creates graph nodes/edges directly. Manager populated by `rebuild_trigger_manager()`.
- **EP3 fixed**: `impl Fn(&serde_json::Value) -> Option<...>` instead of `fn()` pointer.
- **EP4 fixed**: Journal-based migration with checkpoint resume.
- **EP7 fixed**: Rollback clears TriggerManager AND marks nodes in graph.
- **EP12/AW8 fixed**: Single `nodes.read()` collects all data; released before write operations.
- **EP13 fixed**: Manager populated AFTER all graph nodes created, via `rebuild_trigger_manager()`.

#### Implementation Checklist

1. [ ] `bugswarm-evidence/src/graph.rs`: Add `MigrationJournalEntry`, `MigrationResult` structs
2. [ ] `bugswarm-evidence/src/graph.rs`: Add `migrate_existing_bugs()` with journal support
3. [ ] `bugswarm-evidence/src/graph.rs`: Add journal helpers and `default_extract_condition()`
4. [ ] `bugswarm-evidence/src/graph.rs`: Add `rollback_migration()` function
5. [ ] `bugswarm-evidence/src/main.rs`: Add `migrate` CLI subcommand with `--dry-run` and `--journal` flags
6. [ ] `bugswarm-evidence/src/main.rs`: Add `rollback` CLI subcommand
7. [ ] `bugswarm-evidence/tests/trigger_audit.rs`: Add 7 migration tests

#### Success Criteria

| # | Test | Expected Result | Binary |
|---|------|----------------|--------|
| 1 | Graph with 5 ConfirmedBugs, 0 TriggerMatrices -> migrate | 5 matrices created | PASS / FAIL |
| 2 | Migration idempotent (run twice) | Second run creates 0 matrices | PASS / FAIL |
| 3 | ConfirmedBug with SandboxRun receipt -> migrate | TriggerCondition extracted from receipt | PASS / FAIL |
| 4 | Dry-run mode | No nodes created, report shows expected counts | PASS / FAIL |
| 5 | Interrupt at 500/1000 bugs -> resume | Remaining 500 migrated from journal checkpoint | PASS / FAIL |
| 6 | Rollback after migration | TriggerManager cleared, nodes marked rollback=true, journal deleted | PASS / FAIL |
| 7 | Confirms edges correctly traced (KS14) | Receipts from SandboxRun TO ConfirmedBug | PASS / FAIL |

---

## Summary Table

| # | Category | Finding | Effort (hrs) | Depends on | Key File(s) |
|---|----------|---------|-------------|------------|-------------|
| H11 | Data | schema_version + default_schema_version in spec.rs | 1 | -- | spec.rs, trigger.rs |
| H2 | Data | Missing fields on TriggerCondition (DateTime<Utc>) | 3 | H11, H12 | trigger.rs:TriggerCondition |
| H3 | Data | Missing plan structs (TriggerConfig wired) | 4 | H2, H11 | trigger.rs:structs |
| H1 | Algorithm | Density + layer bonuses (per-dim, flat floor) | 2 | -- | spec.rs, trigger.rs |
| H4+H8 | Algorithm | investigation_priority + staleness (spec constants) | 2 | H1 | spec.rs, trigger.rs |
| H9 | Algorithm | Tri-state dedup + review queue (full approve/reject) | 2 | -- | spec.rs, trigger.rs |
| H12 | Algorithm | O(1) hash fast path (32-char, lazy Option<HashMap>) | 2 | H9 | trigger.rs:add_condition+indices |
| H7 | Algorithm | Stale eviction (fold, DateTime<Utc>, index rebuild) | 2 | H12 | trigger.rs:check_eviction |
| H5 | Ops | Persistence (atomic, restore_condition, per-bug files) | 3 | H7 | trigger.rs:save/load, graph.rs, daemon.rs |
| H6 | Ops | Observability (fetch_max, MetricsSnapshot, stale wired) | 2 | H4+H8, H5 | trigger.rs:TriggerMetrics, daemon.rs |
| H10 | Ops | Backfill migration (journal, edges_to, impl Fn) | 4 | H5 | graph.rs:migrate_existing_bugs |

**Revised implementation order**: H11 → H2 → H3 → H1 → H4+H8 → H9 → H12 → H7 → H5 → H6 → H10

**Total estimated effort**: ~27 hrs (core) + 3 hrs (practices/benchmarks) = ~30 hrs

---

## Appendix A: Immunity Mechanism Reference

| Layer | Mechanism | Enforced by | Scope |
|-------|-----------|------------|-------|
| 1 | Property-based tests (proptest) | CI: cargo test | Algorithm correctness (H1, H4+H8, H7) |
| 2 | Spec constants enforcement (no magic numbers) | CI: grep audit | All threshold values (H1, H7, H8, H9) |
| 3 | Regression freeze (known-answer tests) | CI: cargo test | All scoring functions (H1, H4+H8) |
| 4 | Compile-time traits (Prioritizable) | CI: grep for bare .sort() | Priority ordering (H4+H8) |
| 5 | debug_assert! invariants | CI: cargo test (debug mode) | Eviction cap (H7), index consistency (H12) |
| 6 | Serde backward compatibility tests | CI: cargo test | All structs (H2, H3, H11) |
| 7 | Schema version presence check | CI: grep struct definitions | Forward compatibility (H11) |
| 8 | Persistence roundtrip tests | CI: cargo test | Save/load correctness (H5) |
| 9 | Log template CI check | CI: grep for target strings | Observability (H6) |
| 10 | Migration idempotency tests | CI: cargo test | Backfill correctness (H10) |
| 11 | Static time injection (no Utc::now() in tests) | CI: grep for Utc::now in tests/ | Deterministic testing (H4+H8) |
| 12 | Index integrity assertions | Debug-only: assert_indices_consistent() | Hash map correctness (H12) |
| 13 | Coverage gate (MP11) | CI: cargo tarpaulin | 95%+ line coverage on trigger.rs |
| 14 | Benchmark gate (MP12) | CI: cargo bench | All operations within spec budget |
| 15 | Golden snapshot test (MP10) | CI: cargo test | Known matrix config frozen as JSON fixture |

---

## Appendix B: Acceptance Test Suite Outline

```
tests/trigger_audit.rs (extension — 65+ tests)
├── completeness_tests (6 tests)
│   ├── completeness_density_per_dimension             (KS1 verification)
│   ├── completeness_flat_min_floor                    (KS7 verification)
│   ├── completeness_layer_bonus_basic
│   ├── completeness_known_configurations_regression
│   ├── completeness_empty_matrix_returns_zero
│   └── completeness_score_always_bounded (proptest)
├── priority_tests (7 tests)
│   ├── priority_weights_sum_to_one                    (KS4 verification)
│   ├── investigation_priority_severity_10_zero_completeness
│   ├── investigation_priority_severity_1_full_completeness
│   ├── investigation_priority_default_severity
│   ├── investigation_priority_with_staleness_max
│   ├── investigation_priority_spec_constants_used     (KS4 verification)
│   └── prioritizable_trait_compiles                   (KS16 verification)
├── staleness_tests (7 tests)
│   ├── staleness_fresh_returns_zero
│   ├── staleness_30_days_returns_one
│   ├── staleness_15_days_interpolated
│   ├── staleness_clamped_at_one
│   ├── staleness_future_timestamp_returns_zero
│   ├── staleness_date_time_utc_type_check             (EP1 verification)
│   └── staleness_never_panics_on_edge_cases (proptest)
├── eviction_tests (6 tests)
│   ├── eviction_dimension_cap_100
│   ├── eviction_verified_protected
│   ├── eviction_indices_remain_consistent             (KS9 verification)
│   ├── eviction_interleaved_dimensions
│   ├── eviction_debug_assert_triggers
│   └── eviction_single_pass_no_clone                  (AW2 verification)
├── hash_fast_path_tests (6 tests)
│   ├── hash_fast_path_o1_exact_match
│   ├── hash_fast_path_performance_4000_conditions
│   ├── hash_index_rebuild_roundtrip
│   ├── hash_index_lazy_init_after_deserialization     (EP11 verification)
│   ├── hash_index_consistency_after_eviction
│   └── hash_key_stable_under_refactor                 (AW6 verification)
├── struct_field_tests (7 tests)
│   ├── trigger_condition_forward_compatible_deserialization
│   ├── trigger_condition_all_14_fields_present
│   ├── trigger_condition_contributed_at_is_date_time   (EP1 verification)
│   ├── semantic_hash_is_64_chars                      (KS11 verification)
│   ├── all_plan_structs_serialize_completely
│   ├── severity_specific_range_clamp
│   └── trigger_config_defaults_match_spec             (KS15 verification)
├── schema_version_tests (3 tests)
│   ├── schema_version_defaults_on_old_json
│   ├── schema_version_survives_roundtrip
│   └── schema_migration_rejects_unknown_version        (MP14 verification)
├── persistence_tests (7 tests)
│   ├── persistence_save_load_roundtrip
│   ├── persistence_atomic_write_no_corruption          (AW5 verification)
│   ├── persistence_load_backup_on_primary_failure
│   ├── persistence_load_nonexistent_file_ok_zero
│   ├── persistence_per_bug_directory_over_10mb         (AW6 verification)
│   ├── persistence_save_rejects_directory_path         (EP6 verification)
│   └── restore_condition_does_not_trigger_eviction     (KS3 verification)
├── metrics_tests (5 tests)
│   ├── metrics_increment_on_operations
│   ├── metrics_fetch_max_correctness                  (AW3/IC9 verification)
│   ├── metrics_snapshot_has_all_fields
│   ├── stale_gauge_updates_on_vacuum                  (EP5 verification)
│   └── metrics_observability_disabled_respected        (KS15 verification)
├── review_queue_tests (8 tests)
│   ├── equivalence_tri_state_exact_boundary
│   ├── equivalence_tri_state_borderline_boundary
│   ├── borderline_creates_review_queue_entry_not_condition  (KS12 verification)
│   ├── approve_review_merges_contributions              (IC5 verification)
│   ├── reject_review_adds_independent_condition
│   ├── approve_already_resolved_returns_error
│   ├── reject_already_resolved_returns_error
│   └── check_equivalence_uses_qualified_constants      (KS5 verification)
├── migration_tests (7 tests)
│   ├── migration_is_idempotent
│   ├── migration_creates_trigger_matrix
│   ├── migration_empty_graph_noop
│   ├── migration_journal_resume                        (EP4 verification)
│   ├── migration_rollback_clears_all                   (EP7 verification)
│   ├── migration_confirms_edges_incoming               (KS14 verification)
│   └── migration_no_double_nodes                       (KS13 verification)
├── concurrency_tests (2 tests)
│   ├── deadlock_free_rebuild_trigger_manager           (EP12 verification)
│   └── single_lock_acquisition_load                    (AW8 verification)
└── integration_tests (2 tests)
    ├── full_lifecycle_add_dedup_evict_persist_reload   (MP8 verification)
    └── golden_snapshot_regression_test                 (MP10 verification)
```

---

## Appendix C: Implementation Practices (MP1–MP18)

All 18 missing practices from the critique are addressed as mandatory implementation gates.

### MP1: Test-First Development
**All 65+ tests from Appendix B are written and committed, FAILING, BEFORE any implementation code.** Red-green-refactor cycle per finding. Each finding should have its tests pass at commit time. Partial progress OK only within a single finding's commit.

### MP2: Incremental Commit Strategy
Each of the 12 findings (H1–H12, with H4+H8 counted as one) is a separate git commit. Commit message format: `fix(H{n}): <brief description>`. Every commit passes `cargo test --no-fail-fast`. No commit touches more than one finding.

### MP3: Code Review Gate
Every commit is reviewed against this plan by a second engineer. Review checklist: (1) all KS/AW/IC/EP fixes present, (2) all tests pass, (3) code matches plan code blocks, (4) no unqualified constants or magic numbers.

### MP4: Performance Benchmarking Baseline
Before H12: benchmark `add_condition()` with 0, 100, 700, 7000 conditions. After H12: same benchmark. Assert >=10x speedup for exact duplicates (O(1) vs O(n)). Document baseline in `benches/trigger_bench.rs` using `criterion`.

### MP5: Edge-Case Enumeration
Before implementing each algorithm, enumerate boundary cases:

| Algorithm | Edge Cases |
|-----------|-----------|
| H1 completeness | 0 conditions, 1 condition, all 7 covered × 1 cond, 1 covered × 100 conds, 8 dims all maxed, EXCLUDED_DIMENSION present/absent |
| H4+H8 priority | severity=0, severity=255 (clamped), severity=None, completeness=NaN (should not happen) |
| H7 eviction | dimension at exactly MAX, MAX+1, MAX+100, all verified, all unverified, interleaved dims |
| H8 staleness | last_updated=Utc::now(), 30 days ago, 60 days ago, future date, epoch 0 |
| H9 tri-state | JW exactly 0.75, exactly 0.85, identical strings, empty strings, single char |
| H12 hash path | hash collision on 32-char prefix, index after eviction, rebuild after deserialization |

### MP6: Differential Testing
- Jaro-Winkler from `strsim` crate verified against Python `jellyfish` for 100+ known inputs.
- `full_semantic_hash` output verified against `sha256sum <(echo -n "test")` in bash. Save known hashes as test fixtures.

### MP7: Fuzzing
All public APIs fuzzed via `cargo-fuzz` (`cargo fuzz add trigger_fuzz`):
- `add_condition()`: random dimensions, layers, descriptions, unicode, nulls.
- `check_equivalence()`: random string pairs, including binary garbage.
- `recompute_completeness()`: random condition sets.
- `staleness()`: random `DateTime<Utc>` values (within macro-range).

### MP8: Full Lifecycle Integration Test
Single test that exercises: agent describes trigger → fuzzer contributes → dedup merges → completeness updates → priority computed → borderline creates review → human approves → merge completes → matrix persisted to disk → daemon restarted → matrix reloaded → all data correct. (See `full_lifecycle_add_dedup_evict_persist_reload` in Appendix B.)

### MP9: Rust Quality Checks
CI must enforce:
```bash
cargo clippy -- -D warnings
cargo test --no-fail-fast
cargo doc --no-deps --document-private-items
cargo fmt --check
cargo audit
```
And crate-level:
```rust
#![deny(missing_docs)]
#![deny(unsafe_code)]
```
Zero `unsafe` blocks in `src/`. Gated by `grep -r 'unsafe' src/` in CI.

### MP10: Regression Freeze (Golden Snapshot)
After H1–H12 implementation, create a known-good TriggerMatrix configuration, serialize to JSON, commit as `tests/fixtures/golden_trigger_matrix.json`. Any change altering matrix behavior (score, eviction order, serialization) fails `golden_snapshot_regression_test` unless fixture is intentionally updated.

### MP11: Coverage Gate
CI runs `cargo tarpaulin --out Xml` targeting `bugswarm-evidence`. Gate: **95%+ line coverage on `trigger.rs`** (the core module). Coverage regressions fail CI.

### MP12: Benchmark Gate
CI runs `cargo bench` against `benches/`. Gate: All operations within spec budget:
- `add_condition` (exact duplicate): < 10us
- `add_condition` (new, 700 existing): < 100us
- `save` (1000 bugs): < 2s
- `load` (1000 bugs): < 5s

### MP13: Documentation
- **ADR-023**: Architecture Decision Record documenting trigger matrix design decisions, hash policy, eviction strategy. Placed at `docs/adr/023-trigger-matrix.md`.
- **User docs**: `docs/trigger_matrix.md` — how to use `describe_trigger` RPC, review queue, migration.
- **API docs**: All public items have `/// doc comments` (enforced by `#![deny(missing_docs)]`).

### MP14: Rollback CI
CI gate: any commit that **decreases** test count or coverage fails the build. Enforced by comparing `cargo test --list` count and `cargo tarpaulin` coverage against baseline.

### MP15: Canary Deployment
1. Deploy to staging environment with production-like graph.
2. Run migration (`migrate --dry-run` first, verify expected counts, then run for real).
3. Run `describe_trigger` on 100 bugs, verify output.
4. Run `metrics` RPC, verify counters.
5. Send SIGTERM, verify persistence.
6. Restart daemon, verify state restored.

### MP16: Load Test
Script that fires 10,000 concurrent `add_condition` calls across 100 goroutines/tasks:
```rust
// In tests/stress/
// Spawn 100 tokio tasks, each adding 100 conditions
// Verify: no panics, no deadlocks, all conditions present, indices consistent
```
Target: < 5 seconds for 10K calls on a 4-core machine.

### MP17: Chaos Test
1. Start daemon with populated state.
2. Send `SIGKILL -9` to daemon process mid-save.
3. Restart daemon.
4. Verify: trigger state recovered from primary or `.tmp` backup file.
5. Verify: no silent data loss (even if some recent additions lost, matrix consistency maintained).

### MP18: Mutation Test
Inject intentional bugs:
- H1: Swap `base_score` and `density_bonus` in multiplication.
- H7: Evict NEWEST-unverified instead of oldest.
- H12: Use `{:?}` instead of `dimension_key()`.
- H9: Merge borderline matches as Exact.
Verify that corresponding tests catch each bug. Run `cargo mutants` or manual injection.

---

## Appendix D: Hash Policy, Format Stability & Memory Budget

### Hash Policy (fixes IC1)

| Hash Function | Length | Bits | Purpose | Format |
|--------------|--------|------|---------|--------|
| `normalize_hash()` | 16 hex chars | 64 | Legacy `TriggerCondition.id` generation | SHA-256 prefix |
| `hash_prefix()` | 32 hex chars | 128 | `normalized_idx` HashMap key | SHA-256 prefix |
| `full_semantic_hash()` / `semantic_hash` field | 64 hex chars | 256 | Cross-matrix similarity, integrity verification | Full SHA-256 hex |

**Rationale**: The index key needs collision resistance but must be compact (32 chars gives 2^-128 per-pair collision probability, safe for >10^20 entries). The field needs full cryptographic strength for cross-matrix comparisons. The legacy id function is kept for backward compatibility.

### Format Stability

- **Timestamp format**: All `DateTime<Utc>` fields use `#[serde(with = "chrono::serde::ts_seconds")]` — Unix timestamp serialization. Format-stable across Chrono versions.
- **Dimension keys**: `dimension_key()` uses explicit `&'static str` match — format-stable across refactors. Never use `{:?}` Debug formatting.
- **Spec constants**: `TIMESTAMP_FORMAT` for display formatting in `Display` impls and summary strings.

### Concurrency Model (fixes MP15)

- **`TriggerManager`** is wrapped in `RwLock` within `EvidenceGraph`.
- **Write operations** (`add_condition`, `approve_review`, `reject_review`, `check_eviction`, `vacuum`) MUST hold `trigger_manager.write()`.
- **Read operations** (`investigation_priority`, `staleness`, `meets_phase30_gate`) may hold `trigger_manager.read()`.
- **Lock ordering** (prevents deadlocks): `nodes.read()` OR `trigger_manager.write()` — never both nested. If both locks needed, collect data under first lock, release it, then acquire second lock.
- **`TriggerMetrics`** counters are lock-free (`AtomicU64`) and safe for concurrent read/write. Snapshots are approximate.

### Memory Budget (fixes MP13)

| Scenario | Conditions | Index Memory | Total Memory |
|----------|-----------|-------------|-------------|
| 0 bugs | 0 | ~0 KB | ~1 MB |
| 1K bugs × 100 conditions | 100K | ~5 MB (keys) + ~4 MB (indices) | ~50 MB |
| 10K bugs × 700 conditions | 7M | ~280 MB (keys) + ~112 MB (by_dim) | ~3.5 GB |
| 100K bugs (directory mode) | 70M | Per-directory sharding | ~35 GB |

**Mitigations**:
- `MAX_CONDITIONS_PER_DIMENSION = 100` caps per-bug conditions at ~800.
- Per-bug file storage for >10 MB serialized.
- Index rebuilding is O(conditions) but batch-only (not per-add_condition).

---

## Appendix E: Migration & Rollback Protocol

### Pre-Migration Checklist
1. [ ] Take full graph backup (copy `evgraph.json` to `evgraph.json.backup.YYYYMMDD`).
2. [ ] Verify graph integrity: `bugswarm doctor --check-graph`.
3. [ ] Dry-run migration: `bugswarm migrate --dry-run --journal /tmp/migrate_journal.jsonl`.
4. [ ] Review dry-run report: confirm expected matrix/condition counts.
5. [ ] Ensure daemon is stopped (no concurrent writes).

### Migration Execution
```bash
bugswarm migrate \
  --journal /var/lib/bugswarm/migration_journal.jsonl \
  --extractor default
```

### Monitoring During Migration
- Every 100 bugs: progress log with checkpoint.
- Journal file lines = bugs completed.
- Send SIGUSR1 to dump current `MigrationResult` to log.

### Post-Migration Verification
1. `bugswarm metrics` — verify `matrices_count` and `conditions_total`.
2. `bugswarm describe-trigger <bug_id>` on 100 random bugs — verify output.
3. Run migration again — must produce 0 new matrices (idempotency).
4. Compare `bugswarm doctor --stats` pre/post migration.

### Rollback Procedure
```bash
bugswarm rollback --journal /var/lib/bugswarm/migration_journal.jsonl
```
This: (1) clears TriggerManager, (2) marks all `author="migration-backfill"` nodes with `metadata.rollback=true`, (3) deletes the journal file. The in-memory state is empty; a `rebuild_trigger_manager()` will NOT re-create migrated data because rollback-marked nodes are skipped.

### Resume After Interruption
If migration crashes at bug 500/1000:
1. Run `bugswarm rollback --journal ...` to clean up partial state.
2. Or: run `bugswarm migrate --journal ...` again — it reads the journal, skips completed bugs, resumes from the last checkpoint. The graph nodes from completed bugs remain (they're correct). In-memory state is rebuilt via `rebuild_trigger_manager()`.
