//! Spec — single source of truth for all Phase 23+ configuration.
//! Every constant here has a corresponding test that verifies it matches the plan.
//! Modules MUST import from spec.rs, never hardcode these values.

use crate::trigger::TriggerDimension;

/// Dimension weights per the Phase 23 plan (A3 success criteria).
/// OsArch is excluded from completeness calculation (bonus dimension).
pub const TRIGGER_DIMENSION_WEIGHTS: &[(TriggerDimension, f32)] = &[
    (TriggerDimension::Input, 0.30),
    (TriggerDimension::Environment, 0.15),
    (TriggerDimension::Timing, 0.10),
    (TriggerDimension::DataState, 0.20),
    (TriggerDimension::Concurrency, 0.10),
    (TriggerDimension::Configuration, 0.10),
    (TriggerDimension::DependencyVersion, 0.05),
    (TriggerDimension::OsArch, 0.00), // bonus dimension
];

/// Phase 30 gate requirements.
pub const PHASE30_MIN_ROWS: usize = 5;
pub const PHASE30_MIN_LAYERS: usize = 3;

/// Dedup algorithm threshold (Jaro-Winkler).
pub const DEDUP_JARO_WINKLER_THRESHOLD: f64 = 0.85;

/// Minimum string length for fuzzy matching (shorter = exact match only).
pub const DEDUP_MIN_FUZZY_LENGTH: usize = 4;

/// Maximum characters to hash/compare (truncation for performance).
pub const DEDUP_MAX_DESCRIPTION_CHARS: usize = 512;

/// Multiplicative decay factor for danger map propagation.
pub const DANGER_DECAY_FACTOR: f32 = 0.7;
