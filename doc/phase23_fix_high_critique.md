# Phase 23 H1–H12 Remediation Plan — Brutal Audit

**Audited document**: `/root/a/phase23_fix_high.md` (1,722 lines)
**Codebase inspected**: `bugswarm-evidence/src/trigger.rs` (528 L), `spec.rs` (34 L), `graph.rs` (622 L), `types.rs` (265 L), `daemon.rs` (255 L), `lib.rs` (5 L)
**Verdict**: The plan contains at least 16 kill-switch defects that produce wrong behavior on first run or do not compile. 9 additional algorithmic weaknesses. 12 inconsistencies with itself or the existing codebase. 13 error-prone design patterns. 17 missing implementation practices.

---

## Part 1: Kill-Switch Findings — Issues That Cause Immediate Failure

These plan flaws, if implemented as written, produce **wrong behavior on first run**, **will not compile**, or **silently corrupt state**.

---

### KS1: H1 density bonus computes global condition count, not per-dimension depth

**Location**: Plan §H1 lines 95–98

```rust
let density_bonus = {
    let cnt = self.conditions.len() as f32;
    (cnt / crate::spec::DENSITY_BONUS_SATURATION as f32).min(1.0)
};
```

`self.conditions.len()` is the **total** condition count across **all** dimensions. The plan's own Root Cause Analysis (line 49) states the density bonus should "reward having multiple distinct conditions within a single dimension" — per-dimension depth. The implemented formula does the opposite: it rewards **total** condition count, making density indistinguishable from cardinality.

**Concrete failure**: A matrix with 7 conditions all in `Input` (zero coverage of 6 other dims) gets `density_bonus = 7/3 → 1.0`. A matrix with 1 condition in each of 7 dimensions (perfect breadth, zero depth) also gets `density_bonus = 7/3 → 1.0`. Both score identical density despite wildly different coverage semantics.

**Fix**: Compute per-dimension density as `max_{dim} (conditions[dim] / DENSITY_BONUS_SATURATION)` or average across dimensions:

```rust
let density_bonus = {
    let max_in_dim = TriggerDimension::all().iter()
        .map(|d| self.conditions.iter().filter(|c| c.dimension == *d).count())
        .max().unwrap_or(0) as f32;
    (max_in_dim / crate::spec::DENSITY_BONUS_SATURATION as f32).min(1.0)
};
```

---

### KS2: H7 eviction clones `String` in O(n) sort, and uses lexicographic sort where timestamps happen to be RFC 3339

**Location**: Plan §H7 lines 273–277

```rust
.min_by_key(|(_, c)| c.contributed_at.clone())
```

`Iterator::min_by_key` calls the closure for **every** element and takes **ownership** of each returned value. With 100 candidates in one dimension, this clones 100 `String`s (each ~30 bytes of timestamp + heap). Each clone allocates. This is O(n × allocation) for a single eviction in a hot path triggered by every `add_condition()`.

**Secondary**: `contributed_at` is compared as a `String` (lexicographic). RFC 3339 strings do sort lexicographically by temporal order (ISO 8601 is designed that way), but relying on this is fragile — the format assumes the same timezone, same precision, no extra spaces. One dev inserting a `Z` vs `+00:00` suffix breaks the sort.

**Fix**: Use `min_by` (borrows, no clone) and parse timestamps once for comparison, or store `contributed_at: DateTime<Utc>` (see EP1):

```rust
.min_by(|(_, a), (_, b)| a.contributed_at.cmp(&b.contributed_at))
```

---

### KS3: H5 `load()` calls `add_condition()` which triggers dedup, eviction, completeness recompute, and metrics on every restored condition

**Location**: Plan §H5 lines 1073–1075

```rust
.and_modify(|existing| {
    for cond in &matrix.conditions {
        existing.add_condition(cond.clone());
    }
})
```

During restoration, `existing.add_condition()` triggers the **full** pipeline:
1. O(1) hash lookup in `normalized_idx`
2. Per-dimension Jaro-Winkler fuzzy dedup scan (expensive O(n) × m conditions)
3. H7 eviction check (count, sort, remove, cleanup)
4. H1 completeness recompute (HashSet allocation, weight iteration)
5. `last_updated` timestamp update
6. H6 metrics increment

When restoring 700 conditions for a single bug, every single condition goes through the full pipeline against the growing condition list. This is O(n^2) cost on restoration where O(n) insert-without-dedup is correct — the data was already deduplicated when originally persisted. Worse: eviction during load could purge restored conditions that happen to push dimensions over cap.

**Fix**: Add a separate `restore_condition(condition: TriggerCondition)` method that inserts raw, bypasses dedup, bypasses eviction, updates timestamps only once at end, and calls `recompute_completeness()` once. Use this during `load()`. Or better: store and restore the entire `conditions: Vec<TriggerCondition>` directly without per-condition dispatch.

---

### KS4: H8 staleness formula contradicts H4 priority formula — weight collision

**Location**: Plan §H4 line 168 vs §H8 lines 358, 362

H4 defines:
```
PRIORITY_SEVERITY_WEIGHT = 0.7
PRIORITY_INCOMPLETENESS_WEIGHT = 0.3
```

H8 defines:
```
PRIORITY_SEVERITY_WEIGHT = 0.7 → changed to 0.6 (line 362)
PRIORITY_STALENESS_WEIGHT = 0.1 (line 358)
```

Then H8's `investigation_priority()` hardcodes the numbers (lines 388–390):
```rust
0.6 * severity_weight + 0.3 * incompleteness + 0.1 * staleness_urgency
```

Three problems:
1. **H4 text says 0.7, H8 says 0.6** — if implementer follows H4 text literally and fills in `PRIORITY_SEVERITY_WEIGHT = 0.7`, then H8's hardcoded `0.6` now differs from the constant. The code would have both values present in the file.
2. **H8's `investigation_priority()` hardcodes the numbers** instead of using `crate::spec::PRIORITY_SEVERITY_WEIGHT` etc. This violates the plan's own spec-constants rule (Immunity Layer 2, line 122).
3. **Hardcoded 0.6/0.3/0.1 must sum to 1.0** but no runtime assertion validates this. If someone changes only one constant, the priority formula silently breaks.

**Fix**: All three weights must be defined in `spec.rs` simultaneously during H4. H8 adds only the staleness term. `investigation_priority()` must read from `spec.rs`:

```rust
let priority = crate::spec::PRIORITY_SEVERITY_WEIGHT * severity_weight
             + crate::spec::PRIORITY_INCOMPLETENESS_WEIGHT * incompleteness
             + crate::spec::PRIORITY_STALENESS_WEIGHT * staleness_urgency;
```

Add a compile-time assertion (`const_assert!`) or test that weights sum to 1.0.

---

### KS5: H9 `check_equivalence` uses unqualified constants that won't compile

**Location**: Plan §H9 lines 1313–1325

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
```

All four constants (`DEDUP_MIN_FUZZY_LENGTH`, `DEDUP_MAX_DESCRIPTION_CHARS`, `DEDUP_JARO_WINKLER_THRESHOLD`, `DEDUP_BORDERLINE_THRESHOLD`) are used **without a path prefix**. The existing codebase uses `crate::spec::DEDUP_MIN_FUZZY_LENGTH` (trigger.rs:260). If `check_equivalence` is placed in `trigger.rs`, these names resolve only if there's a `use crate::spec::*` import or each is individually imported. Neither is stated in the plan. The codeblock will fail `cargo build`.

**Fix**: Add `use crate::spec::{...}` to the plan code blocks, or always use fully-qualified paths.

---

### KS6: H10 `extract_sandbox_receipts` accesses private field `graph.nodes`

**Location**: Plan §H10 line 1540

```rust
fn extract_sandbox_receipts(graph: &EvidenceGraph, bug_node_id: NodeId) -> Vec<(NodeId, String)> {
    let nodes = graph.nodes.read(); // ← PRIVATE FIELD
```

`EvidenceGraph.nodes` is `nodes: RwLock<Vec<EvidenceNode>>` — **no `pub` modifier** (graph.rs:19). This standalone function cannot access it. Compilation fails.

**Fix**: Either make `extract_sandbox_receipts` a method on `EvidenceGraph` (inherent impl), or add a public accessor `pub fn node_count_by_kind(...)`, or expose `nodes` as `pub(crate)`.

---

### KS7: H1 min-floor uses `.max()` — logically backwards, misnamed

**Location**: Plan §H1 lines 108–109

```rust
let min_floor = (crate::spec::COMPLETENESS_MIN_FLOOR).max(density_bonus * layer_bonus);
```

`0.1.max(X)` returns the **maximum** of 0.1 and X. If `density_bonus * layer_bonus` is 0.4, then `min_floor` = 0.4. If that product is 1.0, then `min_floor` = 1.0. The final line:
```rust
self.completeness_score = effective_score.max(min_floor).min(1.0);
```
When `min_floor` = 1.0, the score is **forced to 1.0**, completely ignoring `base_score`. A bug with only one covered dimension (base_score ≈ 0.3) but max layer and density bonuses would get `completeness_score = 1.0` — a perfect score for nearly-empty coverage. This violates the monotonicity property the plan claims to enforce.

**Root cause**: The name says "min floor" but the code computes a "rising floor" that scales with bonuses. The intent was likely:
```rust
let effective_score = base_score * density_bonus * layer_bonus;
self.completeness_score = effective_score.max(COMPLETENESS_MIN_FLOOR).min(1.0);
```
A constant floor of 0.1, not a bonus-scaled floor.

**Fix**: Remove the `.max()` composition. Use a flat floor:
```rust
self.completeness_score = effective_score
    .max(crate::spec::COMPLETENESS_MIN_FLOOR)
    .min(1.0);
```

---

### KS8: H12 `hash_key` uses 16-char truncated hash — birthday collisions possible

**Location**: Plan §H12 lines 457–458

```rust
let hash_key = format!("{:?}:{}", condition.dimension,
    normalize_hash(&condition.normalized));
```

`normalize_hash()` returns first 16 hex chars of SHA-256 = 64 bits of entropy (trigger.rs:254). With 64 bits, the birthday paradox predicts a collision at ~2^32 ≈ **4 billion** conditions. At 700 conditions/bug × 10K bugs = 7M conditions, collision probability is non-zero but negligible.

**However**: the hash_key prepends dimension Debug formatting. Two conditions with the same hash prefix but different dimensions get different keys. Two conditions with the same dimension and same hash prefix (colliding first 64 bits of SHA-256) would hash-match, triggering an exact-match merge that should NOT happen — since `normalized` strings can differ even when first 64 bits of hash match.

The real kill-switch: the plan's **exact-match check** at lines 463–466 does:
```rust
if existing.dimension == condition.dimension
    && existing.normalized == condition.normalized
```
This double-check partially mitigates hash collision — but only because it re-checks the `normalized` field. A hash collision causes an unnecessary O(k) probe but the re-check prevents false positives. So it's **not a correctness bug**, but it degrades to O(n) on hash collision.

Still, 16 chars is weak for a supposedly O(1) index. Use full hash.

**Fix**: Use full SHA-256 hex (64 chars) for the hash key. The key is only used for HashMap lookup — 64 bytes per entry × 7M entries = 448MB, which IS large. Alternative: use 32 hex chars (128 bits), which eliminates birthday collisions below ~10^20 entries.

---

### KS9: H7 eviction calls `Vec::remove()` but does NOT update H12 indices (`normalized_idx`, `by_dimension`) — they become stale and point to wrong entries

**Location**: Plan §H12 (indices at lines 488–494) vs §H7 (remove at line 279)

H12 adds `normalized_idx: HashMap<String, Vec<usize>>` and `by_dimension: HashMap<TriggerDimension, Vec<usize>>` — both store **indices into `self.conditions`**. When H7 evicts:
```rust
self.conditions.remove(evict_idx); // line 279
```
All elements after `evict_idx` shift left by 1. Every index `> evict_idx` in both HashMaps is now **off by 1**. Subsequent lookups read the wrong condition. Subsequent evictions may remove the wrong condition. Subsequent hash dedup (H12 Phase 1) may merge into the wrong entry.

**Fix**: After any `Vec::remove()`, call `self.rebuild_indices()` to regenerate both maps from scratch. Alternatively, `check_eviction` must recalculate all indices ≥ evict_idx — but that's more error-prone than full rebuild. The plan's line 503 mentions `check_eviction` with "index cleanup" but never shows the cleanup logic.

---

### KS10: H5 `load()` has a conditional-move compile error in the `Entry` chain

**Location**: Plan §H5 lines 1069–1081

```rust
for mut matrix in matrices {
    matrix.rebuild_indices();
    self.matrices
        .entry(matrix.bug_id.clone())
        .and_modify(|existing| {
            for cond in &matrix.conditions {   // borrows &matrix
                existing.add_condition(cond.clone());
            }
        })
        .or_insert_with(|| { loaded += 1; matrix }); // moves matrix
}
```

The `Entry::and_modify` closure borrows `&matrix.conditions`. The `or_insert_with` closure **moves** `matrix` into the HashMap. The Rust borrow checker sees both closures capturing `matrix` (one borrow, one move) and rejects the code. The fact that the closures run in exclusive branches (occupied vs vacant) is not visible to the borrow checker.

**Compilation error**: `error[E0505]: cannot move out of `matrix` because it is borrowed`

**Fix**: Use explicit `match` on the entry:

```rust
match self.matrices.entry(matrix.bug_id.clone()) {
    Entry::Occupied(mut e) => {
        let existing = e.get_mut();
        for cond in &matrix.conditions {
            existing.restore_condition(cond.clone());
        }
    }
    Entry::Vacant(e) => {
        loaded += 1;
        e.insert(matrix);
    }
}
```

Also note: `restore_condition` should be used, not `add_condition` (see KS3).

---

### KS11: H2 calls `full_semantic_hash()` which is never defined in the plan

**Location**: Plan §H2 line 637

```rust
let sem_hash = full_semantic_hash(&normalized);
```

The existing codebase defines only `normalize_hash()` (trigger.rs:250–254) which returns 16 chars. The plan references `full_semantic_hash()` returning "full SHA-256 hex (64 chars)" (line 564) but **never provides its implementation** anywhere in the 1,722-line plan. If the implementer copies the code block verbatim, it fails compilation.

**Fix**: Add the function definition to the plan:

```rust
fn full_semantic_hash(s: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())
}
```

---

### KS12: H9 borderline handling adds a DUPLICATE condition row with no cleanup path

**Location**: Plan §H9 line 1353–1354

> "Borderline handling in `add_condition()`: When fuzzy scan finds a borderline match, push a `ReviewCandidate` with status 'pending-review' and do NOT auto-merge. The condition is still added as a new row until review resolves."

When the Jaro-Winkler score falls in [0.75, 0.85), the condition is added **as a new row**. The `review_queue` gets a `ReviewCandidate` pointing at both the existing condition and the new one. But both exist in `self.conditions` until `approve_review()` is called. This means:

1. Two separate condition rows describe the same trigger surface — `get_by_dimension()` returns both, `meets_phase30_gate()` counts both.
2. `approve_review()` (lines 1358–1368) has a **TODO comment** `// Find and merge into existing, remove proposed, rebuild indices` — it's not implemented.
3. If `approve_review()` is never called, the matrix permanently contains duplicate entries.
4. `reject_review()` (lines 1370–1379) keeps both — but if a third borderline match comes in, which entry does it compare against?

**Fix**: Do NOT add borderline matches to `self.conditions`. Store only in `review_queue` with a clone of the proposed condition. On approve, merge. On reject, add as new condition at that point.

---

### KS13: H10 migration double-creates `TriggerCondition` graph nodes and `Triggers` edges

**Location**: Plan §H10 lines 1486–1506

Inside the migration loop, the code:
1. **Creates a `TriggerCondition` graph node** (lines 1486–1497), creates a `Triggers` edge to the matrix (line 1501–1502)
2. Then calls `graph.trigger_manager.write().add_condition(tc)` (line 1505)

But `TriggerManager::add_condition()` at daemon.rs:199 is followed by `graph.add_trigger_condition()` at daemon.rs:200 — which in graph.rs:114–129 calls `self.get_or_create_trigger_matrix(bug_id)` (finds the migration-created matrix, so no duplicate there) BUT then creates:
- A **second** `TriggerCondition` graph node (graph.rs:117–123)
- A **second** `Triggers` edge (graph.rs:125–126)

Each migrated condition now has **two** `TriggerCondition` graph nodes and **two** `Triggers` edges. The graph counts are wrong. Later queries including `rebuild_trigger_manager()` would reconstruct conditions twice.

**Fix**: The migration should NOT call `graph.add_trigger_condition()`. It already creates the graph node and edge directly. Or: the migration should only call `graph.add_trigger_condition()` and let it handle node/edge creation, removing the manual node/edge creation code.

---

### KS14: H10 `extract_sandbox_receipts` queries the WRONG direction — `edges_from(bug)` returns outgoing edges, but `Confirms` edges are incoming to the bug

**Location**: Plan §H10 lines 1539–1551

```rust
fn extract_sandbox_receipts(graph: &EvidenceGraph, bug_node_id: NodeId) -> Vec<(NodeId, String)> {
    let nodes = graph.nodes.read();
    let edges_from_bug = graph.edges_from(bug_node_id); // LINE 1541
    edges_from_bug.iter().filter_map(|(e, target_id)| {
        if e.kind == EdgeKind::Confirms {
            nodes.get(*target_id).and_then(|n| {
                if n.kind == NodeKind::SandboxRun {
```

In `confirm_bug()` (graph.rs:250), the `Confirms` edge is:
```rust
EvidenceEdge::new(EdgeKind::Confirms, run_id, bug_id, 1.0)
//                                   from=run_id, to=bug_id
```

So `Confirms` edges go **FROM sandbox_run TO confirmed_bug**. The `edges_from(bug_node_id)` method returns outgoing edges — edges whose `from` is the bug. The bug's outgoing edges include `Refines` (to claims) and `Aggregates` (to trigger matrix) — but NOT `Confirms`. The `Confirms` edge is found in `edges_to(bug_node_id)` (incoming edges).

**Result**: `extract_sandbox_receipts` finds **zero** sandbox runs for every bug. Every migrated bug gets an empty TriggerMatrix (0 conditions). The migration reports success but creates no trigger data.

**Fix**: Change line 1541 from `graph.edges_from(bug_node_id)` to `graph.edges_to(bug_node_id)`.

---

### KS15: H3 `TriggerConfig` is defined but never wired into any operation — dead code

**Location**: Plan §H3 lines 738–783

The plan defines `TriggerConfig` with eight overridable thresholds and two boolean flags. It has well-structured `Default` impl. But **nowhere in the 12 sub-plans does any code use `TriggerConfig`**. The dedup code uses `crate::spec::DEDUP_JARO_WINKLER_THRESHOLD` directly. Eviction uses `crate::spec::MAX_CONDITIONS_PER_DIMENSION` directly. No code accepts a `TriggerConfig` parameter. No `TriggerManager` field stores a `TriggerConfig`. No `EvidenceGraph` field stores one.

The struct exists only in the H3 code block and the "All 6 structs compile" acceptance test (line 939). It is dead on arrival.

**Fix**: Either wire `TriggerConfig` into `TriggerManager` (stored as a field, read by operations instead of `spec.rs` constants), or remove it from the plan. A config struct that nothing reads is worse than no config struct — it creates confusion about where thresholds come from.

---

### KS16: H4 `Prioritizable` trait is gated but its definition is never shown anywhere in the plan

**Location**: Plan §H4 line 213, implementation checklist item 4 (line 222)

> **Compile-time trait**: Define `Prioritizable` trait that any struct requiring priority ordering must implement. CI enforces that no code calls `.sort()` on trigger collections without going through this trait.

The plan never defines what methods this trait has, what its bounds are, or where it lives. The acceptance test `prioritizable_trait_compiles` (line 1678) says it should compile — but compile against what? Without a trait definition, the test panics on `use` statement.

**Fix**: Define the trait in the plan, e.g.:

```rust
pub trait Prioritizable {
    fn priority(&self) -> f32;
}
```

---

## Part 2: Algorithmic Weak Points — MID-level things that PEAK would solve

---

### AW1: H12 indices (`normalized_idx`, `by_dimension`) use `HashMap` — not thread-safe for concurrent `add_condition` calls

**Location**: Plan §H12 lines 445, 449

The plan declares:
```rust
normalized_idx: HashMap<String, Vec<usize>>,
by_dimension: HashMap<TriggerDimension, Vec<usize>>,
```

If two threads call `add_condition()` concurrently (the daemon spawns a `tokio::spawn` per connection; `trigger_manager` is wrapped in `RwLock`), the outer `RwLock` on `trigger_manager` ensures mutual exclusion at the `TriggerManager` level. However, the plan could easily lead someone to bypass this lock for internal operations (e.g., `rebuild_indices` is called on `TriggerMatrix` directly). If any code path calls these methods without the lock, the `HashMap` corrupts.

**Fix**: Document the synchronization contract: "All `TriggerMatrix` mutations must occur under `trigger_manager.write()` lock. Indices are single-thread safe within that lock." Or use `Arc<RwLock<HashMap>>` if truly concurrent access is needed.

---

### AW2: H7 eviction does two passes (filter collect + min_by_key) over the conditions vector; single-pass fold is more efficient

**Location**: Plan §H7 lines 273–277

```rust
.filter(|(_, c)| c.dimension == dim && !c.verified)
.min_by_key(|(_, c)| c.contributed_at.clone())
```

`filter()` + `min_by_key()` requires iterating the filtered subset twice (once to apply predicate, once to find min). For 700 conditions, this is 1,400 condition visits. A single-pass `fold`:
```rust
.fold(None, |best, (i, c)| {
    if c.dimension == dim && !c.verified {
        match best {
            None => Some((i, c)),
            Some((_, best_c)) if c.contributed_at < best_c.contributed_at => Some((i, c)),
            other => other,
        }
    } else { best }
})
```
One pass, one clone (or zero with `min_by`).

---

### AW3: H6 `set_completeness` and `set_priority` overwrite each other — broken "max gauge" semantics

**Location**: Plan §H6 lines 1222–1223, 1225–1227

```rust
pub fn set_completeness(&self, score: f32) {
    self.completeness_gauge.store((score * 10000.0) as u64, Ordering::Relaxed);
}
```

The doc comment (line 1194) says `/// max completeness`. But this is a plain `store`, not a compare-and-max. When two matricies update completeness in sequence, the **last writer wins**, not the max. Thread A sets 0.5, Thread B sets 0.3 — result is 0.3, not 0.5.

**Fix**: Use `fetch_max`:
```rust
self.completeness_gauge.fetch_max((score * 10000.0) as u64, Ordering::Relaxed);
```
Note: `fetch_max` is available on `AtomicU64` since Rust 1.75.

---

### AW4: H5 `save()` uses `to_string_pretty` — allocates the entire JSON in memory before writing

**Location**: Plan §H5 line 1058

```rust
let json = serde_json::to_string_pretty(&matrices)?;
std::fs::write(path, json)?;
```

With 10K bugs × 700 conditions × ~500 bytes/condition = 3.5 GB serialized. `to_string_pretty` allocates a 3.5 GB `String` in memory. On a 4 GB system, this OOMs the process.

**Fix**: Use streaming writer: `serde_json::to_writer(file, &matrices)?`. But see also AW5 for atomic write safety.

---

### AW5: H5 `save()` has no atomic write — power loss mid-write corrupts the file

**Location**: Plan §H5 lines 1058–1059

```rust
std::fs::write(path, json)?;
```

If the disk fills or the process crashes mid-write, the file is left partially written with truncated JSON. On next startup, `load()` calls `serde_json::from_str()` which fails with a parse error, and **all trigger data is lost** because `load()` returns `Err`, and the code at line 1098 logs "failed to restore trigger state; starting fresh."

**Fix**: Write to a temp file, fsync, rename atomically:
```rust
let tmp = path.with_extension("tmp");
let mut f = std::fs::File::create(&tmp)?;
serde_json::to_writer(&mut f, &matrices)?;
f.sync_all()?;
std::fs::rename(&tmp, path)?;
```

---

### AW6: H12 `hash_key` uses `{:?}` Debug formatting — fragile and non-semantic

**Location**: Plan §H12 lines 457–458

```rust
let hash_key = format!("{:?}:{}", condition.dimension,
    normalize_hash(&condition.normalized));
```

`TriggerDimension::Input` formats as `"Input"` under `{:?}`. If someone renames the variant to `InputDimension` or adds a custom `Debug` impl, all hash keys become invalid at runtime with zero compiler errors. The `normalized_idx` silently fails to find matches, reverting to O(n) fuzzy scan.

**Fix**: Use `Display` or a custom serialization that is explicitly format-stable:
```rust
fn dimension_key(d: TriggerDimension) -> &'static str {
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

---

### AW7: H1 `recompute_completeness` allocates a `HashSet` on every call — wasteful for hot path

**Location**: Plan §H1 lines 73–76

```rust
let mut covered: HashSet<TriggerDimension> = HashSet::new();
for c in &self.conditions {
    covered.insert(c.dimension);
}
```

Every `add_condition()` calls `recompute_completeness()` (see H7 line 288, H12 line 499). For 8 dimensions, a `HashSet` allocates ~32 bytes of heap. This is minor but avoidable: use a `u8` bitmask since there are exactly 8 dimensions.

---

### AW8: H10 has TOCTOU between collecting confirmed bugs and checking `has_matrix`

**Location**: Plan §H10 lines 1452–1467

```rust
let nodes = graph.nodes.read();
let confirmed_bugs: Vec<(NodeId, String)> = nodes.iter()
    .filter(|n| n.kind == NodeKind::ConfirmedBug)
    .map(|n| (n.id, n.label.clone()))
    .collect();
drop(nodes); // lock released

for (i, (bug_node_id, bug_label)) in confirmed_bugs.iter().enumerate() {
    let has_matrix = {
        let nodes = graph.nodes.read(); // re-acquire lock
        nodes.iter().any(|n| n.kind == NodeKind::TriggerMatrix && n.label == *bug_label)
    };
```

Between the two read locks, another thread could create a `TriggerMatrix`. The `has_matrix` check would see it and skip — no duplicate. But a `ConfirmedBug` could also be created between the first `collect()` and the loop, and would be missed entirely. The migration becomes non-idempotent in a concurrent system.

**Fix**: Hold the read lock for the duration of `confirmed_bugs` collection and all `has_matrix` checks. Or add a migration-specific locking mechanism.

---

### AW9: H5 `load()` calls `recompute_completeness` N times via `add_condition` for N conditions — should be called once

**Location**: Plan §H5 lines 1073–1075 (via KS3 cascade)

When `add_condition` triggers `recompute_completeness` for each restored condition, a 700-condition matrix recalculates completeness 700 times (each O(dimensions)). The first 699 calculations are discarded. For 10K bugs, that's 7M wasted recomputations.

**Fix**: Alongside KS3's `restore_condition`, batch-restore all conditions then call `recompute_completeness()` once.

---

## Part 3: Inconsistencies — Plan contradicts itself or other sub-plans

---

### IC1: H2 `full_semantic_hash` (64 chars) vs H12 `normalize_hash` (16 chars) — plan never explains the split

**Location**: Plan §H2 line 564/605 vs §H12 line 458

H2 adds `semantic_hash: String` — "Full SHA-256 hex digest of the semantic fingerprint (64 hex chars)" (line 605). H12 uses `normalize_hash()` — "First 16 hex chars for brevity" (trigger.rs:254). The plan uses two different hash functions serving different purposes (cross-matrix similarity search vs local O(1) lookup) but never explains:
- Why two different lengths?
- Can `semantic_hash` be used for `normalized_idx` instead of the 16-char hash (eliminating the truncation risk from KS8)?
- If not, what's the actual collision rate difference and when does it matter?

**Fix**: Add a section documenting the hash policy: "normalize_hash for local dedup index (speed > collision resistance), full_semantic_hash for cross-matrix correlation (collision resistance > speed)."

---

### IC2: H1 duplicates weighted-coverage logic that already exists in `recompute_completeness`

**Location**: Plan §H1 lines 73–91

The "Phase 1: base weighted coverage (preserved from original)" reimplements the exact logic from the existing `recompute_completeness()` (trigger.rs:166–187) — but the plan *replaces* that function entirely. The duplication is structural: if dimension weights change in `spec.rs`, this logic changes once, but the plan text now documents an outdated copy of those weights (line 82: "`**d != TriggerDimension::OsArch`").

This is not a code bug but a plan-maintenance risk: the plan and the code drift.

---

### IC3: H4 "severity weight = 0.7" vs H8 changes it to 0.6 — no version ordering specified

Covered in KS4 above. The weights collide.

---

### IC4: H3 `DescribeTriggerResponse.filtered_count` is always equal to `total_count` — useless field

**Location**: Plan §H3 line 907

```rust
filtered_count: matrix.conditions.len(), // line 907
```

The `DescribeTriggerRequest` has filter fields: `dimension`, `layer`, `verified_only`, `include_raw` (lines 856–871). But `DescribeTriggerResponse::from_matrix()` ignores them all — it sets `filtered_count = total_count`. The field exists but carries zero information. Either implement the filtering or rename to `count` and state it's unfiltered total.

---

### IC5: H9 `approve_review` and `reject_review` have `// ...` instead of implementations — these are critical path methods

**Location**: Plan §H9 lines 1364–1365, 1377

```rust
// Find and merge into existing, remove proposed, rebuild indices
// ...
```

`approve_review` — the method that resolves borderline matches — is a placeholder comment. The plan lists "Add `approve_review()` and `reject_review()`" in its checklist (line 1395) but provides no code. This is not a plan — it's a TODO item. The most stateful operation in H9 is undocumented.

---

### IC6: H12 `add_condition` calls `is_semantically_equivalent` (binary), but H9 redefines it to reject borderline matches — changes dedup behavior

**Location**: H12 lines 475–478 vs H9 lines 1328–1330

H12 Phase 2 fuzzy scan uses:
```rust
is_semantically_equivalent(
    &self.conditions[idx].normalized,
    &condition.normalized,
)
```

H9 redefines `is_semantically_equivalent` as:
```rust
pub fn is_semantically_equivalent(a: &str, b: &str) -> bool {
    check_equivalence(a, b) == EquivalenceResult::Exact
}
```

The current code (trigger.rs:258–269) considers a condition equivalent if Jaro-Winkler >= 0.85. After H9, a condition is "equivalent" ONLY if score >= 0.85 (Exact). The [0.75, 0.85) band — previously merged — now returns `false`, so borderline matches are NEVER caught by the fuzzy scan. The review queue entry is never reached because the fuzzy scan doesn't find it.

**Root issue**: H12's Phase 2 should call `check_equivalence()` directly and branch on the tri-state, not call the legacy wrapper. But the implementation order is H12 → H9 (line 1641), so H12 can't use the not-yet-existing `check_equivalence` from H9.

**Fix**: Either swap implementation order (H9 before H12) or design Phase 2 to be upgrade-compatible from the start by calling a placeholder two-state function that H9 later upgrades to tri-state.

---

### IC7: Implementation order places H4 before H8 but H8 modifies H4's formula — H4's spec values are unstable from the start

**Location**: Plan §H4 lines 167–168, §H8 lines 358–359, implementation order line 1641

The order is H4 → H8. H4 must define `PRIORITY_SEVERITY_WEIGHT = 0.7`, then H8 changes it to `0.6`. If a developer commits H4, runs all tests, everything passes. Then H8 changes the constant and must re-run all H4 tests to verify they still pass with the new weights. The success criteria table for H8 (line 422) says `"H4 priority tests still pass with rebalanced weights"` — but this implies H4 tests are rewritten in the H8 commit. Messy.

**Fix**: All three priority weights should be defined together in H4 at their final H8 values but unused until H8. Or H4 should define `PRIORITY_SEVERITY_WEIGHT = 0.6` from the start despite the 0.7 in its text.

---

### IC8: H11 adds `schema_version` to TriggerMatrix but the field appears in both H11's and H12's struct definitions — inconsistent serialization order

**Location**: Plan §H11 lines 964–979 vs §H12 lines 440–450

H11 defines `TriggerMatrix` with `schema_version` as a public serde field. H12 re-defines `TriggerMatrix` with new fields `normalized_idx` and `by_dimension` (both `#[serde(skip)]`) but **does not show `schema_version`** in its struct definition. The implementer who starts from the H12 code block will omit `schema_version`.

**Fix**: Every struct block in the plan that re-defines an earlier struct must show ALL fields, or at minimum include a comment: `// ... existing H2/H11 fields preserved ...`

---

### IC9: H6 metrics doc says "max completeness" but implementation `store`s the latest value — not max

**Location**: Plan §H6 lines 1194, 1222–1223

Comment: `/// max completeness (f32 * 10000 as u64)`  
Code: `self.completeness_gauge.store((score * 10000.0) as u64, Ordering::Relaxed);`

`store` writes the value. If matrix A has completeness 0.8 and matrix B has 0.3, and B writes last, the gauge reads 0.3. The name says "max" but the behavior is "latest." 

Either rename to `last_completeness` or use `fetch_max` (see AW3).

---

### IC10: `default_schema_version()` appears multiple times as if it's unique — which module owns it?

**Location**: Plan §H2 line 629, §H3 lines 717, 757, 796, 814, etc., §H11 lines 973, 996

The plan references `fn default_schema_version() -> u32 { 1 }` in:
- H2: `#[serde(default = "default_schema_version")]` on `TriggerCondition`
- H3: same attribute on 5 different structs
- H11: same attribute on `TriggerMatrix`

If each module declares its own `default_schema_version`, that's 7 duplicate function definitions. If it's a single shared function, it must be `pub(crate)` at the crate root or in `spec.rs`. The plan never specifies.

**Fix**: Define once in `spec.rs` or `lib.rs`:
```rust
pub(crate) fn default_schema_version() -> u32 { 1 }
```
Reference all other structs to it via full path: `#[serde(default = "crate::spec::default_schema_version")]`.

---

### IC11: H10 migration creates `EdgeKind::Aggregates` from Bug to TriggerMatrix — but `Aggregates` is semantically "the source aggregates into the target", making it "Bug aggregates into TriggerMatrix" when it should be "TriggerMatrix aggregates conditions about the Bug"

**Location**: Plan §H10 lines 1475–1477

```rust
let edge = EvidenceEdge::new(EdgeKind::Aggregates, *bug_node_id, matrix_node_id, 1.0);
```

In the existing graph, `EdgeKind::Aggregates` is used in `confirm_bug()` at graph.rs:241:
```rust
EvidenceEdge::new(EdgeKind::Aggregates, claim_id, bug_id, 1.0)
```
This reads "Claim aggregates into ConfirmedBug" — the claim is the source, the bug is the target of aggregation. Reusing the same edge kind but reversing the semantic (Bug aggregates Matrix?) is confusing. Either use a different edge kind or accept that `Aggregates` means "source aggregates to form target" and confirm the H10 edge direction (Bug → Aggregates → TriggerMatrix means "Bug aggregates to form Trigger documentation" — acceptable but should be documented).

---

### IC12: H3 `DescribeTriggerResponse::from_matrix` calls `TriggerMatrixResponse::from_matrix` just to extract `dimension_scores` — unnecessary allocation

**Location**: Plan §H3 line 911

```rust
dimension_scores: TriggerMatrixResponse::from_matrix(matrix).dimension_scores,
```

This constructs a full `TriggerMatrixResponse` (cloning the entire conditions vector at line 840) just to throw away everything except `dimension_scores`. A standalone helper `fn compute_dimension_scores(matrix: &TriggerMatrix) -> Vec<DimensionScore>` would avoid this.

---

## Part 4: Error-Prone Design — Patterns that invite future bugs

---

### EP1: `contributed_at: String` — stringly-typed timestamp compared lexicographically

**Location**: Plan §H2 line 623 (field), §H7 line 276 (comparison), current trigger.rs:85 (field)

Every timestamp in the system is stored as `String` (RFC 3339). Sorting, comparison, age computation (H7 eviction, H8 staleness) all parse or compare strings. If Chrono changes its default format, or two contributors use different timezone formats (`Z` vs `+00:00`), lexicographic sorting silently breaks. The plan should mandate `DateTime<Utc>` with serde serialization:

```rust
#[serde(with = "chrono::serde::ts_seconds")]
pub contributed_at: DateTime<Utc>,
```

This eliminates the clone cost (KS2), makes comparison type-safe, and makes the format invariant explicit.

---

### EP2: H10 `default_extract_condition` uses `serde_json::Value::get()` on receipt JSON — double-parse of already-deserialized data

**Location**: Plan §H10 lines 1554–1576

Inside `migrate_existing_bugs`, the code at line 1482 already parses `receipt_json` into `Value`:
```rust
match serde_json::from_str::<serde_json::Value>(receipt_json) {
    Ok(receipt) => {
        if let Some(tc) = extract_condition(&receipt) {
```

Then `default_extract_condition` calls `.get("trigger_condition")`, `.get("input_hint")`, etc. on the already-parsed `Value`. This is fine (no double-parse of the string) — but the `receipt_json` parameter to `extract_sandbox_receipts` is stored as `String` and must be parsed each time. For 10K sandbox runs, that's 10K JSON parses of potentially large receipts. Should pass the parsed `Value` instead.

---

### EP3: H10 migration takes `fn()` pointer — should be `Fn` trait or enum dispatch

**Location**: Plan §H10 line 1445

```rust
pub fn migrate_existing_bugs(
    graph: &EvidenceGraph,
    extract_condition: fn(&serde_json::Value) -> Option<trigger::TriggerCondition>,
) -> MigrationResult {
```

Function pointers cannot capture state. If a future extractor needs configuration (e.g., extract from a different JSON path per deployment), the function pointer won't work. Use `impl Fn(...)`:

```rust
pub fn migrate_existing_bugs<F>(graph: &EvidenceGraph, extract_condition: F) -> MigrationResult
where F: Fn(&serde_json::Value) -> Option<trigger::TriggerCondition>,
```

Or define an `Extractor` trait with a default implementation.

---

### EP4: H10 has no error recovery — partial migration is permanently committed

**Location**: Plan §H10 lines 1461–1527

The migration iterates bugs, creates nodes and edges, and accumulates errors in a `Vec<String>`. If the process crashes at bug 500/1000, the first 500 bugs have TriggerMatrix nodes, TriggerCondition nodes, Aggregates edges, and Triggers edges in the graph — plus conditions in the in-memory TriggerManager. On restart:
- `rebuild_trigger_manager()` (H5 plan line 1031) would re-read the graph nodes and call `add_condition` (with KS3 cascade) on half-completed migration data.
- No rollback mechanism exists. "Rollback" is described as marking nodes (line 1593) but there's no code path that re-runs a failed migration — it would have to skip already-migrated bugs (via `has_matrix` check) but the partial data lives in the graph forever.

**Fix**: Add a transaction log:
1. Before migration, write a `migration_start` marker to a journal file.
2. After each bug, append `(bug_id, success/fail)` to the journal.
3. On completion, write `migration_complete`.
4. On startup, if the journal has `migration_start` but not `migration_complete`, run rollback by removing all nodes/edges whose `author == "migration-backfill"` from the graph.

---

### EP5: H6 `stale_conditions_gauge` has a setter but no code ever calls it

**Location**: Plan §H6 line 1228–1229

```rust
pub fn set_stale_count(&self, count: u64) {
    self.stale_conditions_gauge.store(count, Ordering::Relaxed);
}
```

Searching the entire plan, `set_stale_count` is never invoked. No code in H7 eviction, H8 `staleness()`, or anywhere else calls this method. The `stale_conditions_gauge` stays at 0 forever — a dead metric that looks alive in the snapshot.

**Fix**: Wire H8's `staleness()` into a periodic metadata scan or call `set_stale_count` after every completeness recompute.

---

### EP6: H5 `save()` writes to a `Path` without validating it's a file, not a directory — obscure error

**Location**: Plan §H5 lines 1056–1061

```rust
pub fn save(&self, path: &std::path::Path) -> Result<(), anyhow::Error> {
    let matrices: Vec<&TriggerMatrix> = self.matrices.values().collect();
    let json = serde_json::to_string_pretty(&matrices)?;
    std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")))?;
    std::fs::write(path, json)?;
    Ok(())
}
```

If `path` is `/nonexistent/deep/dir/trigger_state.json`, `create_dir_all` succeeds. If `path` is `/tmp/` (an existing directory), `parent()` returns `None`, `unwrap_or` gives `"."`, `create_dir_all(".")` succeeds. Then `fs::write("/tmp/", json)` fails with "Is a directory". The error message is confusing.

---

### EP7: H10 `rollback_migration` marks nodes but the existing graph has no "remove node" API — nodes marked "rollback" live in the graph forever

**Location**: Plan §H10 line 1593

> "rollback marks created nodes with `metadata.rollback = 'true'` and clears the TriggerManager"

The graph is append-only — nodes are never deleted (graph.rs design note at line 13-16). "Rolling back" by metadata flag means no memory is freed. All created nodes remain in `nodes: Vec<EvidenceNode>`, inflating `stats()` counts and bloating the graph. A true rollback requires a delete API or a GC pass.

---

### EP8: H9 `ReviewCandidate.queued_at` is a `String` — same problem as EP1 but for queue time

**Location**: Plan §H9 line 1342

```rust
pub queued_at: String,
```

Same timestamp-as-String anti-pattern.

---

### EP9: H1 `base_score` computation iterates `TriggerDimension::all()` and copies pre-computed weights from `spec.rs` — but spec.rs weights may change

**Location**: Plan §H1 lines 78–81

```rust
let total_weight: f32 = TriggerDimension::all().iter()
    .filter(|d| **d != TriggerDimension::OsArch)
    .map(|d| d.weight())
    .sum();
```

This dynamically reads weights from `spec::TRIGGER_DIMENSION_WEIGHTS` via `d.weight()`. That's GOOD — it uses the spec. But the `filter` hardcodes `OsArch` as the excluded dimension. If a future plan revision changes which dimension is the bonus, this filter must be updated in two places (here and the existing `recompute_completeness` at trigger.rs:173). The exclusion should also be a `spec.rs` constant.

---

### EP10: H6 `TriggerMetrics::snapshot()` returns `HashMap<&'static str, f64>` — this mixes metric names with values in a lossy way

**Location**: Plan §H6 line 1232

```rust
pub fn snapshot(&self) -> HashMap<&'static str, f64> {
```

Returning a `HashMap` means metric names and values are co-mingled. If serialized to JSON, a consumer must know all metric names a priori. The plan should define a proper `MetricsSnapshot` struct with named fields.

---

### EP11: H12 `rebuild_indices` called after deserialization but `normalized_idx` and `by_dimension` are `#[serde(skip)]` — so they're ALWAYS empty after load; `rebuild_indices` must always be called, but nothing enforces this

**Location**: Plan §H12 lines 443–444, 448–449

```rust
#[serde(skip)]
normalized_idx: HashMap<String, Vec<usize>>,
#[serde(skip)]
by_dimension: HashMap<TriggerDimension, Vec<usize>>,
```

After `serde_json::from_str`, these fields are empty. `rebuild_indices()` must be called before any `add_condition()`. The plan calls it in `load()` (H5 line 1070) and in `TriggerMatrix::new()` (H12 impl checklist item 2). But if any code path deserializes a `TriggerMatrix` without calling `rebuild_indices()`, the indices are silently empty and every lookup degrades to the O(n) original scan. There's no assertion that indices are populated before use.

**Fix**: Make these fields `Option<HashMap<...>>`. Any method that needs them calls `self.ensure_indices()` which rebuilds if `None`. And mark them as `#[serde(skip, default)]` so they always deserialize as `None`.

---

### EP12: H5 `rebuild_trigger_manager()` calls `tm.get_or_create(bug_id)` then `matrix.add_condition(tc)` on the returned mutable reference — but it also relies on `graph.trigger_manager.write()` holding the lock

**Location**: Plan §H5 lines 1031–1050

The code:
```rust
pub fn rebuild_trigger_manager(&self) {
    let nodes = self.nodes.read();
    let mut tm = self.trigger_manager.write();
    // ... uses tm ...
}
```

This holds both a read lock on `nodes` and a write lock on `trigger_manager` simultaneously. If any other thread holds `trigger_manager.write()` while `rebuild_trigger_manager` holds `nodes.read()`, deadlock if the other thread also needs `nodes`. The lock acquisition order is `nodes → trigger_manager` here, but in `add_trigger_condition` (graph.rs:114–129) the order is `trigger_manager → nodes` (if `get_or_create_trigger_matrix` needs to add a node). Different lock orders = deadlock potential.

**Fix**: Acquire and release `nodes.read()` before acquiring `trigger_manager.write()`. Collect the data you need under the read lock, then operate on the manager under the write lock.

---

### EP13: H10 code multiplies mutation without explicit ordering — `graph.trigger_manager.write().add_condition(tc)` modifies manager while graph nodes are also being added

**Location**: Plan §H10 line 1505

The in-memory TriggerManager is updated WHILE graph nodes are being added. If a daemon RPC call arrives during migration (which the plan explicitly has no locking around), it could read a partially-updated matrix from the manager with half the migrated conditions.

---

## Part 5: Missing Implementation Practices — What the plan doesn't say about HOW to implement

---

### MP1: No test-first development mandate

The plan lists 50+ test names in the appendix (lines 1664–1722) but never states that tests must be written **before** implementation code. Red-green-refactor is not mentioned. Without this, tests may be written to match buggy code rather than to verify correct behavior.

**Mandate**: All tests pass on `main` (failing because features don't exist). Then H1–H12 implemented to make them pass.

---

### MP2: No incremental commit strategy

The 12 findings represent ~26 hours of work (line 6). The plan says "Implementation order: H2 → H3 → ..." but doesn't mandate that each finding is a separate git commit. A 26-hour monolith commit is unreviewable and impossible to bisect.

**Mandate**: Each of the 12 findings is a separate commit with a commit message referencing the finding ID (e.g., `fix(H2): add plan-required fields to TriggerCondition`). Every commit passes `cargo test`.

---

### MP3: No code review gate specified

Plan doesn't require review before merge. Every change should be reviewed by a second engineer against this plan.

---

### MP4: No performance benchmarking baseline

H12 claims to provide O(1) exact-match lookup for a claimed speedup. Before implementing H12:
1. Benchmark current `add_condition()` with 0, 100, 700, 7000 conditions.
2. After H12, benchmark the same sizes.
3. Assert speedup in CI.

---

### MP5: No edge-case enumeration for each algorithm

Each algorithm should enumerate its boundary cases before code is written:

| Algorithm | Edge Cases |
|-----------|-----------|
| H1 completeness | 0 conditions, 1 condition, all 7 covered × 1 cond, 1 covered × 100 conds, 8 dims all maxed |
| H4 priority | severity=0, severity=255 (out of range), severity=None, completeness=NaN |
| H7 eviction | dimension at exactly MAX, MAX+1, MAX+100, all verified, all unverified, interleaved |
| H8 staleness | last_updated="", last_updated="not-a-date", future date, epoch 0 |
| H9 tri-state | exact score=0.75, 0.85, identical strings, empty strings, single char |
| H12 hash path | hash collision, index consistency after eviction, rebuild after serialization |

---

### MP6: No differential testing against reference implementations

- Jaro-Winkler in `strsim` crate should be verified against Python `jellyfish` library for known inputs.
- `full_semantic_hash` output should be verified against `sha256sum <(echo -n "normalized_string")` in bash.

---

### MP7: No fuzzing mandate

All public methods should be fuzzed with `cargo-fuzz` or `proptest`:
- `add_condition()`: random combinations of dimensions, layers, descriptions, nulls, unicode.
- `check_equivalence()`: random string pairs.
- `normalize_description()`: random inputs including binary garbage.
- `recompute_completeness()`: random condition sets.
- `staleness()`: random timestamp strings.

---

### MP8: No end-to-end integration test covering full lifecycle

The plan tests individual components but not the integration:

1. Agent describes trigger → fuzzer contributes → dedup merges → completeness updates → priority computed → review queue entry created (borderline) → human approves → merge completes → matrix persisted → daemon restarted → matrix reloaded → all data correct.

---

### MP9: No Rust-specific quality checks

The plan should mandate:
```bash
cargo clippy -- -D warnings
cargo test --no-fail-fast
cargo doc --no-deps --document-private-items
```

And CI-level:
- `#![deny(missing_docs)]` on `bugswarm-evidence/src/lib.rs`
- Zero `unsafe` blocks (grep `unsafe` in `src/` → must be empty)
- `cargo audit` (no known CVE in dependencies)
- `cargo fmt --check`

---

### MP10: No regression freeze test

After all H1–H12 fixes, serialize a known-good TriggerMatrix configuration to JSON and commit it as a test fixture. Any future change that alters matrix behavior (score calculation, eviction order, serialization format) fails the regression test unless the fixture is intentionally updated.

---

### MP11: No documentation for `contributed_at` timestamp format stability

RFC 3339 via Chrono is the plan. Chrono 0.4 formats `DateTime<Utc>` as `2024-01-15T10:30:00Z`. If Chrono 0.5 changes the format or if a contributor uses `to_rfc3339_opts(SecondsFormat::Millis, true)`, the string width changes from 20 to 24 chars. This breaks any consumer that parses by fixed offset.

**Mandate**: Lock the timestamp format in `spec.rs`:
```rust
pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";
```
And use `chrono::NaiveDateTime::format()` with this constant.

---

### MP12: No structured error types

H9 methods return `Result<(), String>`. Use a proper error enum:

```rust
#[derive(Debug, thiserror::Error)]
pub enum TriggerError {
    #[error("review {0} not found")]
    ReviewNotFound(usize),
    #[error("review {0} already resolved")]
    ReviewAlreadyResolved(usize),
    #[error("persistence error: {0}")]
    Persistence(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

---

### MP13: No memory budget analysis

| Scenario | Memory |
|----------|--------|
| 0 bugs | ~0 MB |
| 1K bugs × 100 conditions | 1K × 100 × 500B = 50 MB |
| 10K bugs × 700 conditions | 10K × 700 × 500B = 3.5 GB |
| `normalized_idx` for 7M entries | 7M × (32B key + 8B Vec overhead) ≈ 280 MB |
| `by_dimension` for 7M entries | 7M × 16B ≈ 112 MB |

Total: ~4 GB at 10K bugs. The plan never acknowledges this. At a minimum, document the expected memory footprint and the maximum supported matrix count.

---

### MP14: No schema version branching logic

The plan adds `schema_version: u32` to every struct (H11) but never defines what happens when schema_version = 2. There's no `match schema_version { 1 => migrate_v1_to_v2() }` code. The version field exists so future developers can write migration logic, but the plan doesn't show them where to put it.

**Mandate**: Add a `fn migrate_schema(&mut self)` method to `TriggerMatrix` that is called after deserialization and branches on `self.schema_version`, with a default arm that returns an error for unknown versions.

---

### MP15: No concurrency model documented

The plan adds:
- `TriggerManager` with a plain `HashMap<String, TriggerMatrix>` (line 273)
- `TriggerMetrics` with `AtomicU64` fields (line 1191–1201)
- Daemon `RPC` methods that acquire `trigger_manager.write()` (daemon.rs:198)

But never documents:
- Is `add_condition` called from a single thread only?
- Can two connections simultaneously `add_condition` for the same bug?
- Is the `RwLock<TriggerManager>` in `EvidenceGraph` the only synchronization point?
- Are `normalized_idx` operations safe without the lock?

Document the threading model explicitly: "Single-writer, multi-reader via `RwLock<TriggerManager>`. All mutations to `TriggerMatrix` must hold the write lock. `TriggerMetrics` counters are lock-free but monotonic-only."

---

### MP16: No panic safety or transactional semantics

If `add_condition` panics mid-way (e.g., between pushing to `conditions` and updating `normalized_idx`), the matrix is left with:
- Condition in `conditions` but not in `normalized_idx` (inconsistent indices)
- `contributing_layers` possibly updated but condition not fully inserted
- `last_updated` timestamp already written

**Mandate**: Use a "prepare-commit" pattern:
1. Compute all mutations in local variables (don't touch `self`).
2. Apply all mutations atomically.
3. If any step fails, roll back to original state.

Or at minimum, document that panics leave the matrix in an inconsistent state requiring `rebuild_indices()` and `recompute_completeness()` for recovery.

---

### MP17: No `Display` impls on key enums

`TriggerDimension`, `ContributionLayer`, `EquivalenceResult`, `SeveritySpecific` all lack `Display`. The plan's `hash_key` and debug prints format them with `{:?}`. Adding `Display` is trivial and eliminates the format-stability risk from AW6.

---

### MP18: No `use` statements in any plan code block — copy-paste fails

Every code block in the plan omits `use` statements. The implementer must guess which crates are imported:
- `chrono::{DateTime, Utc}` — used in H8 `staleness()`
- `strsim::jaro_winkler` — used in H9 `check_equivalence`
- `use sha2::{Sha256, Digest}` — used in H2 `full_semantic_hash`
- `use std::collections::{HashMap, HashSet}` — used throughout
- `use serde::{Serialize, Deserialize}` — used on every struct

The plan should either include `use` statements or reference: "All imports follow the existing `trigger.rs` conventions."

---

## Summary: Severity-Categorized Issue Count

| Category | Count | Critical |
|----------|-------|----------|
| Kill-Switch (KS) | 16 | KS1, KS2, KS3, KS4, KS5, KS6, KS7, KS9, KS10, KS11, KS12, KS13, KS14 |
| Algorithmic Weak Points (AW) | 9 | AW3, AW5 |
| Inconsistencies (IC) | 12 | IC6 |
| Error-Prone Design (EP) | 13 | EP1, EP4, EP11, EP12 |
| Missing Practices (MP) | 18 | MP1, MP2, MP4, MP8, MP14, MP15, MP16 |

**Total**: 68 distinct issues across 5 categories.

---

## Top 5 Implementation Blockers (must be fixed before any code is written)

1. **KS1** — Density bonus formula is semantically wrong. Fix the math before coding.
2. **KS3 / KS10** — `load()` calls `add_condition` which triggers full pipeline + has compilation error. Design `restore_condition` first.
3. **KS9** — H7 eviction invalidates H12 indices. Must rebuild or track stale indices.
4. **KS14** — H10 migration uses wrong edge direction. ZERO conditions will be backfilled.
5. **KS4 / IC3 / IC7** — Priority weight collision between H4 and H8. Define all weights at once.

---

## Recommended Implementation Sequence (revised)

Instead of the plan's H2 → H3 → H11 → H1 → H4 → H12 → H7 → H8 → H5 → H6 → H9 → H10:

1. **H2 + H11** (field additions) — add all fields in one pass, including `schema_version` everywhere, `contributed_at: DateTime<Utc>` (fix EP1), `full_semantic_hash` function
2. **H3** (struct definitions) — all request/response structs, `TriggerConfig` wired or removed
3. **H1 + H4 + H8** (scoring) — implement completeness with corrected density (KS1), priority, and staleness in one atomic change with all weights finalized
4. **H9** (tri-state) — `check_equivalence` with qualified constants (KS5), review queue with working approve/reject (KS12)
5. **H12** (O(1) fast path) — with `check_equivalence` already in place (fixes IC6), use 32-char hash (fixes KS8), add index rebuild on eviction (fixes KS9)
6. **H7** (eviction) — after indices exist, with index-aware eviction
7. **H5** (persistence) — with `restore_condition` (fixes KS3, KS10), atomic writes (fixes AW5), streaming serialization
8. **H6** (metrics) — wire all counters, use `fetch_max` for gauges
9. **H10** (migration) — with correct edge direction (fixes KS14), no double-adds (fixes KS13), journal-based rollback
