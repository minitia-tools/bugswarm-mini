//! Danger Map Export — reverse BFS from sinks with multiplicative decay.
//!
//! C6.2 PEAK: Priority-queue based reverse propagation from all sinks
//! simultaneously through the call graph. Score = sink_score × decay^distance.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::graph::CodePropertyGraph;

/// Security-sensitive sink function names (C standard library).
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

/// Function information extracted from CPG for danger map computation.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub address: u64,
    pub callees: Vec<String>,
    pub callers: Vec<String>,
}

/// Danger map entry: (address, danger_score).
pub type DangerEntry = (u64, f32);

/// Default multiplicative decay factor.
pub const DEFAULT_DECAY: f32 = 0.7;

/// Compute a danger map from function information.
///
/// Algorithm: Priority-queue based reverse propagation (BFS) from all sinks
/// simultaneously. Each sink starts with score 1.0. Scores propagate backward
/// through caller edges: caller_score = max(existing, callee_score × decay).
/// Returns Vec<(address, score)> sorted by address.
pub fn compute_danger_map(
    functions: &[FunctionInfo],
    sink_names: &[&str],
    decay: f32,
) -> Vec<DangerEntry> {
    if functions.is_empty() || sink_names.is_empty() {
        return vec![];
    }

    // Build lookup maps
    let name_to_idx: HashMap<&str, usize> = functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();

    let mut scores: Vec<f32> = vec![0.0; functions.len()];
    let sink_set: HashSet<&str> = sink_names.iter().copied().collect();

    // Initialize sinks with score 1.0
    let mut queue: VecDeque<usize> = VecDeque::new();
    for (i, func) in functions.iter().enumerate() {
        if sink_set.contains(func.name.as_str()) {
            scores[i] = 1.0;
            queue.push_back(i);
        }
    }

    if queue.is_empty() {
        return vec![];
    }

    // Track visited nodes to prevent infinite cycles
    let mut visited: HashSet<usize> = HashSet::new();
    for &idx in &queue {
        visited.insert(idx);
    }

    // BFS backward through callers
    while let Some(callee_idx) = queue.pop_front() {
        let callee_score = scores[callee_idx];
        let mut propagated = callee_score * decay;
        if propagated > 1.0 {
            propagated = 1.0;
        }

        let func = &functions[callee_idx];
        for caller_name in &func.callers {
            if let Some(&caller_idx) = name_to_idx.get(caller_name.as_str()) {
                if propagated > scores[caller_idx] {
                    scores[caller_idx] = propagated;
                    if visited.insert(caller_idx) {
                        queue.push_back(caller_idx);
                    }
                    if visited.contains(&caller_idx) {
                        queue.push_back(caller_idx);
                    }
                }
            }
        }

        // Safety cap to prevent infinite loops
        if queue.len() > functions.len() * 2 {
            break;
        }
    }

    // Collect non-zero entries
    let mut entries: Vec<DangerEntry> = functions
        .iter()
        .enumerate()
        .filter(|(i, _)| scores[*i] > 0.0)
        .map(|(i, f)| (f.address, scores[i]))
        .collect();
    entries.sort_by_key(|(addr, _)| *addr);
    entries
}

/// Simple deterministic hash for address mapping.
fn djb2_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for c in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    hash
}

/// Extract function info from a CPG graph.
pub fn extract_functions(graph: &CodePropertyGraph) -> Vec<FunctionInfo> {
    graph
        .functions
        .values()
        .map(|gf| FunctionInfo {
            name: gf.name.clone(),
            address: djb2_hash(&gf.name),
            callees: gf.calls.clone(),
            callers: gf.called_by.clone(),
        })
        .collect()
}

/// Compute danger map directly from a CPG graph.
pub fn danger_map_from_graph(graph: &CodePropertyGraph, decay: f32) -> Vec<DangerEntry> {
    let functions = extract_functions(graph);
    compute_danger_map(&functions, DEFAULT_SINKS, decay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_djb2_deterministic() {
        let a = djb2_hash("memcpy");
        let b = djb2_hash("memcpy");
        assert_eq!(a, b);
        assert_ne!(djb2_hash("memcpy"), djb2_hash("strcpy"));
    }

    #[test]
    fn test_danger_map_simple_chain() {
        let funcs = vec![
            FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
            FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
            FunctionInfo { name: "C".into(), address: 0x3000, callees: vec![], callers: vec!["B".into()] },
        ];
        let map = compute_danger_map(&funcs, &["C"], 0.7);
        let get = |addr| map.iter().find(|(a, _)| *a == addr).map(|(_, s)| *s).unwrap_or(0.0);
        assert!((get(0x3000) - 1.0).abs() < 0.01);
        assert!((get(0x2000) - 0.7).abs() < 0.01);
        assert!((get(0x1000) - 0.49).abs() < 0.01);
    }

    #[test]
    fn test_empty_inputs() {
        assert!(compute_danger_map(&[], &["memcpy"], 0.7).is_empty());
        assert!(compute_danger_map(
            &[FunctionInfo { name: "x".into(), address: 0, callees: vec![], callers: vec![] }],
            &[],
            0.7,
        )
        .is_empty());
    }

    #[test]
    fn test_known_sinks() {
        assert!(DEFAULT_SINKS.contains(&"memcpy"));
        assert!(DEFAULT_SINKS.contains(&"system"));
        assert!(DEFAULT_SINKS.contains(&"free"));
    }

    #[test]
    fn test_multiple_paths_to_sink() {
        let funcs = vec![
            FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["C".into()], callers: vec![] },
            FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec![] },
            FunctionInfo { name: "C".into(), address: 0x3000, callees: vec![], callers: vec!["A".into(), "B".into()] },
        ];
        let map = compute_danger_map(&funcs, &["C"], 0.7);
        let get = |addr| map.iter().find(|(a, _)| *a == addr).map(|(_, s)| *s).unwrap_or(0.0);
        assert!((get(0x1000) - 0.7).abs() < 0.01);
        assert!((get(0x2000) - 0.7).abs() < 0.01);
        assert!((get(0x3000) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_cycle_safety() {
        // A → B → A (mutual recursion cycle)
        let funcs = vec![
            FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec!["B".into()] },
            FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["A".into()], callers: vec!["A".into()] },
        ];
        // No sinks — should return empty map rather than infinite loop
        let map = compute_danger_map(&funcs, &[], 0.7);
        assert!(map.is_empty());

        // With a sink on the cycle
        let map = compute_danger_map(&funcs, &["A"], 0.7);
        assert!(!map.is_empty());
        let get = |addr| map.iter().find(|(a,_)| *a == addr).map(|(_,s)| *s).unwrap_or(0.0);
        assert!((get(0x1000) - 1.0).abs() < 0.01);
        // B should get score 0.7 from A
        assert!((get(0x2000) - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_decay_above_one_with_cycle() {
        // With decay > 1.0, cycles MUST not infinite loop
        let funcs = vec![
            FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec!["B".into()] },
            FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["A".into()], callers: vec!["A".into()] },
        ];
        let map = compute_danger_map(&funcs, &["A"], 2.0);
        // Should complete without hanging
        assert!(!map.is_empty());
        let get = |addr| map.iter().find(|(a,_)| *a == addr).map(|(_,s)| *s).unwrap_or(0.0);
        assert!((get(0x1000) - 1.0).abs() < 0.01);
        // B gets 1.0 * 2.0 = 2.0, but capped to 1.0
        assert!((get(0x2000) - 1.0).abs() < 0.01, "Score should be capped at 1.0");
    }

    #[test]
    fn test_self_referencing_function() {
        // A calls itself (recursive)
        let funcs = vec![
            FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["A".into()], callers: vec!["A".into()] },
        ];
        let map = compute_danger_map(&funcs, &["A"], 0.7);
        assert!(!map.is_empty());
        assert!((map[0].1 - 1.0).abs() < 0.01);
    }
}
