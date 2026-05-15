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

// ------ H6 + H5 constants ------

/// Staleness window in days.
pub const STALENESS_WINDOW_DAYS: f64 = 30.0;

/// Max conditions per dimension before eviction triggers.
pub const MAX_CONDITIONS_PER_DIMENSION: usize = 100;

/// Max serialized JSON size before per-bug file storage.
pub const MAX_SERIALIZED_SIZE_BYTES: usize = 10 * 1024 * 1024;

/// Priority weights (sum = 1.0).
pub const PRIORITY_SEVERITY_WEIGHT: f64 = 0.6;
pub const PRIORITY_INCOMPLETENESS_WEIGHT: f64 = 0.3;
pub const PRIORITY_STALENESS_WEIGHT: f64 = 0.1;

/// Min conditions in a single dimension for full density bonus.
pub const DENSITY_BONUS_SATURATION: usize = 3;

/// Min layers for full layer diversity bonus.
pub const LAYER_BONUS_SATURATION: usize = 3;

/// Hard floor for completeness score.
pub const COMPLETENESS_MIN_FLOOR: f64 = 0.1;

/// The dimension excluded from weighted coverage (bonus-only).
pub const EXCLUDED_DIMENSION: TriggerDimension = TriggerDimension::OsArch;

/// Schema version for forward compatibility.
pub const fn default_schema_version() -> u32 { 1 }

// ------ H9: Dedup tri-state ------
pub const DEDUP_BORDERLINE_THRESHOLD: f64 = 0.75;

// ------ H12: Hash policy ------
pub const HASH_KEY_PREFIX_LEN: usize = 32;
