use bugswarm_cpg::danger_map::*;
use bugswarm_cpg::graph::CodePropertyGraph;

#[test]
fn test_danger_map_from_empty_graph() {
    let graph = CodePropertyGraph::new();
    let map = danger_map_from_graph(&graph, 0.7);
    assert!(map.is_empty());
}

#[test]
fn test_danger_map_default_decay() {
    assert!((DEFAULT_DECAY - 0.7).abs() < 0.001);
}

#[test]
fn test_danger_map_sinks_non_empty() {
    assert!(!DEFAULT_SINKS.is_empty());
    assert!(DEFAULT_SINKS.contains(&"memcpy"));
    assert!(DEFAULT_SINKS.contains(&"system"));
}

#[test]
fn test_danger_map_sources_non_empty() {
    assert!(!DEFAULT_SOURCES.is_empty());
    assert!(DEFAULT_SOURCES.contains(&"read"));
}

#[test]
fn test_compute_danger_map_chain_three() {
    let funcs = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec![], callers: vec!["B".into()] },
    ];
    let map = compute_danger_map(&funcs, &["C"], 0.7);
    assert_eq!(map.len(), 3);
    let get = |addr| map.iter().find(|(a,_)| *a == addr).map(|(_,s)| *s).unwrap_or(0.0);
    assert!((get(0x3000) - 1.0).abs() < 0.01);
    assert!((get(0x2000) - 0.7).abs() < 0.01);
    assert!((get(0x1000) - 0.49).abs() < 0.01);
}
