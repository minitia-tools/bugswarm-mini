# Chain Detector — Universal Bug Category Expansion

**Date**: 2026-05-18
**Target**: `bugswarm-evidence/src/chain.rs`
**Change type**: Enum expansion + severity weight table — zero algorithm changes

---

## Current State

The chain detector (`chain.rs`) connects bugs via `Enables` edges. BFS traversal follows these edges to discover multi-step chains. The severity calculus scores chains by: `max(sev) + length × 0.5 + RCE_bonus × 2.0 + trust_bonus × 1.5`.

**Problem**: Only models security exploit chains. Effect and precondition types cover InfoLeak, MemoryCorruption, ControlFlow, PrivilegeEsc, StateCorruption, Capability. A memory leak → timeout chain is invisible because "timeout" is not a recognized effect type.

---

## What Changes

### Change 1: Expand EffectType and PreconditionType

**Before (7 security variants):**
```rust
pub enum EffectType {
    InfoLeak,
    MemoryCorruption,
    ControlFlow,
    PrivilegeEsc,
    StateCorruption,
    Capability,
}
```

**After (15 variants covering 10 bug categories):**
```rust
pub enum EffectType {
    // Security (existing)
    InfoLeak,
    MemoryCorruption,
    ControlFlow,
    PrivilegeEsc,
    StateCorruption,
    Capability,
    
    // Non-security (new)
    Crash,
    ResourceExhaustion,
    DataCorruption,
    Deadlock,
    PerformanceDegradation,
    Timeout,
    Incompatibility,
    Regression,
    Inconsistency,
    Bloat,
}
```

Same expansion for `PreconditionType`.

### Change 2: Add ChainType to the Enables edge

```rust
pub enum ChainType {
    SecurityExploit,       // info leak → overflow → RCE
    ResourceCascade,       // leak → bloat → timeout
    DataCorruptionChain,   // corruption → inconsistency → regression
    DeadlockCascade,       // deadlock → timeout → crash
    IncompatibilityChain,  // incompatibility → regression → crash
    PerformanceCascade,    // inefficiency → bloat → timeout
    Hybrid,                // crosses categories
}
```

The `Enables` edge (or `EffectPreconditionMatch`) gets a `chain_type: ChainType` field. The BFS remains identical — it traverses edges regardless of type. The type is metadata used only in severity scoring.

### Change 3: Category-Aware Severity Calculus

**Before** (security only):
```rust
let total = max_sev + length_bonus + rce_bonus + trust_bonus;
```

**After** (category-aware):
```rust
fn category_bonus(terminal_bug: &BugCategory) -> f64 {
    match terminal_bug {
        BugCategory::RCE | BugCategory::CodeExecution => 2.0,
        BugCategory::DataCorruption => 1.5,
        BugCategory::Regression => 1.2,
        BugCategory::Crash | BugCategory::Deadlock => 1.0,
        BugCategory::Inconsistency => 1.0,
        BugCategory::Incompatibility => 0.8,
        BugCategory::Timeout => 0.5,
        BugCategory::Inefficiency | BugCategory::Bloat => 0.3,
        _ => 0.5, // MemoryLeak, InfoLeak, etc.
    }
}

fn compute_chain_severity(
    bug_ids: &[String],
    severities: &HashMap<String, u8>,
    chain_type: ChainType,
    terminal_category: BugCategory,
) -> ChainSeverity {
    let max_sev = bug_ids.iter()
        .filter_map(|id| severities.get(id).copied())
        .max().unwrap_or(1) as f64;
    
    let length_bonus = (bug_ids.len() as f64 - 1.0) * 0.5;
    let cat_bonus = category_bonus(&terminal_category);
    
    let total = (max_sev + length_bonus + cat_bonus).min(10.0);
    
    ChainSeverity { max_sev, length_bonus, cat_bonus, total }
}
```

---

## What Does NOT Change

| Component | Change? | Why |
|-----------|---------|-----|
| **BFS traversal** | None | Graph is graph — nodes connected by edges. Irrelevant what type of bug the node represents. |
| **Semantic matcher** | None | Jaro-Winkler over description text. "Memory leak causes resource exhaustion" vs "requires exhausted heap" — matching already works on text, not enums. |
| **Edge creation** | None | `create_edge(source, target, Enables, score)` — same function, just with a `chain_type` parameter. |
| **Evidence graph** | None | `EdgeKind::Enables` already exists. No new edge types needed. |
| **Daemon handler** | None | `suggest_chain` handler calls the same functions. |
| **Agent tool** | None | `suggest_chain` tool returns chains with a `chain_type` field. |
| **ISG integration** | None | ARIA's Investigation State Graph already stores hypothesis trees — chain type is just metadata on the node. |

---

## Effect Extraction Expansion

The `extract_effects()` function in `chain.rs` currently uses keyword matching for security effects. Extend with the same approach for new categories:

```rust
pub fn extract_effects(bug_id: &str, description: &str, severity: u8) -> Vec<BugEffect> {
    let lower = description.to_lowercase();
    let mut effects = Vec::new();
    
    // Security (existing)
    if lower.contains("leak") || lower.contains("disclose") {
        effects.push(effect(bug_id, EffectType::InfoLeak, severity, "info leak"));
    }
    if lower.contains("overflow") || lower.contains("corrupt") || lower.contains("overwrite") {
        effects.push(effect(bug_id, EffectType::MemoryCorruption, severity, "memory corruption"));
    }
    // ... existing patterns ...
    
    // Crash (new)
    if lower.contains("crash") || lower.contains("segfault") || lower.contains("panic") || lower.contains("sigsegv") {
        effects.push(effect(bug_id, EffectType::Crash, severity, "crash"));
    }
    
    // Resource exhaustion (new)
    if lower.contains("leak") || lower.contains("exhaust") || lower.contains("oom") || lower.contains("out of memory") {
        effects.push(effect(bug_id, EffectType::ResourceExhaustion, severity, "resource exhaustion"));
    }
    
    // Deadlock (new)
    if lower.contains("deadlock") || lower.contains("livelock") || lower.contains("circular wait") {
        effects.push(effect(bug_id, EffectType::Deadlock, severity, "deadlock"));
    }
    
    // Timeout (new)
    if lower.contains("timeout") || lower.contains("hang") || lower.contains("infinite loop") {
        effects.push(effect(bug_id, EffectType::Timeout, severity, "timeout"));
    }
    
    // Data corruption (new)
    if lower.contains("silent corruption") || lower.contains("wrong result") || lower.contains("incorrect output") {
        effects.push(effect(bug_id, EffectType::DataCorruption, severity, "data corruption"));
    }
    
    // Performance degradation (new)
    if lower.contains("slow") || lower.contains("degrad") || lower.contains("regression") || lower.contains("latency") {
        effects.push(effect(bug_id, EffectType::PerformanceDegradation, severity, "performance degradation"));
    }
    
    // Incompatibility (new)
    if lower.contains("incompatible") || lower.contains("breaking change") || lower.contains("api change") {
        effects.push(effect(bug_id, EffectType::Incompatibility, severity, "incompatibility"));
    }
    
    // Regression (new)
    if lower.contains("regression") || lower.contains("previously working") || lower.contains("worked before") {
        effects.push(effect(bug_id, EffectType::Regression, severity, "regression"));
    }
    
    // Inconsistency (new)
    if lower.contains("inconsistent") || lower.contains("divergent") || lower.contains("different result") {
        effects.push(effect(bug_id, EffectType::Inconsistency, severity, "inconsistency"));
    }
    
    // Bloat (new)
    if lower.contains("bloat") || lower.contains("unbounded growth") || lower.contains("accumulat") {
        effects.push(effect(bug_id, EffectType::Bloat, severity, "bloat"));
    }
    
    // Default fallback
    if effects.is_empty() {
        effects.push(effect(bug_id, EffectType::StateCorruption, severity, "unknown effect"));
    }
    
    effects
}
```

Same expansion for `extract_preconditions()` with matching keyword patterns.

---

## Example Chains After Expansion

| Chain | Type | Severity |
|-------|------|----------|
| Memory leak → OOM → crash | ResourceCascade | 7.5 (max_sev=6 + length=2×0.5 + crash=1.0) |
| Deadlock → timeout → watchdog crash | DeadlockCascade | 8.0 (max_sev=5 + length=2×0.5 + crash=1.0 + deadlock=1.0) |
| Data corruption → inconsistency → regression → crash | DataCorruptionChain | 10.0 (max_sev=6 + length=3×0.5 + corruption=1.5 + regression=1.2) |
| Info leak → ASLR bypass → overflow → RCE | SecurityExploit | 10.0 (max_sev=5 + length=3×0.5 + RCE=2.0 + trust=1.5) |
| Incompatibility → regression → timeout | IncompatibilityChain | 6.3 (max_sev=3 + length=2×0.5 + incompat=0.8 + regression=1.2) |

---

## Implementation Checklist

1. Expand `EffectType` enum (chain.rs: line ~10) — add 9 variants
2. Expand `PreconditionType` enum (chain.rs: line ~30) — add 9 variants
3. Add `ChainType` enum (chain.rs: new section)
4. Add `chain_type` field to `EffectPreconditionMatch` struct
5. Add `chain_type` field to `ExploitChain` struct
6. Expand `extract_effects()` with new keyword patterns
7. Expand `extract_preconditions()` with new keyword patterns
8. Replace `compute_chain_severity()` with category-aware version
9. Add `derive_chain_type()` function — determines ChainType from terminal bug category
10. Add 10+ tests for cross-category chains (memory leak→timeout, deadlock→crash, etc.)
11. Update daemon handler to pass chain_type to severity calculator
12. Verify all existing 13 chain tests still pass

## What Doesn't Need Changing (Verified)

- `ChainSemanticMatcher` — Jaro-Winkler over text, category-agnostic ✓
- `build_chain_graph()` — adjacency list construction, category-agnostic ✓
- `detect_chains()` — BFS traversal, category-agnostic ✓
- `EdgeKind::Enables` — already exists in types.rs ✓
- Daemon `suggest_chain` handler — calls same functions ✓
- Agent `suggest_chain` tool — returns chains with new fields ✓
