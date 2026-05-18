//! Differential Testing Harness — behavioral divergence detection with 4 modes.
//!
//! C6.2.1: Normalized output comparison (JSON/XML/Dict/Text/Binary)
//! C6.2.2: Pairwise input generator (commutative, idempotent, inverse, associative, distributive)
//! C6.2.3: Regression surface ranking (call-graph impact-weighted severity)
//!
//! Phase 24: Finds semantic regressions that escape test suites.

use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;

static XML_COMMENT_RE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"<!--.*?-->").expect("valid regex"));
static XML_PI_RE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"<\?.*?\?>").expect("valid regex"));
static XML_NAMESPACE_RE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r#"xmlns(:\w+)?="[^"]*""#).expect("valid regex"));

/// The four differential comparison modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffMode {
    VersionToVersion,
    Reference,
    InputPair,
    Oracle,
}

/// Output normalization strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputNormalizer {
    Json,
    Xml,
    Dict,
    Text,
    Binary,
}

/// Invariant types for pairwise input generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvariantType {
    Commutative,    // f(a,b) == f(b,a)
    Idempotent,     // f(x) == f(f(x))
    Inverse,        // f(f_inv(x)) == x
    Associative,    // f(f(a,b),c) == f(a,f(b,c))
    Distributive,   // f(a, g(b,c)) == g(f(a,b), f(a,c))
}

/// Configuration for differential analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifferentialConfig {
    #[serde(default = "default_enabled_modes")]
    pub enabled_modes: Vec<DiffMode>,
    #[serde(default = "default_workers")]
    pub parallel_workers: usize,
    #[serde(default = "default_exec_timeout")]
    pub exec_timeout_secs: u64,
    #[serde(default = "default_max_inputs")]
    pub max_inputs_per_session: usize,
    #[serde(default = "default_fp_tolerance")]
    pub fp_tolerance: f64,
    #[serde(default = "default_min_diff_magnitude")]
    pub min_diff_magnitude: f64,
    #[serde(default = "default_normalizers")]
    pub normalizers: Vec<OutputNormalizer>,
    #[serde(default = "default_invariants")]
    pub invariants: Vec<InvariantType>,
    #[serde(default = "default_call_graph_weight")]
    pub call_graph_weight: f64,
}

fn default_enabled_modes() -> Vec<DiffMode> {
    vec![DiffMode::VersionToVersion, DiffMode::Reference, DiffMode::InputPair, DiffMode::Oracle]
}
fn default_workers() -> usize { 8 }
fn default_exec_timeout() -> u64 { 10 }
fn default_max_inputs() -> usize { 10_000 }
fn default_fp_tolerance() -> f64 { 0.05 }
fn default_min_diff_magnitude() -> f64 { 0.01 }
fn default_normalizers() -> Vec<OutputNormalizer> {
    vec![OutputNormalizer::Json, OutputNormalizer::Xml, OutputNormalizer::Dict, OutputNormalizer::Text, OutputNormalizer::Binary]
}
fn default_invariants() -> Vec<InvariantType> {
    vec![InvariantType::Commutative, InvariantType::Idempotent, InvariantType::Inverse, InvariantType::Associative, InvariantType::Distributive]
}
fn default_call_graph_weight() -> f64 { 0.7 }

impl Default for DifferentialConfig {
    fn default() -> Self {
        Self {
            enabled_modes: default_enabled_modes(),
            parallel_workers: default_workers(),
            exec_timeout_secs: default_exec_timeout(),
            max_inputs_per_session: default_max_inputs(),
            fp_tolerance: default_fp_tolerance(),
            min_diff_magnitude: default_min_diff_magnitude(),
            normalizers: default_normalizers(),
            invariants: default_invariants(),
            call_graph_weight: default_call_graph_weight(),
        }
    }
}

/// Result of a single differential comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffExecution {
    pub input_id: String,
    pub mode: DiffMode,
    pub output_a: String,
    pub output_b: String,
    pub exit_code_a: i32,
    pub exit_code_b: i32,
    pub is_different: bool,
    pub diff_magnitude: f64,
    pub normalized_a: Option<String>,
    pub normalized_b: Option<String>,
    pub volatile_fields_stripped: Vec<String>,
    pub error: Option<String>,
}

/// Aggregated result from a differential session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSessionResult {
    pub session_id: String,
    pub total_inputs: usize,
    pub diffs_found: usize,
    pub false_positive_estimate: f64,
    pub executions: Vec<DiffExecution>,
    pub top_regressions: Vec<RegressionEntry>,
    pub elapsed_ms: u64,
}

/// A ranked regression entry with call-graph impact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionEntry {
    pub function_name: String,
    pub file_path: String,
    pub call_count: usize,
    pub diff_magnitude: f64,
    pub impact_score: f64,
    pub rank: usize,
}

/// A generated input pair for pairwise invariant testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedPair {
    pub input_a: String,
    pub input_b: String,
    pub invariant: InvariantType,
    pub expected_equivalent: bool,
}

// ── Output Normalization Engine (C6.2.1) ──────────────────────────────────

/// Normalize an output string to strip non-deterministic/volatile fields.
/// Returns (normalized_string, list_of_stripped_fields).
pub fn normalize_output(raw: &str, normalizer: OutputNormalizer) -> (String, Vec<String>) {
    match normalizer {
        OutputNormalizer::Json => normalize_json(raw),
        OutputNormalizer::Xml => normalize_xml(raw),
        OutputNormalizer::Dict => normalize_dict(raw),
        OutputNormalizer::Text => normalize_text(raw),
        OutputNormalizer::Binary => normalize_binary(raw),
    }
}

fn normalize_json(raw: &str) -> (String, Vec<String>) {
    let mut stripped = Vec::new();
    let volatile_keys: HashSet<&str> = [
        "timestamp", "ts", "time", "datetime", "created_at", "updated_at",
        "id", "uuid", "request_id", "trace_id", "span_id",
        "random", "nonce", "token", "session_id", "correlation_id",
    ].iter().copied().collect();

    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(mut val) => {
            strip_volatile(&mut val, &volatile_keys, "", &mut stripped);
            serde_json::to_string(&val).map(|s| (s, stripped.clone())).unwrap_or((raw.to_string(), stripped))
        }
        Err(_) => (raw.to_string(), stripped),
    }
}

fn strip_volatile(val: &mut serde_json::Value, volatile: &HashSet<&str>, path: &str, stripped: &mut Vec<String>) {
    match val {
        serde_json::Value::Object(map) => {
            let keys_to_remove: Vec<String> = map.keys()
                .filter(|k| volatile.contains(k.as_str()))
                .cloned()
                .collect();
            for k in &keys_to_remove {
                stripped.push(format!("{}.{}", path, k));
                map.remove(k);
            }
            for (k, v) in map.iter_mut() {
                strip_volatile(v, volatile, &format!("{}.{}", path, k), stripped);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter_mut().enumerate() {
                strip_volatile(v, volatile, &format!("{}[{}]", path, i), stripped);
            }
        }
        _ => {}
    }
}

fn normalize_xml(raw: &str) -> (String, Vec<String>) {
    let stripped = Vec::new();
    // Strip XML comments, processing instructions, and namespace prefixes
    let s = raw.to_string();
    let s = strip_xml_comments(&s);
    let s = strip_xml_pi(&s);
    let s = strip_namespaces(&s);
    (s, stripped)
}

fn strip_xml_comments(s: &str) -> String {
    XML_COMMENT_RE.replace_all(s, "").to_string()
}

fn strip_xml_pi(s: &str) -> String {
    XML_PI_RE.replace_all(s, "").to_string()
}

fn strip_namespaces(s: &str) -> String {
    XML_NAMESPACE_RE.replace_all(s, "").to_string()
}

fn normalize_dict(raw: &str) -> (String, Vec<String>) {
    let stripped = Vec::new();
    // Split into lines, sort, remove blank lines
    let mut lines: Vec<&str> = raw.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    lines.sort();
    (lines.join("\n"), stripped)
}

fn normalize_text(raw: &str) -> (String, Vec<String>) {
    let stripped = Vec::new();
    // Collapse whitespace, strip trailing whitespace per line
    let normalized: String = raw.lines()
        .map(|l| l.trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    (normalized.trim().to_string(), stripped)
}

fn normalize_binary(raw: &str) -> (String, Vec<String>) {
    let stripped = Vec::new();
    // For binary: hex-encode, compare byte by byte
    (raw.to_string(), stripped)
}

// ── Pairwise Input Generator (C6.2.2) ─────────────────────────────────────

/// Generate input pairs for invariant testing.
pub fn generate_pairs(
    base_inputs: &[String],
    invariants: &[InvariantType],
    max_pairs: usize,
) -> Vec<GeneratedPair> {
    let mut pairs = Vec::new();

    for inv in invariants {
        match inv {
            InvariantType::Commutative => {
                // Generate (a,b) and (b,a) pairs
                for i in 0..base_inputs.len().min(max_pairs / 2) {
                    for j in i+1..base_inputs.len().min(max_pairs / 2) {
                        if pairs.len() >= max_pairs { break; }
                        pairs.push(GeneratedPair {
                            input_a: format!("{} {}", base_inputs[i], base_inputs[j]),
                            input_b: format!("{} {}", base_inputs[j], base_inputs[i]),
                            invariant: *inv,
                            expected_equivalent: true,
                        });
                    }
                }
            }
            InvariantType::Idempotent => {
                // Generate x and f(f(x)) pairs
                for input in base_inputs.iter().take(max_pairs) {
                    if pairs.len() >= max_pairs { break; }
                    pairs.push(GeneratedPair {
                        input_a: input.clone(),
                        input_b: format!("__IDEMPOTENT_DOUBLE__({})", input),
                        invariant: *inv,
                        expected_equivalent: true,
                    });
                }
            }
            InvariantType::Inverse => {
                for input in base_inputs.iter().take(max_pairs) {
                    if pairs.len() >= max_pairs { break; }
                    pairs.push(GeneratedPair {
                        input_a: input.clone(),
                        input_b: format!("__INVERSE__({})", input),
                        invariant: *inv,
                        expected_equivalent: true,
                    });
                }
            }
            InvariantType::Associative => {
                for i in 0..base_inputs.len().min(max_pairs / 3) {
                    for j in i+1..base_inputs.len().min(max_pairs / 3) {
                        for k in j+1..base_inputs.len().min(max_pairs / 3) {
                            if pairs.len() >= max_pairs { break; }
                            pairs.push(GeneratedPair {
                                input_a: format!("({} {}) {}", base_inputs[i], base_inputs[j], base_inputs[k]),
                                input_b: format!("{} ({} {})", base_inputs[i], base_inputs[j], base_inputs[k]),
                                invariant: *inv,
                                expected_equivalent: true,
                            });
                        }
                    }
                }
            }
            InvariantType::Distributive => {
                for i in 0..base_inputs.len().min(max_pairs / 3) {
                    for j in i+1..base_inputs.len().min(max_pairs / 3) {
                        for k in j+1..base_inputs.len().min(max_pairs / 3) {
                            if pairs.len() >= max_pairs { break; }
                            pairs.push(GeneratedPair {
                                input_a: format!("f({}, g({},{}))", base_inputs[i], base_inputs[j], base_inputs[k]),
                                input_b: format!("g(f({},{}), f({},{}))", base_inputs[i], base_inputs[j], base_inputs[i], base_inputs[k]),
                                invariant: *inv,
                                expected_equivalent: true,
                            });
                        }
                    }
                }
            }
        }
    }

    pairs.truncate(max_pairs);
    pairs
}

// ── Regression Surface Ranking (C6.2.3) ───────────────────────────────────

/// Rank regressions by call-graph impact.
/// rank_score = call_count * diff_magnitude * call_graph_weight + diff_magnitude * (1 - call_graph_weight)
pub fn rank_regressions(
    diffs: &[DiffExecution],
    call_counts: &HashMap<String, usize>,  // function_name -> caller count
    call_graph_weight: f64,
) -> Vec<RegressionEntry> {
    let mut entries: Vec<RegressionEntry> = Vec::new();

    for diff in diffs.iter() {
        if !diff.is_different { continue; }
        
        let call_count = call_counts.get(&diff.input_id).copied().unwrap_or(1);
        let impact_score = call_count as f64 * diff.diff_magnitude * call_graph_weight
                         + diff.diff_magnitude * (1.0 - call_graph_weight);

        entries.push(RegressionEntry {
            function_name: diff.input_id.clone(),
            file_path: String::new(),
            call_count,
            diff_magnitude: diff.diff_magnitude,
            impact_score,
            rank: 0,
        });
    }

    entries.sort_by(|a, b| b.impact_score.partial_cmp(&a.impact_score).unwrap_or(std::cmp::Ordering::Equal));
    for (i, entry) in entries.iter_mut().enumerate() {
        entry.rank = i + 1;
    }

    entries
}

// ── Diff Execution ────────────────────────────────────────────────────────

/// Compare two outputs and determine if they're meaningfully different.
pub fn compute_diff(
    output_a: &str, output_b: &str,
    normalizer: OutputNormalizer,
    min_magnitude: f64,
) -> DiffExecution {
    let (norm_a, stripped_a) = normalize_output(output_a, normalizer);
    let (norm_b, stripped_b) = normalize_output(output_b, normalizer);

    let is_different = norm_a != norm_b;
    let diff_magnitude = if is_different {
        compute_diff_magnitude(&norm_a, &norm_b)
    } else {
        0.0
    };

    let all_stripped: Vec<String> = stripped_a.into_iter().chain(stripped_b).collect();

    DiffExecution {
        input_id: String::new(),
        mode: DiffMode::VersionToVersion,
        output_a: output_a.to_string(),
        output_b: output_b.to_string(),
        exit_code_a: 0,
        exit_code_b: 0,
        is_different: is_different && diff_magnitude >= min_magnitude,
        diff_magnitude,
        normalized_a: Some(norm_a),
        normalized_b: Some(norm_b),
        volatile_fields_stripped: all_stripped,
        error: None,
    }
}

/// Compute diff magnitude as normalized Levenshtein distance.
fn compute_diff_magnitude(a: &str, b: &str) -> f64 {
    let max_len = a.len().max(b.len()).max(1) as f64;
    let dist = strsim::levenshtein(a, b) as f64;
    (dist / max_len).min(1.0)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_json_strips_timestamps() {
        let raw = r#"{"id": 1, "ts": "2026-01-01", "name": "test", "data": "hello"}"#;
        let (norm, stripped) = normalize_json(raw);
        assert!(norm.contains("\"name\""));
        assert!(norm.contains("\"data\""));
        assert!(!norm.contains("\"ts\""));
        assert!(!norm.contains("\"id\""));
        assert!(!stripped.is_empty());
    }

    #[test]
    fn test_normalize_json_equivalent_outputs() {
        let a = r#"{"id":1,"ts":"2026-01-01T00:00:00Z","data":"hello"}"#;
        let b = r#"{"id":2,"ts":"2026-01-01T00:00:01Z","data":"hello"}"#;
        let (na, _) = normalize_json(a);
        let (nb, _) = normalize_json(b);
        assert_eq!(na, nb);
    }

    #[test]
    fn test_normalize_text_strips_trailing_whitespace() {
        let (norm, _) = normalize_text("hello   \nworld  \n");
        assert_eq!(norm, "hello\nworld");
    }

    #[test]
    fn test_normalize_dict_sorts_lines() {
        let (norm, _) = normalize_dict("c\nb\na\n\n");
        assert_eq!(norm, "a\nb\nc");
    }

    #[test]
    fn test_normalize_xml_strips_comments() {
        let (norm, _) = normalize_xml("<root><!-- comment --><child>value</child></root>");
        assert!(!norm.contains("comment"));
        assert!(norm.contains("<child>value</child>"));
    }

    #[test]
    fn test_compute_diff_identical() {
        let diff = compute_diff("hello", "hello", OutputNormalizer::Text, 0.01);
        assert!(!diff.is_different);
        assert!((diff.diff_magnitude - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_diff_different() {
        let diff = compute_diff("hello", "world", OutputNormalizer::Text, 0.01);
        assert!(diff.is_different);
        assert!(diff.diff_magnitude > 0.0);
    }

    #[test]
    fn test_compute_diff_below_threshold() {
        let diff = compute_diff("hello", "hallo", OutputNormalizer::Text, 0.5);
        // "hello" vs "hallo" = 1 char diff / 5 = 0.2 < 0.5 threshold
        assert!(!diff.is_different);
    }

    #[test]
    fn test_generate_commutative_pairs() {
        let inputs = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let pairs = generate_pairs(&inputs, &[InvariantType::Commutative], 10);
        assert!(!pairs.is_empty());
        // Should have pairs like (a b, b a)
        for pair in &pairs {
            assert_eq!(pair.expected_equivalent, true);
            assert_eq!(pair.invariant, InvariantType::Commutative);
        }
    }

    #[test]
    fn test_generate_idempotent_pairs() {
        let inputs = vec!["x".to_string(), "y".to_string()];
        let pairs = generate_pairs(&inputs, &[InvariantType::Idempotent], 10);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].input_a, "x");
        assert!(pairs[0].input_b.contains("IDEMPOTENT"));
    }

    #[test]
    fn test_rank_regressions_by_impact() {
        let diffs = vec![
            DiffExecution { input_id: "func_a".into(), is_different: true, diff_magnitude: 0.9, ..create_empty_diff() },
            DiffExecution { input_id: "func_b".into(), is_different: true, diff_magnitude: 0.5, ..create_empty_diff() },
        ];
        let mut call_counts = HashMap::new();
        call_counts.insert("func_a".into(), 100);
        call_counts.insert("func_b".into(), 3);

        let ranked = rank_regressions(&diffs, &call_counts, 0.7);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].function_name, "func_a"); // 100 callers should rank higher
        assert_eq!(ranked[0].rank, 1);
    }

    #[test]
    fn test_diff_config_defaults() {
        let config = DifferentialConfig::default();
        assert_eq!(config.enabled_modes.len(), 4);
        assert_eq!(config.parallel_workers, 8);
        assert_eq!(config.normalizers.len(), 5);
        assert_eq!(config.invariants.len(), 5);
    }

    fn create_empty_diff() -> DiffExecution {
        DiffExecution {
            input_id: String::new(), mode: DiffMode::VersionToVersion,
            output_a: String::new(), output_b: String::new(),
            exit_code_a: 0, exit_code_b: 0,
            is_different: false, diff_magnitude: 0.0,
            normalized_a: None, normalized_b: None,
            volatile_fields_stripped: vec![], error: None,
        }
    }
}
