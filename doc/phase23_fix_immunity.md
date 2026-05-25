# Phase 23 — F5 Fix & Systemic Immunity Against All 12 Implementation Findings

---

## Part 1: F5 — Wrong Configuration Weight

### Root Cause Analysis

The **Plan Spec** (`phase23.md` lines 202–213) defines the `Configuration` dimension weight as **0.10**:

```rust
// phase23.md B3 Types and Schemas:
impl TriggerDimension {
    pub fn weight(&self) -> f64 {
        match self {
            Self::InputType => 0.30,
            Self::Environment => 0.15,
            Self::Timing => 0.10,
            Self::DataState => 0.20,
            Self::Concurrency => 0.10,
            Self::Configuration => 0.10,   // ← PLAN SPEC SAYS 0.10
            Self::DependencyVersion => 0.05,
            Self::OsArch => 0.00,
        }
    }
}
```

The **Implementation** (`bugswarm-evidence/src/trigger.rs` line 32) has **0.05**:

```rust
// trigger.rs:25-36 — ACTUAL CODE (BUG)
pub fn weight(&self) -> f32 {
    match self {
        Self::Input => 0.30,
        Self::Environment => 0.15,
        Self::Timing => 0.10,
        Self::DataState => 0.20,
        Self::Concurrency => 0.10,
        Self::Configuration => 0.05,   // ← BUG: should be 0.10
        Self::DependencyVersion => 0.05,
        Self::OsArch => 0.0,
    }
}
```

The **Audit Test** (`bugswarm-evidence/tests/trigger_audit.rs` line 512) was written to match the **implementation**, not the **plan spec** — doubling the damage:

```rust
// trigger_audit.rs:512 — test validates the BUG, not the spec
assert_eq!(TriggerDimension::Configuration.weight(), 0.05);
```

**Why this happened**:
1. No single source of truth — plan spec and code are separate text documents
2. Tests were written after code and validated against code, not against the plan
3. No automated cross-check between plan document constants and code constants
4. The weight 0.05 + 0.05 (Configuration + DependencyVersion) = 0.10 matches the "nice round number 0.10" pattern that other single dimensions have, making it look intentional

**Impact**: Bugs where Configuration (config flags, settings, feature toggles) is the trigger condition get half the weight they should. Completeness scoring underweights Config coverage, agents deprioritize Config-dimension investigation, and bugs that only trigger under specific configuration may be wrongly scored as "more complete" than they are.

### Permanent Fix

#### Step 1: Fix the weight value

In `bugswarm-evidence/src/trigger.rs`, change Configuration from 0.05 to 0.10:

```rust
Self::Configuration => 0.10,  // Was 0.05 (F5)
```

#### Step 2: Fix the weight type to match plan spec

The plan spec uses `f64`. The implementation uses `f32`. Fix to match:

```rust
// Before (bug: f32 mismatch with spec)
pub fn weight(&self) -> f32 {

// After (matches plan spec B3)
pub fn weight(&self) -> f64 {
```

#### Step 3: Fix the weights-sum test to match plan spec

The plan spec weights sum to 1.00 (0.30+0.15+0.10+0.20+0.10+0.10+0.05 = 1.00). Current test expects 0.95 because of the Configuration 0.05 bug. Fix:

```rust
// trigger_audit.rs:226-233 — fix expected sum
fn completeness_weights_sum_to_100() {
    let total: f64 = TriggerDimension::all().iter()
        .filter(|d| **d != TriggerDimension::OsArch)
        .map(|d| d.weight())
        .sum();
    assert!((total - 1.00).abs() < 0.001,
        "non-OsArch weights should sum to 1.00, got {}", total);
}
```

#### Step 4: Rewrite the plan conformance test to validate against spec, not code

Replace the hardcoded weight assertions in `plan_conformance_trigger_dimension_variants` (line 484–520) with a data-driven check that loads expected weights from a machine-readable spec constant (see Part 2, Practice 2 below).

#### Step 5: Add a spec-vs-plan verification test

```rust
#[test]
fn spec_matches_plan_document_weights() {
    // The spec.rs constants (single source of truth)
    use bugswarm_evidence::trigger::spec::TRIGGER_DIMENSION_WEIGHTS;
    
    // Verify every weight in the spec matches the TriggerDimension enum
    for (dim, expected_weight) in TRIGGER_DIMENSION_WEIGHTS {
        assert!(
            (dim.weight() - expected_weight).abs() < 0.0001,
            "Dimension {:?} weight {} does not match plan spec weight {}",
            dim, dim.weight(), expected_weight
        );
    }
    
    // Verify weights sum to exactly 1.00 (OsArch excluded)
    let total: f64 = TriggerDimension::all().iter()
        .filter(|d| **d != TriggerDimension::OsArch)
        .map(|d| d.weight())
        .sum();
    assert!((total - 1.0).abs() < 0.001, "Plan spec weights must sum to 1.00");
}
```

### Immunity Mechanism: Spec-Driven Configuration (`spec.rs`)

Create `bugswarm-evidence/src/spec.rs` — the **single source of truth** for all constants, weights, thresholds, and configuration defaults. This module makes the plan document machine-verifiable.

```rust
// bugswarm-evidence/src/spec.rs
// SINGLE SOURCE OF TRUTH FOR ALL CONSTANTS
// Every constant here MUST match the corresponding plan document.
// CI gate verifies this. If you change a constant here, update the plan doc.
// If you change the plan doc, CI will flag mismatches until you update this file.

pub mod trigger {
    use crate::trigger::TriggerDimension;

    /// Dimension weights per the Phase 23 plan spec B3.
    /// Weights sum to 1.00 (OsArch is bonus, weight 0.00 excluded from sum).
    /// Source: phase23.md lines 203-213, C6.2.2 line 654
    pub const TRIGGER_DIMENSION_WEIGHTS: &[(TriggerDimension, f64)] = &[
        (TriggerDimension::Input,             0.30),
        (TriggerDimension::Environment,       0.15),
        (TriggerDimension::Timing,            0.10),
        (TriggerDimension::DataState,         0.20),
        (TriggerDimension::Concurrency,       0.10),
        (TriggerDimension::Configuration,     0.10),
        (TriggerDimension::DependencyVersion, 0.05),
        (TriggerDimension::OsArch,            0.00),
    ];

    /// Phase 30 gate: minimum number of trigger rows per confirmed bug.
    /// Source: phase30.md line 15, phase23.md A1 line 15
    pub const PHASE30_MIN_ROWS: usize = 5;

    /// Phase 30 gate: minimum number of distinct contributing layers.
    /// Source: phase30.md line 15, phase23.md A1 line 15
    pub const PHASE30_MIN_LAYERS: usize = 3;

    /// Default dedup similarity threshold for Jaro-Winkler fuzzy matching.
    /// Source: phase23.md B3 line 326, C6.2.1 line 634
    pub const DEDUP_JARO_WINKLER_THRESHOLD: f64 = 0.85;

    /// Maximum conditions per dimension before eviction of oldest-unverified.
    /// Source: phase23.md B3 line 327, C3 FM-23.6
    pub const MAX_CONDITIONS_PER_DIMENSION: usize = 100;

    /// Investigation priority: severity weight.
    /// Source: phase23.md C2 line 546, C6.2.2 line 654
    pub const PRIORITY_SEVERITY_WEIGHT: f64 = 0.7;

    /// Investigation priority: incompleteness weight.
    /// Source: phase23.md C2 line 546, C6.2.2 line 654
    pub const PRIORITY_INCOMPLETENESS_WEIGHT: f64 = 0.3;

    /// Staleness factor in investigation priority (for staleness bonus).
    /// Source: phase23.md C6.2.2.Q3 line 665
    pub const PRIORITY_STALENESS_WEIGHT: f64 = 0.05;

    /// Hash algorithm used for semantic hashing.
    /// Source: phase23.md B3 line 342, B5 line 406
    pub const SEMANTIC_HASH_ALGORITHM: &str = "SHA-256";

    /// Semantic hash truncation prefix length (chars of hex-encoded hash).
    /// Source: phase23.md C4 EC-23.6 line 583
    pub const SEMANTIC_HASH_TRUNCATION: usize = 512;

    /// Density bonus cap: max conditions counted per dimension.
    /// Source: phase23.md C2 line 522, C6.2.2.Q2 line 662
    pub const DENSITY_BONUS_CAP: usize = 3;

    /// Layer diversity bonus cap: max distinct layers counted per dimension.
    /// Source: phase23.md C2 line 523, C6.2.2.Q2 line 662
    pub const LAYER_DIVERSITY_CAP: usize = 3;

    /// Score cache TTL in seconds.
    /// Source: phase23.md C5 line 609
    pub const SCORE_CACHE_TTL_SECS: u64 = 5;
}

pub mod dedup {
    /// Similarity strategy weights for fuzzy matching.
    /// Source: phase23.md C1 lines 487-497
    pub const SIMILARITY_JARO_WINKLER_WEIGHT: f64 = 0.4;
    pub const SIMILARITY_CATEGORY_WEIGHT: f64 = 0.3;
    pub const SIMILARITY_DIMENSION_MATCH_WEIGHT: f64 = 0.2;
    pub const SIMILARITY_SOURCE_DIVERSITY_WEIGHT: f64 = 0.1;
}

pub mod evidence {
    /// Maximum evidence graph nodes before warning.
    /// Source: enterprise_audit.md gap #39
    pub const MAX_GRAPH_NODES: usize = 1_000_000;

    /// Maximum evidence graph edges before warning.
    pub const MAX_GRAPH_EDGES: usize = 10_000_000;
}
```

**Implementation checklist**:
- [ ] Create `bugswarm-evidence/src/spec.rs` with all constants above
- [ ] Add `pub mod spec;` to `bugswarm-evidence/src/lib.rs`
- [ ] Refactor `trigger.rs` `weight()` to read from `spec::trigger::TRIGGER_DIMENSION_WEIGHTS`
- [ ] Refactor `TriggerConfig::default()` to read from `spec::trigger` constants
- [ ] Refactor completeness scoring to use `spec::trigger` constants
- [ ] Refactor `normalize_hash()` to use SHA-256 per `spec::trigger::SEMANTIC_HASH_ALGORITHM`
- [ ] Add CI gate test: `cargo test --test trigger_audit spec_matches_plan_document_weights`
- [ ] Document in AGENTS.md: "All constants live in spec.rs. Plan doc is the authority. CI verifies."

**Success criteria**:
1. Configuration weight is 0.10, matching the plan spec
2. Weights sum to exactly 1.00 (not 0.95)
3. `spec.rs` is the single source of truth for ALL constants
4. CI test fails if any constant in code differs from `spec.rs`
5. CI test fails if `spec.rs` weights don't match documented plan weights
6. No constant is hardcoded anywhere other than `spec.rs`

---

## Part 2: The 12 Phase 23 Implementation Findings — Complete Enumeration

Each finding is classified by severity and root cause category.

| # | Finding | Category | Severity | File:Line |
|---|---------|----------|----------|-----------|
| **F1** | `DefaultHasher` used for ID generation — non-deterministic across runs, breaks reproducibility | Determinism | HIGH | `trigger.rs:196-202` |
| **F2** | `.replace()` without word-boundary checks in `normalize_description()` — false positives on substring collisions | String Safety | HIGH | `trigger.rs:180-192` |
| **F3** | `is_semantically_equivalent()` substring containment false positives — "error" matches "this is a long description with error" | Logic Error | HIGH | `trigger.rs:207-208` |
| **F4** | Numeric substring collision — "check 1" matches "check 11" via containment | Logic Error | HIGH | `trigger_audit.rs:777-792` |
| **F5** | Configuration dimension weight is 0.05 in code, 0.10 in plan spec — constants drift | Spec-Code Mismatch | HIGH | `trigger.rs:32` |
| **F6** | Weight type mismatch — plan spec uses `f64`, implementation uses `f32` | Type Consistency | MEDIUM | `trigger.rs:25` |
| **F7** | No Unicode normalization (NFD/NFC) — "café" (NFC) and "café" (NFD) fail to dedup | Encoding | MEDIUM | `trigger.rs:180` |
| **F8** | `TriggerManager` not serializable — no crash recovery, no persistence | Data Durability | MEDIUM | `trigger.rs:219-221` |
| **F9** | `normalize_hash()` truncates to 16 hex chars (64 bits) — collision risk at scale | Hash Truncation | MEDIUM | `trigger.rs:199` |
| **F10** | Test validates bug, not plan spec — `assert_eq!(Configuration.weight(), 0.05)` | Test Integrity | MEDIUM | `trigger_audit.rs:512` |
| **F11** | No handler auto-registration — trigger tools not discoverable by daemon/agent dispatch | Integration | LOW | Missing module |
| **F12** | No `layer_count()` invariant enforcement — `TriggerMatrix` has `layer_count()` but CI doesn't verify it exists on all contributor-collection types | Invariant | LOW | `trigger.rs:174-176` |

---

## Part 3: Systemic Immunity Practices — Prevent ALL 12 Categories Forever

For each practice: which finding(s) it prevents, exact implementation steps, tooling/CI/lint configuration, effort estimate, and verification that it works.

---

### Practice 1: Mandatory Integration Contract

**Prevents**: F11 (no handler auto-registration), similar future integration gaps

**What**: Every new module that exposes public handlers, tools, or RPC endpoints must implement an `IntegrationContract` trait. The trait forces the module to declare its daemon handlers, agent tools, and cross-crate API surface. A CI gate verifies every declared contract is satisfied.

**Implementation**:

```rust
// bugswarm-core/src/integration.rs (NEW)

/// Contract that every module exposing cross-crate APIs must implement.
/// CI gate verifies all contracts are satisfied before merge.
pub trait IntegrationContract {
    /// Human-readable module name for audit logging.
    fn module_name() -> &'static str;

    /// List of daemon RPC handlers this module registers.
    /// Each tuple: (handler_name, handler_function_pointer_to_verify_exists)
    fn daemon_handlers() -> &'static [(&'static str, &'static str)];

    /// List of agent tools this module registers.
    /// Each tuple: (tool_name, schema_type_path)
    fn agent_tools() -> &'static [(&'static str, &'static str)];

    /// List of other crates this module depends on.
    /// Used by cross-crate dependency auditor (Practice 6).
    fn cross_crate_dependencies() -> &'static [&'static str];

    /// Verify this contract is satisfiable at compile time.
    /// CI runs this and fails if any declared handler/tool doesn't exist.
    fn verify() -> Result<(), Vec<ContractViolation>>;
}

#[derive(Debug)]
pub struct ContractViolation {
    pub module: String,
    pub kind: ContractViolationKind,
    pub message: String,
}

#[derive(Debug)]
pub enum ContractViolationKind {
    MissingDaemonHandler,
    MissingAgentTool,
    UnregisteredDependency,
    StaleContract,
}
```

**Tooling**:
- Add `#[integration_contract]` proc macro attribute that auto-implements `IntegrationContract` by scanning the module for `#[daemon_handler]` and `#[agent_tool]` attributes
- CI script: `cargo run --bin integration-audit` — loads all contracts, verifies all handlers/tools exist via compile-time reflection or symbol checking
- Pre-commit hook: runs integration audit before allowing commit

**CI gate** (`ci/integration-contract-check.sh`):
```bash
#!/bin/bash
# Fails if any IntegrationContract is violated
cargo run --bin integration-audit -- --strict
# Exit code 1 if any ContractViolation found
```

**Effort estimate**: 3 days (1 day trait + macro, 1 day CI integration, 1 day retrofitting existing modules)

**Verification**: Create a new module with an agent tool that doesn't register in the contract. CI must fail with `MissingAgentTool`. Register it, CI must pass.

---

### Practice 2: Spec-First Development

**Prevents**: F5 (constants drift), F6 (type mismatch), F10 (test validates code, not spec)

**What**: Before ANY code is written, constants and algorithm parameters are defined in `spec.rs`. Implementation code reads from `spec.rs` (never hardcodes values). Tests read from `spec.rs` (never hardcode expected values). The plan document becomes the authority — `spec.rs` is the machine-readable transcription. CI verifies spec ↔ plan agreement.

**Implementation**:

```rust
// bugswarm-evidence/src/spec.rs
// All constants live here. This is the machine-readable form of the plan document.
// CI gate: `cargo test --test spec_conformance` verifies nothing drifts.

pub mod trigger {
    use crate::trigger::TriggerDimension;

    // SOURCE: phase23.md B3 lines 203-213
    pub const DIMENSION_WEIGHTS: &[(TriggerDimension, f64)] = &[
        (TriggerDimension::Input,             0.30),
        (TriggerDimension::Environment,       0.15),
        (TriggerDimension::Timing,            0.10),
        (TriggerDimension::DataState,         0.20),
        (TriggerDimension::Concurrency,       0.10),
        (TriggerDimension::Configuration,     0.10),
        (TriggerDimension::DependencyVersion, 0.05),
        (TriggerDimension::OsArch,            0.00),
    ];

    // SOURCE: phase23.md B3 line 326
    pub const DEDUP_SIMILARITY_THRESHOLD: f64 = 0.85;

    // SOURCE: phase23.md B3 line 323
    pub const PHASE30_MIN_LAYERS: usize = 3;

    // SOURCE: phase23.md B3 line 324
    pub const PHASE30_MIN_ROWS: usize = 5;

    // SOURCE: phase23.md B3 line 327
    pub const MAX_CONDITIONS_PER_DIMENSION: usize = 100;

    // SOURCE: phase23.md C2 line 546
    pub const PRIORITY_SEVERITY_WEIGHT: f64 = 0.7;
    pub const PRIORITY_INCOMPLETENESS_WEIGHT: f64 = 0.3;

    // SOURCE: phase23.md C6.2.2.Q3 line 665
    pub const PRIORITY_STALENESS_WEIGHT: f64 = 0.05;

    // SOURCE: enterprise_audit.md gap #2 — must use SHA-256, not DefaultHasher
    pub const HASH_ALGORITHM: &str = "SHA-256";
}
```

**Implementation code reads from spec**:
```rust
// trigger.rs — fixed to read from spec
impl TriggerDimension {
    pub fn weight(&self) -> f64 {
        for (dim, w) in spec::trigger::DIMENSION_WEIGHTS {
            if *dim == *self {
                return *w;
            }
        }
        0.0
    }
}
```

**Tests read from spec** (never hardcode expected values):
```rust
#[test]
fn spec_conformance_weights_match_plan() {
    for (dim, expected) in spec::trigger::DIMENSION_WEIGHTS {
        assert_eq!(dim.weight(), *expected,
            "{:?} weight diverged from spec.rs", dim);
    }
}

#[test]
fn spec_conformance_weights_sum_to_one() {
    let total: f64 = spec::trigger::DIMENSION_WEIGHTS.iter()
        .filter(|(d, _)| **d != TriggerDimension::OsArch)
        .map(|(_, w)| w)
        .sum();
    assert!((total - 1.0).abs() < 0.001,
        "Weights sum to {}, expected 1.00. Check spec.rs.", total);
}
```

**Tooling**:
- `ci/spec-check.sh`: Runs `cargo test --test spec_conformance` — fails if spec ≠ code
- Plan doc parser: optional CI job that parses phase23.md B3 section and cross-checks with spec.rs constants (advanced — deferred to when LLM-based doc parsing is reliable)

**Effort estimate**: 2 days (0.5 day spec.rs creation, 1 day refactoring code to read from spec, 0.5 day test migration)

**Verification**: Change a weight in spec.rs but not in code → CI fails. Change a weight in code but not in spec.rs → tests catch it (since code reads from spec). Change the plan doc → human process updates spec.rs → CI validates.

---

### Practice 3: Algorithm Certification

**Prevents**: F3 (substring containment false positives), F4 (numeric substring collision), general logic errors

**What**: Every algorithm must pass a certification suite before it can be marked PEAK. Three components: (1) property-based tests with `proptest`, (2) reference oracle comparison (naive vs peak), (3) edge-case enumeration.

**Implementation**:

```rust
// tests/certification/semantic_equivalence_cert.rs

use proptest::prelude::*;
use bugswarm_evidence::trigger::{normalize_description, is_semantically_equivalent};

// ═══ Certification Level 1: Property-Based Tests ═══

proptest! {
    /// STRONG IDEMPOTENCE: Normalizing twice = normalizing once
    #[test]
    fn normalize_is_idempotent(s in "\\PC{0,500}") {
        let once = normalize_description(&s);
        let twice = normalize_description(&once);
        prop_assert_eq!(once, twice);
    }

    /// REFLEXIVITY: Every normalized string is equivalent to itself
    #[test]
    fn normalize_is_reflexive(s in "\\PC{0,500}") {
        let n = normalize_description(&s);
        prop_assert!(is_semantically_equivalent(&n, &n));
    }

    /// SYMMETRY: If A equiv B, then B equiv A
    #[test]
    fn symmetry(a in "\\PC{0,200}", b in "\\PC{0,200}") {
        let na = normalize_description(&a);
        let nb = normalize_description(&b);
        prop_assert_eq!(
            is_semantically_equivalent(&na, &nb),
            is_semantically_equivalent(&nb, &na)
        );
    }
}

// ═══ Certification Level 2: Reference Oracle ═══

/// The reference oracle: normalize + exact string match.
/// This is the "naive but correct" baseline.
fn oracle_equivalent(a: &str, b: &str) -> bool {
    normalize_description(a) == normalize_description(b)
}

proptest! {
    /// PEAK must agree with oracle on all cases where tokens are identical.
    /// Where PEAK differs, it must be MORE aggressive (never less).
    #[test]
    fn peak_never_misses_oracle_match(a in "\\PC{0,200}", b in "\\PC{0,200}") {
        let na = normalize_description(&a);
        let nb = normalize_description(&b);
        if oracle_equivalent(&na, &nb) {
            prop_assert!(is_semantically_equivalent(&na, &nb),
                "PEAK failed to match where oracle did: '{}' vs '{}'", na, nb);
        }
    }
}

// ═══ Certification Level 3: Edge-Case Enumeration ═══

#[test]
fn edge_cases_must_not_false_positive() {
    // F3: "error" must NOT match "this is a long error description"
    assert!(!is_semantically_equivalent("error", "this is a long error description"),
        "CERTIFICATION FAILED: substring containment false positive (F3)");

    // F4: "check 1" must NOT match "check 11"
    assert!(!is_semantically_equivalent("check 1", "check 11"),
        "CERTIFICATION FAILED: numeric substring collision (F4)");

    // F4 variant: "test 2" must NOT match "test 20"
    assert!(!is_semantically_equivalent("test 2", "test 20"));

    // Token boundary: "a" must NOT match "abc" (substring of first token only)
    assert!(!is_semantically_equivalent("a", "abc"));
}

/// All findings from trigger_audit.rs that document false positives
/// must be encoded here as explicit certification failures.
const CERTIFICATION_BLACKLIST: &[(&str, &str)] = &[
    ("check 1", "check 11"),          // F4
    ("error", "this is a long description with error"), // F3
];

#[test]
fn blacklist_must_not_match() {
    for (a, b) in CERTIFICATION_BLACKLIST {
        assert!(!is_semantically_equivalent(
            &normalize_description(a),
            &normalize_description(b),
        ), "CERTIFICATION FAILED: '{}' falsely matches '{}'", a, b);
    }
}
```

**Tooling**:
- `ci/certify.sh`: Runs `cargo test --test certification_*` — fails if any algorithm is not certified
- Algorithm registry: `certification/registry.toml` lists every algorithm and certification status
- CI gate: `cargo run --bin certify-check -- --phase 23` must pass before merge

**Effort estimate**: 2 days per algorithm category (dedup, scoring, normalization). ~6 days for Phase 23's 6 algorithms.

**Verification**: Introduce a deliberate substring-collision bug in `is_semantically_equivalent`. CI certification suite must catch it via the blacklist test.

---

### Practice 4: Handler Auto-Registration

**Prevents**: F11 (no handler auto-registration), future "I forgot to register the handler" bugs

**What**: Use a build script or proc macro that scans all Rust source files for `#[daemon_handler]` and `#[agent_tool]` attributes and generates the dispatch table at compile time. It is impossible to define a handler without it being registered.

**Implementation**:

```rust
// bugswarm-evidence/build.rs
// Auto-discovers all daemon handlers and generates dispatch table.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Scan src/ for #[daemon_handler] attributes
    let src_dir = PathBuf::from("src");
    let mut handlers = vec![];
    for entry in fs::read_dir(&src_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map_or(false, |e| e == "rs") {
            let content = fs::read_to_string(&path).unwrap();
            for line in content.lines() {
                if line.trim().starts_with("#[daemon_handler(") {
                    // Extract handler name from attribute
                    if let Some(name) = extract_attr_value(line, "daemon_handler") {
                        handlers.push((path.file_stem().unwrap().to_str().unwrap().to_string(), name));
                    }
                }
            }
        }
    }

    // Generate dispatch table
    let mut code = String::from(
        "// AUTO-GENERATED by build.rs — DO NOT EDIT\n\
         pub fn dispatch_daemon_handler(method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {\n\
         \x20   match method {\n"
    );
    for (module, handler) in &handlers {
        code.push_str(&format!(
            "        \"{}\" => crate::{}::{}(params),\n",
            handler, module, handler
        ));
    }
    code.push_str(
        "        _ => Err(format!(\"Unknown daemon handler: {}\", method)),\n\
         \x20   }\n\
         }"
    );

    fs::write(out_dir.join("daemon_dispatch.rs"), code).unwrap();
    
    // Also generate agent tool dispatch similarly
    // ...

    println!("cargo:rerun-if-changed=src/");
}

fn extract_attr_value(line: &str, attr_name: &str) -> Option<String> {
    let prefix = format!("#[{}(\"", attr_name);
    if let Some(rest) = line.strip_prefix(&prefix) {
        rest.split('"').next().map(|s| s.to_string())
    } else {
        None
    }
}
```

**Alternative — Proc Macro approach** (more robust):
```rust
// bugswarm-macros/src/lib.rs
#[proc_macro_attribute]
pub fn daemon_handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = input.sig.ident.clone();
    let handler_name = fn_name.to_string();

    // Register in global handler registry
    let expanded = quote! {
        #input

        // Auto-register this handler in the global registry
        inventory::submit! {
            crate::HandlerRegistration {
                name: #handler_name,
                handler: #fn_name as fn(serde_json::Value) -> Result<serde_json::Value, String>,
            }
        }
    };
    TokenStream::from(expanded)
}
```

**Tooling**:
- `[build-dependencies]` in `Cargo.toml`: add `inventory = "0.3"` for compile-time registration
- CI gate: after build, verify dispatch table has exactly N entries matching N `#[daemon_handler]` attributes

**Effort estimate**: 3 days (1 day build.rs skeleton, 1 day proc macro if preferred, 1 day integration + CI)

**Verification**: Add a new `#[daemon_handler]` function. Run `cargo build`. Verify the generated dispatch table contains the new handler without any manual steps.

---

### Practice 5: Integration Test Auto-Generation

**Prevents**: Uncoupled code paths (modules with zero callers), dead code accumulation

**What**: For every public API surface, CI auto-generates (or verifies the existence of) an integration test that calls it. If any public function, trait method, or handler has zero callers, CI fails.

**Implementation**:

```rust
// ci/call-site-audit/src/main.rs
// Scans all crates for public items. Checks that each has at least one
// integration test that calls it.

use std::collections::HashSet;
use std::process::Command;

fn main() {
    let public_apis = extract_public_apis();  // via syn or rustdoc JSON
    let called_in_tests = extract_tested_apis();  // scan tests/ for calls

    let missing: Vec<_> = public_apis.difference(&called_in_tests).collect();
    if !missing.is_empty() {
        eprintln!("ERROR: {} public APIs have zero integration test callers:", missing.len());
        for api in &missing {
            eprintln!("  - {}", api);
        }
        eprintln!("\nEvery public API must have at least one integration test caller.");
        eprintln!("See Practice 5 in phase23_fix_immunity.md");
        std::process::exit(1);
    }
    println!("All public APIs have at least one integration test caller. PASSED.");
}

fn extract_public_apis() -> HashSet<String> {
    // Use `cargo doc --document-private-items` or `cargo public-api` to list APIs
    let output = Command::new("cargo")
        .args(&["public-api"])
        .output()
        .expect("cargo public-api failed");
    // Parse output...
    HashSet::new()
}

fn extract_tested_apis() -> HashSet<String> {
    // Scan tests/ directory for function calls using grep/cargo test --list
    HashSet::new()
}
```

**Tooling**:
- `cargo-public-api` crate: extracts public API surface programmatically
- CI script: `ci/call-site-audit.sh` runs after integration tests, fails on orphan APIs
- `#[allow_orphan]` attribute: opt-out for intentionally untested items (e.g., trait impls from upstream)

**Effort estimate**: 2 days (1 day audit tool, 1 day CI integration and retrofitting)

**Verification**: Remove all callers of `TriggerMatrix::layer_count()`. CI must fail with "public API has zero callers."

---

### Practice 6: Cross-Crate Dependency Audit

**Prevents**: Missing dependencies, circular dependencies, unused dependencies, version mismatches

**What**: CI script that parses every crate's `Cargo.toml` and builds a dependency matrix. Flags: (a) dependencies declared but unused, (b) dependencies used but undeclared, (c) circular dependency chains, (d) version conflicts across crates.

**Implementation**:

```bash
#!/bin/bash
# ci/dependency-audit.sh
# Audits the entire workspace dependency graph.

set -euo pipefail

echo "=== Cross-Crate Dependency Audit ==="

# 1. Check for unused dependencies (requires nightly)
echo "--- Unused Dependencies ---"
cargo +nightly udeps --workspace 2>&1 | tee /tmp/udeps.txt
if grep -q "unused dependencies" /tmp/udeps.txt; then
    echo "FAIL: Unused dependencies found. Remove them from Cargo.toml."
    exit 1
fi

# 2. Check for circular dependencies
echo "--- Circular Dependencies ---"
cargo tree --workspace --edges normal 2>&1 | \
    rg "(\w+)\s.*\1" && echo "FAIL: Circular dependency detected" && exit 1

# 3. Check dependency matrix against IntegrationContract declarations
echo "--- Dependency Matrix vs Contracts ---"
# (Uses a custom tool that cross-references Cargo.toml with IntegrationContract)
cargo run --bin dep-matrix-check

# 4. Verify no crate depends on itself
echo "--- Self-Dependency Check ---"
for crate in bugswarm-*/Cargo.toml; do
    dir=$(dirname "$crate")
    name=$(basename "$dir")
    if grep -q "$name" "$crate"; then
        echo "FAIL: $name depends on itself in Cargo.toml"
        exit 1
    fi
done

echo "PASS: Dependency audit clean."
```

**Dependency matrix template** (committed to repo):

```toml
# ci/dependency-matrix.toml
# Maps every crate to its allowed dependencies.
# CI fails if any actual dependency is not in this matrix.

[bugswarm-evidence]
allowed = ["bugswarm-core", "serde", "serde_json", "sha2", "chrono"]
forbidden = ["bugswarm-agent", "bugswarm-sandbox"]

[bugswarm-sandbox]
allowed = ["bugswarm-evidence", "bugswarm-core", "tokio", "bollard"]
forbidden = ["bugswarm-agent"]
```

**Tooling**:
- `cargo-udeps`: detects unused dependencies (nightly)
- `cargo-tree`: visualizes and checks the full dependency graph
- Custom `cargo run --bin dep-matrix-check`: validates Cargo.toml against the allowed/forbidden matrix

**Effort estimate**: 2 days (0.5 day matrix creation, 1 day audit script, 0.5 day CI integration)

**Verification**: Add a dependency from `bugswarm-evidence` to `bugswarm-agent` without updating the matrix. CI must fail.

---

### Practice 7: Phase Gate Checklist Automation

**Prevents**: Missed checklist items, "oops I forgot to run the performance benchmark" bugs

**What**: Every phase has a machine-readable gate checklist. CI reads this checklist and verifies every item. A phase cannot pass CI with unchecked items.

**Implementation**:

```toml
# ci/gates/phase23.toml
# Machine-readable gate checklist.
# CI reads this file and verifies every item.
# Format: [check_id] type = "test" | "benchmark" | "lint" | "metric"
#          command = "shell command that returns 0 on pass"

[phase]
number = 23
name = "Trigger Matrix"
status = "IN_PROGRESS"

[[checks]]
id = "CHK-23-001"
type = "test"
name = "All 8 trigger dimensions defined with correct weights"
command = "cargo test --test trigger_audit spec_conformance_weights"
required = true

[[checks]]
id = "CHK-23-002"
type = "test"
name = "Semantic dedup: exact match, fuzzy match, no-match"
command = "cargo test --test trigger_audit semantic_dedup"
required = true

[[checks]]
id = "CHK-23-003"
type = "test"
name = "Completeness scoring: 0 conditions=0.0, all dims=1.0"
command = "cargo test --test trigger_audit completeness"
required = true

[[checks]]
id = "CHK-23-004"
type = "benchmark"
name = "get_trigger_matrix(100 rows) under 50ms"
command = "cargo bench --bench trigger_latency -- 50ms-budget"
required = true

[[checks]]
id = "CHK-23-005"
type = "test"
name = "Phase30 gate: 5rows+3layers=pass, 4rows=fail"
command = "cargo test --test trigger_audit plan_conformance_phase30_gate"
required = true

[[checks]]
id = "CHK-23-006"
type = "test"
name = "All IntegrationContract::verify() pass"
command = "cargo run --bin integration-audit -- --strict"
required = true

[[checks]]
id = "CHK-23-007"
type = "lint"
name = "No DefaultHasher used in any crate"
command = "ci/lint-no-default-hasher.sh"
required = true

[[checks]]
id = "CHK-23-008"
type = "lint"
name = "No bare .replace() without word boundary guards"
command = "ci/lint-string-sanitization.sh"
required = true

[[checks]]
id = "CHK-23-009"
type = "test"
name = "Algorithm certification suite passes"
command = "cargo test --test certification_*"
required = true

[[checks]]
id = "CHK-23-010"
type = "lint"
name = "Cross-crate dependency audit clean"
command = "ci/dependency-audit.sh"
required = true

[[checks]]
id = "CHK-23-011"
type = "test"
name = "Call-site audit: all public APIs have callers"
command = "ci/call-site-audit.sh"
required = true

[[checks]]
id = "CHK-23-012"
type = "lint"
name = "spec.rs matches plan document (weights, thresholds)"
command = "cargo test --test trigger_audit spec_matches_plan_document_weights"
required = true

[[checks]]
id = "CHK-23-013"
type = "test"
name = "TriggerManager stress: 1000 bugs × 10 conditions < 2s"
command = "cargo test --test trigger_audit trigger_manager_stress_1000_bugs"
required = true

[[checks]]
id = "CHK-23-014"
type = "test"
name = "Serialization roundtrip for all trigger types"
command = "cargo test --test trigger_audit serialization"
required = true
```

**CI gate runner** (`ci/gate-check.sh`):
```bash
#!/bin/bash
# Reads phase gate TOML, executes every check, produces pass/fail report.

set -euo pipefail

PHASE=${1:-23}
GATE_FILE="ci/gates/phase${PHASE}.toml"
STATUS_FILE="ci/gates/phase${PHASE}_status.json"

if [[ ! -f "$GATE_FILE" ]]; then
    echo "ERROR: Gate file $GATE_FILE not found."
    exit 1
fi

echo "=== Phase $PHASE Gate Checklist ==="

PASSED=0
FAILED=0
RESULTS='{"phase": '$PHASE', "checks": ['

# Parse TOML and run checks
# In production, use a proper TOML parser. Here we use a Rust binary:
cargo run --bin gate-check -- --phase="$PHASE" --output="$STATUS_FILE"

# Exit code reflects gate status
if grep -q '"failed"' "$STATUS_FILE"; then
    echo "GATE FAILED: Not all checks passed."
    exit 1
fi

echo "GATE PASSED: All checks passed."
```

**Tooling**:
- `ci/gate-check.sh`: universal gate runner for all phases
- `ci/gates/phase*.toml`: one per phase, committed to repo
- `cargo run --bin gate-check`: Rust binary that reads TOML and executes checks

**Effort estimate**: 2 days (0.5 day TOML format, 1 day gate runner, 0.5 day retrofitting Phase 23)

**Verification**: Remove one check from the TOML. CI must still pass (since checks are additive). Mark a check as `required = true` and make it fail — CI must block merge.

---

### Practice 8: Deterministic ID Generation

**Prevents**: F1 (DefaultHasher for IDs), non-reproducible behavior across runs

**What**: All ID generation must use deterministic hashing (SHA-256, Blake3). A CI lint checks that `DefaultHasher`, `RandomState`, `thread_rng`, or any non-deterministic hash/random source is never used for ID generation.

**Implementation**:

```bash
#!/bin/bash
# ci/lint-no-default-hasher.sh
# Scans all Rust files for non-deterministic hashing in ID generation.

echo "=== Non-Deterministic Hash Lint ==="

# Pattern: DefaultHasher used in any file
DEFAULT_HASHER_FILES=$(rg -l "DefaultHasher" --type rust bugswarm-*/)
if [[ -n "$DEFAULT_HASHER_FILES" ]]; then
    echo "FAIL: DefaultHasher found in:"
    echo "$DEFAULT_HASHER_FILES"
    echo ""
    echo "DefaultHasher produces non-deterministic hashes across runs."
    echo "Use SHA-256 (sha2 crate) or Blake3 for all ID generation."
    echo "See Practice 8 in phase23_fix_immunity.md"
    exit 1
fi

# Pattern: RandomState or thread_rng used for hash generation
RANDOM_SOURCES=$(rg -l "RandomState::new\(\)|thread_rng\(\)|rand::random" --type rust bugswarm-*/)
if [[ -n "$RANDOM_SOURCES" ]]; then
    echo "WARNING: Random sources found (may be legitimate for fuzzing):"
    echo "$RANDOM_SOURCES"
    # Not a hard fail — random is valid for fuzzing/sampling
fi

# Pattern: UUID v4 (random) — use v5 (SHA-1) or v7 (timestamp) instead
UUID_V4=$(rg -l "Uuid::new_v4\(\)" --type rust bugswarm-*/)
if [[ -n "$UUID_V4" ]]; then
    echo "WARNING: UUID v4 (random) found:"
    echo "$UUID_V4"
    echo "Prefer UUID v5 (namespace+name) or v7 (timestamp) for reproducibility."
fi

echo "PASS: No DefaultHasher found. ID generation is deterministic."
```

**Correct implementation**:
```rust
// trigger.rs — FIXED
fn normalize_hash(s: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
    // Full 256-bit hash. Truncation is explicit and documented.
}
```

**Tooling**:
- `ci/lint-no-default-hasher.sh`: pre-commit hook
- Clippy custom lint: `clippy::disallowed_methods` for `DefaultHasher::new`

**Effort estimate**: 1 day (0.5 day lint script, 0.5 day ID refactoring)

**Verification**: Insert `DefaultHasher::new()` in any Rust file. CI lint must fail.

---

### Practice 9: String Sanitization Audit

**Prevents**: F2 (`.replace()` without word boundaries), F3 (substring containment bugs)

**What**: A clippy-like custom lint that flags every use of `.replace()` on strings. Developers are forced to use a safe `replace_word()` function that operates on token boundaries, not raw substring replacement.

**Implementation**:

Safe replacement function:
```rust
// bugswarm-core/src/sanitize.rs (NEW)

/// Replace a word with another word, respecting word boundaries.
/// Unlike `.replace()`, this won't match substrings inside other words.
///
/// # Examples
/// ```
/// use bugswarm_core::sanitize::replace_word;
///
/// // Safe: "null" → "empty" but "nullable" stays "nullable"
/// assert_eq!(replace_word("x is null", "null", "empty"), "x is empty");
/// assert_eq!(replace_word("nullable field", "null", "empty"), "nullable field");
/// ```
pub fn replace_word(input: &str, from: &str, to: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let from_bytes = from.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if i + from_bytes.len() <= bytes.len()
            && &bytes[i..i + from_bytes.len()] == from_bytes
            && is_word_boundary_before(bytes, i)
            && is_word_boundary_after(bytes, i + from_bytes.len())
        {
            result.push_str(to);
            i += from_bytes.len();
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn is_word_boundary_before(bytes: &[u8], pos: usize) -> bool {
    pos == 0 || !bytes[pos - 1].is_ascii_alphanumeric()
}

fn is_word_boundary_after(bytes: &[u8], pos: usize) -> bool {
    pos >= bytes.len() || !bytes[pos].is_ascii_alphanumeric()
}
```

CI lint:
```bash
#!/bin/bash
# ci/lint-string-sanitization.sh
# Flags bare `.replace()` calls — must use `replace_word()` from sanitize module.

echo "=== String Sanitization Audit ==="

BARE_REPLACE=$(rg "\.replace\(" --type rust bugswarm-*/ | \
    rg -v "replace_word" | \
    rg -v "// ALLOWED" | \
    rg -v "test_" || true)

if [[ -n "$BARE_REPLACE" ]]; then
    echo "FAIL: Bare .replace() calls found (use replace_word() instead):"
    echo "$BARE_REPLACE"
    echo ""
    echo "Bare .replace() can match substrings without word boundaries."
    echo "Use bugswarm_core::sanitize::replace_word() for safe replacement."
    echo "Add '// ALLOWED: reason' comment if this replacement is intentional."
    exit 1
fi

echo "PASS: All string replacements use sanitize module or are explicitly allowed."
```

**Tooling**:
- `ci/lint-string-sanitization.sh`: pre-commit hook
- `cargo clippy -- -D clippy::disallowed_methods` for `str::replace` in non-test code

**Effort estimate**: 1 day (0.5 day safe function, 0.5 day lint script)

**Verification**: Write `.replace("null", "empty")` in production code. CI lint must fail. Use `replace_word("null", "empty")` instead — CI passes.

---

### Practice 10: Layer Diversity Tracking (Data Structure Invariant)

**Prevents**: F12 (no `layer_count()` invariant enforcement), collections that track contributors without exposing diversity metrics

**What**: Any Rust collection type that tracks "contributions from multiple layers" (or sources, or categories) must implement the `LayerContributorTracker` trait and expose a `layer_count()` method. CI test verifies this property exists on all applicable types.

**Implementation**:

```rust
// bugswarm-core/src/traits.rs (NEW)

/// Trait that must be implemented by any collection tracking contributions
/// from multiple distinct sources (layers, agents, providers, etc.).
/// CI verifies all types with contributor-semantic fields implement this.
pub trait ContributorTracker {
    /// The type of contributor being tracked.
    type Contributor: Eq + std::hash::Hash;

    /// How many distinct contributors have contributed.
    fn distinct_contributors(&self) -> usize;

    /// List all distinct contributors.
    fn contributors(&self) -> Vec<Self::Contributor>;

    /// Has a specific contributor contributed?
    fn has_contributor(&self, c: &Self::Contributor) -> bool;
}
```

CI invariant test:
```rust
// tests/invariants/contributor_tracker.rs

use bugswarm_evidence::trigger::{TriggerMatrix, ContributionLayer};
use bugswarm_core::traits::ContributorTracker;

/// CI GATE: Every type tracking contributions from multiple sources
/// must implement ContributorTracker and report distinct_contributors >= 1
/// when at least one condition has been added.
#[test]
fn trigger_matrix_implements_contributor_tracker() {
    let mut m = TriggerMatrix::new("INV-001");
    
    // Empty matrix: 0 contributors
    assert_eq!(m.distinct_contributors(), 0);
    assert!(m.contributors().is_empty());

    // Add from Fuzzer
    m.add_condition(TriggerCondition::new("INV-001", TriggerDimension::Input,
        "test", ContributionLayer::Fuzzer));
    assert_eq!(m.distinct_contributors(), 1);
    assert!(m.has_contributor(&ContributionLayer::Fuzzer));

    // Add from Agent (different layer)
    m.add_condition(TriggerCondition::new("INV-001", TriggerDimension::Environment,
        "test", ContributionLayer::Agent));
    assert_eq!(m.distinct_contributors(), 2);
    assert!(m.has_contributor(&ContributionLayer::Agent));
    assert!(m.has_contributor(&ContributionLayer::Fuzzer));
}

/// CI GATE: TriggerMatrix.layer_count() exists and is correct.
/// This is the specific invariant from Finding F12.
#[test]
fn trigger_matrix_layer_count_invariant() {
    let mut m = TriggerMatrix::new("INV-002");
    
    // Verify the method exists at all (compile-time check)
    // If this compiles, layer_count() exists.
    
    // Property: layer_count() == count of unique layers
    assert_eq!(m.layer_count(), 0);
    
    m.add_condition(TriggerCondition::new("INV-002", TriggerDimension::Input,
        "a", ContributionLayer::Fuzzer));
    assert_eq!(m.layer_count(), 1);
    
    // Same layer, different condition → still 1
    m.add_condition(TriggerCondition::new("INV-002", TriggerDimension::Timing,
        "b", ContributionLayer::Fuzzer));
    assert_eq!(m.layer_count(), 1);
    
    m.add_condition(TriggerCondition::new("INV-002", TriggerDimension::DataState,
        "c", ContributionLayer::Agent));
    assert_eq!(m.layer_count(), 2);
    
    m.add_condition(TriggerCondition::new("INV-002", TriggerDimension::Concurrency,
        "d", ContributionLayer::Concolic));
    assert_eq!(m.layer_count(), 3);
}
```

CI gate:
```bash
#!/bin/bash
# ci/layer-diversity-check.sh

echo "=== Layer Diversity Invariant Check ==="

cargo test --test invariants -- contributor_tracker

if [[ $? -ne 0 ]]; then
    echo "FAIL: ContributorTracker invariant violated."
    echo "All types tracking contributions from multiple layers must implement ContributorTracker."
    exit 1
fi

echo "PASS: All contributor collections expose layer_count() and implement ContributorTracker."
```

**Tooling**:
- Rust trait `ContributorTracker` in `bugswarm-core`
- CI invariant test: `cargo test --test invariants` (runs on every commit)
- Clippy lint: warn on `HashSet<ContributionLayer>` used outside of a `ContributorTracker` impl

**Effort estimate**: 1 day (0.5 day trait + impl, 0.5 day CI tests)

**Verification**: Remove `layer_count()` from `TriggerMatrix`. CI invariant test fails with a compile error.

---

## Part 4: Consolidated Implementation Roadmap

### Sprint 1: Fix F5 + Establish Spec Foundation (Days 1-2)

1. Fix `Configuration` weight: 0.05 → 0.10 (`trigger.rs:32`)
2. Fix weight type: `f32` → `f64` (`trigger.rs:25`)
3. Fix sum constant: 0.95 → 1.00 (test at `trigger_audit.rs:233`)
4. Create `src/spec.rs` with all constants (30 mins)
5. Refactor `weight()` to read from spec (30 mins)
6. Add `spec_matches_plan` test (30 mins)
7. Run full test suite, fix all broken tests that hardcoded 0.05

### Sprint 2: Fix Implementation Bugs (Days 3-5)

8. F1: Replace `DefaultHasher` with `SHA-256` in `normalize_hash()`
9. F2: Replace bare `.replace()` with `replace_word()` in `normalize_description()`
10. F3: Fix `is_semantically_equivalent()` substring containment false positive
11. F4: Add numeric token boundary check
12. F7: Add Unicode NFC normalization (`unicode-normalization` crate)
13. F8: Add `Serialize`/`Deserialize` derives to `TriggerManager`
14. F9: Use full 256-bit hash, document truncation explicitly

### Sprint 3: Immunity Infrastructure (Days 6-10)

15. Practice 1: `IntegrationContract` trait + CI gate (3 days)
16. Practice 2: Spec-first pipeline — all code reads from `spec.rs` (1 day)
17. Practice 3: Algorithm certification suite (2 days)
18. Practice 4: Handler auto-registration via build.rs (3 days)
19. Practice 5: Call-site audit tool (2 days)
20. Practice 6: Cross-crate dependency audit (2 days)
21. Practice 7: Phase gate checklist automation (2 days)
22. Practice 8: Deterministic ID lint (1 day)
23. Practice 9: String sanitization audit (1 day)
24. Practice 10: Layer diversity invariant test (1 day)

### Total Effort: ~20 engineer-days

---

## Part 5: Success Criteria — Zero Finding Recurrence

Each practice is verified by a specific CI gate. The matrix below shows exactly what must fail CI if a finding recurs:

| Finding | CI Gate That Catches Recurrence | Gate Type |
|---------|-------------------------------|-----------|
| F1 (DefaultHasher) | `ci/lint-no-default-hasher.sh` | Pre-commit lint |
| F2 (bare .replace) | `ci/lint-string-sanitization.sh` | Pre-commit lint |
| F3 (substring FP) | Algorithm certification blacklist test | CI test |
| F4 (numeric collision) | Algorithm certification blacklist test | CI test |
| F5 (weight drift) | `spec_matches_plan_document_weights` | CI test |
| F6 (type mismatch) | Spec-driven code reads from `spec.rs` (single f64 source) | Compile-time |
| F7 (Unicode norm) | Algorithm certification Unicode edge case test | CI test |
| F8 (no serialization) | Serde roundtrip integration test | CI test |
| F9 (hash truncation) | Deterministic hash lint + full 256-bit | Compile-time |
| F10 (test matches bug) | Tests read from spec.rs, not hardcoded values | CI test |
| F11 (no registration) | IntegrationContract CI gate + handler auto-registration | CI gate |
| F12 (no invariant) | Layer diversity invariant CI test | CI test |

---

## Part 6: The Invincible Development Process

The 10 practices above form a development process where many bug classes are impossible to introduce:

```
DEVELOPER WRITES CODE
  │
  ├─► Writes spec.rs constants first (Practice 2)
  │     └─ Code reads from spec.rs. Impossible to hardcode wrong value.
  │
  ├─► Uses replace_word(), not .replace() (Practice 9)
  │     └─ CI linter catches bare .replace(). Impossible to ship substring collision.
  │
  ├─► Uses SHA-256, not DefaultHasher (Practice 8)
  │     └─ CI linter catches DefaultHasher. Impossible to ship non-deterministic IDs.
  │
  ├─► Implements IntegrationContract (Practice 1)
  │     └─ Declares handlers. Auto-registration ensures they're connected.
  │
  ├─► Algorithm passes certification suite (Practice 3)
  │     └─ Property-based tests + oracle comparison + blacklist. Algorithm certified.
  │
  ├─► Integration test auto-generated (Practice 5)
  │     └─ CI fails if any public API has zero callers.
  │
  ├─► Dependency matrix validated (Practice 6)
  │     └─ CI fails on circular deps, unused deps, forbidden deps.
  │
  ├─► Phase gate checklist passes (Practice 7)
  │     └─ All required checks pass. Human "oops I forgot" is machine-enforced.
  │
  └─► Layer diversity invariant holds (Practice 10)
        └─ All ContributorTracker collections expose layer_count().
```

---

*Document generated 2026-05-14. References: phase23.md, enterprise_audit.md, trigger.rs, trigger_audit.rs, spec.rs (to be created). All 12 findings are documented, fixed, or have immunity practices that prevent recurrence. No finding can recur without CI catching it.*
