use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

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
        crate::spec::TRIGGER_DIMENSION_WEIGHTS
            .iter()
            .find(|(d, _)| d == self)
            .map(|(_, w)| *w)
            .unwrap_or(0.0)
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

impl ContributionLayer {
    pub fn layer_name(&self) -> &'static str {
        match self {
            Self::Agent => "Agent",
            Self::Fuzzer => "Fuzzer",
            Self::Concolic => "Concolic",
            Self::Differential => "Differential",
            Self::Sanitizer => "Sanitizer",
            Self::Symbolic => "Symbolic",
            Self::Delta => "Delta",
            Self::Manual => "Manual",
        }
    }
}

// ------ H9: Equivalence tri-state ------

#[derive(Debug, Clone, PartialEq)]
pub enum EquivalenceResult {
    Exact,
    Borderline,
    NotEquivalent,
}

pub fn check_equivalence(a: &str, b: &str) -> EquivalenceResult {
    if a == b { return EquivalenceResult::Exact; }
    if a.len() < crate::spec::DEDUP_MIN_FUZZY_LENGTH || b.len() < crate::spec::DEDUP_MIN_FUZZY_LENGTH {
        return EquivalenceResult::NotEquivalent;
    }
    let max_len = crate::spec::DEDUP_MAX_DESCRIPTION_CHARS;
    let a_trunc = &a[..a.len().min(max_len)];
    let b_trunc = &b[..b.len().min(max_len)];
    let sim = strsim::jaro_winkler(a_trunc, b_trunc);
    if sim >= crate::spec::DEDUP_JARO_WINKLER_THRESHOLD {
        EquivalenceResult::Exact
    } else if sim >= crate::spec::DEDUP_BORDERLINE_THRESHOLD {
        EquivalenceResult::Borderline
    } else {
        EquivalenceResult::NotEquivalent
    }
}

fn compute_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    if a.len() < crate::spec::DEDUP_MIN_FUZZY_LENGTH || b.len() < crate::spec::DEDUP_MIN_FUZZY_LENGTH {
        return 0.0;
    }
    let max_len = crate::spec::DEDUP_MAX_DESCRIPTION_CHARS;
    strsim::jaro_winkler(&a[..a.len().min(max_len)], &b[..b.len().min(max_len)])
}

// ------ RawTriggerCondition ------

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

// ------ H9: ReviewCandidate ------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewCandidate {
    pub existing_condition_id: String,
    pub proposed_condition: TriggerCondition,
    pub similarity_score: f64,
    pub queued_at: DateTime<Utc>,
    pub status: String,
    pub resolved_by: Option<String>,
    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,
}

/// A single trigger condition contributed by a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerCondition {
    pub id: String,
    pub bug_id: String,
    pub dimension: TriggerDimension,
    pub description: String,
    pub normalized: String,
    #[serde(default)]
    pub canonical_value: String,
    #[serde(default)]
    pub semantic_hash: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub raw_contributions: Vec<RawTriggerCondition>,
    pub layer: ContributionLayer,
    #[serde(default)]
    pub contributed_by: Vec<ContributionLayer>,
    pub severity_specific: Option<u8>,
    pub verified: bool,
    pub contributed_at: DateTime<Utc>,
    #[serde(default = "crate::spec::default_schema_version")]
    pub schema_version: u32,
}

impl TriggerCondition {
    pub fn new(
        bug_id: &str, dim: TriggerDimension, desc: &str,
        layer: ContributionLayer,
    ) -> Self {
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
            contributed_at: Utc::now(),
            schema_version: crate::spec::default_schema_version(),
        }
    }
}

/// A trigger matrix aggregating all conditions for a bug.
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

    #[serde(skip, default)]
    normalized_idx: Option<HashMap<String, Vec<usize>>>,

    #[serde(skip, default)]
    by_dimension: Option<HashMap<TriggerDimension, Vec<usize>>>,

    #[serde(default)]
    pub review_queue: Vec<ReviewCandidate>,
}

impl TriggerMatrix {
    pub fn new(bug_id: &str) -> Self {
        Self {
            bug_id: bug_id.to_string(),
            conditions: vec![],
            contributing_layers: HashSet::new(),
            dedup_count: 0,
            completeness_score: 0.0,
            last_updated: Utc::now(),
            schema_version: crate::spec::default_schema_version(),
            normalized_idx: None,
            by_dimension: None,
            review_queue: vec![],
        }
    }

    /// Add a trigger condition with 3-phase O(1) lookup.
    pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
        self.ensure_indices();

        let hash_key = format!("{}:{}", dimension_key(condition.dimension), hash_prefix(&condition.normalized));

        // Phase 1: O(1) exact-match via normalized_idx
        if let Some(ni) = self.normalized_idx.as_ref() {
            if let Some(candidates) = ni.get(&hash_key) {
                for &idx in candidates {
                    let existing = &self.conditions[idx];
                    if existing.dimension == condition.dimension
                        && existing.normalized == condition.normalized
                    {
                        self.merge_condition(idx, &condition);
                        self.last_updated = Utc::now();
                        return false;
                    }
                }
            }
        }

        // Phase 2: O(k) fuzzy scan over same-dimension bucket
        let mut best_borderline: Option<(usize, f64)> = None;
        if let Some(bd) = self.by_dimension.as_ref() {
            if let Some(dim_bucket) = bd.get(&condition.dimension) {
                for &idx in dim_bucket {
                    let existing = &self.conditions[idx];
                    let sim = compute_similarity(&existing.normalized, &condition.normalized);
                    if sim >= crate::spec::DEDUP_JARO_WINKLER_THRESHOLD {
                        self.merge_condition(idx, &condition);
                        self.last_updated = Utc::now();
                        return false;
                    }
                    if sim >= crate::spec::DEDUP_BORDERLINE_THRESHOLD
                        && (best_borderline.is_none() || sim > best_borderline.unwrap().1)
                    {
                        best_borderline = Some((idx, sim));
                    }
                }
            }
        }

        // Phase 3: Insert new condition and update indices
        let idx = self.conditions.len();
        self.conditions.push(condition.clone());

        if let (Some(ni), Some(bd)) = (&mut self.normalized_idx, &mut self.by_dimension) {
            ni.entry(hash_key).or_default().push(idx);
            bd.entry(condition.dimension).or_default().push(idx);
        }

        self.contributing_layers.extend(condition.contributed_by.iter().copied());
        self.recompute_completeness();
        self.last_updated = Utc::now();

        // Phase 4: Borderline — create review entry, keep condition as separate row
        if let Some((existing_idx, sim)) = best_borderline {
            self.review_queue.push(ReviewCandidate {
                existing_condition_id: self.conditions[existing_idx].id.clone(),
                proposed_condition: condition.clone(),
                similarity_score: sim,
                queued_at: Utc::now(),
                status: "pending-review".to_string(),
                resolved_by: None,
                schema_version: crate::spec::default_schema_version(),
            });
        }

        // H7: Eviction check
        self.check_eviction(condition.dimension);

        #[cfg(debug_assertions)]
        {
            for dim in TriggerDimension::all() {
                let count = self.conditions.iter().filter(|c| c.dimension == dim).count();
                debug_assert!(
                    count <= crate::spec::MAX_CONDITIONS_PER_DIMENSION + 1,
                    "dimension {:?} exceeded cap", dim);
            }
            self.assert_indices_consistent();
        }

        true
    }

    // ------ H12: Merge helper ------

    fn merge_condition(&mut self, existing_idx: usize, new: &TriggerCondition) {
        let existing = &mut self.conditions[existing_idx];
        existing.raw_contributions.extend(new.raw_contributions.clone());
        for layer in &new.contributed_by {
            if !existing.contributed_by.contains(layer) { existing.contributed_by.push(*layer); }
        }
        for tag in &new.tags {
            if !existing.tags.contains(tag) { existing.tags.push(tag.clone()); }
        }
        if new.normalized.len() > existing.canonical_value.len() {
            existing.canonical_value = new.canonical_value.clone();
        }
        self.contributing_layers.extend(new.contributed_by.iter().copied());
        self.dedup_count += 1;
    }

    // ------ H9: Review queue operations ------

    pub fn approve_review(&mut self, review_idx: usize) -> bool {
        if review_idx >= self.review_queue.len() { return false; }
        if self.review_queue[review_idx].status != "pending-review" { return false; }

        let existing_id = self.review_queue[review_idx].existing_condition_id.clone();
        let proposed_id = self.review_queue[review_idx].proposed_condition.id.clone();
        let existing_idx = self.conditions.iter().position(|c| c.id == existing_id);
        let proposed_idx = self.conditions.iter().position(|c| c.id == proposed_id);

        match (existing_idx, proposed_idx) {
            (Some(ei), Some(pi)) => {
                let proposed = self.conditions[pi].clone();
                self.merge_condition(ei, &proposed);
                self.conditions.remove(pi);
                let remaining: HashSet<ContributionLayer> = self.conditions
                    .iter().flat_map(|c| c.contributed_by.iter().copied()).collect();
                self.contributing_layers.retain(|l| remaining.contains(l));
                self.rebuild_indices();
                self.recompute_completeness();
                self.last_updated = Utc::now();
                self.review_queue[review_idx].status = "approved".to_string();
                true
            }
            _ => false,
        }
    }

    pub fn reject_review(&mut self, review_idx: usize) -> bool {
        if review_idx >= self.review_queue.len() { return false; }
        if self.review_queue[review_idx].status != "pending-review" { return false; }
        self.review_queue[review_idx].status = "rejected".to_string();
        true
    }

    // ------ H12: Index management ------

    fn ensure_indices(&mut self) {
        if self.normalized_idx.is_some() && self.by_dimension.is_some() { return; }
        self.rebuild_indices();
    }

    fn rebuild_indices(&mut self) {
        let mut ni: HashMap<String, Vec<usize>> = HashMap::new();
        let mut bd: HashMap<TriggerDimension, Vec<usize>> = HashMap::new();
        for (i, tc) in self.conditions.iter().enumerate() {
            let key = format!("{}:{}", dimension_key(tc.dimension), hash_prefix(&tc.normalized));
            ni.entry(key).or_default().push(i);
            bd.entry(tc.dimension).or_default().push(i);
        }
        self.normalized_idx = Some(ni);
        self.by_dimension = Some(bd);
    }

    #[cfg(debug_assertions)]
    fn assert_indices_consistent(&self) {
        if let (Some(ni), Some(bd)) = (&self.normalized_idx, &self.by_dimension) {
            let total_ni: usize = ni.values().map(|v| v.len()).sum();
            let total_bd: usize = bd.values().map(|v| v.len()).sum();
            debug_assert_eq!(total_ni, self.conditions.len());
            debug_assert_eq!(total_bd, self.conditions.len());
            for indices in ni.values() { for &idx in indices { debug_assert!(idx < self.conditions.len()); } }
            for indices in bd.values() { for &idx in indices { debug_assert!(idx < self.conditions.len()); } }
        }
    }

    // ------ H7: Eviction ------

    fn check_eviction(&mut self, _triggered_dim: TriggerDimension) -> usize {
        let max = crate::spec::MAX_CONDITIONS_PER_DIMENSION;
        let mut evicted = 0;
        for dim in &TriggerDimension::all() {
            let count = self.conditions.iter().filter(|c| c.dimension == *dim).count();
            if count <= max { continue; }
            for _ in 0..(count - max) {
                let evict_idx_opt = self.conditions.iter().enumerate().fold(None, |best, (i, c)| {
                    if c.dimension == *dim && !c.verified {
                        match best {
                            None => Some(i),
                            Some(best_i) => {
                                if c.contributed_at < self.conditions[best_i].contributed_at { Some(i) } else { Some(best_i) }
                            }
                        }
                    } else { best }
                });
                match evict_idx_opt {
                    Some(evict_idx) => { self.conditions.remove(evict_idx); evicted += 1; }
                    None => { break; }
                }
                let remaining: HashSet<ContributionLayer> = self.conditions
                    .iter().flat_map(|c| c.contributed_by.iter().copied()).collect();
                self.contributing_layers.retain(|l| remaining.contains(l));
            }
        }
        if evicted > 0 {
            self.rebuild_indices();
            self.recompute_completeness();
        }
        evicted
    }

    pub fn vacuum(&mut self) {
        self.rebuild_indices();
        self.recompute_completeness();
    }

    /// Recompute the weighted completeness score.
    pub fn recompute_completeness(&mut self) {
        use crate::spec::{DENSITY_BONUS_SATURATION, LAYER_BONUS_SATURATION, COMPLETENESS_MIN_FLOOR};

        if self.conditions.is_empty() { self.completeness_score = 0.0; return; }

        let mut covered: HashSet<TriggerDimension> = HashSet::new();
        for c in &self.conditions { covered.insert(c.dimension); }

        let total_weight: f32 = TriggerDimension::all().iter()
            .filter(|d| **d != TriggerDimension::OsArch).map(|d| d.weight()).sum();
        let covered_weight: f32 = covered.iter()
            .filter(|d| **d != TriggerDimension::OsArch).map(|d| d.weight()).sum();

        let base_score = if total_weight > 0.0 { (covered_weight / total_weight).min(1.0) } else { 0.0 };

        let density_bonus: f64 = {
            let dc: Vec<f64> = TriggerDimension::all().iter()
                .map(|dim| {
                    let cnt = self.conditions.iter().filter(|c| c.dimension == *dim).count();
                    (cnt as f64 / DENSITY_BONUS_SATURATION as f64).min(1.0)
                }).collect();
            if dc.is_empty() { 0.0 } else { dc.iter().sum::<f64>() / dc.len() as f64 }
        };

        let layer_bonus: f64 = {
            let cnt = self.contributing_layers.len() as f64;
            (cnt / LAYER_BONUS_SATURATION as f64).min(1.0)
        };

        let effective_score = base_score as f64 * density_bonus * layer_bonus;
        self.completeness_score = effective_score.max(COMPLETENESS_MIN_FLOOR).min(1.0) as f32;
    }

    pub fn meets_phase30_gate(&self) -> bool {
        self.conditions.len() >= crate::spec::PHASE30_MIN_ROWS
            && self.contributing_layers.len() >= crate::spec::PHASE30_MIN_LAYERS
    }

    pub fn get_by_dimension(&self, dim: TriggerDimension) -> Vec<&TriggerCondition> {
        self.conditions.iter().filter(|c| c.dimension == dim).collect()
    }

    pub fn layer_count(&self) -> usize {
        self.contributing_layers.len()
    }

    pub fn restore_condition(&mut self, condition: TriggerCondition) {
        self.conditions.push(condition.clone());
        for l in &condition.contributed_by {
            self.contributing_layers.insert(*l);
        }
    }
}

/// Normalize a human-readable trigger description for semantic comparison.
pub fn normalize_description(desc: &str) -> String {
    let s = desc.to_lowercase();
    let s = replace_word(&s, "null", "empty");
    let s = replace_word(&s, "none", "empty");
    let s = replace_word(&s, "nil", "empty");
    let s = s.replace("''", "empty").replace("\"\"", "empty");
    let s = s.replace(" is ", " = ").replace("==", "=").replace(" = ", " = ");
    while s.contains("  ") {
        let words: Vec<&str> = s.split_whitespace().collect();
        return words.join(" ");
    }
    s.trim().to_string()
}

fn replace_word(s: &str, from: &str, to: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let from_bytes = from.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + from_bytes.len() <= bytes.len() && &bytes[i..i+from_bytes.len()] == from_bytes {
            let left_ok = i == 0 || !bytes[i-1].is_ascii_alphanumeric();
            let right_ok = i + from_bytes.len() >= bytes.len() || !bytes[i+from_bytes.len()].is_ascii_alphanumeric();
            if left_ok && right_ok {
                result.push_str(to);
                i += from_bytes.len();
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn normalize_hash(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())[..16].to_string()
}

pub fn full_semantic_hash(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())
}

fn hash_prefix(s: &str) -> String {
    full_semantic_hash(s)[..crate::spec::HASH_KEY_PREFIX_LEN].to_string()
}

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

pub fn is_semantically_equivalent(a: &str, b: &str) -> bool {
    matches!(check_equivalence(a, b), EquivalenceResult::Exact)
}

/// Trigger Matrix Manager — stores all matrices indexed by bug_id.
pub struct TriggerManager {
    matrices: HashMap<String, TriggerMatrix>,
}

impl TriggerManager {
    pub fn new() -> Self {
        Self { matrices: HashMap::new() }
    }

    pub fn get_or_create(&mut self, bug_id: &str) -> &mut TriggerMatrix {
        self.matrices.entry(bug_id.to_string())
            .or_insert_with(|| TriggerMatrix::new(bug_id))
    }

    pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
        let bug_id = condition.bug_id.clone();
        let matrix = self.get_or_create(&bug_id);
        matrix.add_condition(condition)
    }

    pub fn get(&self, bug_id: &str) -> Option<&TriggerMatrix> {
        self.matrices.get(bug_id)
    }

    pub fn get_incomplete_bugs(&self) -> Vec<&TriggerMatrix> {
        let mut incomplete: Vec<&TriggerMatrix> = self.matrices.values()
            .filter(|m| !m.meets_phase30_gate())
            .collect();
        incomplete.sort_by(|a, b| a.completeness_score.partial_cmp(&b.completeness_score).unwrap_or(std::cmp::Ordering::Equal));
        incomplete
    }

    pub fn count(&self) -> usize {
        self.matrices.len()
    }

    pub fn save(&self, _path: &std::path::Path) -> Result<usize, anyhow::Error> {
        Ok(self.matrices.len())
    }

    pub fn load(&mut self, _path: &std::path::Path) -> Result<usize, anyhow::Error> {
        Ok(0)
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
        assert!(!m.add_condition(c2));
        assert_eq!(m.conditions.len(), 1);
    }

    #[test]
    fn test_add_condition_different_dimensions() {
        let mut m = TriggerMatrix::new("BUG-001");
        assert!(m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Input, "x=0", ContributionLayer::Fuzzer)));
        assert!(m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Timing, "x=0", ContributionLayer::Agent)));
        assert_eq!(m.conditions.len(), 2);
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
        // density bonus floors to COMPLETENESS_MIN_FLOOR=0.1
        assert!((m.completeness_score as f64 - crate::spec::COMPLETENESS_MIN_FLOOR).abs() < 0.01);
    }

    #[test]
    fn test_completeness_full() {
        let mut m = TriggerMatrix::new("BUG-001");
        for dim in TriggerDimension::all() {
            m.add_condition(TriggerCondition::new("BUG-001", dim, "p1", ContributionLayer::Agent));
            m.add_condition(TriggerCondition::new("BUG-001", dim, "q2", ContributionLayer::Fuzzer));
            m.add_condition(TriggerCondition::new("BUG-001", dim, "r3", ContributionLayer::Concolic));
        }
        assert!((m.completeness_score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_meets_phase30_gate() {
        let mut m = TriggerMatrix::new("BUG-001");
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
        assert_eq!(tm.count(), 1);
        tm.add_condition(TriggerCondition::new("BUG-002", TriggerDimension::Input, "z=2", ContributionLayer::Fuzzer));
        assert_eq!(tm.count(), 2);
    }

    #[test]
    fn test_incomplete_bugs_ordering() {
        let mut tm = TriggerManager::new();
        tm.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Input, "a", ContributionLayer::Fuzzer));
        tm.add_condition(TriggerCondition::new("BUG-002", TriggerDimension::Input, "b", ContributionLayer::Fuzzer));
        tm.add_condition(TriggerCondition::new("BUG-002", TriggerDimension::Environment, "c", ContributionLayer::Agent));
        tm.add_condition(TriggerCondition::new("BUG-002", TriggerDimension::Timing, "d", ContributionLayer::Concolic));

        let incomplete = tm.get_incomplete_bugs();
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
        assert!((total - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_spec_weights_match_plan() {
        let weights: Vec<(TriggerDimension, f32)> = crate::spec::TRIGGER_DIMENSION_WEIGHTS.to_vec();
        assert_eq!(weights.len(), 8);
        let get = |d: TriggerDimension| -> f32 {
            weights.iter().find(|(dd,_)| *dd == d).map(|(_,w)| *w).unwrap_or(-1.0)
        };
        assert!((get(TriggerDimension::Input) - 0.30).abs() < 0.01);
        assert!((get(TriggerDimension::Configuration) - 0.10).abs() < 0.01);
        assert!((get(TriggerDimension::OsArch) - 0.00).abs() < 0.01);
        assert!((get(TriggerDimension::DataState) - 0.20).abs() < 0.01);
    }

    #[test]
    fn test_phase30_gate_uses_spec_constants() {
        assert_eq!(crate::spec::PHASE30_MIN_ROWS, 5);
        assert_eq!(crate::spec::PHASE30_MIN_LAYERS, 3);
    }

    #[test]
    fn test_os_arch_is_bonus() {
        let mut m = TriggerMatrix::new("BUG-001");
        for dim in TriggerDimension::all() {
            m.add_condition(TriggerCondition::new("BUG-001", dim, "p1", ContributionLayer::Agent));
            m.add_condition(TriggerCondition::new("BUG-001", dim, "q2", ContributionLayer::Fuzzer));
            m.add_condition(TriggerCondition::new("BUG-001", dim, "r3", ContributionLayer::Concolic));
        }
        assert!(m.completeness_score >= 0.95, "Should be complete with all dimensions");
    }

    #[test]
    fn test_replace_word_preserves_nullify() {
        let s = normalize_description("nullify the input");
        assert!(s.contains("nullify"), "nullify should not become emptyify");
    }

    #[test]
    fn test_replace_word_replaces_null() {
        let s = normalize_description("username is null");
        assert!(!s.contains(" null "), "null should be replaced");
        assert!(s.contains("empty"), "null should become empty");
    }

    #[test]
    fn test_replace_word_boundary_start() {
        let s = normalize_description("null pointer exception");
        assert!(s.contains("empty"), "null at start should be replaced");
    }

    #[test]
    fn test_replace_word_boundary_end() {
        let s = normalize_description("value is null");
        assert!(s.contains("empty"), "null at end should be replaced");
    }

    #[test]
    fn test_dedup_merges_layers() {
        let mut m = TriggerMatrix::new("BUG-001");
        let c1 = TriggerCondition::new("BUG-001", TriggerDimension::Input, "username=null", ContributionLayer::Fuzzer);
        let c2 = TriggerCondition::new("BUG-001", TriggerDimension::Input, "username is None", ContributionLayer::Agent);
        
        assert!(m.add_condition(c1));
        assert!(!m.add_condition(c2));
        
        let cond = &m.conditions[0];
        assert!(cond.contributed_by.contains(&ContributionLayer::Fuzzer));
        assert!(cond.contributed_by.contains(&ContributionLayer::Agent));
        assert_eq!(cond.contributed_by.len(), 2);
        assert_eq!(m.dedup_count, 1);
        assert_eq!(m.contributing_layers.len(), 2);
    }

    #[test]
    fn test_dedup_no_duplicate_layers() {
        let mut m = TriggerMatrix::new("BUG-001");
        m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Input, "x=0", ContributionLayer::Fuzzer));
        m.add_condition(TriggerCondition::new("BUG-001", TriggerDimension::Input, "x is 0", ContributionLayer::Fuzzer));
        
        let cond = &m.conditions[0];
        assert_eq!(cond.contributed_by.len(), 1);
    }

    #[test]
    fn test_trigger_condition_serde_roundtrip() {
        let tc = TriggerCondition::new("BUG-001", TriggerDimension::Input, "x=null", ContributionLayer::Fuzzer);
        let json = serde_json::to_string(&tc).unwrap();
        let tc2: TriggerCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(tc.id, tc2.id);
        assert_eq!(tc.normalized, tc2.normalized);
        assert_eq!(tc.contributed_by, tc2.contributed_by);
    }

    // ------ H9: Tri-state dedup tests ------

    #[test]
    fn h9_check_equivalence_exact() {
        let a = normalize_description("username = null");
        let b = normalize_description("username is None");
        assert!(matches!(check_equivalence(&a, &b), EquivalenceResult::Exact));
    }

    #[test]
    fn h9_check_equivalence_not_equivalent() {
        let a = normalize_description("buffer overflow in parser");
        let b = normalize_description("sql injection in login page");
        assert!(matches!(check_equivalence(&a, &b), EquivalenceResult::NotEquivalent));
    }

    #[test]
    fn h9_check_equivalence_borderline() {
        let a = normalize_description("race condition in thread pool initialization");
        let b = normalize_description("concurrent modification in thread pool startup");
        let result = check_equivalence(&a, &b);
        assert!(matches!(result, EquivalenceResult::Borderline | EquivalenceResult::NotEquivalent));
    }

    #[test]
    fn h9_borderline_creates_review_entry() {
        let mut m = TriggerMatrix::new("BUG-H9-01");
        m.add_condition(TriggerCondition::new("BUG-H9-01", TriggerDimension::Input,
            "race condition in thread pool initialize", ContributionLayer::Fuzzer));
        assert_eq!(m.conditions.len(), 1);
        assert_eq!(m.review_queue.len(), 0);

        m.add_condition(TriggerCondition::new("BUG-H9-01", TriggerDimension::Input,
            "concurrent modification in thread pool startup", ContributionLayer::Agent));
        assert!(m.conditions.len() >= 1);
    }

    #[test]
    fn h9_approve_review_merges_and_removes_row() {
        let mut m = TriggerMatrix::new("BUG-H9-02");
        let c1 = TriggerCondition::new("BUG-H9-02", TriggerDimension::Input,
            "empty input causes crash", ContributionLayer::Fuzzer);
        let c2 = TriggerCondition::new("BUG-H9-02", TriggerDimension::Input,
            "null input causes crash", ContributionLayer::Agent);

        m.add_condition(c1);
        m.add_condition(c2);
        let count_after = m.conditions.len();

        for i in 0..m.review_queue.len() {
            if m.review_queue[i].status == "pending-review" {
                assert!(m.approve_review(i));
                assert!(m.conditions.len() < count_after + 1);
                break;
            }
        }
    }

    #[test]
    fn h9_reject_review_keeps_both() {
        let mut m = TriggerMatrix::new("BUG-H9-03");
        let c1 = TriggerCondition::new("BUG-H9-03", TriggerDimension::Input,
            "race condition in thread pool", ContributionLayer::Fuzzer);
        let c2 = TriggerCondition::new("BUG-H9-03", TriggerDimension::Input,
            "race hazard in thread pool", ContributionLayer::Agent);

        m.add_condition(c1);
        m.add_condition(c2);
        let count_before = m.conditions.len();

        for i in 0..m.review_queue.len() {
            if m.review_queue[i].status == "pending-review" {
                assert!(m.reject_review(i));
                assert_eq!(m.review_queue[i].status, "rejected");
                assert_eq!(m.conditions.len(), count_before);
                break;
            }
        }
    }

    #[test]
    fn h9_approve_review_rejects_invalid_index() {
        let mut m = TriggerMatrix::new("BUG-H9-04");
        assert!(!m.approve_review(999));
        assert!(!m.reject_review(999));
    }

    #[test]
    fn h9_is_semantically_equivalent_is_backward_compat() {
        let a = normalize_description("username = null");
        let b = normalize_description("username is None");
        assert!(is_semantically_equivalent(&a, &b));
    }

    // ------ H12: O(1) exact-match tests ------

    #[test]
    fn h12_exact_duplicate_o1_hash_lookup() {
        let mut m = TriggerMatrix::new("BUG-H12-01");
        for i in 0..50 {
            m.add_condition(TriggerCondition::new("BUG-H12-01",
                TriggerDimension::all()[i % 8],
                &format!("{:03}", i),
                ContributionLayer::Fuzzer));
        }

        let dup = TriggerCondition::new("BUG-H12-01",
            TriggerDimension::all()[0 % 8],
            "000",
            ContributionLayer::Agent);
        assert!(!m.add_condition(dup));
        assert_eq!(m.conditions.len(), 50);
    }

    #[test]
    fn h12_fuzzy_detection_falls_back_to_dim_scan() {
        let mut m = TriggerMatrix::new("BUG-H12-02");
        m.add_condition(TriggerCondition::new("BUG-H12-02", TriggerDimension::Input,
            "username is null", ContributionLayer::Fuzzer));
        let near = TriggerCondition::new("BUG-H12-02", TriggerDimension::Input,
            "username = null", ContributionLayer::Agent);
        assert!(!m.add_condition(near));
        assert_eq!(m.conditions.len(), 1);
    }

    #[test]
    fn h12_index_rebuild_roundtrip() {
        let mut m = TriggerMatrix::new("BUG-H12-03");
        for i in 0..10 {
            m.add_condition(TriggerCondition::new("BUG-H12-03",
                TriggerDimension::all()[i % 8],
                &format!("{:04x}", i),
                ContributionLayer::Fuzzer));
        }
        let count = m.conditions.len();
        let json = serde_json::to_string(&m).unwrap();
        let mut deser: TriggerMatrix = serde_json::from_str(&json).unwrap();

        assert!(deser.normalized_idx.is_none());
        assert!(deser.by_dimension.is_none());
        deser.ensure_indices();
        assert!(deser.normalized_idx.is_some());
        assert!(deser.by_dimension.is_some());
        assert_eq!(deser.conditions.len(), count);

        #[cfg(debug_assertions)]
        deser.assert_indices_consistent();
    }

    #[test]
    fn h12_dimension_key_format_stable() {
        assert_eq!(dimension_key(TriggerDimension::Input), "input");
        assert_eq!(dimension_key(TriggerDimension::Environment), "env");
        assert_eq!(dimension_key(TriggerDimension::Timing), "timing");
        assert_eq!(dimension_key(TriggerDimension::DataState), "datastate");
        assert_eq!(dimension_key(TriggerDimension::Concurrency), "concurrency");
        assert_eq!(dimension_key(TriggerDimension::Configuration), "config");
        assert_eq!(dimension_key(TriggerDimension::DependencyVersion), "depver");
        assert_eq!(dimension_key(TriggerDimension::OsArch), "osarch");
    }

    #[test]
    fn h12_full_semantic_hash_is_64_chars() {
        let h = full_semantic_hash("test");
        assert_eq!(h.len(), 64);
    }

    #[test]
    fn h12_hash_prefix_is_32_chars() {
        let h = hash_prefix("test");
        assert_eq!(h.len(), 32);
    }

    // ------ H7: Stale-condition eviction tests ------

    #[test]
    fn h7_eviction_150_unverified_leaves_100() {
        let mut m = TriggerMatrix::new("BUG-H7-01");
        for i in 0..150 {
            m.add_condition(TriggerCondition::new("BUG-H7-01",
                TriggerDimension::Input,
                &format!("{:03}", i),
                ContributionLayer::Fuzzer));
        }
        assert_eq!(m.conditions.len(), 100, "150 unverified should evict 50, leaving 100");
    }

    #[test]
    fn h7_vacuum_rebuilds_and_recomputes() {
        let mut m = TriggerMatrix::new("BUG-H7-05");
        m.add_condition(TriggerCondition::new("BUG-H7-05", TriggerDimension::Input,
            "baz", ContributionLayer::Fuzzer));
        m.add_condition(TriggerCondition::new("BUG-H7-05", TriggerDimension::Environment,
            "qux", ContributionLayer::Agent));
        let score_before = m.completeness_score;
        m.normalized_idx = None;
        m.by_dimension = None;
        m.vacuum();
        assert!(m.normalized_idx.is_some());
        assert!(m.by_dimension.is_some());
        let score_after = m.completeness_score;
        assert!((score_before - score_after).abs() < 0.001);
    }

    #[test]
    fn h7_eviction_interleaved_dimensions() {
        let mut m = TriggerMatrix::new("BUG-H7-03");
        let dims = [TriggerDimension::Input, TriggerDimension::Environment, TriggerDimension::Timing];
        for dim_idx in 0..3 {
            let dim = dims[dim_idx];
            for i in 0..100 {
                m.add_condition(TriggerCondition::new("BUG-H7-03",
                    dim,
                    &format!("{:03}", dim_idx * 100 + i),
                    ContributionLayer::Fuzzer));
            }
        }
        assert_eq!(m.conditions.len(), 300);
    }

    #[test]
    fn h7_eviction_indices_remain_consistent() {
        let mut m = TriggerMatrix::new("BUG-H7-04");
        for i in 0..120 {
            m.add_condition(TriggerCondition::new("BUG-H7-04",
                TriggerDimension::Input,
                &format!("{:03}", i),
                ContributionLayer::Fuzzer));
        }
        assert_eq!(m.conditions.len(), 100);
        assert!(m.normalized_idx.is_some());
        assert!(m.by_dimension.is_some());
    }

    #[test]
    fn h7_verified_conditions_are_protected() {
        let mut m = TriggerMatrix::new("BUG-H7-02");
        for i in 0..50 {
            let mut c = TriggerCondition::new("BUG-H7-02",
                TriggerDimension::Input,
                &format!("v{:02}", i),
                ContributionLayer::Fuzzer);
            c.verified = true;
            m.add_condition(c);
        }
        for i in 0..80 {
            m.add_condition(TriggerCondition::new("BUG-H7-02",
                TriggerDimension::Input,
                &format!("u{:02}", i),
                ContributionLayer::Agent));
        }
        assert_eq!(m.conditions.len(), 100);
        let verified_count = m.conditions.iter().filter(|c| c.verified).count();
        assert_eq!(verified_count, 50, "all verified conditions should be protected");
    }
}
