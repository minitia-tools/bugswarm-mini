use serde::{Deserialize, Serialize};

/// A symbolic variable representing an unknown input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolicVar {
    pub name: String,
    pub bit_width: u32,
    pub var_type: SymbolicType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolicType {
    Int64,
    String,
    Bool,
    Float64,
    Array,
}

/// A constraint on symbolic variables collected from a code path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathConstraint {
    pub description: String,
    pub variables: Vec<String>,
    pub expression: String,  // SMT-LIB2 format
}

/// A concrete solution produced by the SMT solver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcreteSolution {
    pub variables: Vec<VariableAssignment>,
    pub solver_used: String,
    pub solve_time_ms: u64,
    pub is_satisfiable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableAssignment {
    pub name: String,
    pub value: String,
    pub var_type: SymbolicType,
}

/// Configuration for symbolic execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolicConfig {
    pub max_paths: usize,
    pub max_depth: usize,
    pub timeout_secs: u64,
    pub use_multi_solver: bool,
    pub concretize_externals: bool,
    pub distance_heuristic: bool,
}

impl Default for SymbolicConfig {
    fn default() -> Self {
        Self {
            max_paths: 1000,
            max_depth: 50,
            timeout_secs: 30,
            use_multi_solver: true,
            concretize_externals: true,
            distance_heuristic: true,
        }
    }
}

/// Result of a symbolic execution session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolicSessionResult {
    pub target_location: String,
    pub paths_explored: usize,
    pub constraints_generated: usize,
    pub solutions_found: usize,
    pub solutions: Vec<ConcreteSolution>,
    pub elapsed_ms: u64,
    pub solver_stats: SolverStats,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SolverStats {
    pub z3_attempts: usize,
    pub z3_successes: usize,
    pub cvc5_attempts: usize,
    pub cvc5_successes: usize,
    pub concolic_fallbacks: usize,
    pub timeouts: usize,
}
