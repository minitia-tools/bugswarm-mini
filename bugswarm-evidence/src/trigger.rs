use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

/// The 8 trigger dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TriggerDimension {
    Input,
    Environment,
    Timing,
    DataState,
    Concurrency,
    Configuration,
    DependencyVersion,
    OsArch,
}

impl TriggerDimension {
    pub fn all() -> Vec<TriggerDimension> {
        vec![
            Self::Input, Self::Environment, Self::Timing, Self::DataState,
            Self::Concurrency, Self::Configuration, Self::DependencyVersion, Self::OsArch,
        ]
    }

    pub fn weight(&self) -> f32 {
        match self {
            Self::Input => 0.30,
            Self::Environment => 0.15,
            Self::Timing => 0.10,
            Self::DataState => 0.20,
            Self::Concurrency => 0.10,
            Self::Configuration => 0.05,
            Self::DependencyVersion => 0.05,
            Self::OsArch => 0.0,  // Bonus dimension, doesn't count toward completeness
        }
    }
}

/// Layers that can contribute trigger conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContributionLayer {
    Agent,
    Fuzzer,
    Concolic,
    Differential,
    Sanitizer,
    Symbolic,
    Delta,
    Manual,
}

/// A single trigger condition contributed by a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerCondition {
    /// Unique ID for this condition.
    pub id: String,
    /// Which bug this condition triggers.
    pub bug_id: String,
    /// Which dimension this condition describes.
    pub dimension: TriggerDimension,
    /// Human-readable description of the condition.
    pub description: String,
    /// Machine-readable normalized description (for dedup).
    pub normalized: String,
    /// Which layer contributed this condition.
    pub layer: ContributionLayer,
    /// Severity-specific? (e.g., "only critical when X")
    pub severity_specific: Option<u8>,
    /// Whether this condition was verified by sandbox execution.
    pub verified: bool,
    /// Timestamp of contribution.
    pub contributed_at: String,
}

impl TriggerCondition {
    pub fn new(
        bug_id: &str, dim: TriggerDimension, desc: &str,
        layer: ContributionLayer,
    ) -> Self {
        let normalized = normalize_description(desc);
        Self {
            id: format!("tc-{}-{:?}-{}", bug_id, dim, normalize_hash(&normalized)),
            bug_id: bug_id.to_string(),
            dimension: dim,
            description: desc.to_string(),
            normalized,
            layer,
            severity_specific: None,
            verified: false,
            contributed_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// A trigger matrix aggregating all conditions for a bug.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerMatrix {
    /// The bug this matrix describes.
    pub bug_id: String,
    /// All trigger conditions (deduplicated).
    pub conditions: Vec<TriggerCondition>,
    /// Which layers have contributed.
    pub contributing_layers: HashSet<ContributionLayer>,
    /// Completeness score (0.0-1.0).
    pub completeness_score: f32,
    /// Timestamp of last update.
    pub last_updated: String,
}

impl TriggerMatrix {
    pub fn new(bug_id: &str) -> Self {
        Self {
            bug_id: bug_id.to_string(),
            conditions: vec![],
            contributing_layers: HashSet::new(),
            completeness_score: 0.0,
            last_updated: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Add a trigger condition, deduplicating semantically equivalent ones.
    /// Returns true if the condition was new (not a duplicate).
    pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
        // Semantic dedup: check if an equivalent condition already exists
        if self.conditions.iter().any(|existing| {
            existing.dimension == condition.dimension
            && is_semantically_equivalent(&existing.normalized, &condition.normalized)
        }) {
            return false; // Duplicate
        }

        self.contributing_layers.insert(condition.layer);
        self.conditions.push(condition);
        self.recompute_completeness();
        self.last_updated = chrono::Utc::now().to_rfc3339();
        true
    }

    /// Recompute the weighted completeness score.
    pub fn recompute_completeness(&mut self) {
        let mut covered: HashSet<TriggerDimension> = HashSet::new();
        for c in &self.conditions {
            covered.insert(c.dimension);
        }

        let total_weight: f32 = TriggerDimension::all().iter()
            .filter(|d| **d != TriggerDimension::OsArch) // OsArch is bonus
            .map(|d| d.weight())
            .sum();

        let covered_weight: f32 = covered.iter()
            .filter(|d| **d != TriggerDimension::OsArch)
            .map(|d| d.weight())
            .sum();

        self.completeness_score = if total_weight > 0.0 {
            (covered_weight / total_weight).min(1.0)
        } else {
            0.0
        };
    }

    /// Check if the matrix meets the Phase 30 gate: 5+ rows, 3+ layers.
    pub fn meets_phase30_gate(&self) -> bool {
        self.conditions.len() >= 5 && self.contributing_layers.len() >= 3
    }

    /// Get conditions for a specific dimension.
    pub fn get_by_dimension(&self, dim: TriggerDimension) -> Vec<&TriggerCondition> {
        self.conditions.iter().filter(|c| c.dimension == dim).collect()
    }

    /// Get unique layers that contributed.
    pub fn layer_count(&self) -> usize {
        self.contributing_layers.len()
    }
}

/// Normalize a human-readable trigger description for semantic comparison.
pub fn normalize_description(desc: &str) -> String {
    let mut s = desc.to_lowercase();
    s = s.replace("null", "empty");
    s = s.replace("none", "empty");
    s = s.replace("''", "empty");
    s = s.replace("\"\"", "empty");
    s = s.replace(" is ", " = ");
    s = s.replace("==", "=");
    s = s.replace("=", " = ");
    while s.contains("  ") {
        s = s.replace("  ", " ");
    }
    s.trim().to_string()
}

/// Quick hash for normalized descriptions.
fn normalize_hash(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Check if two normalized descriptions are semantically equivalent.
pub fn is_semantically_equivalent(a: &str, b: &str) -> bool {
    if a == b { return true; }
    // Fuzzy: one contains the other
    if a.contains(b) || b.contains(a) { return true; }
    // Token overlap > 80%
    let tokens_a: HashSet<&str> = a.split_whitespace().collect();
    let tokens_b: HashSet<&str> = b.split_whitespace().collect();
    if tokens_a.is_empty() || tokens_b.is_empty() { return false; }
    let intersection = tokens_a.intersection(&tokens_b).count();
    let union = tokens_a.union(&tokens_b).count();
    (intersection as f64 / union as f64) > 0.8
}

/// Trigger Matrix Manager — stores all matrices indexed by bug_id.
pub struct TriggerManager {
    matrices: HashMap<String, TriggerMatrix>,
}

impl TriggerManager {
    pub fn new() -> Self {
        Self { matrices: HashMap::new() }
    }

    /// Get or create a trigger matrix for a bug.
    pub fn get_or_create(&mut self, bug_id: &str) -> &mut TriggerMatrix {
        self.matrices.entry(bug_id.to_string())
            .or_insert_with(|| TriggerMatrix::new(bug_id))
    }

    /// Add a trigger condition to a bug's matrix. Returns true if new.
    pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
        let bug_id = condition.bug_id.clone();
        let matrix = self.get_or_create(&bug_id);
        matrix.add_condition(condition)
    }

    /// Get the trigger matrix for a bug.
    pub fn get(&self, bug_id: &str) -> Option<&TriggerMatrix> {
        self.matrices.get(bug_id)
    }

    /// Get all bugs that don't meet the Phase 30 gate, sorted by completeness (ascending).
    pub fn get_incomplete_bugs(&self) -> Vec<&TriggerMatrix> {
        let mut incomplete: Vec<&TriggerMatrix> = self.matrices.values()
            .filter(|m| !m.meets_phase30_gate())
            .collect();
        incomplete.sort_by(|a, b| a.completeness_score.partial_cmp(&b.completeness_score).unwrap_or(std::cmp::Ordering::Equal));
        incomplete
    }

    /// Count total matrices.
    pub fn count(&self) -> usize {
        self.matrices.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_equivalent_nulls() {
        let a = normalize_description("username = null");
        let b = normalize_description("username is None");
        let c = normalize_description("username = ''");
        assert!(is_semantically_equivalent(&a, &b));
        assert!(is_semantically_equivalent(&a, &c));
    }

    #[test]
    fn test_normalize_different_meanings() {
        let a = normalize_description("username = null");
        let b = normalize_description("password = null");
        assert!(!is_semantically_equivalent(&a, &b));
    }

    #[test]
    fn test_trigger_matrix_new() {
        let m = TriggerMatrix::new("BUG-001");
        assert_eq!(m.bug_id, "BUG-001");
        assert_eq!(m.conditions.len(), 0);
        assert_eq!(m.completeness_score, 0.0);
    }

    #[test]
    fn test_add_condition_dedup() {
        let mut m = TriggerMatrix::new("BUG-001");
        let c1 = TriggerCondition::new("BUG-001", TriggerDimension::Input, "username=null", ContributionLayer::Fuzzer);
        let c2 = TriggerCondition::new("BUG-001", TriggerDimension::Input, "username is None", ContributionLayer::Agent);

        assert!(m.add_condition(c1));
        assert!(!m.add_condition(c2)); // Should be dedup'd
        assert_eq!(m.conditions.len(), 1);
    }

    #[test]
    fn test_add_condition_different_dimensions() {
        let mut m = TriggerMatrix::new("BUG-001");
        assert!(m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Input, "x=0", ContributionLayer::Fuzzer)));
        assert!(m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Timing, "x=0", ContributionLayer::Agent)));
        assert_eq!(m.conditions.len(), 2); // Same desc but different dimensions
    }

    #[test]
    fn test_completeness_empty() {
        let m = TriggerMatrix::new("BUG-001");
        assert!((m.completeness_score - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_completeness_partial() {
        let mut m = TriggerMatrix::new("BUG-001");
        m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Input, "x=null", ContributionLayer::Fuzzer));
        m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Environment, "DEBUG=true", ContributionLayer::Agent));
        // Input (0.30) + Environment (0.15) = 0.45 / 0.95 = ~0.473
        assert!(m.completeness_score > 0.4 && m.completeness_score < 0.55);
    }

    #[test]
    fn test_completeness_full() {
        let mut m = TriggerMatrix::new("BUG-001");
        for dim in &[TriggerDimension::Input, TriggerDimension::Environment, TriggerDimension::Timing, TriggerDimension::DataState, TriggerDimension::Concurrency, TriggerDimension::Configuration, TriggerDimension::DependencyVersion] {
            m.add_condition(TriggerCondition::new("BUG-001", *dim, "test", ContributionLayer::Agent));
        }
        assert!((m.completeness_score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_meets_phase30_gate() {
        let mut m = TriggerMatrix::new("BUG-001");
        // Add 5 conditions from 3 different layers
        m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Input, "a", ContributionLayer::Fuzzer));
        m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Environment, "b", ContributionLayer::Agent));
        m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Timing, "c", ContributionLayer::Concolic));
        m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::DataState, "d", ContributionLayer::Fuzzer));
        m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Concurrency, "e", ContributionLayer::Agent));
        assert!(m.meets_phase30_gate());
    }

    #[test]
    fn test_trigger_manager_get_or_create() {
        let mut tm = TriggerManager::new();
        tm.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Input, "x=0", ContributionLayer::Fuzzer));
        assert_eq!(tm.count(), 1);
        tm.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Environment, "y=1", ContributionLayer::Agent));
        assert_eq!(tm.count(), 1); // Same bug_id
        tm.add_condition(TriggerCondition::new("BUG-002", TriggerDimension::Input, "z=2", ContributionLayer::Fuzzer));
        assert_eq!(tm.count(), 2);
    }

    #[test]
    fn test_incomplete_bugs_ordering() {
        let mut tm = TriggerManager::new();
        // BUG-001: low completeness
        tm.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Input, "a", ContributionLayer::Fuzzer));
        // BUG-002: higher completeness
        tm.add_condition(TriggerCondition::new("BUG-002", TriggerDimension::Input, "b", ContributionLayer::Fuzzer));
        tm.add_condition(TriggerCondition::new("BUG-002", TriggerDimension::Environment, "c", ContributionLayer::Agent));
        tm.add_condition(TriggerCondition::new("BUG-002", TriggerDimension::Timing, "d", ContributionLayer::Concolic));

        let incomplete = tm.get_incomplete_bugs();
        // Lower completeness should come first
        assert!(incomplete.len() >= 1);
        if incomplete.len() >= 2 {
            assert!(incomplete[0].completeness_score <= incomplete[1].completeness_score);
        }
    }

    #[test]
    fn test_dimension_weights_sum() {
        let total: f32 = TriggerDimension::all().iter()
            .filter(|d| **d != TriggerDimension::OsArch)
            .map(|d| d.weight()).sum();
        assert!((total - 0.95).abs() < 0.01); // OsArch excluded
    }

    #[test]
    fn test_os_arch_is_bonus() {
        let mut m = TriggerMatrix::new("BUG-001");
        // Add all dimensions EXCEPT OsArch
        for dim in TriggerDimension::all() {
            if dim != TriggerDimension::OsArch {
                m.add_condition(TriggerCondition::new("BUG-001", dim, "test", ContributionLayer::Agent));
            }
        }
        assert!((m.completeness_score - 1.0).abs() < 0.01, "Should be complete without OsArch");
    }
}
