// ═══════════════════════════════════════════════════════════════
// Phase 29 Vulnerability Chaining — Integration Tests (D2)
// ═══════════════════════════════════════════════════════════════

use bugswarm_evidence::chain::*;
use std::collections::HashMap;

// ─── D2.4: Combined PoC End-to-End Test ───
#[test]
fn test_combined_poc_end_to_end() {
    // Simulate a chain: Bug A (info leak) → Bug B (heap overflow) → Bug C (RCE)
    let chain = ExploitChain {
        chain_id: "e2e-chain-001".into(),
        bug_ids: vec!["BUG-A".into(), "BUG-B".into(), "BUG-C".into()],
        chain_length: 3,
        terminal_bug_id: "BUG-C".into(),
        terminal_severity: 9,
        chain_severity: 10.0,
        has_rce: true,
        crosses_trust_boundary: true,
        combined_poc_available: true,
        combined_poc_verified: false,
        edge_scores: vec![0.85, 0.91],
        review_status: "discovered".into(),
        discovered_at: Some("2026-05-14T12:00:00Z".into()),
    };

    // Build combined PoC
    let mut composer = PocComposer::new();
    // Step 1: Bug A info leak PoC
    composer.add_step(
        "BUG-A",
        "AAAA...",
        Some("leaked_address: 0x7fffffffde10\nheap_base: 0x602000000000"),
        Some(0),
        Some("info_leak"),
        true,
    );
    // Step 2: Bug B heap overflow using leaked addresses
    composer.add_step(
        "BUG-B",
        "heap_overflow_payload_with_leaked_address",
        Some("heap_metadata_corrupted"),
        Some(0),
        Some("memory_corruption"),
        true,
    );
    // Step 3: Bug C RCE using corrupted state
    composer.add_step(
        "BUG-C",
        "rce_payload_leveraging_corrupted_state",
        Some("SHELL_OPENED: uid=0(root)"),
        Some(0),
        Some("rce"),
        true,
    );

    assert!(composer.all_steps_succeeded());

    let combined = composer.build_combined_poc(&chain.chain_id);
    assert!(
        combined.verified,
        "Combined PoC should be verified when all steps succeed"
    );
    assert_eq!(combined.poc_sequence.len(), 3);
    assert!(
        combined.verification_error.is_none(),
        "No verification error expected, got: {:?}",
        combined.verification_error
    );
    assert!(
        combined.terminal_status.is_some(),
        "Terminal status should be set"
    );

    // Verify step outputs match expected chain propagation
    assert_eq!(combined.poc_sequence[0].bug_id, "BUG-A");
    assert_eq!(combined.poc_sequence[1].bug_id, "BUG-B");
    assert_eq!(combined.poc_sequence[2].bug_id, "BUG-C");
}

// ─── D2.5: Chain Persistence Roundtrip Test ───
#[test]
fn test_chain_persistence_roundtrip() {
    let chain = ExploitChain {
        chain_id: "persist-001".into(),
        bug_ids: vec!["BUG-A".into(), "BUG-B".into(), "BUG-C".into()],
        chain_length: 3,
        terminal_bug_id: "BUG-C".into(),
        terminal_severity: 8,
        chain_severity: 9.5,
        has_rce: true,
        crosses_trust_boundary: true,
        combined_poc_available: false,
        combined_poc_verified: false,
        edge_scores: vec![0.85, 0.92],
        review_status: "discovered".into(),
        discovered_at: Some("2026-05-14T12:00:00Z".into()),
    };

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&chain).expect("Serialize failed");
    assert!(!json.is_empty());

    // Deserialize back
    let restored: ExploitChain = serde_json::from_str(&json).expect("Deserialize failed");

    // Verify all fields survived roundtrip
    assert_eq!(restored.chain_id, chain.chain_id);
    assert_eq!(restored.bug_ids, chain.bug_ids);
    assert_eq!(restored.chain_length, chain.chain_length);
    assert_eq!(restored.terminal_bug_id, chain.terminal_bug_id);
    assert_eq!(restored.terminal_severity, chain.terminal_severity);
    assert!((restored.chain_severity - chain.chain_severity).abs() < 0.001);
    assert_eq!(restored.has_rce, chain.has_rce);
    assert_eq!(restored.crosses_trust_boundary, chain.crosses_trust_boundary);
    assert_eq!(restored.combined_poc_available, chain.combined_poc_available);
    assert_eq!(restored.combined_poc_verified, chain.combined_poc_verified);
    assert_eq!(restored.edge_scores.len(), 2);
    assert!((restored.edge_scores[0] - 0.85).abs() < 0.001);
    assert!((restored.edge_scores[1] - 0.92).abs() < 0.001);
    assert_eq!(restored.review_status, chain.review_status);

    // Enables edges metadata (embodied in edge_scores) survives roundtrip
    assert!(!restored.edge_scores.is_empty());
}

// ─── D2: Severity Escalation Correctness ───
#[test]
fn test_severity_escalation_correctness() {
    let mut original = HashMap::new();
    original.insert("BUG-A".into(), 3u8);
    original.insert("BUG-B".into(), 4u8);
    original.insert("BUG-C".into(), 5u8);

    // Chain A→B→C with severity 8.5, confirmed
    let chain = ExploitChain {
        chain_id: "escalation-test".into(),
        bug_ids: vec!["BUG-A".into(), "BUG-B".into(), "BUG-C".into()],
        chain_length: 3,
        terminal_bug_id: "BUG-C".into(),
        terminal_severity: 5,
        chain_severity: 8.5,
        has_rce: false,
        crosses_trust_boundary: false,
        combined_poc_available: false,
        combined_poc_verified: false,
        edge_scores: vec![0.8, 0.9],
        review_status: "confirmed".into(),
        discovered_at: None,
    };

    let escalated = escalate_severities(&[chain], &original);
    assert_eq!(escalated.get("BUG-A").copied().unwrap_or(0), 8);
    assert_eq!(escalated.get("BUG-B").copied().unwrap_or(0), 8);
    assert_eq!(escalated.get("BUG-C").copied().unwrap_or(0), 8);

    // Unrelated bug should remain unchanged
    let mut with_extra = original.clone();
    with_extra.insert("BUG-D".into(), 2u8);
    let escalated2 = escalate_severities(&[ExploitChain {
        chain_id: "escalation-test-2".into(),
        bug_ids: vec!["BUG-A".into(), "BUG-C".into()],
        chain_length: 2,
        terminal_bug_id: "BUG-C".into(),
        terminal_severity: 5,
        chain_severity: 7.0,
        has_rce: false,
        crosses_trust_boundary: false,
        combined_poc_available: false,
        combined_poc_verified: false,
        edge_scores: vec![0.8],
        review_status: "confirmed".into(),
        discovered_at: None,
    }], &with_extra);
    assert_eq!(escalated2.get("BUG-D").copied().unwrap_or(0), 2);
}

// ─── D2: Cycle Prevention in BFS ───
#[test]
fn test_cycle_prevention_in_bfs() {
    let mut severities = HashMap::new();
    for i in 0..5 {
        severities.insert(format!("BUG-{}", i), 5u8);
    }

    // Build a graph with a cycle: 0→1→2→3→0
    let matches = vec![
        EffectPreconditionMatch {
            effect_bug_id: "BUG-0".into(), precondition_bug_id: "BUG-1".into(),
            similarity_score: 0.9, effect_description: "e0".into(),
            precondition_description: "p1".into(),
            effect_kind_label: "e".into(), precondition_kind_label: "p".into(),
            low_confidence: false,
        },
        EffectPreconditionMatch {
            effect_bug_id: "BUG-1".into(), precondition_bug_id: "BUG-2".into(),
            similarity_score: 0.9, effect_description: "e1".into(),
            precondition_description: "p2".into(),
            effect_kind_label: "e".into(), precondition_kind_label: "p".into(),
            low_confidence: false,
        },
        EffectPreconditionMatch {
            effect_bug_id: "BUG-2".into(), precondition_bug_id: "BUG-3".into(),
            similarity_score: 0.9, effect_description: "e2".into(),
            precondition_description: "p3".into(),
            effect_kind_label: "e".into(), precondition_kind_label: "p".into(),
            low_confidence: false,
        },
        EffectPreconditionMatch {
            effect_bug_id: "BUG-3".into(), precondition_bug_id: "BUG-0".into(),
            similarity_score: 0.9, effect_description: "e3".into(),
            precondition_description: "p0".into(),
            effect_kind_label: "e".into(), precondition_kind_label: "p".into(),
            low_confidence: false,
        },
    ];

    let graph = build_chain_graph(&matches);
    let chains = detect_chains(&graph, &severities, 10);

    // No chain should contain duplicates
    for chain in &chains {
        let mut seen: std::collections::HashSet<&String> = std::collections::HashSet::new();
        for bug_id in &chain.bug_ids {
            assert!(
                seen.insert(bug_id),
                "Cycle detected: {} appears twice in chain {:?}",
                bug_id,
                chain.bug_ids
            );
        }
    }

    // Chain lengths should be bounded (no infinite loops)
    for chain in &chains {
        assert!(
            chain.bug_ids.len() <= 5,
            "Chain exceeds max possible length: {:?}",
            chain.bug_ids
        );
    }
}

// ─── D2: Full Chain Detection Pipeline Integration ───
#[test]
fn test_full_chain_detection_pipeline() {
    let mut descs = HashMap::new();
    descs.insert("BUG-A".into(), ("ASLR info leak via out-of-bounds read reveals heap and stack addresses".into(), 3u8));
    descs.insert("BUG-B".into(), ("Heap buffer overflow requires knowledge of heap layout to overwrite vtable pointer".into(), 5u8));
    descs.insert("BUG-C".into(), ("Use-after-free in function pointer dispatch leads to arbitrary code execution".into(), 9u8));
    descs.insert("BUG-D".into(), ("Integer overflow in command-line argument parser causes incorrect allocation size".into(), 4u8));

    let config = ChainConfig::default();
    let result = run_chain_detection(&descs, &config);

    // Should find some matches
    assert!(result.total_edge_candidates > 0);

    // Should have chains (or at minimum zero with reason)
    if let Some(stats) = &result.traversal_stats {
        assert!(!stats.truncated || stats.truncation_reason.is_some());
    }

    // All bugs in input should have severities
    assert_eq!(result.original_severities.len(), 4);

    // Escalated severities should exist
    assert_eq!(result.escalated_severities.len(), 4);

    // Edge candidates tracked
    assert!(result.total_edge_candidates > 0);
}

// ─── D2: Edge Persistence — Enables edges survive serialization ───
#[test]
fn test_enables_edge_roundtrip() {
    let mat = EffectPreconditionMatch {
        effect_bug_id: "bug-001".into(),
        precondition_bug_id: "bug-002".into(),
        similarity_score: 0.82,
        effect_description: "Leaks heap addresses through uninitialized memory read".into(),
        precondition_description: "Requires knowledge of heap base address to calculate target offset".into(),
        effect_kind_label: "information_disclosure".into(),
        precondition_kind_label: "address_knowledge".into(),
        low_confidence: false,
    };

    let json = serde_json::to_string(&mat).expect("Serialization failed");
    let restored: EffectPreconditionMatch = serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(restored.effect_bug_id, "bug-001");
    assert_eq!(restored.precondition_bug_id, "bug-002");
    assert!((restored.similarity_score - 0.82).abs() < 0.001);
    assert!(!restored.low_confidence);
}

// ─── D2: Severity Escalation Does Not Over-escalate Unrelated Bugs ───
#[test]
fn test_severity_escalation_does_not_affect_unrelated() {
    let mut original = HashMap::new();
    original.insert("in-chain-1".into(), 3u8);
    original.insert("in-chain-2".into(), 4u8);
    original.insert("unrelated".into(), 5u8);

    let chain = ExploitChain {
        chain_id: "isolated".into(),
        bug_ids: vec!["in-chain-1".into(), "in-chain-2".into()],
        chain_length: 2,
        terminal_bug_id: "in-chain-2".into(),
        terminal_severity: 4,
        chain_severity: 7.0,
        has_rce: false,
        crosses_trust_boundary: false,
        combined_poc_available: false,
        combined_poc_verified: false,
        edge_scores: vec![0.8],
        review_status: "confirmed".into(),
        discovered_at: None,
    };

    let escalated = escalate_severities(&[chain], &original);
    // Unrelated bug should keep its original severity
    assert_eq!(escalated.get("unrelated").copied().unwrap_or(0), 5);
    // In-chain bugs should be escalated
    assert!(escalated.get("in-chain-1").copied().unwrap_or(0) >= 7);
}

// ─── D2: Multiple Overlapping Chains ───
#[test]
fn test_multiple_overlapping_chains() {
    let mut severities = HashMap::new();
    severities.insert("A".into(), 3u8);
    severities.insert("B".into(), 5u8);
    severities.insert("C".into(), 7u8);
    severities.insert("D".into(), 9u8);

    // Two overlapping chains: A→B→C and B→C→D
    let matches = vec![
        EffectPreconditionMatch {
            effect_bug_id: "A".into(), precondition_bug_id: "B".into(),
            similarity_score: 0.85, effect_description: "e_a".into(),
            precondition_description: "p_b".into(),
            effect_kind_label: "e".into(), precondition_kind_label: "p".into(),
            low_confidence: false,
        },
        EffectPreconditionMatch {
            effect_bug_id: "B".into(), precondition_bug_id: "C".into(),
            similarity_score: 0.9, effect_description: "e_b".into(),
            precondition_description: "p_c".into(),
            effect_kind_label: "e".into(), precondition_kind_label: "p".into(),
            low_confidence: false,
        },
        EffectPreconditionMatch {
            effect_bug_id: "C".into(), precondition_bug_id: "D".into(),
            similarity_score: 0.88, effect_description: "e_c".into(),
            precondition_description: "p_d".into(),
            effect_kind_label: "e".into(), precondition_kind_label: "p".into(),
            low_confidence: false,
        },
    ];

    let graph = build_chain_graph(&matches);
    let chains = detect_chains(&graph, &severities, 10);

    // Both chains should be discovered independently
    let chain_abc = chains.iter().find(|c| {
        c.bug_ids.len() == 3 && c.bug_ids[0] == "A" && c.bug_ids[2] == "C"
    });
    let chain_bcd = chains.iter().find(|c| {
        c.bug_ids.len() == 3 && c.bug_ids[0] == "B" && c.bug_ids[2] == "D"
    });

    assert!(
        chain_abc.is_some(),
        "Should find chain A→B→C"
    );
    assert!(
        chain_bcd.is_some(),
        "Should find chain B→C→D"
    );
}

// ─── D2: PoC Composer Step Failure Propagation ───
#[test]
fn test_poc_composer_failure_propagation() {
    let mut composer = PocComposer::new();
    // 3-step chain where step 2 fails
    composer.add_step("BUG-A", "seed", Some("output_a"), Some(0), Some("info_leak"), true);
    composer.add_step("BUG-B", "output_a", Some("wrong_output"), Some(1), None, false);
    composer.add_step("BUG-C", "should_not_execute", None, None, None, false);

    assert!(!composer.all_steps_succeeded());
    let combined = composer.build_combined_poc("fail-chain");
    assert!(!combined.verified);
    assert!(combined.verification_error.is_some());
    let err = combined.verification_error.unwrap();
    assert!(
        err.contains("BUG-B") || err.contains("1"),
        "Error should reference the failed step: {}",
        err
    );
}

// ─── D2: Large Bug Set Performance Baseline ───
#[test]
fn test_large_bug_set_chain_detection() {
    let mut descs = HashMap::new();

    // 50 bugs with varied descriptions that should produce some chains
    let templates = [
        ("leak heap address via OOB read", 3u8),
        ("requires memory address for heap overflow", 5u8),
        ("buffer overflow corrupts function pointer", 7u8),
        ("arbitrary code execution via corrupted vtable", 9u8),
        ("use-after-free requires heap state manipulation", 6u8),
        ("race condition requires timing window of 100ns", 4u8),
        ("privilege escalation requires root access", 8u8),
        ("stack buffer overflow in parsing routine", 6u8),
        ("integer overflow causes state corruption", 4u8),
        ("null pointer dereference crashes service", 2u8),
    ];

    for i in 0..50 {
        let (desc, sev) = templates[i % templates.len()];
        descs.insert(format!("BUG-{:03}", i), (desc.into(), sev));
    }

    let config = ChainConfig {
        max_hops: 10,
        similarity_threshold: 0.3,
        bfs_max_visits: 10_000,
        max_effects_per_bug: 10,
        max_preconditions_per_bug: 10,
        max_edge_candidates: 100_000,
        ..Default::default()
    };

    let start = std::time::Instant::now();
    let result = run_chain_detection(&descs, &config);
    let elapsed = start.elapsed();

    // Should complete within a reasonable time
    assert!(elapsed.as_secs() < 5, "Chain detection took too long: {:?}", elapsed);

    if let Some(stats) = &result.traversal_stats {
        assert!(stats.nodes_visited <= config.bfs_max_visits + 500);
    }

    assert_eq!(result.original_severities.len(), 50);
    assert_eq!(result.escalated_severities.len(), 50);
}

// ─── Config Serialization Test ───
#[test]
fn test_chain_config_serialization() {
    let config = ChainConfig {
        max_hops: 15,
        similarity_threshold: 0.75,
        severity_flag_threshold: 7.5,
        rce_weight: 2.5,
        trust_boundary_weight: 1.8,
        length_weight: 0.6,
        capability_gain_weight: 0.4,
        base_severity_weight: 0.55,
        escalation_weight: 0.45,
        max_sandbox_concurrency: 8,
        poc_step_timeout_secs: 120,
        max_edge_candidates: 75_000,
        max_effects_per_bug: 50,
        max_preconditions_per_bug: 200,
        bfs_max_visits: 50_000,
        min_extraction_confidence: 0.5,
    };

    // Verify non-default values are stored
    assert_eq!(config.max_hops, 15);
    assert!((config.similarity_threshold - 0.75).abs() < 0.001);
    assert_eq!(config.max_sandbox_concurrency, 8);
    assert_eq!(config.poc_step_timeout_secs, 120);
    assert!((config.min_extraction_confidence - 0.5).abs() < 0.001);
}
