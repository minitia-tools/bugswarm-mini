//! Phase 21 integration tests: taint-guided fuzzing danger map pipeline.
//!
//! Tests compute_danger_map, extract_functions, danger_map_from_graph,
//! sink/source constants, daemon integration, edge cases, and performance.

use bugswarm_cpg::danger_map::*;
use bugswarm_cpg::graph::{CodePropertyGraph, FunctionInfo as GraphFunctionInfo};
use std::time::Instant;

// ============================================================
// 1. Danger Map Computation Correctness
// ============================================================

#[test]
fn test_simple_chain_a_to_b_to_c() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec![], callers: vec!["B".into()] },
    ];
    let map = compute_danger_map(&funcs, &["C"], 0.7);
    assert_eq!(map.len(), 3);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x3000) - 1.0).abs() < 0.001);
    assert!((get(0x2000) - 0.7).abs() < 0.001);
    assert!((get(0x1000) - 0.49).abs() < 0.001);
}

#[test]
fn test_multiple_sinks_all_get_one() {
    let funcs = vec![
        FunctionInfo { name: "X".into(), address: 0x1000, callees: vec!["S1".into(), "S2".into()], callers: vec![] },
        FunctionInfo { name: "S1".into(), address: 0x2000, callees: vec![], callers: vec!["X".into()] },
        FunctionInfo { name: "S2".into(), address: 0x3000, callees: vec![], callers: vec!["X".into()] },
    ];
    let map = compute_danger_map(&funcs, &["S1", "S2"], 0.7);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x2000) - 1.0).abs() < 0.001, "Sink S1");
    assert!((get(0x3000) - 1.0).abs() < 0.001, "Sink S2");
    assert!((get(0x1000) - 0.7).abs() < 0.001, "Caller of both");
}

#[test]
fn test_cycle_in_call_graph_no_infinite_loop() {
    // A→B→C→A (3-cycle). C is sink. Decay 0.7 ensures termination.
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec!["C".into()] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec!["A".into()], callers: vec!["B".into()] },
    ];
    let map = compute_danger_map(&funcs, &["C"], 0.7);
    assert_eq!(map.len(), 3);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x3000) - 1.0).abs() < 0.001);
    assert!((get(0x2000) - 0.7).abs() < 0.001);
    assert!((get(0x1000) - 0.49).abs() < 0.001);
}

#[test]
fn test_self_referencing_function_no_infinite_loop() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["A".into()], callers: vec!["A".into()] },
    ];
    let map = compute_danger_map(&funcs, &["A"], 0.7);
    assert_eq!(map.len(), 1);
    assert!((map[0].1 - 1.0).abs() < 0.001);
}

#[test]
fn test_self_referencing_with_external_caller() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["A".into()], callers: vec!["B".into(), "A".into()] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["A".into()], callers: vec![] },
    ];
    let map = compute_danger_map(&funcs, &["A"], 0.7);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x1000) - 1.0).abs() < 0.001);
    assert!((get(0x2000) - 0.7).abs() < 0.001);
}

#[test]
fn test_disconnected_graph_components() {
    // A→B (B=sink), and C→D (disconnected). Only A,B get scores.
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec![], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec!["D".into()], callers: vec![] },
        FunctionInfo { name: "D".into(), address: 0x4000, callees: vec![], callers: vec!["C".into()] },
    ];
    let map = compute_danger_map(&funcs, &["B"], 0.7);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x2000) - 1.0).abs() < 0.001);
    assert!((get(0x1000) - 0.7).abs() < 0.001);
    assert_eq!(get(0x3000), -1.0, "C should not be in map");
    assert_eq!(get(0x4000), -1.0, "D should not be in map");
}

#[test]
fn test_custom_decay_0_0() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec![], callers: vec!["A".into()] },
    ];
    let map = compute_danger_map(&funcs, &["B"], 0.0);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x2000) - 1.0).abs() < 0.001, "Sink always 1.0");
    assert_eq!(get(0x1000), -1.0, "decay=0.0 means 0>0=false, caller excluded");
}

#[test]
fn test_custom_decay_0_5() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec![], callers: vec!["A".into()] },
    ];
    let map = compute_danger_map(&funcs, &["B"], 0.5);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x2000) - 1.0).abs() < 0.001);
    assert!((get(0x1000) - 0.5).abs() < 0.001);
}

#[test]
fn test_custom_decay_1_0() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec![], callers: vec!["B".into()] },
    ];
    let map = compute_danger_map(&funcs, &["C"], 1.0);
    assert_eq!(map.len(), 3);
    for (_, score) in &map {
        assert!((*score - 1.0).abs() < 0.001, "All scores = 1.0 with decay=1.0");
    }
}

#[test]
fn test_custom_decay_2_0_on_simple_chain() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec![], callers: vec!["B".into()] },
    ];
    let map = compute_danger_map(&funcs, &["C"], 2.0);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x3000) - 1.0).abs() < 0.001);
    // Scores capped at 1.0 for cycle safety
    assert!((get(0x2000) - 1.0).abs() < 0.001);
    assert!((get(0x1000) - 1.0).abs() < 0.001);
}

#[test]
fn test_negative_decay_value() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec![], callers: vec!["A".into()] },
    ];
    let map = compute_danger_map(&funcs, &["B"], -0.5);
    // propagated = 1.0 * -0.5 = -0.5; -0.5 > 0.0 is FALSE, so A excluded
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x2000) - 1.0).abs() < 0.001);
    assert_eq!(get(0x1000), -1.0, "Negative decay excludes callers");
}

#[test]
fn test_large_chain_performance() {
    const N: usize = 1000;
    let mut funcs = Vec::with_capacity(N);
    for i in 0..N {
        let name = format!("f{}", i);
        let callees = if i < N - 1 { vec![format!("f{}", i + 1)] } else { vec![] };
        let callers = if i > 0 { vec![format!("f{}", i - 1)] } else { vec![] };
        funcs.push(FunctionInfo { name, address: 0x1000 + i as u64, callees, callers });
    }
    let sink = format!("f{}", N - 1);
    let start = Instant::now();
    let map = compute_danger_map(&funcs, &[&sink], 0.7);
    let elapsed = start.elapsed();
    assert_eq!(map.len(), N);
    assert!(elapsed.as_millis() < 100, "1000-chain took {}ms", elapsed.as_millis());
}

// ============================================================
// 2. Sink Source Constants
// ============================================================

#[test]
fn test_default_sinks_critical_c_library() {
    // Memory
    for name in &["memcpy", "memmove", "strcpy", "strncpy", "sprintf", "snprintf", "strcat", "strncat"] {
        assert!(DEFAULT_SINKS.contains(name), "Missing sink: {}", name);
    }
    // Execution
    for name in &["system", "execve", "execvp", "execl", "execlp", "popen"] {
        assert!(DEFAULT_SINKS.contains(name), "Missing sink: {}", name);
    }
    // Memory mapping
    for name in &["mmap", "mprotect"] {
        assert!(DEFAULT_SINKS.contains(name), "Missing sink: {}", name);
    }
    // I/O
    for name in &["read", "recv", "recvfrom", "recvmsg"] {
        assert!(DEFAULT_SINKS.contains(name), "Missing sink: {}", name);
    }
    // Input
    for name in &["fgets", "gets", "scanf", "fscanf", "sscanf"] {
        assert!(DEFAULT_SINKS.contains(name), "Missing sink: {}", name);
    }
    // Allocation
    for name in &["free", "malloc", "realloc", "calloc"] {
        assert!(DEFAULT_SINKS.contains(name), "Missing sink: {}", name);
    }
    // Dynamic loading
    assert!(DEFAULT_SINKS.contains(&"dlopen"));
}

#[test]
fn test_default_sources_critical_input() {
    for name in &["read", "fread", "recv", "recvfrom", "recvmsg"] {
        assert!(DEFAULT_SOURCES.contains(name), "Missing source: {}", name);
    }
    for name in &["scanf", "fscanf", "sscanf", "gets", "fgets"] {
        assert!(DEFAULT_SOURCES.contains(name), "Missing source: {}", name);
    }
    for name in &["getenv", "getopt", "getopt_long", "argv", "argc"] {
        assert!(DEFAULT_SOURCES.contains(name), "Missing source: {}", name);
    }
    for name in &["mmap", "mmap64"] {
        assert!(DEFAULT_SOURCES.contains(name), "Missing source: {}", name);
    }
}

#[test]
fn test_empty_sink_list_returns_empty_map() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec![], callers: vec![] },
    ];
    assert!(compute_danger_map(&funcs, &[], 0.7).is_empty());
}

#[test]
fn test_sink_not_present_in_functions() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec![], callers: vec![] },
    ];
    assert!(compute_danger_map(&funcs, &["nonexistent"], 0.7).is_empty());
}

#[test]
fn test_sink_present_but_no_callers() {
    let funcs = vec![
        FunctionInfo { name: "solo_sink".into(), address: 0x1000, callees: vec![], callers: vec![] },
    ];
    let map = compute_danger_map(&funcs, &["solo_sink"], 0.7);
    assert_eq!(map.len(), 1);
    assert!((map[0].1 - 1.0).abs() < 0.001);
}

// ============================================================
// 3. FunctionInfo Extraction from Graph
// ============================================================

#[test]
fn test_extract_functions_from_populated_graph() {
    let mut graph = CodePropertyGraph::new();
    graph.functions.insert("func_a".into(), GraphFunctionInfo {
        name: "func_a".into(), file: "test.c".into(),
        line_start: 1, line_end: 10, params: vec![], return_type: None,
        calls: vec!["func_b".into()], called_by: vec![],
        is_async: false, is_exported: false,
    });
    graph.functions.insert("func_b".into(), GraphFunctionInfo {
        name: "func_b".into(), file: "test.c".into(),
        line_start: 12, line_end: 20, params: vec![], return_type: None,
        calls: vec![], called_by: vec!["func_a".into()],
        is_async: false, is_exported: false,
    });

    let funcs = extract_functions(&graph);
    assert_eq!(funcs.len(), 2);
    let a = funcs.iter().find(|f| f.name == "func_a").unwrap();
    let b = funcs.iter().find(|f| f.name == "func_b").unwrap();
    assert!(a.address > 0);
    assert!(b.address > 0);
    assert_ne!(a.address, b.address, "djb2 hash should differ for different names");
    assert_eq!(a.callees, vec!["func_b"]);
    assert_eq!(b.callers, vec!["func_a"]);
}

#[test]
fn test_extract_functions_from_empty_graph() {
    let graph = CodePropertyGraph::new();
    assert!(extract_functions(&graph).is_empty());
}

#[test]
fn test_extract_functions_graph_with_no_calls() {
    let mut graph = CodePropertyGraph::new();
    for i in 0..5 {
        graph.functions.insert(format!("f{}", i), GraphFunctionInfo {
            name: format!("f{}", i), file: "test.c".into(),
            line_start: i * 10, line_end: i * 10 + 5,
            params: vec![], return_type: None,
            calls: vec![], called_by: vec![],
            is_async: false, is_exported: false,
        });
    }
    let funcs = extract_functions(&graph);
    assert_eq!(funcs.len(), 5);
    for f in &funcs {
        assert!(f.callees.is_empty());
        assert!(f.callers.is_empty());
    }
}

#[test]
fn test_danger_map_from_graph_realistic() {
    let mut graph = CodePropertyGraph::new();
    graph.functions.insert("memcpy".into(), GraphFunctionInfo {
        name: "memcpy".into(), file: "string.c".into(),
        line_start: 1, line_end: 10, params: vec![], return_type: None,
        calls: vec![], called_by: vec!["process".into(), "copy_data".into()],
        is_async: false, is_exported: true,
    });
    graph.functions.insert("process".into(), GraphFunctionInfo {
        name: "process".into(), file: "main.c".into(),
        line_start: 20, line_end: 30, params: vec![], return_type: None,
        calls: vec!["memcpy".into()], called_by: vec![],
        is_async: false, is_exported: false,
    });
    graph.functions.insert("copy_data".into(), GraphFunctionInfo {
        name: "copy_data".into(), file: "main.c".into(),
        line_start: 35, line_end: 45, params: vec![], return_type: None,
        calls: vec!["memcpy".into()], called_by: vec![],
        is_async: false, is_exported: false,
    });

    let map = danger_map_from_graph(&graph, 0.7);
    assert_eq!(map.len(), 3, "memcpy + 2 callers");
    for (_, score) in &map {
        assert!(*score > 0.0, "All entries should have positive score");
    }
}

// ============================================================
// 4. Cross-Crate Consistency
// ============================================================

#[test]
fn test_danger_map_from_graph_type_is_danger_entry() {
    let graph = CodePropertyGraph::new();
    let map: Vec<DangerEntry> = danger_map_from_graph(&graph, 0.7);
    // Type assertion — DangerEntry is (u64, f32)
    let _: &Vec<(u64, f32)> = &map;
    assert!(map.is_empty());
}

#[test]
fn test_default_decay_matches_sandbox_config() {
    assert!((DEFAULT_DECAY - 0.7).abs() < 0.001);
    assert!(DEFAULT_DECAY.is_finite());
    assert!(DEFAULT_DECAY >= 0.0);
}

#[test]
fn test_danger_entry_type_values() {
    let entry: DangerEntry = (0xdeadbeef, 0.75);
    assert_eq!(entry.0, 0xdeadbeef);
    assert!((entry.1 - 0.75).abs() < 0.001);
}

#[test]
fn test_graph_function_info_fields_match() {
    let gf = GraphFunctionInfo {
        name: "test_func".into(), file: "test.c".into(),
        line_start: 1, line_end: 10,
        params: vec!["a".into()], return_type: Some("int".into()),
        calls: vec!["callee_a".into(), "callee_b".into()],
        called_by: vec!["caller_x".into()],
        is_async: false, is_exported: true,
    };
    assert_eq!(gf.calls.len(), 2);
    assert_eq!(gf.called_by.len(), 1);
    assert_eq!(gf.name, "test_func");
}

// ============================================================
// 5. Edge Cases
// ============================================================

#[test]
fn test_special_characters_in_names() {
    let funcs = vec![
        FunctionInfo { name: "caller_!@#$".into(), address: 0x1000, callees: vec!["sink_%^&*".into()], callers: vec![] },
        FunctionInfo { name: "sink_%^&*".into(), address: 0x2000, callees: vec![], callers: vec!["caller_!@#$".into()] },
    ];
    let map = compute_danger_map(&funcs, &["sink_%^&*"], 0.7);
    assert_eq!(map.len(), 2);
}

#[test]
fn test_whitespace_in_names() {
    let funcs = vec![
        FunctionInfo { name: "my func".into(), address: 0x1000, callees: vec!["sink func".into()], callers: vec![] },
        FunctionInfo { name: "sink func".into(), address: 0x2000, callees: vec![], callers: vec!["my func".into()] },
    ];
    assert_eq!(compute_danger_map(&funcs, &["sink func"], 0.7).len(), 2);
}

#[test]
fn test_unicode_names() {
    let funcs = vec![
        FunctionInfo { name: "ñandú".into(), address: 0x1000, callees: vec!["café".into()], callers: vec![] },
        FunctionInfo { name: "café".into(), address: 0x2000, callees: vec![], callers: vec!["ñandú".into()] },
    ];
    assert_eq!(compute_danger_map(&funcs, &["café"], 0.7).len(), 2);
}

#[test]
fn test_very_long_function_name_255_plus() {
    let long_name = "a".repeat(300);
    let funcs = vec![
        FunctionInfo { name: long_name.clone(), address: 0x1000, callees: vec![], callers: vec![] },
    ];
    let map = compute_danger_map(&funcs, &[&long_name], 0.7);
    assert_eq!(map.len(), 1);

    let long_caller = "b".repeat(300);
    let funcs2 = vec![
        FunctionInfo { name: long_caller.clone(), address: 0x2000, callees: vec![long_name.clone()], callers: vec![] },
        FunctionInfo { name: long_name.clone(), address: 0x1000, callees: vec![], callers: vec![long_caller] },
    ];
    assert_eq!(compute_danger_map(&funcs2, &[&long_name], 0.7).len(), 2);
}

#[test]
fn test_same_name_different_addresses() {
    // Two functions share a name. name_to_idx only keeps one.
    // Both are sinks so both get score 1.0, but propagation may only reach one.
    let funcs = vec![
        FunctionInfo { name: "dup".into(), address: 0x1000, callees: vec![], callers: vec![] },
        FunctionInfo { name: "dup".into(), address: 0x2000, callees: vec![], callers: vec![] },
    ];
    let map = compute_danger_map(&funcs, &["dup"], 0.7);
    assert!(map.len() >= 1, "At least one duplicate-named sink should appear");
    for (_, score) in &map {
        assert!((*score - 1.0).abs() < 0.001, "Both are sinks, score 1.0");
    }
}

#[test]
fn test_all_functions_same_name_degenerate() {
    let funcs = vec![
        FunctionInfo { name: "same".into(), address: 0x1000, callees: vec!["same".into()], callers: vec!["same".into()] },
        FunctionInfo { name: "same".into(), address: 0x2000, callees: vec!["same".into()], callers: vec!["same".into()] },
        FunctionInfo { name: "same".into(), address: 0x3000, callees: vec!["same".into()], callers: vec!["same".into()] },
    ];
    // Should not crash or infinite loop
    let map = compute_danger_map(&funcs, &["same"], 0.7);
    assert!(map.len() >= 1);
}

#[test]
fn test_decay_nan() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec![], callers: vec!["A".into()] },
    ];
    // NaN * score = NaN; NaN > 0.0 = false in Rust → only sink appears
    let map = compute_danger_map(&funcs, &["B"], f32::NAN);
    assert_eq!(map.len(), 1);
    assert!((map[0].1 - 1.0).abs() < 0.001);
}

#[test]
fn test_decay_infinity() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec![], callers: vec!["A".into()] },
    ];
    let map = compute_danger_map(&funcs, &["B"], f32::INFINITY);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x2000) - 1.0).abs() < 0.001);
    // Score capped at 1.0 for cycle safety
    assert!((get(0x1000) - 1.0).abs() < 0.001);
}

// ============================================================
// 6. Daemon Integration
// ============================================================

#[test]
fn test_danger_map_from_graph_public_api() {
    let graph = CodePropertyGraph::new();
    let map = danger_map_from_graph(&graph, 0.7);
    assert!(map.is_empty());
}

#[test]
fn test_default_decay_matches_daemon_default_decay() {
    // daemon.rs:37: fn default_decay() -> f32 { 0.7 }
    assert!((DEFAULT_DECAY - 0.7).abs() < 0.001);
}

#[test]
fn test_danger_map_cli_sig() {
    let graph = CodePropertyGraph::new();
    let _map = danger_map_from_graph(&graph, 0.7);
}

#[test]
fn test_danger_map_handler_json_shape() {
    // Simulate what the daemon's "danger_map" handler produces
    let mut graph = CodePropertyGraph::new();
    graph.functions.insert("system".into(), GraphFunctionInfo {
        name: "system".into(), file: "test.c".into(),
        line_start: 1, line_end: 1, params: vec![], return_type: None,
        calls: vec![], called_by: vec!["main".into()],
        is_async: false, is_exported: false,
    });
    graph.functions.insert("main".into(), GraphFunctionInfo {
        name: "main".into(), file: "test.c".into(),
        line_start: 5, line_end: 10, params: vec![], return_type: None,
        calls: vec!["system".into()], called_by: vec![],
        is_async: false, is_exported: false,
    });

    let map = danger_map_from_graph(&graph, 0.7);
    let entries: Vec<serde_json::Value> = map.iter().map(|(addr, score)| {
        serde_json::json!({"address": format!("0x{:x}", addr), "danger_score": score})
    }).collect();
    let result = serde_json::json!({
        "num_entries": entries.len(),
        "sink_count": DEFAULT_SINKS.len(),
        "decay_factor": 0.7,
        "entries": entries,
        "sinks": DEFAULT_SINKS,
        "timestamp": "2024-01-01T00:00:00Z",
    });

    assert!(result["num_entries"].as_u64().unwrap() > 0);
    assert!(result["sink_count"].as_u64().unwrap() > 0);
    assert!((result["decay_factor"].as_f64().unwrap() - 0.7).abs() < 0.01);
    assert!(result["entries"].as_array().unwrap().len() > 0);
    assert!(result["sinks"].as_array().unwrap().len() > 0);
    assert!(result["timestamp"].as_str().is_some());
}

// ============================================================
// 7. Performance
// ============================================================

#[test]
fn test_performance_500_functions_1000_call_edges() {
    const N: usize = 500;
    let mut funcs = Vec::with_capacity(N + 1);
    // 10 chains of 50, all converging on one sink
    for chain in 0..10 {
        for i in 0..50 {
            let idx = chain * 50 + i;
            let name = format!("c{}_f{}", chain, i);
            let callees = if i < 49 {
                vec![format!("c{}_f{}", chain, i + 1)]
            } else {
                vec!["ultimate_sink".into()]
            };
            let callers = if i > 0 {
                vec![format!("c{}_f{}", chain, i - 1)]
            } else {
                vec![]
            };
            funcs.push(FunctionInfo { name, address: 0x1000 + idx as u64, callees, callers });
        }
    }
    let mut sink_callers: Vec<String> = Vec::new();
    for chain in 0..10 {
        sink_callers.push(format!("c{}_f49", chain));
    }
    funcs.push(FunctionInfo {
        name: "ultimate_sink".into(), address: 0x2000,
        callees: vec![], callers: sink_callers,
    });

    let start = Instant::now();
    let map = compute_danger_map(&funcs, &["ultimate_sink"], 0.7);
    let elapsed = start.elapsed();
    assert_eq!(map.len(), 501);
    assert!(elapsed.as_millis() < 100, "500 functions took {}ms", elapsed.as_millis());
}

#[test]
fn test_performance_dense_graph_100_to_1() {
    const N: usize = 100;
    let mut funcs = Vec::with_capacity(N + 1);
    let mut sink_callers = Vec::with_capacity(N);
    for i in 0..N {
        let name = format!("caller_{}", i);
        sink_callers.push(name.clone());
        funcs.push(FunctionInfo { name, address: 0x1000 + i as u64, callees: vec!["sink".into()], callers: vec![] });
    }
    funcs.push(FunctionInfo { name: "sink".into(), address: 0x2000, callees: vec![], callers: sink_callers });

    let start = Instant::now();
    let map = compute_danger_map(&funcs, &["sink"], 0.7);
    let elapsed = start.elapsed();
    assert_eq!(map.len(), N + 1);
    assert!(elapsed.as_millis() < 50, "Dense graph took {}ms", elapsed.as_millis());
    for (addr, score) in &map {
        if *addr != 0x2000 {
            assert!((*score - 0.7).abs() < 0.001, "Caller {:x} score={}", addr, score);
        }
    }
}

#[test]
fn test_performance_500_dense_fanout() {
    const N: usize = 500;
    let mut funcs = Vec::with_capacity(N + 1);
    let mut sink_callers = Vec::with_capacity(N);
    for i in 0..N {
        let name = format!("c{}", i);
        sink_callers.push(name.clone());
        funcs.push(FunctionInfo { name, address: 0x1000 + i as u64, callees: vec!["sink".into()], callers: vec![] });
    }
    funcs.push(FunctionInfo { name: "sink".into(), address: 0x2000, callees: vec![], callers: sink_callers });

    let start = Instant::now();
    let map = compute_danger_map(&funcs, &["sink"], 0.7);
    let elapsed = start.elapsed();
    assert_eq!(map.len(), N + 1);
    assert!(elapsed.as_millis() < 100, "500-fan dense took {}ms", elapsed.as_millis());
}

// ============================================================
// 8. Bug Discovery / Edge Cases
// ============================================================

#[test]
fn test_bug_empty_graph_no_crash() {
    let graph = CodePropertyGraph::new();
    assert!(danger_map_from_graph(&graph, 0.7).is_empty());
}

#[test]
fn test_bug_sink_names_not_in_graph() {
    let graph = CodePropertyGraph::new();
    // DEFAULT_SINKS has many names but none in the empty graph
    assert!(danger_map_from_graph(&graph, 0.7).is_empty());
}

#[test]
fn test_bug_nan_decay_chain() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec![], callers: vec!["B".into()] },
    ];
    let map = compute_danger_map(&funcs, &["C"], f32::NAN);
    assert_eq!(map.len(), 1);
    assert!((map[0].1 - 1.0).abs() < 0.001);
}

#[test]
fn test_bug_extract_functions_compiles() {
    let graph = CodePropertyGraph::new();
    let funcs: Vec<FunctionInfo> = extract_functions(&graph);
    assert!(funcs.is_empty());
}

#[test]
fn test_bug_graph_functions_field_accessible() {
    let graph = CodePropertyGraph::new();
    assert!(graph.functions.is_empty());
    let _: &std::collections::HashMap<String, GraphFunctionInfo> = &graph.functions;
}

#[test]
fn test_bug_empty_functions_list() {
    assert!(compute_danger_map(&[], &["memcpy"], 0.7).is_empty());
}

#[test]
fn test_bug_nonexistent_sink_name_in_list() {
    let funcs = vec![
        FunctionInfo { name: "real".into(), address: 0x1000, callees: vec![], callers: vec![] },
    ];
    assert!(compute_danger_map(&funcs, &["not_a_real_function_name"], 0.7).is_empty());
}

#[test]
fn test_bug_duplicate_sink_names_in_list() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec![], callers: vec!["A".into()] },
    ];
    let map = compute_danger_map(&funcs, &["B", "B", "B"], 0.7);
    assert_eq!(map.len(), 2);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x2000) - 1.0).abs() < 0.001);
    assert!((get(0x1000) - 0.7).abs() < 0.001);
}

#[test]
fn test_bug_mutual_recursion() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec!["B".into()] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["A".into()], callers: vec!["A".into()] },
    ];
    let map = compute_danger_map(&funcs, &["B"], 0.7);
    assert_eq!(map.len(), 2);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x2000) - 1.0).abs() < 0.001);
    assert!((get(0x1000) - 0.7).abs() < 0.001);
}

#[test]
fn test_bug_zero_decay_no_propagation() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec![], callers: vec!["B".into()] },
    ];
    let map = compute_danger_map(&funcs, &["C"], 0.0);
    assert_eq!(map.len(), 1);
    assert!((map[0].1 - 1.0).abs() < 0.001);
}

#[test]
fn test_bug_very_small_decay() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec![], callers: vec!["A".into()] },
    ];
    let map = compute_danger_map(&funcs, &["B"], f32::MIN_POSITIVE);
    assert_eq!(map.len(), 2);
    let a_score = map.iter().find(|(a,_)| *a==0x1000).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!(a_score > 0.0);
    assert!(a_score < 0.001);
}

#[test]
fn test_bug_full_propagation_at_decay_1() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec!["D".into()], callers: vec!["B".into()] },
        FunctionInfo { name: "D".into(), address: 0x4000, callees: vec![], callers: vec!["C".into()] },
    ];
    let map = compute_danger_map(&funcs, &["D"], 1.0);
    assert_eq!(map.len(), 4);
    for (_, score) in &map {
        assert!((*score - 1.0).abs() < 0.001);
    }
}

#[test]
fn test_bug_decay_2_long_chain() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec!["D".into()], callers: vec!["B".into()] },
        FunctionInfo { name: "D".into(), address: 0x4000, callees: vec![], callers: vec!["C".into()] },
    ];
    let map = compute_danger_map(&funcs, &["D"], 2.0);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x4000) - 1.0).abs() < 0.001);
    // Scores capped at 1.0 for cycle safety
    assert!((get(0x3000) - 1.0).abs() < 0.001);
    assert!((get(0x2000) - 1.0).abs() < 0.001);
    assert!((get(0x1000) - 1.0).abs() < 0.001);
}

#[test]
fn test_bug_result_sorted_by_address() {
    let funcs = vec![
        FunctionInfo { name: "Z".into(), address: 0x5000, callees: vec!["Y".into()], callers: vec![] },
        FunctionInfo { name: "Y".into(), address: 0x1000, callees: vec![], callers: vec!["Z".into()] },
    ];
    let map = compute_danger_map(&funcs, &["Y"], 0.7);
    assert_eq!(map.len(), 2);
    assert_eq!(map[0].0, 0x1000, "First entry should be lowest address");
    assert_eq!(map[1].0, 0x5000);
}

#[test]
fn test_bug_branching_chain() {
    // A1→B, A2→B, B→S (sink). S=1.0, B=0.7, A1=A2=0.49
    let funcs = vec![
        FunctionInfo { name: "A1".into(), address: 0x1001, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "A2".into(), address: 0x1002, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["S".into()], callers: vec!["A1".into(), "A2".into()] },
        FunctionInfo { name: "S".into(), address: 0x3000, callees: vec![], callers: vec!["B".into()] },
    ];
    let map = compute_danger_map(&funcs, &["S"], 0.7);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x3000) - 1.0).abs() < 0.001);
    assert!((get(0x2000) - 0.7).abs() < 0.001);
    assert!((get(0x1001) - 0.49).abs() < 0.001);
    assert!((get(0x1002) - 0.49).abs() < 0.001);
}

#[test]
fn test_bug_diamond_dependency() {
    // A→B, A→C, B→D(sink), C→D(sink). D=1.0, B=C=0.7, A=max(0.49,0.49)=0.49
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into(), "C".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["D".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec!["D".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "D".into(), address: 0x4000, callees: vec![], callers: vec!["B".into(), "C".into()] },
    ];
    let map = compute_danger_map(&funcs, &["D"], 0.7);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x4000) - 1.0).abs() < 0.001);
    assert!((get(0x2000) - 0.7).abs() < 0.001);
    assert!((get(0x3000) - 0.7).abs() < 0.001);
    assert!((get(0x1000) - 0.49).abs() < 0.001);
}

#[test]
fn test_bug_many_independent_chains() {
    // 5 independent chains: Ai→Bi→C (sink), all hitting the same sink
    let mut funcs = Vec::new();
    let mut sink_callers = Vec::new();
    for i in 0..5 {
        let a_name = format!("A{}", i);
        let b_name = format!("B{}", i);
        sink_callers.push(b_name.clone());
        funcs.push(FunctionInfo { name: a_name.clone(), address: 0x1000 + i * 0x100, callees: vec![b_name.clone()], callers: vec![] });
        funcs.push(FunctionInfo { name: b_name, address: 0x2000 + i * 0x100, callees: vec!["S".into()], callers: vec![a_name] });
    }
    funcs.push(FunctionInfo { name: "S".into(), address: 0x3000, callees: vec![], callers: sink_callers });

    let map = compute_danger_map(&funcs, &["S"], 0.7);
    assert_eq!(map.len(), 11); // 5*2 + 1
    for (addr, score) in &map {
        if *addr == 0x3000 {
            assert!((*score - 1.0).abs() < 0.001);
        } else if *addr >= 0x2000 && *addr < 0x3000 {
            assert!((*score - 0.7).abs() < 0.001);
        } else {
            assert!((*score - 0.49).abs() < 0.001);
        }
    }
}

#[test]
fn test_bug_sink_is_also_caller() {
    // A→B, B→A, A is sink. A=1.0, B=0.7. Then B→A: A=max(1.0, 0.49)=1.0
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec!["B".into()] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["A".into()], callers: vec!["A".into()] },
    ];
    let map = compute_danger_map(&funcs, &["A"], 0.7);
    assert_eq!(map.len(), 2);
    let get = |addr| map.iter().find(|(a,_)| *a==addr).map(|(_,s)| *s).unwrap_or(-1.0);
    assert!((get(0x1000) - 1.0).abs() < 0.001);
    assert!((get(0x2000) - 0.7).abs() < 0.001);
}

#[test]
fn test_bug_sink_with_no_incoming_edges() {
    // Sink exists, but no function declares it as a callee in their calls list,
    // and the sink has no callers listed. Only the sink itself appears.
    let funcs = vec![
        FunctionInfo { name: "orphan".into(), address: 0x1000, callees: vec![], callers: vec![] },
        FunctionInfo { name: "sink".into(), address: 0x2000, callees: vec![], callers: vec![] },
    ];
    let map = compute_danger_map(&funcs, &["sink"], 0.7);
    assert_eq!(map.len(), 1);
    assert!((map[0].1 - 1.0).abs() < 0.001);
    assert_eq!(map[0].0, 0x2000);
}
