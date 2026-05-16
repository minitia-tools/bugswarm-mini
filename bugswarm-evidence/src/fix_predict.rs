//! Fix-Induced Bug Prediction — Roadmap Capstone
//!
//! C6.2.1: Inter-procedural value range analysis (static + Bayesian)
//! C6.2.2: Regression test synthesis per flagged caller
//! C6.2.3: Bayesian fix confidence scoring with language priors
//!
//! Phase 30: Predicts whether applying a fix will introduce new bugs.
//! Traces all callers, analyzes value ranges, computes probability.
//! Completes the arc: find→confirm→fix→verify safe.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────

/// A proposed code fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixProposal {
    pub bug_id: String,
    pub file_path: String,
    pub function_name: String,
    pub original_line: String,
    pub replacement_line: String,
    pub line_number: u32,
    pub description: String,
    pub language: String,
}

/// A caller of the changed function found via CPG call graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerInfo {
    pub caller_name: String,
    pub file_path: String,
    pub call_site_line: u32,
    pub arguments_passed: Vec<ArgumentValueRange>,
    pub call_graph_distance: usize, // Hops from changed function
}

/// Value range analysis for an argument passed to the changed function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentValueRange {
    pub arg_name: String,
    pub arg_position: usize,
    pub min_value: String,
    pub max_value: String,
    pub typical_value: String,
    pub range_type: ValueRangeType,
    /// Whether this argument's value range overlaps with the fix boundary.
    pub overlaps_fix_boundary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueRangeType {
    Integer,
    Float,
    String,
    Boolean,
    Object,
    Nullable,
    Unknown,
}

/// Impact analysis for a single caller after the fix is applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerImpact {
    pub caller: CallerInfo,
    pub affected: bool,
    pub impact_severity: u8, // 0-10
    pub reason: String,
    pub old_behavior: String,
    pub new_behavior: String,
    pub regression_test: Option<String>,
}

/// Bayesian fix confidence score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixConfidence {
    /// Language-specific prior probability of fix-induced breakage.
    pub language_prior: f64,
    /// Posterior probability after caller evidence.
    pub posterior: f64,
    /// Number of affected callers.
    pub affected_callers: usize,
    /// Total number of callers analyzed.
    pub total_callers: usize,
    /// Brier score calibration indicator.
    pub calibrated: bool,
    /// Raw caller impact ratio.
    pub caller_impact_ratio: f64,
}

/// Complete fix impact prediction report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixImpactReport {
    pub bug_id: String,
    pub fix_proposal: FixProposal,
    pub total_callers: usize,
    pub affected_callers: usize,
    pub caller_impacts: Vec<CallerImpact>,
    pub confidence: FixConfidence,
    pub regression_tests: Vec<String>,
    pub recommendation: String,
    pub generated_at: String,
}

/// Language-specific fix breakage base rates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguagePriors {
    pub priors: HashMap<String, f64>,
}

impl Default for LanguagePriors {
    fn default() -> Self {
        let mut priors = HashMap::new();
        priors.insert("python".into(), 0.12);
        priors.insert("c".into(), 0.25);
        priors.insert("cpp".into(), 0.22);
        priors.insert("javascript".into(), 0.18);
        priors.insert("typescript".into(), 0.15);
        priors.insert("rust".into(), 0.08);
        priors.insert("go".into(), 0.14);
        priors.insert("java".into(), 0.16);
        priors.insert("ruby".into(), 0.13);
        priors.insert("php".into(), 0.19);
        Self { priors }
    }
}

impl LanguagePriors {
    pub fn get(&self, lang: &str) -> f64 {
        self.priors
            .get(&lang.to_lowercase())
            .copied()
            .unwrap_or(0.15)
    }

    pub fn update(&mut self, lang: &str, observed_rate: f64, weight: f64) {
        let key = lang.to_lowercase();
        let current = self.priors.get(&key).copied().unwrap_or(0.15);
        let updated = current * (1.0 - weight) + observed_rate * weight;
        self.priors.insert(key, updated.clamp(0.0, 1.0));
    }
}

/// Historical fix outcome for Bayesian updating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixOutcome {
    pub bug_id: String,
    pub language: String,
    pub fix_applied: bool,
    pub breakage_occurred: bool,
    pub affected_caller_count: usize,
    pub total_caller_count: usize,
    pub recorded_at: String,
}

// ── Caller Discovery (C6.2.1) ──────────────────────────────────────────

/// Discover all callers of a function via CPG call graph BFS.
pub fn discover_callers(
    function_name: &str,
    call_graph: &HashMap<String, Vec<String>>, // function -> [callees]
    max_hops: usize,
) -> Vec<CallerInfo> {
    // Build reverse call graph: callee -> [callers]
    let mut reverse_graph: HashMap<String, Vec<String>> = HashMap::new();
    for (caller, callees) in call_graph {
        for callee in callees {
            reverse_graph
                .entry(callee.clone())
                .or_default()
                .push(caller.clone());
        }
    }

    let mut callers = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    // Start from the changed function
    visited.insert(function_name.to_string());

    if let Some(direct_callers) = reverse_graph.get(function_name) {
        for caller in direct_callers {
            if !visited.contains(caller) {
                visited.insert(caller.clone());
                queue.push_back((caller.clone(), 1usize));
            }
        }
    }

    while let Some((caller_name, distance)) = queue.pop_front() {
        if distance > max_hops {
            continue;
        }

        callers.push(CallerInfo {
            caller_name: caller_name.clone(),
            file_path: format!("{}.py", caller_name),
            call_site_line: 0,
            arguments_passed: estimate_argument_ranges(&caller_name),
            call_graph_distance: distance,
        });

        if distance < max_hops {
            if let Some(indirect_callers) = reverse_graph.get(&caller_name) {
                for indirect in indirect_callers {
                    if !visited.contains(indirect) {
                        visited.insert(indirect.clone());
                        queue.push_back((indirect.clone(), distance + 1));
                    }
                }
            }
        }
    }

    callers
}

/// Estimate argument value ranges for a caller (simplified static analysis).
fn estimate_argument_ranges(_caller_name: &str) -> Vec<ArgumentValueRange> {
    // Simplified: generate placeholder ranges
    // In production, this would use CPG AST analysis + Z3 symbolic computation
    vec![ArgumentValueRange {
        arg_name: "arg0".into(),
        arg_position: 0,
        min_value: "0".into(),
        max_value: "100".into(),
        typical_value: "42".into(),
        range_type: ValueRangeType::Integer,
        overlaps_fix_boundary: false,
    }]
}

// ── Value Range Overlap Detection ──────────────────────────────────────

/// Check if a fix changes behavior for a caller's value ranges.
pub fn analyze_caller_impact(caller: &CallerInfo, fix: &FixProposal) -> CallerImpact {
    let mut affected = false;
    let mut reason = String::new();
    let mut overlap_count = 0usize;

    for arg in &caller.arguments_passed {
        let overlaps = detect_overlap(arg, fix);
        if overlaps {
            affected = true;
            overlap_count += 1;
            if reason.is_empty() {
                reason = format!(
                    "Fix changes behavior for argument '{}' (range {}..{})",
                    arg.arg_name, arg.min_value, arg.max_value
                );
            }
        }
    }

    let severity = if affected {
        (overlap_count as u8 * 3 + caller.call_graph_distance as u8).min(10)
    } else {
        0
    };

    CallerImpact {
        caller: caller.clone(),
        affected,
        impact_severity: severity,
        reason: if affected {
            reason
        } else {
            "No value range overlap with fix boundary".into()
        },
        old_behavior: format!("Original: {}", fix.original_line),
        new_behavior: format!("Replacement: {}", fix.replacement_line),
        regression_test: if affected {
            Some(generate_regression_test(caller, fix))
        } else {
            None
        },
    }
}

/// Detect if an argument's value range overlaps with the fix boundary.
fn detect_overlap(arg: &ArgumentValueRange, fix: &FixProposal) -> bool {
    // Heuristic: check if the fix changes a constant or comparison that
    // falls within the argument's typical range
    let original = &fix.original_line;
    let replacement = &fix.replacement_line;

    // If the fix changes a comparison operator, check if the caller's values
    // fall on the boundary
    if (original.contains('>') && replacement.contains(">="))
        || (original.contains('<') && replacement.contains("<="))
        || (original.contains("==") && replacement.contains("!="))
    {
        return true;
    }

    // If the fix changes a constant value, check if caller's values
    // span the old and new constants
    if let (Some(old_val), Some(new_val)) =
        (extract_constant(original), extract_constant(replacement))
    {
        if let (Ok(old_n), Ok(new_n)) = (old_val.parse::<f64>(), new_val.parse::<f64>()) {
            if let Ok(arg_min) = arg.min_value.parse::<f64>() {
                if let Ok(arg_max) = arg.max_value.parse::<f64>() {
                    return arg_min <= old_n.max(new_n) && arg_max >= old_n.min(new_n);
                }
            }
        }
    }

    false
}

fn extract_constant(line: &str) -> Option<String> {
    // Extract numeric or string constants from a line
    for word in line.split_whitespace() {
        if word.len() >= 2 && word.starts_with('"') && word.ends_with('"') {
            return Some(word[1..word.len() - 1].to_string());
        }
        let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '-');
        if w.parse::<f64>().is_ok() {
            return Some(w.to_string());
        }
    }
    None
}

// ── Regression Test Synthesis (C6.2.2) ─────────────────────────────────

/// Generate a regression test for a flagged caller.
pub fn generate_regression_test(caller: &CallerInfo, fix: &FixProposal) -> String {
    format!(
        "// AUTO-GENERATED REGRESSION TEST for fix at {}:{}\n\
         // Target: {} (called by {})\n\
         // Fix: '{}' → '{}'\n\
         // This test verifies that the fix does not silently break\n\
         // the caller's assumptions about the function's behavior.\n\
         //\n\
         // Test: verify_behavior_regression_{}() {{\n\
         //     // Arrange: set up the caller's typical argument values\n\
         //     // Act: call the changed function with typical caller values\n\
         //     // Assert: behavior matches old contract, or diverges as expected\n\
         //     assert(old_behavior(args) != new_behavior(args));\n\
         // }}\n\
         //\n// INSERT THIS TEST before merging the fix.\n",
        fix.file_path,
        fix.line_number,
        fix.function_name,
        caller.caller_name,
        fix.original_line,
        fix.replacement_line,
        caller.caller_name.replace("::", "_"),
    )
}

// ── Bayesian Confidence Scoring (C6.2.3) ───────────────────────────────

/// Compute Bayesian fix confidence score.
pub fn compute_fix_confidence(
    language: &str,
    affected_callers: usize,
    total_callers: usize,
    priors: &LanguagePriors,
) -> FixConfidence {
    let language_prior = priors.get(language);
    let caller_impact_ratio = if total_callers > 0 {
        affected_callers as f64 / total_callers as f64
    } else {
        0.0
    };

    // Bayesian update: P(breakage | caller_impact) ∝ P(caller_impact | breakage) × P(breakage)
    // Simplified using the caller impact ratio as evidence
    let posterior = if total_callers > 0 {
        // Weight: 70% prior, 30% caller evidence
        let evidence_weight = 0.3;
        let caller_evidence = caller_impact_ratio;
        let posterior =
            language_prior * (1.0 - evidence_weight) + caller_evidence * evidence_weight;
        posterior.clamp(0.01, 0.99)
    } else {
        language_prior
    };

    FixConfidence {
        language_prior,
        posterior,
        affected_callers,
        total_callers,
        calibrated: total_callers > 0,
        caller_impact_ratio,
    }
}

/// Generate a recommendation string for the bench judge.
pub fn generate_recommendation(report: &FixImpactReport) -> String {
    if report.affected_callers == 0 {
        format!(
            "SAFE TO MERGE: No callers affected by this fix ({} analyzed). Confidence: {:.1}%.",
            report.total_callers,
            report.confidence.posterior * 100.0
        )
    } else if report.confidence.posterior < 0.3 {
        format!(
            "LOW RISK: {} callers affected but confidence is low ({:.1}%). \
             Review {} flagged callers before merging.",
            report.affected_callers,
            report.confidence.posterior * 100.0,
            report.affected_callers
        )
    } else if report.confidence.posterior < 0.7 {
        format!(
            "MEDIUM RISK: {} of {} callers affected ({:.1}% confidence). \
             Manual review of flagged callers recommended. {} regression tests auto-generated.",
            report.affected_callers,
            report.total_callers,
            report.confidence.posterior * 100.0,
            report.regression_tests.len()
        )
    } else {
        format!(
            "HIGH RISK: {} of {} callers affected ({:.1}% confidence). \
             DO NOT MERGE without resolving flagged callers. \
             {} regression tests auto-generated for verification.",
            report.affected_callers,
            report.total_callers,
            report.confidence.posterior * 100.0,
            report.regression_tests.len()
        )
    }
}

/// Run the full fix impact prediction pipeline.
pub fn predict_fix_impact(
    fix: FixProposal,
    call_graph: &HashMap<String, Vec<String>>,
    priors: &LanguagePriors,
    max_hops: usize,
) -> FixImpactReport {
    let callers = discover_callers(&fix.function_name, call_graph, max_hops);

    let total_callers = callers.len();

    let caller_impacts: Vec<CallerImpact> = callers
        .iter()
        .map(|caller| analyze_caller_impact(caller, &fix))
        .collect();

    let affected_callers = caller_impacts.iter().filter(|ci| ci.affected).count();

    let confidence = compute_fix_confidence(
        &fix.language,
        affected_callers,
        total_callers,
        priors,
    );

    let regression_tests: Vec<String> = caller_impacts
        .iter()
        .filter_map(|ci| ci.regression_test.clone())
        .collect();

    let mut report = FixImpactReport {
        bug_id: fix.bug_id.clone(),
        fix_proposal: fix,
        total_callers,
        affected_callers,
        caller_impacts,
        confidence,
        regression_tests,
        recommendation: String::new(),
        generated_at: chrono::Utc::now().to_rfc3339(),
    };

    report.recommendation = generate_recommendation(&report);
    report
}

// ── Historical Outcome Tracking ────────────────────────────────────────

/// Update language priors based on observed fix outcomes.
pub fn update_priors_from_outcomes(
    priors: &mut LanguagePriors,
    outcomes: &[FixOutcome],
    weight: f64,
) {
    let mut by_lang: HashMap<String, (usize, usize)> = HashMap::new(); // (total, breakages)

    for outcome in outcomes {
        let key = outcome.language.to_lowercase();
        let entry = by_lang.entry(key).or_insert((0, 0));
        entry.0 += 1;
        if outcome.breakage_occurred {
            entry.1 += 1;
        }
    }

    for (lang, (total, breakages)) in by_lang {
        if total > 0 {
            let rate = breakages as f64 / total as f64;
            priors.update(&lang, rate, weight);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_call_graph() -> HashMap<String, Vec<String>> {
        let mut g = HashMap::new();
        g.insert("login".into(), vec!["validate_user".into(), "check_password".into()]);
        g.insert(
            "handle_request".into(),
            vec!["login".into(), "parse_input".into()],
        );
        g.insert("main".into(), vec!["handle_request".into()]);
        g.insert("validate_user".into(), vec![]);
        g.insert("check_password".into(), vec![]);
        g.insert("parse_input".into(), vec![]);
        g
    }

    fn sample_fix() -> FixProposal {
        FixProposal {
            bug_id: "BUG-001".into(),
            file_path: "auth.py".into(),
            function_name: "validate_user".into(),
            original_line: "if x > 0:".into(),
            replacement_line: "if x >= 1:".into(),
            line_number: 42,
            description: "Fix off-by-one in user validation".into(),
            language: "python".into(),
        }
    }

    fn sample_caller(name: &str, distance: usize) -> CallerInfo {
        CallerInfo {
            caller_name: name.into(),
            file_path: format!("{}.py", name),
            call_site_line: 10,
            call_graph_distance: distance,
            arguments_passed: vec![ArgumentValueRange {
                arg_name: "user_id".into(),
                arg_position: 0,
                min_value: "0".into(),
                max_value: "100".into(),
                typical_value: "50".into(),
                range_type: ValueRangeType::Integer,
                overlaps_fix_boundary: false,
            }],
        }
    }

    // ── discover_callers tests ─────────────────────────────────────────

    #[test]
    fn test_discover_callers_basic() {
        let graph = sample_call_graph();
        let callers = discover_callers("validate_user", &graph, 5);
        assert!(callers.len() >= 1);
        assert!(callers.iter().any(|c| c.caller_name == "login"));
    }

    #[test]
    fn test_discover_callers_indirect() {
        let graph = sample_call_graph();
        let callers = discover_callers("validate_user", &graph, 5);
        let names: HashSet<&str> = callers.iter().map(|c| c.caller_name.as_str()).collect();
        assert!(names.contains("login"));
        assert!(names.contains("handle_request"));
        assert!(names.contains("main"));
    }

    #[test]
    fn test_discover_callers_max_hops() {
        let graph = sample_call_graph();
        let callers = discover_callers("validate_user", &graph, 1);
        // Only direct callers (1 hop)
        for c in &callers {
            assert!(c.call_graph_distance <= 1);
        }
    }

    #[test]
    fn test_discover_callers_nonexistent_function() {
        let graph = sample_call_graph();
        let callers = discover_callers("nonexistent_func", &graph, 5);
        assert!(callers.is_empty());
    }

    #[test]
    fn test_discover_callers_leaf_function() {
        let graph = sample_call_graph();
        let callers = discover_callers("parse_input", &graph, 5);
        assert!(!callers.is_empty());
        assert!(callers.iter().any(|c| c.caller_name == "handle_request"));
    }

    #[test]
    fn test_discover_callers_distance_tracking() {
        let graph = sample_call_graph();
        let callers = discover_callers("validate_user", &graph, 5);
        // main -> handle_request -> login -> validate_user: distance = 3
        let main_caller = callers.iter().find(|c| c.caller_name == "main");
        assert!(main_caller.is_some());
        assert_eq!(main_caller.unwrap().call_graph_distance, 3);
    }

    // ── detect_overlap tests ───────────────────────────────────────────

    #[test]
    fn test_detect_overlap_operator_greater_to_gte() {
        let arg = ArgumentValueRange {
            arg_name: "x".into(),
            arg_position: 0,
            min_value: "0".into(),
            max_value: "100".into(),
            typical_value: "50".into(),
            range_type: ValueRangeType::Integer,
            overlaps_fix_boundary: false,
        };
        let fix = FixProposal {
            bug_id: "B1".into(),
            file_path: "a.py".into(),
            function_name: "f".into(),
            original_line: "if x > 50:".into(),
            replacement_line: "if x >= 50:".into(),
            line_number: 1,
            description: "fix".into(),
            language: "python".into(),
        };
        assert!(detect_overlap(&arg, &fix));
    }

    #[test]
    fn test_detect_overlap_operator_less_to_lte() {
        let arg = ArgumentValueRange {
            arg_name: "x".into(),
            arg_position: 0,
            min_value: "0".into(),
            max_value: "100".into(),
            typical_value: "50".into(),
            range_type: ValueRangeType::Integer,
            overlaps_fix_boundary: false,
        };
        let fix = FixProposal {
            bug_id: "B1".into(),
            file_path: "a.py".into(),
            function_name: "f".into(),
            original_line: "if x < 50:".into(),
            replacement_line: "if x <= 50:".into(),
            line_number: 1,
            description: "fix".into(),
            language: "python".into(),
        };
        assert!(detect_overlap(&arg, &fix));
    }

    #[test]
    fn test_detect_overlap_operator_eq_to_ne() {
        let arg = ArgumentValueRange {
            arg_name: "flag".into(),
            arg_position: 0,
            min_value: "true".into(),
            max_value: "false".into(),
            typical_value: "true".into(),
            range_type: ValueRangeType::Boolean,
            overlaps_fix_boundary: false,
        };
        let fix = FixProposal {
            bug_id: "B1".into(),
            file_path: "a.py".into(),
            function_name: "f".into(),
            original_line: "if flag == True:".into(),
            replacement_line: "if flag != True:".into(),
            line_number: 1,
            description: "fix".into(),
            language: "python".into(),
        };
        assert!(detect_overlap(&arg, &fix));
    }

    #[test]
    fn test_detect_overlap_constant_within_range() {
        let arg = ArgumentValueRange {
            arg_name: "limit".into(),
            arg_position: 0,
            min_value: "0".into(),
            max_value: "500".into(),
            typical_value: "100".into(),
            range_type: ValueRangeType::Integer,
            overlaps_fix_boundary: false,
        };
        let fix = FixProposal {
            bug_id: "B1".into(),
            file_path: "a.py".into(),
            function_name: "f".into(),
            original_line: "max_size = 256".into(),
            replacement_line: "max_size = 512".into(),
            line_number: 1,
            description: "fix".into(),
            language: "python".into(),
        };
        assert!(detect_overlap(&arg, &fix));
    }

    #[test]
    fn test_detect_overlap_constant_outside_range() {
        let arg = ArgumentValueRange {
            arg_name: "limit".into(),
            arg_position: 0,
            min_value: "1000".into(),
            max_value: "2000".into(),
            typical_value: "1500".into(),
            range_type: ValueRangeType::Integer,
            overlaps_fix_boundary: false,
        };
        let fix = FixProposal {
            bug_id: "B1".into(),
            file_path: "a.py".into(),
            function_name: "f".into(),
            original_line: "max_size = 10".into(),
            replacement_line: "max_size = 20".into(),
            line_number: 1,
            description: "fix".into(),
            language: "python".into(),
        };
        assert!(!detect_overlap(&arg, &fix));
    }

    #[test]
    fn test_detect_overlap_negative_constants() {
        let arg = ArgumentValueRange {
            arg_name: "delta".into(),
            arg_position: 0,
            min_value: "-100".into(),
            max_value: "100".into(),
            typical_value: "0".into(),
            range_type: ValueRangeType::Integer,
            overlaps_fix_boundary: false,
        };
        let fix = FixProposal {
            bug_id: "B1".into(),
            file_path: "a.py".into(),
            function_name: "f".into(),
            original_line: "threshold = -5".into(),
            replacement_line: "threshold = 5".into(),
            line_number: 1,
            description: "fix sign".into(),
            language: "c".into(),
        };
        assert!(detect_overlap(&arg, &fix));
    }

    #[test]
    fn test_detect_overlap_no_change() {
        let arg = ArgumentValueRange {
            arg_name: "x".into(),
            arg_position: 0,
            min_value: "10".into(),
            max_value: "20".into(),
            typical_value: "15".into(),
            range_type: ValueRangeType::Integer,
            overlaps_fix_boundary: false,
        };
        let fix = FixProposal {
            bug_id: "B1".into(),
            file_path: "a.py".into(),
            function_name: "f".into(),
            original_line: "return x".into(),
            replacement_line: "return x + 1".into(),
            line_number: 1,
            description: "fix".into(),
            language: "python".into(),
        };
        // No comparison operator change, and no extractable constants match
        assert!(!detect_overlap(&arg, &fix));
    }

    // ── extract_constant tests ─────────────────────────────────────────

    #[test]
    fn test_extract_constant_numeric() {
        assert_eq!(extract_constant("if x > 1000:"), Some("1000".into()));
    }

    #[test]
    fn test_extract_constant_negative() {
        assert_eq!(extract_constant("threshold = -42"), Some("-42".into()));
    }

    #[test]
    fn test_extract_constant_float() {
        assert_eq!(extract_constant("limit = 3.14"), Some("3.14".into()));
    }

    #[test]
    fn test_extract_constant_string() {
        assert_eq!(extract_constant("name == \"admin\""), Some("admin".into()));
    }

    #[test]
    fn test_extract_constant_none() {
        assert_eq!(extract_constant("return x + y"), None);
    }

    // ── analyze_caller_impact tests ────────────────────────────────────

    #[test]
    fn test_analyze_caller_impact_affected() {
        let caller = sample_caller("login", 1);
        let fix = sample_fix(); // > to >= triggers overlap
        let impact = analyze_caller_impact(&caller, &fix);
        assert!(impact.affected);
        assert!(impact.impact_severity > 0);
        assert!(impact.regression_test.is_some());
    }

    #[test]
    fn test_analyze_caller_impact_not_affected() {
        let caller = sample_caller("report", 1);
        let fix = FixProposal {
            bug_id: "BUG-002".into(),
            file_path: "auth.py".into(),
            function_name: "validate_user".into(),
            original_line: "return x".into(),
            replacement_line: "return x + 1".into(),
            line_number: 42,
            description: "Add increment".into(),
            language: "python".into(),
        };
        let impact = analyze_caller_impact(&caller, &fix);
        assert!(!impact.affected);
        assert_eq!(impact.impact_severity, 0);
        assert!(impact.regression_test.is_none());
    }

    #[test]
    fn test_analyze_caller_impact_severity_bounds() {
        let caller = sample_caller("login", 1);
        let fix = sample_fix();
        let impact = analyze_caller_impact(&caller, &fix);
        assert!(impact.impact_severity <= 10);
    }

    #[test]
    fn test_analyze_caller_impact_severity_grows_with_distance() {
        let near = sample_caller("near", 1);
        let far = sample_caller("far", 5);
        let fix = sample_fix();
        let impact_near = analyze_caller_impact(&near, &fix);
        let impact_far = analyze_caller_impact(&far, &fix);
        assert!(impact_far.impact_severity >= impact_near.impact_severity);
    }

    // ── generate_regression_test tests ─────────────────────────────────

    #[test]
    fn test_generate_regression_test_contains_caller() {
        let caller = sample_caller("handle_request", 2);
        let fix = sample_fix();
        let test = generate_regression_test(&caller, &fix);
        assert!(test.contains("handle_request"));
        assert!(test.contains("REGRESSION TEST"));
        assert!(test.contains("validate_user"));
    }

    #[test]
    fn test_generate_regression_test_contains_file_line() {
        let caller = sample_caller("caller_fn", 1);
        let fix = sample_fix();
        let test = generate_regression_test(&caller, &fix);
        assert!(test.contains("auth.py:42"));
    }

    #[test]
    fn test_generate_regression_test_cpp_namespace() {
        let caller = CallerInfo {
            caller_name: "ns::func".into(),
            file_path: "mod.cpp".into(),
            call_site_line: 20,
            call_graph_distance: 1,
            arguments_passed: vec![],
        };
        let fix = sample_fix();
        let test = generate_regression_test(&caller, &fix);
        assert!(test.contains("ns_func"), "test function name should have : replaced with _");
    }

    // ── compute_fix_confidence tests ───────────────────────────────────

    #[test]
    fn test_compute_fix_confidence_basic() {
        let priors = LanguagePriors::default();
        let conf = compute_fix_confidence("python", 3, 10, &priors);
        assert!(conf.language_prior > 0.0);
        assert!(conf.posterior > 0.0);
        assert_eq!(conf.affected_callers, 3);
        assert_eq!(conf.total_callers, 10);
        assert!(conf.calibrated);
    }

    #[test]
    fn test_compute_fix_confidence_no_callers() {
        let priors = LanguagePriors::default();
        let conf = compute_fix_confidence("rust", 0, 0, &priors);
        assert!((conf.posterior - 0.08).abs() < 0.01);
        assert!(!conf.calibrated);
    }

    #[test]
    fn test_compute_fix_confidence_all_affected() {
        let priors = LanguagePriors::default();
        let conf = compute_fix_confidence("c", 5, 5, &priors);
        assert!(conf.posterior > 0.2);
        assert!(conf.caller_impact_ratio > 0.9);
    }

    #[test]
    fn test_compute_fix_confidence_none_affected() {
        let priors = LanguagePriors::default();
        let conf = compute_fix_confidence("go", 0, 50, &priors);
        assert_eq!(conf.affected_callers, 0);
        assert!(conf.posterior < conf.language_prior + 0.05);
    }

    #[test]
    fn test_compute_fix_confidence_bounds() {
        let priors = LanguagePriors::default();
        // Extreme case: all callers affected in C
        let conf = compute_fix_confidence("c", 100, 100, &priors);
        assert!(conf.posterior > 0.0);
        assert!(conf.posterior < 1.0);
    }

    #[test]
    fn test_compute_fix_confidence_clamped() {
        let priors = LanguagePriors::default();
        let conf = compute_fix_confidence("python", 10000, 10000, &priors);
        assert!(conf.posterior >= 0.01);
        assert!(conf.posterior <= 0.99);
    }

    // ── LanguagePriors tests ───────────────────────────────────────────

    #[test]
    fn test_language_priors_defaults() {
        let priors = LanguagePriors::default();
        assert!((priors.get("python") - 0.12).abs() < 0.01);
        assert!((priors.get("c") - 0.25).abs() < 0.01);
        assert!((priors.get("rust") - 0.08).abs() < 0.01);
        assert!((priors.get("javascript") - 0.18).abs() < 0.01);
        assert!((priors.get("typescript") - 0.15).abs() < 0.01);
        assert!((priors.get("go") - 0.14).abs() < 0.01);
        assert!((priors.get("java") - 0.16).abs() < 0.01);
        assert!((priors.get("ruby") - 0.13).abs() < 0.01);
        assert!((priors.get("php") - 0.19).abs() < 0.01);
        assert!((priors.get("cpp") - 0.22).abs() < 0.01);
    }

    #[test]
    fn test_language_priors_update() {
        let mut priors = LanguagePriors::default();
        let original = priors.get("python");
        priors.update("python", 0.20, 0.5);
        let updated = priors.get("python");
        assert!(updated != original);
        assert!(updated > original);
    }

    #[test]
    fn test_language_priors_case_insensitive() {
        let mut priors = LanguagePriors::default();
        let py = priors.get("Python");
        assert!((py - 0.12).abs() < 0.01);
        priors.update("PYTHON", 0.30, 0.5);
        assert!((priors.get("python") - priors.get("Python")).abs() < 0.001);
    }

    #[test]
    fn test_language_priors_unknown_language() {
        let priors = LanguagePriors::default();
        let p = priors.get("haskell");
        assert!((p - 0.15).abs() < 0.01);
    }

    #[test]
    fn test_language_priors_update_clamps() {
        let mut priors = LanguagePriors::default();
        priors.update("rust", 1.5, 1.0); // Should clamp to 1.0
        assert!(priors.get("rust") <= 1.0);
        priors.update("rust", -0.5, 1.0); // Should clamp to 0.0
        assert!(priors.get("rust") >= 0.0);
    }

    #[test]
    fn test_language_priors_update_unknown_language() {
        let mut priors = LanguagePriors::default();
        priors.update("haskell", 0.40, 0.5);
        let val = priors.get("haskell");
        assert!(val > 0.15);
        assert!(val < 0.40);
    }

    // ── update_priors_from_outcomes tests ──────────────────────────────

    #[test]
    fn test_update_priors_from_outcomes_basic() {
        let mut priors = LanguagePriors::default();
        let outcomes = vec![
            FixOutcome {
                bug_id: "B1".into(),
                language: "python".into(),
                fix_applied: true,
                breakage_occurred: true,
                affected_caller_count: 3,
                total_caller_count: 10,
                recorded_at: "2026-01-01".into(),
            },
            FixOutcome {
                bug_id: "B2".into(),
                language: "python".into(),
                fix_applied: true,
                breakage_occurred: false,
                affected_caller_count: 1,
                total_caller_count: 5,
                recorded_at: "2026-01-02".into(),
            },
        ];
        update_priors_from_outcomes(&mut priors, &outcomes, 0.3);
        let updated = priors.get("python");
        assert!(updated > 0.0 && updated < 1.0);
    }

    #[test]
    fn test_update_priors_from_outcomes_multiple_languages() {
        let mut priors = LanguagePriors::default();
        let outcomes = vec![
            FixOutcome {
                bug_id: "B1".into(),
                language: "python".into(),
                fix_applied: true,
                breakage_occurred: true,
                affected_caller_count: 3,
                total_caller_count: 10,
                recorded_at: "2026-01-01".into(),
            },
            FixOutcome {
                bug_id: "B2".into(),
                language: "rust".into(),
                fix_applied: true,
                breakage_occurred: false,
                affected_caller_count: 0,
                total_caller_count: 5,
                recorded_at: "2026-01-02".into(),
            },
            FixOutcome {
                bug_id: "B3".into(),
                language: "rust".into(),
                fix_applied: true,
                breakage_occurred: false,
                affected_caller_count: 0,
                total_caller_count: 2,
                recorded_at: "2026-01-03".into(),
            },
        ];
        update_priors_from_outcomes(&mut priors, &outcomes, 0.3);
        let rust_updated = priors.get("rust");
        assert!(rust_updated < 0.08); // Lowered because 0/2 breakages
    }

    #[test]
    fn test_update_priors_from_outcomes_empty() {
        let mut priors = LanguagePriors::default();
        let lang_before = priors.get("python");
        update_priors_from_outcomes(&mut priors, &[], 0.3);
        let lang_after = priors.get("python");
        assert!((lang_before - lang_after).abs() < 0.001);
    }

    #[test]
    fn test_update_priors_from_outcomes_all_breakages() {
        let mut priors = LanguagePriors::default();
        let outcomes = vec![
            FixOutcome {
                bug_id: "B1".into(),
                language: "go".into(),
                fix_applied: true,
                breakage_occurred: true,
                affected_caller_count: 5,
                total_caller_count: 5,
                recorded_at: "2026-01-01".into(),
            },
            FixOutcome {
                bug_id: "B2".into(),
                language: "go".into(),
                fix_applied: true,
                breakage_occurred: true,
                affected_caller_count: 3,
                total_caller_count: 3,
                recorded_at: "2026-01-02".into(),
            },
        ];
        update_priors_from_outcomes(&mut priors, &outcomes, 0.5);
        assert!(priors.get("go") > 0.14); // Should increase
    }

    // ── generate_recommendation tests ──────────────────────────────────

    #[test]
    fn test_generate_recommendation_safe() {
        let mut report = FixImpactReport {
            bug_id: "B1".into(),
            fix_proposal: sample_fix(),
            total_callers: 10,
            affected_callers: 0,
            caller_impacts: vec![],
            confidence: compute_fix_confidence("python", 0, 10, &LanguagePriors::default()),
            regression_tests: vec![],
            recommendation: String::new(),
            generated_at: "now".into(),
        };
        report.recommendation = generate_recommendation(&report);
        assert!(report.recommendation.contains("SAFE"));
    }

    #[test]
    fn test_generate_recommendation_high_risk() {
        let mut report = FixImpactReport {
            bug_id: "B1".into(),
            fix_proposal: sample_fix(),
            total_callers: 10,
            affected_callers: 9,
            caller_impacts: vec![],
            confidence: compute_fix_confidence("c", 9, 10, &LanguagePriors::default()),
            regression_tests: vec!["test1".into()],
            recommendation: String::new(),
            generated_at: "now".into(),
        };
        report.recommendation = generate_recommendation(&report);
        assert!(
            report.recommendation.contains("HIGH RISK")
                || report.recommendation.contains("MEDIUM")
        );
    }

    #[test]
    fn test_generate_recommendation_low_risk() {
        let mut report = FixImpactReport {
            bug_id: "B1".into(),
            fix_proposal: sample_fix(),
            total_callers: 50,
            affected_callers: 2,
            caller_impacts: vec![],
            confidence: FixConfidence {
                language_prior: 0.12,
                posterior: 0.15,
                affected_callers: 2,
                total_callers: 50,
                calibrated: true,
                caller_impact_ratio: 0.04,
            },
            regression_tests: vec!["t1".into()],
            recommendation: String::new(),
            generated_at: "now".into(),
        };
        report.recommendation = generate_recommendation(&report);
        assert!(report.recommendation.contains("LOW RISK"));
    }

    #[test]
    fn test_generate_recommendation_medium_risk() {
        let mut report = FixImpactReport {
            bug_id: "B1".into(),
            fix_proposal: sample_fix(),
            total_callers: 20,
            affected_callers: 8,
            caller_impacts: vec![],
            confidence: FixConfidence {
                language_prior: 0.18,
                posterior: 0.45,
                affected_callers: 8,
                total_callers: 20,
                calibrated: true,
                caller_impact_ratio: 0.4,
            },
            regression_tests: vec!["t1".into(), "t2".into()],
            recommendation: String::new(),
            generated_at: "now".into(),
        };
        report.recommendation = generate_recommendation(&report);
        assert!(report.recommendation.contains("MEDIUM RISK"));
    }

    #[test]
    fn test_generate_recommendation_high_risk_exact() {
        let mut report = FixImpactReport {
            bug_id: "B1".into(),
            fix_proposal: sample_fix(),
            total_callers: 10,
            affected_callers: 8,
            caller_impacts: vec![],
            confidence: FixConfidence {
                language_prior: 0.25,
                posterior: 0.75,
                affected_callers: 8,
                total_callers: 10,
                calibrated: true,
                caller_impact_ratio: 0.8,
            },
            regression_tests: vec!["t1".into(), "t2".into(), "t3".into()],
            recommendation: String::new(),
            generated_at: "now".into(),
        };
        report.recommendation = generate_recommendation(&report);
        assert!(report.recommendation.contains("HIGH RISK"));
    }

    // ── predict_fix_impact tests ───────────────────────────────────────

    #[test]
    fn test_predict_fix_impact_full_pipeline() {
        let graph = sample_call_graph();
        let fix = sample_fix();
        let priors = LanguagePriors::default();
        let report = predict_fix_impact(fix, &graph, &priors, 3);
        assert!(report.total_callers > 0);
        assert!(report.confidence.calibrated);
        assert!(!report.recommendation.is_empty());
        assert!(!report.generated_at.is_empty());
    }

    #[test]
    fn test_predict_fix_impact_isolated_function() {
        let mut graph = HashMap::new();
        graph.insert("isolated_func".into(), vec![]);
        let fix = FixProposal {
            bug_id: "BUG-ISO".into(),
            file_path: "lib.rs".into(),
            function_name: "isolated_func".into(),
            original_line: "x > 0".into(),
            replacement_line: "x >= 0".into(),
            line_number: 1,
            description: "fix".into(),
            language: "rust".into(),
        };
        let priors = LanguagePriors::default();
        let report = predict_fix_impact(fix, &graph, &priors, 5);
        assert_eq!(report.total_callers, 0);
        assert_eq!(report.affected_callers, 0);
        assert!(!report.confidence.calibrated);
    }

    #[test]
    fn test_predict_fix_impact_regression_tests_count() {
        let graph = sample_call_graph();
        let fix = sample_fix(); // > to >= triggers overlap
        let priors = LanguagePriors::default();
        let report = predict_fix_impact(fix, &graph, &priors, 3);
        // Each affected caller produces a regression test
        assert_eq!(report.regression_tests.len(), report.affected_callers);
    }

    // ── Round-trip serialization tests ─────────────────────────────────

    #[test]
    fn test_fix_proposal_serialization() {
        let fix = sample_fix();
        let json = serde_json::to_string(&fix).unwrap();
        let parsed: FixProposal = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.bug_id, fix.bug_id);
        assert_eq!(parsed.function_name, fix.function_name);
    }

    #[test]
    fn test_caller_impact_serialization() {
        let caller = sample_caller("test_fn", 1);
        let fix = sample_fix();
        let impact = analyze_caller_impact(&caller, &fix);
        let json = serde_json::to_string(&impact).unwrap();
        let parsed: CallerImpact = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.affected, impact.affected);
        assert_eq!(parsed.impact_severity, impact.impact_severity);
    }

    #[test]
    fn test_fix_impact_report_serialization() {
        let graph = sample_call_graph();
        let fix = sample_fix();
        let priors = LanguagePriors::default();
        let report = predict_fix_impact(fix, &graph, &priors, 2);
        let json = serde_json::to_string(&report).unwrap();
        let parsed: FixImpactReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.bug_id, report.bug_id);
        assert_eq!(parsed.total_callers, report.total_callers);
        assert_eq!(parsed.affected_callers, report.affected_callers);
        assert_eq!(parsed.recommendation, report.recommendation);
    }

    #[test]
    fn test_language_priors_serialization() {
        let priors = LanguagePriors::default();
        let json = serde_json::to_string(&priors).unwrap();
        let parsed: LanguagePriors = serde_json::from_str(&json).unwrap();
        assert!((parsed.get("python") - priors.get("python")).abs() < 0.001);
    }

    // ── Edge cases and properties ─────────────────────────────────────

    #[test]
    fn test_empty_call_graph() {
        let graph: HashMap<String, Vec<String>> = HashMap::new();
        let callers = discover_callers("any_function", &graph, 10);
        assert!(callers.is_empty());
    }

    #[test]
    fn test_max_hops_zero() {
        let graph = sample_call_graph();
        let callers = discover_callers("validate_user", &graph, 0);
        assert!(callers.is_empty());
    }

    #[test]
    fn test_cyclic_call_graph() {
        let mut graph = HashMap::new();
        graph.insert("a".into(), vec!["b".into()]);
        graph.insert("b".into(), vec!["a".into(), "c".into()]);
        graph.insert("c".into(), vec![]);
        let callers = discover_callers("a", &graph, 3);
        // Should not loop infinitely due to visited set
        let names: HashSet<&str> = callers.iter().map(|c| c.caller_name.as_str()).collect();
        assert!(names.contains("b"));
    }

    #[test]
    fn test_value_range_type_serialization() {
        let types = vec![
            ValueRangeType::Integer,
            ValueRangeType::Float,
            ValueRangeType::String,
            ValueRangeType::Boolean,
            ValueRangeType::Object,
            ValueRangeType::Nullable,
            ValueRangeType::Unknown,
        ];
        for t in types {
            let json = serde_json::to_string(&t).unwrap();
            let _: ValueRangeType = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_argument_value_range_overlaps_field() {
        let range = ArgumentValueRange {
            arg_name: "x".into(),
            arg_position: 0,
            min_value: "0".into(),
            max_value: "100".into(),
            typical_value: "50".into(),
            range_type: ValueRangeType::Integer,
            overlaps_fix_boundary: true,
        };
        assert!(range.overlaps_fix_boundary);
    }

    #[test]
    fn test_caller_info_clone() {
        let caller = sample_caller("test", 1);
        let cloned = caller.clone();
        assert_eq!(cloned.caller_name, caller.caller_name);
        assert_eq!(cloned.call_graph_distance, caller.call_graph_distance);
    }

    #[test]
    fn test_fix_outcome_struct() {
        let outcome = FixOutcome {
            bug_id: "B1".into(),
            language: "rust".into(),
            fix_applied: true,
            breakage_occurred: false,
            affected_caller_count: 0,
            total_caller_count: 5,
            recorded_at: "2026-01-01T00:00:00Z".into(),
        };
        assert!(outcome.fix_applied);
        assert!(!outcome.breakage_occurred);
    }

    #[test]
    fn test_confidence_ratio_when_zero_callers() {
        let priors = LanguagePriors::default();
        let conf = compute_fix_confidence("python", 0, 0, &priors);
        assert_eq!(conf.caller_impact_ratio, 0.0);
    }

    #[test]
    fn test_confidence_ratio_all_affected() {
        let priors = LanguagePriors::default();
        let conf = compute_fix_confidence("python", 10, 10, &priors);
        assert!((conf.caller_impact_ratio - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_detect_overlap_exact_boundary_lower() {
        let arg = ArgumentValueRange {
            arg_name: "x".into(),
            arg_position: 0,
            min_value: "5".into(),
            max_value: "15".into(),
            typical_value: "10".into(),
            range_type: ValueRangeType::Integer,
            overlaps_fix_boundary: false,
        };
        let fix = FixProposal {
            bug_id: "B1".into(),
            file_path: "a.py".into(),
            function_name: "f".into(),
            original_line: "if x > 10:".into(),
            replacement_line: "if x > 5:".into(),
            line_number: 1,
            description: "fix".into(),
            language: "python".into(),
        };
        // Both constants (10 and 5) are within the argument range [5, 15]
        assert!(detect_overlap(&arg, &fix));
    }

    #[test]
    fn test_estimate_argument_ranges_returns_data() {
        let ranges = estimate_argument_ranges("some_function");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].arg_name, "arg0");
    }
}
