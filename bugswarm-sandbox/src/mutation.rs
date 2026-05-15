//! Mutation Testing as Bug Oracle
//! Phase 26: Injects artificial faults ("mutants") into code and runs tests.
//! Survivors (passing tests on mutated code) are flagged as untested code paths.

use serde::{Deserialize, Serialize};

/// Mutation operator types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MutationOperator {
    /// Flip arithmetic: +↔-, *↔/, %↔*
    Arithmetic,
    /// Flip comparison: ==↔!=, >↔>=, <↔<=
    Comparison,
    /// Flip logical: &&↔||, ! removed
    Logical,
    /// Mutate constants: int→int+1, str→str+'\0'
    Constant,
    /// Remove null checks, unwrap, is_none guards
    NullCheck,
    /// Invert control flow: if-condition→true/false
    ControlFlow,
}

impl MutationOperator {
    pub fn all() -> Vec<Self> {
        vec![Self::Arithmetic, Self::Comparison, Self::Logical,
             Self::Constant, Self::NullCheck, Self::ControlFlow]
    }
    
    pub fn name(&self) -> &'static str {
        match self {
            Self::Arithmetic => "Arithmetic",
            Self::Comparison => "Comparison",
            Self::Logical => "Logical",
            Self::Constant => "Constant",
            Self::NullCheck => "NullCheck",
            Self::ControlFlow => "ControlFlow",
        }
    }
}

/// Configuration for mutation testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationConfig {
    /// Operators to apply.
    #[serde(default = "MutationOperator::all")]
    pub operators: Vec<MutationOperator>,
    /// Max mutants to generate (safety limit).
    #[serde(default = "default_max_mutants")]
    pub max_mutants: usize,
    /// Filter: only mutate code within N hops of taint sinks.
    #[serde(default = "default_taint_radius")]
    pub taint_radius: usize,
    /// Enable equivalent mutant detection.
    #[serde(default = "default_true")]
    pub detect_equivalents: bool,
    /// Rerun tests N times for flaky detection.
    #[serde(default = "default_reruns")]
    pub rerun_count: usize,
}

fn default_max_mutants() -> usize { 500 }
fn default_taint_radius() -> usize { 2 }
fn default_true() -> bool { true }
fn default_reruns() -> usize { 3 }

impl Default for MutationConfig {
    fn default() -> Self {
        Self {
            operators: MutationOperator::all(),
            max_mutants: default_max_mutants(),
            taint_radius: default_taint_radius(),
            detect_equivalents: true,
            rerun_count: default_reruns(),
        }
    }
}

/// A single source-line mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutant {
    pub id: String,
    pub file_path: String,
    pub line_number: u32,
    pub operator: MutationOperator,
    pub original_line: String,
    pub mutated_line: String,
    pub description: String,
    /// Distance in CFG hops from nearest taint sink.
    pub taint_distance: Option<usize>,
}

/// Result of executing a single mutant against the test suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutantResult {
    pub mutant: Mutant,
    /// Number of tests that PASSED (bad — means untested).
    pub tests_passed: usize,
    /// Number of tests that FAILED (good — test caught it).
    pub tests_failed: usize,
    /// Whether the mutant survived (all tests passed).
    pub survived: bool,
    /// Whether the mutant is equivalent (logically same as original).
    pub is_equivalent: bool,
    /// Confidence that this is NOT equivalent (1.0 = definitely different).
    pub kill_confidence: f64,
    /// Suggested test that would kill this mutant.
    pub prescribed_test: Option<String>,
}

/// Aggregated mutation testing session result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationSessionResult {
    pub total_mutants: usize,
    pub mutants_killed: usize,
    pub mutants_survived: usize,
    pub equivalents_detected: usize,
    pub mutation_score: f64,
    pub results: Vec<MutantResult>,
    pub top_survivors: Vec<MutantResult>,
    pub elapsed_ms: u64,
}

// ── Mutant Generator ────────────────────────────────────────────────────

/// Generate mutants from source code. Simplified: operates on line-level mutations.
pub fn generate_mutants(
    source_lines: &[&str],
    file_path: &str,
    operators: &[MutationOperator],
    config: &MutationConfig,
) -> Vec<Mutant> {
    let mut mutants = Vec::new();
    
    for (line_idx, line) in source_lines.iter().enumerate() {
        let line_num = (line_idx + 1) as u32;
        let trimmed = line.trim();
        
        if mutants.len() >= config.max_mutants { break; }
        
        for op in operators {
            match op {
                MutationOperator::Comparison => {
                    if trimmed.contains("==") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            &line.replace("==", "!="),
                            "flipped equality to inequality",
                            None,
                        ));
                    } else if trimmed.contains("!=") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            &line.replace("!=", "=="),
                            "flipped inequality to equality",
                            None,
                        ));
                    } else if trimmed.contains(">") && !trimmed.contains(">>") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            &line.replacen(">", ">=", 1),
                            "changed > to >=",
                            None,
                        ));
                    } else if trimmed.contains("<") && !trimmed.contains("<<") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            &line.replacen("<", "<=", 1),
                            "changed < to <=",
                            None,
                        ));
                    }
                }
                MutationOperator::Arithmetic => {
                    if trimmed.contains(" + ") && !trimmed.contains("++") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            &line.replacen(" + ", " - ", 1),
                            "flipped addition to subtraction",
                            None,
                        ));
                    } else if trimmed.contains(" - ") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            &line.replacen(" - ", " + ", 1),
                            "flipped subtraction to addition",
                            None,
                        ));
                    } else if trimmed.contains(" * ") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            &line.replacen(" * ", " / ", 1),
                            "flipped multiply to divide",
                            None,
                        ));
                    }
                }
                MutationOperator::Logical => {
                    if trimmed.contains("&&") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            &line.replace("&&", "||"),
                            "flipped AND to OR",
                            None,
                        ));
                    } else if trimmed.contains("||") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            &line.replace("||", "&&"),
                            "flipped OR to AND",
                            None,
                        ));
                    }
                }
                MutationOperator::Constant => {
                    if let Some(constant_line) = mutate_constants(trimmed) {
                        if constant_line != trimmed {
                            mutants.push(create_mutant(
                                file_path, line_num, *op, line,
                                &constant_line,
                                "mutated constant value",
                                None,
                            ));
                        }
                    }
                }
                MutationOperator::NullCheck => {
                    if trimmed.contains(".unwrap()") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            &line.replace(".unwrap()", "/* removed unwrap */"),
                            "removed unwrap null check",
                            None,
                        ));
                    } else if trimmed.contains("if") && (trimmed.contains("is_none()") || trimmed.contains(" == None") || trimmed.contains(" == null")) {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            "// null check removed",
                            "removed null check guard",
                            None,
                        ));
                    }
                }
                MutationOperator::ControlFlow => {
                    if trimmed.starts_with("if ") {
                        mutants.push(create_mutant(
                            file_path, line_num, *op, line,
                            "if true: // control flow mutant",
                            "always-true branch mutation",
                            None,
                        ));
                    }
                }
            }
        }
    }
    
    mutants.truncate(config.max_mutants);
    mutants
}

fn create_mutant(file: &str, line: u32, op: MutationOperator,
                 orig: &str, mutated: &str, desc: &str,
                 taint_dist: Option<usize>) -> Mutant {
    Mutant {
        id: format!("mut-{}-L{}-{:?}", file.replace('/', "_"), line, op),
        file_path: file.to_string(),
        line_number: line,
        operator: op,
        original_line: orig.to_string(),
        mutated_line: mutated.to_string(),
        description: desc.to_string(),
        taint_distance: taint_dist,
    }
}

/// Mutate constant values in a line.
fn mutate_constants(line: &str) -> Option<String> {
    // Find numeric constants and modify them
    let words: Vec<&str> = line.split_whitespace().collect();
    let mut changed = false;
    let result: Vec<String> = words.iter().map(|w| {
        if let Ok(n) = w.trim_matches(|c: char| !c.is_ascii_digit() && c != '-').parse::<i64>() {
            changed = true;
            w.replace(&n.to_string(), &(n + 1).to_string())
        } else {
            w.to_string()
        }
    }).collect();
    if changed { Some(result.join(" ")) } else { None }
}

// ── Equivalent Mutant Detection ─────────────────────────────────────────

/// Check if a mutant is logically equivalent to the original.
/// Simplified heuristic: identical behavior for common patterns.
pub fn is_equivalent(original: &str, mutated: &str, operator: MutationOperator) -> bool {
    match operator {
        MutationOperator::Comparison => {
            // "if x > 0" → "if x >= 1" are equivalent for integers
            // Simplified: just check basic reversals
            false // Conservative: assume not equivalent unless proven
        }
        MutationOperator::Arithmetic => {
            // "x + 0" → "x - 0" are equivalent
            // "x * 1" → "x / 1" are equivalent
            original.trim() == mutated.trim()
        }
        MutationOperator::Logical => {
            // De Morgan equivalent: !(a && b) == (!a || !b)
            false // Conservative
        }
        _ => false,
    }
}

// ── Mutation Session ────────────────────────────────────────────────────

/// Run a mutation testing session.
pub fn run_mutation_session(
    source_code: &str,
    file_path: &str,
    config: &MutationConfig,
    test_runner: impl Fn(&str) -> (usize, usize), // (passed, failed)
) -> MutationSessionResult {
    let t0 = std::time::Instant::now();
    let lines: Vec<&str> = source_code.lines().collect();
    
    let mutants = generate_mutants(&lines, file_path, &config.operators, config);
    
    let mut results = Vec::new();
    let mut killed = 0;
    let mut survived = 0;
    let mut equivalents = 0;
    
    for mutant in &mutants {
        // Build mutated source
        let mut mutated_source = source_code.to_string();
        // Replace the specific line
        let line_idx = (mutant.line_number - 1) as usize;
        if line_idx < lines.len() {
            let mut new_lines: Vec<&str> = source_code.lines().collect();
            new_lines[line_idx] = &mutant.mutated_line;
            mutated_source = new_lines.join("\n");
        }
        
        let (passed, failed) = test_runner(&mutated_source);
        
        let equivalent = config.detect_equivalents
            && is_equivalent(&mutant.original_line, &mutant.mutated_line, mutant.operator);
        
        let survived_bool = failed == 0 && passed > 0;
        
        if survived_bool && !equivalent {
            survived += 1;
        } else if equivalent {
            equivalents += 1;
        } else {
            killed += 1;
        }
        
        let prescribed = if survived_bool {
            Some(generate_prescribed_test(mutant))
        } else {
            None
        };
        
        results.push(MutantResult {
            mutant: mutant.clone(),
            tests_passed: passed,
            tests_failed: failed,
            survived: survived_bool && !equivalent,
            is_equivalent: equivalent,
            kill_confidence: if equivalent { 0.0 } else if survived_bool { 0.1 } else { 0.9 },
            prescribed_test: prescribed,
        });
    }
    
    let total = mutants.len();
    let mutation_score = if total > 0 { killed as f64 / total as f64 } else { 0.0 };
    
    let mut top_survivors: Vec<MutantResult> = results.iter()
        .filter(|r| r.survived)
        .take(20)
        .cloned()
        .collect();
    top_survivors.sort_by(|a, b| {
        a.mutant.taint_distance.unwrap_or(999)
            .cmp(&b.mutant.taint_distance.unwrap_or(999))
    });
    
    MutationSessionResult {
        total_mutants: total,
        mutants_killed: killed,
        mutants_survived: survived,
        equivalents_detected: equivalents,
        mutation_score,
        results,
        top_survivors,
        elapsed_ms: t0.elapsed().as_millis() as u64,
    }
}

/// Generate a suggested test that would kill this surviving mutant.
fn generate_prescribed_test(mutant: &Mutant) -> String {
    format!(
        "// Suggested test to kill mutant at {}:{}:\n\
         // Original: {}\n\
         // Mutated:  {}\n\
         // Operator: {:?}\n\
         // This mutant survived because no test exercises the {}\n\
         // boundary condition at this line. Consider adding a test\n\
         // that specifically targets this edge case.",
        mutant.file_path, mutant.line_number,
        mutant.original_line, mutant.mutated_line,
        mutant.operator, mutant.operator.name(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mutants_comparison() {
        let source = vec!["if x == 5:", "    return True", "if y != 3:", "    return False"];
        let config = MutationConfig::default();
        let mutants = generate_mutants(&source, "test.py", &[MutationOperator::Comparison], &config);
        assert!(mutants.len() >= 2);
        assert!(mutants.iter().any(|m| m.original_line.contains("==")));
        assert!(mutants.iter().any(|m| m.original_line.contains("!=")));
    }

    #[test]
    fn test_generate_mutants_arithmetic() {
        let source = vec!["result = a + b", "x = y * z"];
        let config = MutationConfig::default();
        let mutants = generate_mutants(&source, "test.py", &[MutationOperator::Arithmetic], &config);
        assert!(mutants.len() >= 2);
    }

    #[test]
    fn test_generate_mutants_logical() {
        let source = vec!["if a && b:", "    run()"];
        let config = MutationConfig::default();
        let mutants = generate_mutants(&source, "test.py", &[MutationOperator::Logical], &config);
        assert!(!mutants.is_empty());
        assert!(mutants[0].mutated_line.contains("||"));
    }

    #[test]
    fn test_generate_mutants_null_check() {
        let source = vec!["value.unwrap()", "if x.is_none():"];
        let config = MutationConfig::default();
        let mutants = generate_mutants(&source, "test.py", &[MutationOperator::NullCheck], &config);
        assert!(mutants.len() >= 1);
    }

    #[test]
    fn test_is_equivalent_basic() {
        assert!(!is_equivalent("x == 5", "x != 5", MutationOperator::Comparison));
    }

    #[test]
    fn test_constant_mutation() {
        let result = mutate_constants("x = 42");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "x = 43");
    }

    #[test]
    fn test_run_mutation_session() {
        let source = "def add(a, b):\n    return a + b\n";
        let config = MutationConfig { max_mutants: 10, ..Default::default() };
        
        let test_runner = |code: &str| -> (usize, usize) {
            if code.contains(" - ") {
                (1, 1) // Subtraction → test fails (caught it!)
            } else if code.contains(" * ") {
                (1, 0) // Multiplication → test passes (survivor!)
            } else {
                (2, 0) // Original passes
            }
        };
        
        let result = run_mutation_session(source, "add.py", &config, test_runner);
        assert!(result.total_mutants > 0);
        assert!(result.mutation_score >= 0.0);
    }

    #[test]
    fn test_mutation_config_defaults() {
        let config = MutationConfig::default();
        assert_eq!(config.operators.len(), 6);
        assert_eq!(config.max_mutants, 500);
        assert_eq!(config.taint_radius, 2);
        assert!(config.detect_equivalents);
    }

    #[test]
    fn test_generate_mutants_respects_max() {
        let config = MutationConfig { max_mutants: 3, ..Default::default() };
        let lines: Vec<String> = (0..100).map(|i| format!("x == {}", i)).collect();
        let source: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let mutants = generate_mutants(&source, "big.py", &[MutationOperator::Comparison], &config);
        assert_eq!(mutants.len(), 3);
    }

    #[test]
    fn test_prescribed_test_generated() {
        let mutant = create_mutant("test.py", 42, MutationOperator::Comparison,
            "if x == 5:", "if x != 5:", "flipped", None);
        let test = generate_prescribed_test(&mutant);
        assert!(test.contains("test.py"));
        assert!(test.contains("42"));
        assert!(test.contains("Comparison"));
    }
}
