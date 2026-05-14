use bugswarm_cpg::danger_map::*;

#[test]
fn test_compute_danger_map_simple() {
    // A -> B -> C (sink)
    // A gets score = 1.0 * 0.7^2 = 0.49
    // B gets score = 1.0 * 0.7 = 0.7
    // C gets score = 1.0
    let functions = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["B".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec!["A".into()] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec![], callers: vec!["B".into()] },
    ];
    let sinks = vec!["C"];
    let map = compute_danger_map(&functions, &sinks, 0.7);
    let get = |addr: u64| -> f32 { map.iter().find(|(a,_)| *a == addr).map(|(_,s)| *s).unwrap_or(0.0) };
    assert!(get(0x3000) >= 0.99); // sink itself
    assert!((get(0x2000) - 0.7).abs() < 0.01);
    assert!((get(0x1000) - 0.49).abs() < 0.01);
}

#[test]
fn test_compute_danger_map_no_sinks() {
    let functions = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec![], callers: vec![] },
    ];
    let map = compute_danger_map(&functions, &[], 0.7);
    assert!(map.is_empty());
}

#[test]
fn test_danger_map_multiple_paths() {
    // A -> C (sink), B -> C (sink)
    // Both A and B get score 0.7
    let functions = vec![
        FunctionInfo { name: "A".into(), address: 0x1000, callees: vec!["C".into()], callers: vec![] },
        FunctionInfo { name: "B".into(), address: 0x2000, callees: vec!["C".into()], callers: vec![] },
        FunctionInfo { name: "C".into(), address: 0x3000, callees: vec![], callers: vec!["A".into(), "B".into()] },
    ];
    let map = compute_danger_map(&functions, &["C"], 0.7);
    let get = |addr: u64| -> f32 { map.iter().find(|(a,_)| *a == addr).map(|(_,s)| *s).unwrap_or(0.0) };
    assert!((get(0x1000) - 0.7).abs() < 0.01);
    assert!((get(0x2000) - 0.7).abs() < 0.01);
}

#[test]
fn test_known_sinks_default() {
    assert!(DEFAULT_SINKS.contains(&"memcpy"));
    assert!(DEFAULT_SINKS.contains(&"system"));
    assert!(DEFAULT_SOURCES.contains(&"read"));
    assert!(DEFAULT_SOURCES.contains(&"recv"));
}
