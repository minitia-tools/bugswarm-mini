use std::collections::{HashMap, VecDeque};

use crate::graph::CodePropertyGraph;

/// Danger map entry: (function_address, danger_score)
pub type DangerEntry = (u64, f32);

/// Security-sensitive sink function names.
pub const DEFAULT_SINKS: &[&str] = &[
    "memcpy", "memmove", "strcpy", "strncpy", "sprintf", "snprintf",
    "system", "execve", "execvp", "execl", "execlp",
    "mmap", "mprotect",
    "read", "recv", "recvfrom", "recvmsg",
    "fgets", "gets", "scanf", "fscanf", "sscanf",
    "free", "malloc", "realloc", "calloc",
    "strcat", "strncat",
    "popen", "dlopen",
];

/// Taint source function names.
pub const DEFAULT_SOURCES: &[&str] = &[
    "read", "fread", "recv", "recvfrom", "recvmsg",
    "scanf", "fscanf", "sscanf", "gets", "fgets",
    "getenv", "getopt", "getopt_long",
    "argv", "argc",
    "mmap", "mmap64",
];

/// Function information extracted for danger map computation.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub address: u64,
    pub callees: Vec<String>,
    pub callers: Vec<String>,
}

/// Compute a danger map from the call graph and sink list.
///
/// Algorithm: Reverse BFS from all sinks, propagating danger scores
/// backward through the call graph with multiplicative decay.
/// For each node, the danger score is the maximum score reachable
/// from any sink via reverse call chain, multiplied by decay at each hop.
pub fn compute_danger_map(
    functions: &[FunctionInfo],
    sinks: &[&str],
    decay_factor: f32,
) -> Vec<DangerEntry> {
    if sinks.is_empty() || functions.is_empty() {
        return Vec::new();
    }

    // Map function name to its index for fast lookup
    let name_to_idx: HashMap<&str, usize> = functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();

    // scores[i] = current danger score for functions[i]
    let mut scores: Vec<f32> = vec![0.0; functions.len()];
    let mut queue = VecDeque::new();

    // Initialize sink nodes with score 1.0
    for &sink in sinks {
        if let Some(&idx) = name_to_idx.get(sink) {
            scores[idx] = 1.0;
            queue.push_back(idx);
        }
    }

    // Reverse BFS with max-tracking
    while let Some(current) = queue.pop_front() {
        let current_score = scores[current];
        let caller_names = &functions[current].callers;

        for caller_name in caller_names {
            if let Some(&caller_idx) = name_to_idx.get(caller_name.as_str()) {
                let new_score = current_score * decay_factor;
                if new_score > scores[caller_idx] {
                    scores[caller_idx] = new_score;
                    queue.push_back(caller_idx);
                }
            }
        }
    }

    // Collect scored nodes, filter out zero scores, sort by address
    let mut entries: Vec<DangerEntry> = functions
        .iter()
        .enumerate()
        .filter(|(i, _)| scores[*i] > 0.0)
        .map(|(i, f)| (f.address, scores[i]))
        .collect();

    entries.sort_by_key(|(addr, _)| *addr);
    entries
}

/// Compute a stable hash from a function name to use as an address.
fn name_to_address(name: &str) -> u64 {
    let mut hash: u64 = 5381;
    for &byte in name.as_bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

/// Build function info from a Code Property Graph's function table.
pub fn extract_functions(graph: &CodePropertyGraph) -> Vec<FunctionInfo> {
    graph
        .functions
        .values()
        .map(|gf| FunctionInfo {
            name: gf.name.clone(),
            address: name_to_address(&gf.name),
            callees: gf.calls.clone(),
            callers: gf.called_by.clone(),
        })
        .collect()
}

/// Compute danger map from a code property graph using default sinks.
pub fn danger_map_from_graph(graph: &CodePropertyGraph, decay: f32) -> Vec<DangerEntry> {
    let functions = extract_functions(graph);
    compute_danger_map(&functions, DEFAULT_SINKS, decay)
}
