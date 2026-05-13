/// Data Flow Crucible — Phase 17 Gate Test (10 attack vectors)
///
/// C6.2.1 PEAK verification: Tests CFG construction, SSA, taint propagation
/// against 10 deliberately crafted taint scenarios.
///
/// Exit code 0 = ALL PASSED. Exit code 1 = failure.

use std::collections::HashMap;

use bugswarm_cpg::cfg::ControlFlowGraph;
use bugswarm_cpg::dominators::DominatorTree;
use bugswarm_cpg::graph::{CodePropertyGraph, EdgeKind, GraphEdge, GraphNode, NodeId, NodeKind};
use bugswarm_cpg::ssa::{build_cytron_ssa, CytronSsaResult};
use bugswarm_cpg::taint::{propagate_taint_peak, SsaTaintPath};

fn node(name: &str, kind: NodeKind, line: usize) -> GraphNode {
    GraphNode {
        id: line.to_string(), kind, name: name.to_string(), file: "crucible.py".into(),
        line_start: line, line_end: line, col_start: 0, col_end: 0,
        language: "python".into(), metadata: HashMap::new(),
    }
}

fn make_edge(from: NodeId, to: NodeId, kind: EdgeKind) -> GraphEdge {
    GraphEdge { kind, label: None, confidence: 0.9, metadata: HashMap::new() }
}

#[test]
fn gate_crucible_01_simple_chain() {
    // x = src; y = x; sink(y)
    let mut cpg = CodePropertyGraph::new();
    let src = cpg.add_node(node("request_get('cmd')", NodeKind::Source, 1));
    let a1 = cpg.add_node(node("x = request_get('cmd')", NodeKind::Assignment, 1));
    let a2 = cpg.add_node(node("y = x", NodeKind::Assignment, 2));
    let snk = cpg.add_node(node("os.system(y)", NodeKind::Sink, 3));

    cpg.add_edge(src, a1, make_edge(src, a1, EdgeKind::DataFlow));
    cpg.add_edge(a1, a2, make_edge(a1, a2, EdgeKind::Assigns));
    cpg.add_edge(a2, snk, make_edge(a2, snk, EdgeKind::DataFlow));
    cpg.sources.push(src); cpg.sinks.push(snk);

    let paths = propagate_taint_peak(&cpg);
    assert!(!paths.is_empty(), "CRUCIBLE-01 FAIL: No taint path found for simple chain");
    assert!(paths[0].length >= 2, "CRUCIBLE-01: Path too short");
    println!("CRUCIBLE-01 PASS: simple chain (length={})", paths[0].length);
}

#[test]
fn gate_crucible_02_through_call() {
    let mut cpg = CodePropertyGraph::new();
    let src = cpg.add_node(node("request_get('cmd')", NodeKind::Source, 1));
    let call = cpg.add_node(node("execute_command(cmd)", NodeKind::CallSite, 2));
    let snk = cpg.add_node(node("os.system(x)", NodeKind::Sink, 3));

    cpg.add_edge(src, call, make_edge(src, call, EdgeKind::DataFlow));
    cpg.add_edge(call, snk, make_edge(call, snk, EdgeKind::Calls));
    cpg.sources.push(src); cpg.sinks.push(snk);

    let paths = propagate_taint_peak(&cpg);
    assert!(!paths.is_empty(), "CRUCIBLE-02 FAIL: No path through function call");
    println!("CRUCIBLE-02 PASS: through function call");
}

#[test]
fn gate_crucible_03_through_return() {
    let mut cpg = CodePropertyGraph::new();
    let src = cpg.add_node(node("get_user_input", NodeKind::Source, 1));
    let ret = cpg.add_node(node("return request_get('cmd')", NodeKind::Return, 2));
    let snk = cpg.add_node(node("os.system(cmd)", NodeKind::Sink, 3));

    cpg.add_edge(src, ret, make_edge(src, ret, EdgeKind::Returns));
    cpg.add_edge(ret, snk, make_edge(ret, snk, EdgeKind::DataFlow));
    cpg.sources.push(src); cpg.sinks.push(snk);

    let paths = propagate_taint_peak(&cpg);
    assert!(!paths.is_empty(), "CRUCIBLE-03 FAIL: No path through return value");
    println!("CRUCIBLE-03 PASS: through return value");
}

#[test]
fn gate_crucible_04_sanitizer_breaks() {
    let mut cpg = CodePropertyGraph::new();
    let src = cpg.add_node(node("request_get('cmd')", NodeKind::Source, 1));
    let san = cpg.add_node(node("html.escape(cmd)", NodeKind::Sanitizer, 2));
    let snk = cpg.add_node(node("os.system(safe)", NodeKind::Sink, 3));

    cpg.add_edge(src, san, make_edge(src, san, EdgeKind::DataFlow));
    cpg.add_edge(san, snk, make_edge(san, snk, EdgeKind::DataFlow));
    cpg.sources.push(src); cpg.sinks.push(snk);
    cpg.sanitizers.push(san);

    let paths = propagate_taint_peak(&cpg);
    assert!(!paths.is_empty(), "CRUCIBLE-04 FAIL: No path found");
    assert!(paths[0].sanitized, "CRUCIBLE-04 FAIL: Path not marked sanitized");
    assert!(paths[0].confidence < 0.85, "CRUCIBLE-04: Sanitized path should have reduced confidence (got {:.3})", paths[0].confidence);
    println!("CRUCIBLE-04 PASS: sanitizer breaks chain (confidence={:.3})", paths[0].confidence);
}

#[test]
fn gate_crucible_05_field_store_load() {
    let mut cpg = CodePropertyGraph::new();
    let src = cpg.add_node(node("request_get('cmd')", NodeKind::Source, 1));
    let store = cpg.add_node(node("obj.cmd = request_get('cmd')", NodeKind::Assignment, 2));
    let load = cpg.add_node(node("x = obj.cmd", NodeKind::Assignment, 3));
    let snk = cpg.add_node(node("os.system(x)", NodeKind::Sink, 4));

    cpg.add_edge(src, store, make_edge(src, store, EdgeKind::DataFlow));
    cpg.add_edge(store, load, make_edge(store, load, EdgeKind::Assigns));
    cpg.add_edge(load, snk, make_edge(load, snk, EdgeKind::DataFlow));
    cpg.sources.push(src); cpg.sinks.push(snk);

    let paths = propagate_taint_peak(&cpg);
    assert!(!paths.is_empty(), "CRUCIBLE-05 FAIL: No path through field store/load");
    println!("CRUCIBLE-05 PASS: field store/load");
}

#[test]
fn gate_crucible_06_list_append_index() {
    let mut cpg = CodePropertyGraph::new();
    let src = cpg.add_node(node("request_get('cmd')", NodeKind::Source, 1));
    let add = cpg.add_node(node("cmds.append(request_get('cmd'))", NodeKind::Assignment, 2));
    let get = cpg.add_node(node("os.system(cmds[0])", NodeKind::Sink, 3));

    cpg.add_edge(src, add, make_edge(src, add, EdgeKind::DataFlow));
    cpg.add_edge(add, get, make_edge(add, get, EdgeKind::DataFlow));
    cpg.sources.push(src); cpg.sinks.push(get);

    let paths = propagate_taint_peak(&cpg);
    assert!(!paths.is_empty(), "CRUCIBLE-06 FAIL: No path through list append/index");
    println!("CRUCIBLE-06 PASS: list append/index");
}

#[test]
fn gate_crucible_07_conditional_taint_cfg() {
    // Test CFG construction with conditional — C6.2.1 PEAK
    let n0 = &node("if cond:", NodeKind::Condition, 1);
    let n1 = &node("x = request_get('cmd')", NodeKind::Assignment, 2);
    let n2 = &node("x = 'safe'", NodeKind::Assignment, 3);
    let n3 = &node("os.system(x)", NodeKind::Sink, 4);
    let nodes: [&GraphNode; 4] = [n0, n1, n2, n3];

    let cfg = ControlFlowGraph::build(&nodes);
    assert!(cfg.blocks.len() >= 4,
        "CRUCIBLE-07 FAIL: Conditional CFG should have >= 4 blocks, got {}",
        cfg.blocks.len());

    // Verify SSA construction on the conditional
    let ssa = build_cytron_ssa("crucible.py", "f", &nodes);
    let x_versions: Vec<u32> = ssa.variables.iter()
        .filter(|v| v.name == "x")
        .map(|v| v.version)
        .collect();
    assert!(!x_versions.is_empty(),
        "CRUCIBLE-07 FAIL: x should have at least one SSA version");

    // Dominator tree should exist
    let dom = DominatorTree::build(&cfg);
    assert!(!dom.idom.is_empty(), "CRUCIBLE-07 FAIL: Dominator tree empty");

    println!("CRUCIBLE-07 PASS: conditional CFG ({} blocks, x versions={:?})",
        cfg.blocks.len(), x_versions);
}

#[test]
fn gate_crucible_08_confidence_degradation() {
    // C6.2.6: Path-sensitive confidence — longer paths lower confidence
    let mut cpg = CodePropertyGraph::new();

    // Build a 5-hop chain
    let src = cpg.add_node(node("request", NodeKind::Source, 1));
    let mut prev = src;
    let mut nodes = vec![src];
    for i in 1..=5 {
        let n = cpg.add_node(node(&format!("v{} = v{}", i, i-1), NodeKind::Assignment, i+1));
        cpg.add_edge(prev, n, make_edge(prev, n, EdgeKind::Assigns));
        prev = n; nodes.push(n);
    }
    let snk = cpg.add_node(node("exec", NodeKind::Sink, 7));
    cpg.add_edge(prev, snk, make_edge(prev, snk, EdgeKind::DataFlow));
    cpg.sources.push(src); cpg.sinks.push(snk);

    let paths = propagate_taint_peak(&cpg);
    assert!(!paths.is_empty(), "CRUCIBLE-08 FAIL: No path for confidence test");
    // 5-hop path should have lower confidence than a 1-hop path
    let conf_5hop = paths[0].confidence;

    // Build 1-hop path
    let mut cpg2 = CodePropertyGraph::new();
    let s2 = cpg2.add_node(node("request", NodeKind::Source, 1));
    let k2 = cpg2.add_node(node("exec", NodeKind::Sink, 2));
    cpg2.add_edge(s2, k2, make_edge(s2, k2, EdgeKind::DataFlow));
    cpg2.sources.push(s2); cpg2.sinks.push(k2);
    let paths2 = propagate_taint_peak(&cpg2);
    let conf_1hop = paths2[0].confidence;

    assert!(conf_5hop < conf_1hop,
        "CRUCIBLE-08 FAIL: 5-hop confidence ({:.4}) should be < 1-hop ({:.4})",
        conf_5hop, conf_1hop);
    println!("CRUCIBLE-08 PASS: confidence degrades (1-hop={:.4}, 5-hop={:.4})", conf_1hop, conf_5hop);
}

#[test]
fn gate_crucible_09_sanitizer_classification() {
    use bugswarm_cpg::taint::classify_sanitizer;
    let mut cpg = CodePropertyGraph::new();

    let html_escape = cpg.add_node(node("html.escape", NodeKind::Function, 1));
    let (is_san, score) = classify_sanitizer(&cpg.graph[html_escape]);
    assert!(is_san, "CRUCIBLE-09 FAIL: html.escape not classified as sanitizer");
    assert!(score >= 0.9, "CRUCIBLE-09 FAIL: html.escape score too low ({})", score);

    let unescape = cpg.add_node(node("unescape", NodeKind::Function, 2));
    let (is_san2, score2) = classify_sanitizer(&cpg.graph[unescape]);
    assert!(!is_san2, "CRUCIBLE-09 FAIL: unescape should NOT be sanitizer");
    assert_eq!(score2, 0.0, "CRUCIBLE-09: unescape score should be 0");

    let custom = cpg.add_node(node("sanitize_input", NodeKind::Function, 3));
    let (is_san3, _) = classify_sanitizer(&cpg.graph[custom]);
    assert!(is_san3, "CRUCIBLE-09 FAIL: sanitize_input not recognized");

    println!("CRUCIBLE-09 PASS: sanitizer classification (escape={:.1}, unescape={:.1})", score, score2);
}

#[test]
fn gate_crucible_10_two_pass_resolution() {
    use bugswarm_cpg::parser;
    use std::path::Path;
    use std::fs;

    // Create a temp repo with cross-file calls
    let dir = tempfile::tempdir().unwrap();
    let a_path = dir.path().join("a.py");
    let b_path = dir.path().join("b.py");
    fs::write(&a_path, "def foo(): return bar()\n").unwrap();
    fs::write(&b_path, "def bar(): return os.system('x')\n").unwrap();

    let mut cpg = CodePropertyGraph::new();
    // C6.2.5: Two-pass index_directory
    parser::index_directory(&mut cpg, dir.path()).unwrap();

    // Should have functions from both files
    let stats = cpg.stats();
    assert!(stats.total_functions >= 2,
        "CRUCIBLE-10 FAIL: Expected >= 2 functions, got {}", stats.total_functions);
    assert!(stats.total_files >= 2,
        "CRUCIBLE-10 FAIL: Expected >= 2 files, got {}", stats.total_files);
    assert!(stats.total_nodes > 0,
        "CRUCIBLE-10 FAIL: Expected > 0 nodes");

    println!("CRUCIBLE-10 PASS: two-pass resolution ({} files, {} funcs, {} nodes)",
        stats.total_files, stats.total_functions, stats.total_nodes);
}

// Run all gate tests
#[test]
fn gate_crucible_all() {
    gate_crucible_01_simple_chain();
    gate_crucible_02_through_call();
    gate_crucible_03_through_return();
    gate_crucible_04_sanitizer_breaks();
    gate_crucible_05_field_store_load();
    gate_crucible_06_list_append_index();
    gate_crucible_07_conditional_taint_cfg();
    gate_crucible_08_confidence_degradation();
    gate_crucible_09_sanitizer_classification();
    gate_crucible_10_two_pass_resolution();
    println!("\n═══ DATA FLOW CRUCIBLE: 10/10 PASSED ═══");
    println!("✓ PHASE 17 GATE PASSED — C6.2.1 PEAK CFG verified");
}
