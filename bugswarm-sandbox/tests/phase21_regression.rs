/// Phase 21 (Taint-Guided Fuzzing) Regression & Attack Tests
/// =========================================================
/// Goal: BREAK the danger map / danger feed / shared memory / FuzzController
/// integration against Phase 20 (Coverage-Guided Fuzzing) baseline.
///
/// Test categories:
///   1. FuzzConfig Danger Field Consistency
///   2. DangerFeed Edge Cases
///   3. DangerConfig Validation
///   4. CampaignStats Danger Fields
///   5. DangerMap Binary Search Correctness
///   6. Shared Memory Round-Trip
///   7. Cross-Crate Integration
///   8. FuzzController Danger Integration
///   9. Bug Discovery (panic / crash probes)
///  10. Memory and Resource Leaks

use bugswarm_sandbox::danger_map::*;
use bugswarm_sandbox::fuzzer::*;
use std::collections::HashMap;

// ============================================================================
// 1. FuzzConfig Danger Field Consistency
// ============================================================================

#[test]
fn test_fuzzconfig_default_danger_is_disabled() {
    let cfg = FuzzConfig::default();
    assert!(!cfg.danger_map_enabled, "BUG: FuzzConfig::default() should have danger_map_enabled=false");
    assert!(cfg.danger_config.is_none(), "BUG: FuzzConfig::default() should have danger_config=None");
    assert!(cfg.danger_map_shm_name.is_none(), "BUG: FuzzConfig::default() should have danger_map_shm_name=None");
}

/// DangerConfig::default() also has enabled=true independently of FuzzConfig danger_map_enabled.
/// This is an inconsistency — DangerFeed needs to respect FuzzConfig.danger_map_enabled, not DangerConfig.enabled.
/// Bug? The DangerFeed is constructed with `danger_map_enabled` from FuzzConfig, which bypasses DangerConfig.enabled.
#[test]
fn test_dangerconfig_default_has_enabled_true_inconsistency() {
    let dc = DangerConfig::default();
    // DangerConfig internal `enabled` is ALWAYS true by default, while FuzzConfig defaults to false.
    // This is an inconsistency but FuzzController overrides it with the FuzzConfig flag.
    assert!(dc.enabled, "INFO: DangerConfig::default().enabled=true but FuzzConfig.danger_map_enabled=false by default");
}

#[test]
fn test_danger_map_enabled_false_ignores_shm_name() {
    let cfg = FuzzConfig {
        danger_map_enabled: false,
        danger_map_shm_name: Some("/should_be_ignored".into()),
        ..Default::default()
    };
    let ctrl = FuzzController::new(cfg, DedupConfig::default());
    // When danger_map_enabled=false, the DangerFeed should have enabled=false
    // and loading from SHM should be skipped.
    let stats = ctrl.danger_stats();
    assert_eq!(stats.executions, 0);
    // compute_power_score should still work but give clamped result with 0 danger
    let score = ctrl.compute_power_score(0x1000, 0.5);
    // With enabled=false, the DangerFeed still computes (bug? or feature?)
    // Let's see: it uses DangerConfig::default() which isn't bypassed at compute level
    assert!(score >= 0.0 && score <= 1.0, "Score should be clamped to [0,1]");
}

#[test]
fn test_danger_config_none_but_enabled_true_uses_defaults() {
    let cfg = FuzzConfig {
        danger_map_enabled: true,
        danger_config: None,
        ..Default::default()
    };
    let ctrl = FuzzController::new(cfg, DedupConfig::default());
    let score = ctrl.compute_power_score(0x1000, 0.5);
    // When danger_config is None, FuzzController uses DangerConfig::default()
    // which has taint_weight=0.7, coverage_weight=0.3
    assert!((score - 0.15).abs() < 0.001, "score should be 0.5*0.3 + 0.0*0.7 = 0.15");
}

#[test]
fn test_sandbox_config_danger_defaults() {
    let cfg = bugswarm_sandbox::config::SandboxConfig::default();
    assert!(!cfg.fuzz_danger_map_enabled, "BUG: SandboxConfig default should have fuzz_danger_map_enabled=false");
    assert!((cfg.fuzz_danger_decay - 0.7).abs() < 0.001);
    assert!((cfg.fuzz_danger_taint_weight - 0.7).abs() < 0.001);
    assert!((cfg.fuzz_danger_coverage_weight - 0.3).abs() < 0.001);
}

// ============================================================================
// 2. DangerFeed Edge Cases
// ============================================================================

#[test]
fn test_dangerfeed_zero_entries_stats_returns_defaults() {
    let config = DangerConfig::default();
    let feed = DangerFeed::new(true, config);
    let stats = feed.stats();
    assert_eq!(stats.executions, 0);
    assert_eq!(stats.avg_danger_score, 0.0);
    assert_eq!(stats.avg_power_score, 0.0);
    assert_eq!(stats.danger_min, 0.0);
    assert_eq!(stats.danger_max, 0.0);
    assert_eq!(stats.danger_p50, 0.0);
    assert_eq!(stats.danger_p95, 0.0);
    assert_eq!(stats.sink_mutations, 0);
}

#[test]
fn test_dangerfeed_disabled_record_execution_noop() {
    let config = DangerConfig::default();
    let mut feed = DangerFeed::new(false, config);
    feed.load_map(vec![(0x1000, 0.9), (0x2000, 0.5)]);
    let score = feed.record_execution(0x1000, 0.5);
    // FIXED: record_execution now honors the enabled flag — returns 0.0 when disabled
    assert_eq!(score, 0.0, "FIXED: DangerFeed::record_execution should return 0.0 when disabled");
    let stats = feed.stats();
    assert_eq!(stats.executions, 0, "FIXED: No executions recorded when disabled");
}

#[test]
fn test_dangerfeed_load_map_empty_vector() {
    let config = DangerConfig::default();
    let mut feed = DangerFeed::new(true, config);
    feed.load_map(vec![]);
    assert_eq!(feed.map.len(), 0);
    // Should compute score 0 for any address (floor semantics fallback)
    let score = feed.compute_score(0x1000, 0.5);
    // coverage_rarity * coverage_weight + danger_score * taint_weight = 0.5*0.3 + 0*0.7 = 0.15
    assert!((score - 0.15).abs() < 0.001);
}

#[test]
fn test_dangerfeed_load_map_duplicate_addresses() {
    let config = DangerConfig::default();
    let mut feed = DangerFeed::new(true, config);
    // Load map with duplicate addresses — sort_by_key is stable, dedup keeps first occurrence
    feed.load_map(vec![(0x1000, 0.1), (0x1000, 0.9), (0x1000, 0.5)]);
    // After dedup: only first occurrence (0.1) kept. After normalize with max=0.1: score=1.0
    assert_eq!(feed.map.len(), 1, "Duplicate addresses should be deduplicated to 1 entry");
    // First occurrence kept (0.1), normalized by max (0.1) → 1.0
    assert!((feed.map.lookup(0x1000) - 1.0).abs() < 0.01,
        "Dedup keeps first entry: 0.1/0.1 = 1.0");
}

#[test]
fn test_dangerfeed_load_map_random_order() {
    let config = DangerConfig::default();
    let mut feed = DangerFeed::new(true, config);
    // NOTE: load_map NORMALIZES scores (divides by max), so absolute scores shift
    feed.load_map(vec![
        (0x5000, 1.0),
        (0x1000, 0.2),
        (0x4000, 0.8),
        (0x2000, 0.4),
        (0x3000, 0.6),
    ]);
    // Max score = 1.0, so no change from normalization.
    // After sort: 0x1000→0.2, 0x2000→0.4, 0x3000→0.6, 0x4000→0.8, 0x5000→1.0
    assert!((feed.map.lookup(0x1000) - 0.2).abs() < 0.001);
    assert!((feed.map.lookup(0x2000) - 0.4).abs() < 0.001);
    assert!((feed.map.lookup(0x3000) - 0.6).abs() < 0.001);
    assert!((feed.map.lookup(0x4000) - 0.8).abs() < 0.001);
    assert!((feed.map.lookup(0x5000) - 1.0).abs() < 0.001);
    // Floor semantics between entries
    assert!((feed.map.lookup(0x2500) - 0.4).abs() < 0.001, "Floor from 0x2000");
    assert!((feed.map.lookup(0x4500) - 0.8).abs() < 0.001, "Floor from 0x4000");
}

#[test]
fn test_dangerfeed_load_map_large_performance() {
    let config = DangerConfig::default();
    let mut feed = DangerFeed::new(true, config);
    let mut pairs = Vec::with_capacity(100_000);
    for i in 0..100_000u64 {
        pairs.push((i * 16, (i % 1000) as f32 / 1000.0));
    }
    feed.load_map(pairs);
    assert_eq!(feed.map.len(), 100_000);

    // Time 100 lookups — should be well under 1ms (O(log n))
    use std::time::Instant;
    let start = Instant::now();
    for i in 0..100u64 {
        let addr = (i * 13 + 77777).wrapping_mul(16);
        let _score = feed.map.lookup(addr);
    }
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 10,
        "BUG: DangerMap lookup performance degradation: {}ms for 100 lookups (100K entries)",
        elapsed.as_millis());
}

// ============================================================================
// 3. DangerConfig Validation
// ============================================================================

#[test]
fn test_dangerconfig_weights_dont_need_to_sum_to_one() {
    let cfg = DangerConfig {
        taint_weight: 0.8,
        coverage_weight: 0.8,
        ..Default::default()
    };
    let score = cfg.compute_power_schedule(1.0, 0.0);
    // 1.0 * 0.8 + 0.0 * 0.8 = 0.8, clamped to [0,1] -> 0.8
    assert!((score - 0.8).abs() < 0.001);

    let score2 = cfg.compute_power_schedule(0.5, 0.5);
    // 0.5 * 0.8 + 0.5 * 0.8 = 0.8, clamped -> 0.8
    assert!((score2 - 0.8).abs() < 0.001);
}

#[test]
fn test_dangerconfig_extreme_zero_weights() {
    let cfg = DangerConfig {
        taint_weight: 0.0,
        coverage_weight: 0.0,
        ..Default::default()
    };
    let score = cfg.compute_power_schedule(0.5, 0.9);
    assert!((score - 0.0).abs() < 0.001, "Both weights zero => score should be 0");
}

#[test]
fn test_dangerconfig_extreme_one_weights_scores_can_exceed_one_before_clamp() {
    let cfg = DangerConfig {
        taint_weight: 1.0,
        coverage_weight: 1.0,
        ..Default::default()
    };
    // Before clamping: 0.8 * 1.0 + 0.8 * 1.0 = 1.6
    // After clamping: min(1.6, 1.0) = 1.0
    let score = cfg.compute_power_schedule(0.8, 0.8);
    assert!((score - 1.0).abs() < 0.001, "Clamped to 1.0");

    // DOCUMENTED: Scores can exceed 1.0 in the raw calculation but are clamped.
    // This is not a bug but a design choice — high weights + high scores saturate.
}

#[test]
fn test_dangerconfig_negative_weights_not_rejected() {
    let cfg = DangerConfig {
        taint_weight: -0.5,
        coverage_weight: -0.5,
        ..Default::default()
    };
    let score = cfg.compute_power_schedule(1.0, 1.0);
    // -0.5 * 1.0 + -0.5 * 1.0 = -1.0, clamped to [0,1] -> 0.0
    assert!((score - 0.0).abs() < 0.001,
        "POTENTIAL ISSUE: Negative weights are not rejected. They produce clamped 0.0.");

    // Try: high negative taint_weight can make dangerous code get score 0
    let cfg2 = DangerConfig {
        taint_weight: -10.0,
        coverage_weight: 0.5,
        ..Default::default()
    };
    let score2 = cfg2.compute_power_schedule(0.8, 0.9);
    // -10*0.9 + 0.5*0.8 = -9 + 0.4 = -8.6, clamped to 0
    assert!((score2 - 0.0).abs() < 0.001,
        "ISSUE: Negative taint_weight defeats taint guidance entirely. Score: {}", score2);
}

// ============================================================================
// 4. CampaignStats Danger Fields
// ============================================================================

#[test]
fn test_campaignstats_danger_after_crashes() {
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            danger_config: Some(DangerConfig::default()),
            ..Default::default()
        },
        DedupConfig::default(),
    );
    ctrl.start().unwrap();
    ctrl.load_danger_map(vec![(0x1000, 0.9), (0x2000, 0.3), (0x3000, 0.99)]);

    // Record 5 crashes at different addresses with different scores
    for i in 0..5 {
        let addr = 0x1000 + (i * 0x1000);
        ctrl.record_crash(
            vec![i as u8],
            format!("#0 crash_{}", i),
            11,
            "".into(),
            addr,
        );
    }

    let stats = ctrl.stats();
    assert_eq!(stats.unique_crashes, 5);

    // danger_score_min should be >= 0.0 (floor semantics for above max = max entry score)
    assert!(stats.danger_score_min >= 0.0);
    // danger_score_max should be <= max entry score
    assert!(stats.danger_score_max <= 1.0, "Max danger should be <= 1.0 after normalizing");

    // p50 should be reasonable
    assert!(stats.danger_score_p50 >= stats.danger_score_min);
    assert!(stats.danger_score_p50 <= stats.danger_score_max);

    // FIXED: danger_score_min/max/p50 are now updated in record_crash()
    assert!(stats.danger_score_min > 0.0, "FIXED: danger_score_min should be updated with actual min");
    assert!(stats.danger_score_max > 0.0, "FIXED: danger_score_max should be updated with actual max");
}

#[test]
fn test_campaignstats_sink_mutations_only_when_danger_above_half() {
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            ..Default::default()
        },
        DedupConfig::default(),
    );
    ctrl.start().unwrap();
    // Load map with various scores
    ctrl.load_danger_map(vec![
        (0x1000, 0.1),  // low danger
        (0x2000, 0.3),  // low danger
        (0x3000, 0.6),  // high danger (> 0.5)
        (0x4000, 0.9),  // high danger (> 0.5)
    ]);

    // Crash at 0x1000 (score 0.1) → should NOT increment sink_mutations_count
    ctrl.record_crash(vec![1], "#0 low1".into(), 11, "".into(), 0x1000);
    let stats = ctrl.stats();
    assert_eq!(stats.sink_mutations_count, 0,
        "BUG: sink_mutations_count should be 0 for danger <= 0.5, got {}", stats.sink_mutations_count);

    // Crash at 0x3000 (score 0.6) → SHOULD increment sink_mutations_count
    ctrl.record_crash(vec![2], "#0 high1".into(), 11, "".into(), 0x3000);
    let stats2 = ctrl.stats();
    assert_eq!(stats2.sink_mutations_count, 1,
        "BUG: sink_mutations_count should be 1 for danger > 0.5, got {}", stats2.sink_mutations_count);

    // Crash at 0x4000 (score 0.9) → SHOULD increment
    ctrl.record_crash(vec![3], "#0 high2".into(), 11, "".into(), 0x4000);
    let stats3 = ctrl.stats();
    assert_eq!(stats3.sink_mutations_count, 2,
        "BUG: sink_mutations_count should be 2 after 2 high-danger crashes, got {}", stats3.sink_mutations_count);
}

#[test]
fn test_power_score_computations_matches_calls() {
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            ..Default::default()
        },
        DedupConfig::default(),
    );
    ctrl.start().unwrap();
    ctrl.load_danger_map(vec![(0x1000, 0.5)]);

    for i in 0..10 {
        ctrl.record_power_score(0x1000, 0.3);
    }

    // FIXED: record_power_score now increments power_score_computations
    let stats = ctrl.stats();
    assert_eq!(stats.power_score_computations, 10,
        "FIXED: power_score_computations should now match call count");
}

#[test]
fn test_campaignstats_survive_pause_resume() {
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            ..Default::default()
        },
        DedupConfig::default(),
    );
    ctrl.start().unwrap();
    ctrl.load_danger_map(vec![(0x1000, 0.5), (0x2000, 0.8)]);
    ctrl.record_crash(vec![1], "#0 c1".into(), 11, "".into(), 0x1000);
    ctrl.record_crash(vec![2], "#0 c2".into(), 11, "".into(), 0x2000);

    let unique_before = ctrl.stats().unique_crashes;
    let sink_before = ctrl.stats().sink_mutations_count;

    ctrl.pause().unwrap();
    ctrl.resume().unwrap();

    assert_eq!(ctrl.stats().unique_crashes, unique_before,
        "stats.unique_crashes should survive pause/resume");
    assert_eq!(ctrl.stats().sink_mutations_count, sink_before,
        "stats.sink_mutations_count should survive pause/resume");
}

// ============================================================================
// 5. DangerMap Binary Search Correctness
// ============================================================================

#[test]
fn test_dangermap_lookup_zero_entries() {
    let map = DangerMap::new();
    assert_eq!(map.lookup(0), 0.0);
    assert_eq!(map.lookup(0x1000), 0.0);
    assert_eq!(map.lookup(u64::MAX), 0.0);
}

#[test]
fn test_dangermap_lookup_one_entry() {
    let map = DangerMap::from_pairs(vec![(0x2000, 0.7)]);
    assert_eq!(map.lookup(0x2000), 0.7);     // exact match
    assert_eq!(map.lookup(0x1000), 0.0);     // below
    assert_eq!(map.lookup(0x3000), 0.7);     // above → floor → 0x2000
    assert_eq!(map.lookup(u64::MAX), 0.7);   // far above → floor → 0x2000
}

#[test]
fn test_dangermap_lookup_two_entries() {
    let map = DangerMap::from_pairs(vec![(0x1000, 0.2), (0x5000, 0.8)]);
    assert_eq!(map.lookup(0x0500), 0.0);     // below all
    assert_eq!(map.lookup(0x1000), 0.2);     // exact
    assert_eq!(map.lookup(0x2000), 0.2);     // floor → 0x1000
    assert_eq!(map.lookup(0x3000), 0.2);     // floor → 0x1000
    assert_eq!(map.lookup(0x5000), 0.8);     // exact
    assert_eq!(map.lookup(0x6000), 0.8);     // floor → 0x5000
    assert_eq!(map.lookup(u64::MAX), 0.8);   // floor → 0x5000
}

#[test]
fn test_dangermap_lookup_power_of_two_entries() {
    let mut pairs = Vec::new();
    for i in 0u64..8u64 {
        pairs.push((i * 0x1000, (i + 1) as f32 / 10.0));
    }
    let map = DangerMap::from_pairs(pairs);
    assert_eq!(map.len(), 8);

    // Exact matches
    assert!((map.lookup(0x0000) - 0.1).abs() < 0.001);
    assert!((map.lookup(0x7000) - 0.8).abs() < 0.001);

    // Floor between entries
    assert!((map.lookup(0x1500) - 0.2).abs() < 0.001);  // floor → 0x1000=0.2
    assert!((map.lookup(0x3500) - 0.4).abs() < 0.001);  // floor → 0x3000=0.4
}

#[test]
fn test_dangermap_lookup_non_power_of_two_entries() {
    let pairs = vec![
        (0x1000, 0.1),
        (0x2000, 0.2),
        (0x3000, 0.3),
        (0x4000, 0.4),
        (0x5000, 0.5),
        (0x6000, 0.6),
        (0x7000, 0.7),  // 7 entries (not power of 2)
    ];
    let map = DangerMap::from_pairs(pairs);
    assert_eq!(map.len(), 7);

    // Floor between entries
    assert!((map.lookup(0x2500) - 0.2).abs() < 0.001);  // floor → 0x2000=0.2
    assert!((map.lookup(0x6500) - 0.6).abs() < 0.001);  // floor → 0x6000=0.6 (not 0x7000!)

    // Exact matches at boundaries
    assert!((map.lookup(0x1000) - 0.1).abs() < 0.001);
    assert!((map.lookup(0x7000) - 0.7).abs() < 0.001);
}

#[test]
fn test_dangermap_lookup_exact_boundary() {
    let map = DangerMap::from_pairs(vec![(0x1000, 0.3), (0x2000, 0.7)]);
    // Exactly at 0x1000 (boundary) → exact match
    assert!((map.lookup(0x1000) - 0.3).abs() < 0.001);
    // Exactly at 0x2000 → exact match
    assert!((map.lookup(0x2000) - 0.7).abs() < 0.001);
    // One below boundary
    assert!((map.lookup(0x0FFF) - 0.0).abs() < 0.001, "Below 0x1000 should be 0.0");
    // One above boundary  
    assert!((map.lookup(0x1FFF) - 0.3).abs() < 0.001, "Between 0x1000 and 0x2000 → floor 0x1000");
    // One below upper boundary
    assert!((map.lookup(0x2001) - 0.7).abs() < 0.001, "Above 0x2000, floor is 0x2000");
}

#[test]
fn test_dangermap_lookup_u64_max() {
    let map = DangerMap::from_pairs(vec![
        (u64::MAX - 10, 0.5),
        (u64::MAX, 1.0),
    ]);
    assert!((map.lookup(u64::MAX) - 1.0).abs() < 0.001, "Exact at u64::MAX");
    assert!((map.lookup(u64::MAX - 1) - 0.5).abs() < 0.001, "Floor → u64::MAX-10");
    assert!((map.lookup(u64::MAX - 20) - 0.0).abs() < 0.001, "Below all");
}

#[test]
fn test_dangermap_special_scores() {
    // Test with f32::MAX score
    let map = DangerMap::from_pairs(vec![
        (0x1000, f32::MAX),
        (0x2000, 0.5),
        (0x3000, f32::MIN_POSITIVE),
    ]);
    assert_eq!(map.lookup(0x1000), f32::MAX);
    assert!((map.lookup(0x2000) - 0.5).abs() < 0.001);
    assert!((map.lookup(0x3000) - f32::MIN_POSITIVE).abs() < 1e-40);

    // Test normalize with f32::MAX
    let mut map_norm = DangerMap::from_pairs(vec![
        (0x1000, f32::MAX),
        (0x2000, f32::MAX / 2.0),
    ]);
    map_norm.normalize();
    assert!((map_norm.lookup(0x1000) - 1.0).abs() < 0.001, "Max should normalize to 1.0");
    assert!((map_norm.lookup(0x2000) - 0.5).abs() < 0.001, "Half of max");
}

#[test]
fn test_dangermap_special_scores_nan() {
    // What happens with NaN scores?
    let map = DangerMap::from_pairs(vec![(0x1000, f32::NAN)]);
    let score = map.lookup(0x1000);
    assert!(score.is_nan(), "NaN score should be preserved on lookup");

    // Normalize with NaN — NaN should be replaced with 0.0 after fix
    let mut map2 = DangerMap::from_pairs(vec![(0x1000, f32::NAN), (0x2000, 1.0)]);
    map2.normalize();
    // After fix: NaN → 0.0, finite values normalized by max(1.0) → 1.0/1.0 = 1.0
    assert_eq!(map2.lookup(0x1000), 0.0, "NaN should become 0.0 after normalize (fixed)");
    assert!((map2.lookup(0x2000) - 1.0).abs() < 0.001);
}

#[test]
fn test_dangermap_special_scores_infinity() {
    let map = DangerMap::from_pairs(vec![(0x1000, f32::INFINITY)]);
    let score = map.lookup(0x1000);
    assert!(score.is_infinite() && score > 0.0, "Positive infinity should be preserved");

    // Normalize with infinity — INF replaced with 0.0 after fix, finite values normalize normally
    let mut map2 = DangerMap::from_pairs(vec![(0x1000, f32::INFINITY), (0x2000, 5.0)]);
    map2.normalize();
    // After fix: INF → 0.0, max finite = 5.0, so 5.0/5.0 = 1.0
    assert_eq!(map2.lookup(0x1000), 0.0, "INF should become 0.0 after normalize (fixed)");

    let score2_norm = map2.lookup(0x2000);
    // 5.0 / 5.0 = 1.0
    assert!((score2_norm - 1.0).abs() < 1e-10, "5.0/5.0 should be 1.0 with INF filtered out");
}

#[test]
fn test_dangermap_negative_scores() {
    let map = DangerMap::from_pairs(vec![(0x1000, -1.0), (0x2000, -0.5)]);
    assert!((map.lookup(0x1000) + 1.0).abs() < 0.001);
    assert!((map.lookup(0x2000) + 0.5).abs() < 0.001);
    assert_eq!(map.lookup(0x0500), 0.0);  // below all → 0.0

    // Floor between entries
    assert!((map.lookup(0x1500) + 1.0).abs() < 0.001, "Floor to 0x1000 = -1.0");
    assert!((map.lookup(0x3000) + 0.5).abs() < 0.001, "Floor to 0x2000 = -0.5");

    // BUG: lookup(low_addr) on entry at 0x0500 returns 0.0 (below all guard), 
    // but the only entries have negative scores. Should floor semantics return negative?
    // This is the design: below all → 0.0. This means the first entry's score could be 
    // negative but a query below it returns 0.0 — a discontinuity.
}

// ============================================================================
// 6. Shared Memory Round-Trip
// ============================================================================

#[test]
fn test_shm_roundtrip_0_entries() {
    let name = "/phase21_shm_0";
    let _ = shm_unlink(name);

    let dm = DangerMap::new();
    assert_eq!(dm.len(), 0);
    danger_map_to_shm(&dm, name).unwrap();

    let dm2 = danger_map_from_shm(name).unwrap();
    assert_eq!(dm2.len(), 0);
}

#[test]
fn test_shm_roundtrip_1_entry() {
    let name = "/phase21_shm_1";
    let _ = shm_unlink(name);

    let dm = DangerMap::from_pairs(vec![(0x4000, 0.42)]);
    danger_map_to_shm(&dm, name).unwrap();
    let dm2 = danger_map_from_shm(name).unwrap();

    assert_eq!(dm2.len(), 1);
    assert!((dm2.lookup(0x4000) - 0.42).abs() < 0.001);
}

#[test]
fn test_shm_roundtrip_1000_entries() {
    let name = "/phase21_shm_1000";
    let _ = shm_unlink(name);

    let mut pairs = Vec::with_capacity(1000);
    for i in 0..1000u64 {
        pairs.push((i * 0x100, (i as f32 * 0.001)));
    }
    let dm = DangerMap::from_pairs(pairs);
    assert_eq!(dm.len(), 1000);
    danger_map_to_shm(&dm, name).unwrap();
    let dm2 = danger_map_from_shm(name).unwrap();

    assert_eq!(dm2.len(), 1000);
    // Spot check
    assert!((dm2.lookup(0) - 0.0).abs() < 0.001);
    assert!((dm2.lookup(500 * 0x100) - 0.5).abs() < 0.001);
    assert!((dm2.lookup(999 * 0x100) - 0.999).abs() < 0.001);
}

#[test]
fn test_shm_unlink_cleans_up() {
    let name = "/phase21_shm_cleanup";
    let _ = shm_unlink(name);

    // Create and write SHM
    let dm = DangerMap::from_pairs(vec![(0x1000, 0.5)]);
    danger_map_to_shm(&dm, name).unwrap();

    // First read should succeed (danger_map_from_shm reads then unlinks)
    let result1 = danger_map_from_shm(name);
    assert!(result1.is_ok(), "First read should succeed");

    // The SHM was unlinked at the end of danger_map_from_shm.
    // Second attempt should fail because the SHM segment no longer exists.
    let result2 = danger_map_from_shm(name);
    assert!(result2.is_err(), "BUG: SHM not cleaned up — second read should fail after unlink");
}

#[test]
fn test_shm_recreate_same_name() {
    let name = "/phase21_shm_recreate";
    let _ = shm_unlink(name);

    // Create first
    let dm1 = DangerMap::from_pairs(vec![(0x1000, 0.1)]);
    danger_map_to_shm(&dm1, name).unwrap();
    let read1 = danger_map_from_shm(name);
    assert!(read1.is_ok(), "First read should succeed");

    // After read, SHM is unlinked. Create again with same name.
    let dm2 = DangerMap::from_pairs(vec![(0x2000, 0.9)]);
    danger_map_to_shm(&dm2, name).unwrap();
    let read2 = danger_map_from_shm(name).unwrap();

    assert_eq!(read2.len(), 1);
    assert!((read2.lookup(0x2000) - 0.9).abs() < 0.001);
}

#[test]
fn test_dangermap_to_bytes_from_bytes_integrity() {
    let pairs: Vec<(u64, f32)> = (0..1000u64).map(|i| (i * 8, i as f32 / 1000.0)).collect();
    let dm = DangerMap::from_pairs(pairs);
    let bytes = dm.to_bytes();
    let dm2 = DangerMap::from_bytes(&bytes).unwrap();

    assert_eq!(dm2.len(), 1000);
    for i in [0u64, 1, 10, 100, 500, 999] {
        let expected = i as f32 / 1000.0;
        let actual = dm2.lookup(i * 8);
        assert!((actual - expected).abs() < 0.001,
            "Entry {}: expected {}, got {}", i, expected, actual);
    }
}

// ============================================================================
// 7. Cross-Crate Integration
// ============================================================================

#[test]
fn test_sandbox_config_danger_fields_accessible() {
    let cfg = bugswarm_sandbox::config::SandboxConfig::default();
    // Verify all danger fields are accessible
    assert!(!cfg.fuzz_danger_map_enabled);
    assert_eq!(cfg.fuzz_danger_decay, 0.7);
    assert_eq!(cfg.fuzz_danger_taint_weight, 0.7);
    assert_eq!(cfg.fuzz_danger_coverage_weight, 0.3);
}

#[test]
fn test_dangerconfig_from_danger_map_module_in_fuzzconfig() {
    let dc = bugswarm_sandbox::danger_map::DangerConfig {
        taint_weight: 0.9,
        coverage_weight: 0.1,
        ..Default::default()
    };
    let fz = FuzzConfig {
        danger_map_enabled: true,
        danger_config: Some(dc),
        ..Default::default()
    };
    assert!(fz.danger_config.is_some());
    let unwrapped = fz.danger_config.unwrap();
    assert!((unwrapped.taint_weight - 0.9).abs() < 0.001);
    assert!((unwrapped.coverage_weight - 0.1).abs() < 0.001);
}

#[test]
fn test_campaign_id_serde_roundtrip() {
    use serde::{Serialize, Deserialize};
    let id = CampaignId(uuid::Uuid::new_v4());
    let json = serde_json::to_string(&id).unwrap();
    // CampaignId is transparent — should serialize as a plain UUID string
    let parsed: CampaignId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, parsed);

    // Test deserialization from raw UUID string
    let uuid_str = "\"550e8400-e29b-41d4-a716-446655440000\"";
    let parsed2: CampaignId = serde_json::from_str(uuid_str).unwrap();
    assert_eq!(parsed2.0.to_string(), "550e8400-e29b-41d4-a716-446655440000");
}

#[test]
fn test_campaign_stats_serde_roundtrip() {
    let stats = CampaignStats {
        total_execs: 1000,
        execs_per_sec: 125.5,
        unique_crashes: 5,
        duplicate_crashes: 3,
        hangs: 2,
        corpus_entries: 50,
        corpus_favored: 10,
        edges_found: 200,
        edges_total: 500,
        coverage_pct: 40.0,
        cycles_done: 12,
        elapsed_secs: 60,
        pending_favs: 8,
        pending_total: 30,
        stability_pct: 99.0,
        sink_mutations_count: 3,
        avg_danger_score: 0.65,
        danger_score_min: 0.1,
        danger_score_max: 0.95,
        danger_score_p50: 0.6,
        power_scores_total: 42.0,
        power_score_computations: 100,
    };
    let json = serde_json::to_string(&stats).unwrap();
    let parsed: CampaignStats = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.total_execs, stats.total_execs);
    assert_eq!(parsed.unique_crashes, stats.unique_crashes);
    assert_eq!(parsed.sink_mutations_count, stats.sink_mutations_count);
    assert!((parsed.avg_danger_score - stats.avg_danger_score).abs() < 0.001);
    assert!((parsed.danger_score_min - stats.danger_score_min).abs() < 0.001);
    assert!((parsed.danger_score_max - stats.danger_score_max).abs() < 0.001);
}

// ============================================================================
// 8. FuzzController Danger Integration
// ============================================================================

#[test]
fn test_fuzzcontroller_with_danger_enabled_compute_power_score() {
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            danger_config: Some(DangerConfig {
                taint_weight: 0.5,
                coverage_weight: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        },
        DedupConfig::default(),
    );
    ctrl.start().unwrap();
    ctrl.load_danger_map(vec![(0x1000, 0.8), (0x2000, 0.2)]);

    // NOTE: load_map NORMALIZES. 0.8→1.0, 0.2→0.25 (0.2/0.8)
    let score1 = ctrl.compute_power_score(0x1000, 0.5);
    // 0.5*0.5 + 1.0*0.5 = 0.25 + 0.50 = 0.75
    assert!((score1 - 0.75).abs() < 0.001, "After normalize: 1.0 danger => score 0.75, got {}", score1);

    let score2 = ctrl.compute_power_score(0x2000, 0.5);
    // 0.5*0.5 + 0.25*0.5 = 0.25 + 0.125 = 0.375
    assert!((score2 - 0.375).abs() < 0.001, "After normalize: 0.25 danger => score 0.375, got {}", score2);

    let score3 = ctrl.compute_power_score(0x3000, 0.5);
    // floor → 0x2000, danger=0.25: 0.5*0.5 + 0.25*0.5 = 0.375
    assert!((score3 - 0.375).abs() < 0.001, "Floor to 0x2000: expected 0.375, got {}", score3);
}

#[test]
fn test_fuzzcontroller_danger_disabled_scores_low() {
    let ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: false,
            ..Default::default()
        },
        DedupConfig::default(),
    );
    // Even with danger disabled, DangerConfig::default() is used.
    // Without loading a map, danger_score is always 0.0.
    // Score = coverage_rarity * 0.3 + 0.0 * 0.7 = coverage_rarity * 0.3
    let score = ctrl.compute_power_score(0x1000, 1.0);
    assert!((score - 0.3).abs() < 0.001,
        "With empty map (danger=0), score = coverage * coverage_weight = 1.0*0.3 = 0.3, got {}", score);
}

#[test]
fn test_load_danger_map_if_configured_with_valid_shm() {
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            danger_map_shm_name: Some("/phase21_ctrl_shm".into()),
            ..Default::default()
        },
        DedupConfig::default(),
    );

    // Write a danger map to SHM first
    let dm = DangerMap::from_pairs(vec![(0x1000, 0.7), (0x2000, 0.3)]);
    danger_map_to_shm(&dm, "/phase21_ctrl_shm").unwrap();

    let result = ctrl.load_danger_map_if_configured();
    assert!(result.is_ok(), "load_danger_map_if_configured should succeed with valid SHM");
    // After loading, compute_power_score should reflect the map
    let score = ctrl.compute_power_score(0x1000, 0.5);
    assert!(score > 0.15, "Score should reflect loaded map data");
}

#[test]
fn test_load_danger_map_if_configured_with_invalid_shm_does_not_crash() {
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            danger_map_shm_name: Some("/nonexistent_shm_xyz_999".into()),
            ..Default::default()
        },
        DedupConfig::default(),
    );

    // Should NOT panic
    let result = ctrl.load_danger_map_if_configured();
    assert!(result.is_ok(), "load_danger_map_if_configured with invalid SHM should not crash, should return Ok");
}

#[test]
fn test_record_crash_updates_danger_stats() {
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            ..Default::default()
        },
        DedupConfig::default(),
    );
    ctrl.start().unwrap();
    ctrl.load_danger_map(vec![(0x1000, 0.9), (0x2000, 0.4)]);

    // NOTE: load_map normalizes. 0.9→1.0, 0.4→0.444... (0.4/0.9)
    // Crash at 0x1000 → normalized danger=1.0
    ctrl.record_crash(vec![1], "#0 h".into(), 11, "".into(), 0x1000);

    // avg_danger_score should now be 1.0 (normalized max)
    assert!((ctrl.stats().avg_danger_score - 1.0).abs() < 0.01,
        "avg_danger_score should be 1.0 after one crash with normalized score 1.0, got {}", ctrl.stats().avg_danger_score);

    // Crash at 0x2000 → normalized danger ≈ 0.444
    ctrl.record_crash(vec![2], "#0 l".into(), 11, "".into(), 0x2000);

    let expected = (1.0 + 0.4 / 0.9) / 2.0;
    // 1.0 + 0.444... = 1.444... / 2 = 0.722...
    assert!((ctrl.stats().avg_danger_score - expected).abs() < 0.01,
        "avg_danger_score after 2 crashes should be ~{:.3}, got {}", expected, ctrl.stats().avg_danger_score);
}

// ============================================================================
// 9. Bug Discovery — Try to find actual bugs
// ============================================================================

#[test]
fn test_bug_record_crash_before_start() {
    let mut ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
    // record_crash has NO state check — you can call it before start()!
    let crash = ctrl.record_crash(
        vec![1, 2, 3],
        "#0 pre_start".into(),
        11,
        "err".into(),
        0xdead,
    );
    assert!(crash.is_none(), "FIXED: record_crash now has state guard — returns None before start()");
    assert_eq!(ctrl.stats().unique_crashes, 0, "FIXED: No crash recorded before start");
    assert_eq!(ctrl.state(), CampaignState::Provisioning, "State unchanged after pre-start crash");
}

#[test]
fn test_bug_compute_power_score_before_loading_map() {
    let ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            ..Default::default()
        },
        DedupConfig::default(),
    );
    // compute_power_score without loading a map — should return danger=0
    let score = ctrl.compute_power_score(0x1000, 1.0);
    assert!((score - 0.3).abs() < 0.001,
        "BUG: compute_power_score before loading map returns {} — expected 0.3 (coverage only)", score);
    // This works fine — empty map returns 0.0 for lookup.
}

#[test]
fn test_bug_normalize_empty_danger_map() {
    let mut map = DangerMap::new();
    map.normalize(); // should be a no-op, not panic
    assert_eq!(map.len(), 0);
}

#[test]
fn test_bug_danger_map_from_bytes_corrupted_count() {
    // Create bytes where the count is larger than actual entries provided
    let dm = DangerMap::from_pairs(vec![(0x1000, 0.5)]);
    let mut bytes = dm.to_bytes();
    // Hijack the count: say there are 100 entries instead of 1
    // Without enough data, this should error
    let count_bytes = 100u32.to_le_bytes();
    bytes[0] = count_bytes[0];
    bytes[1] = count_bytes[1];
    bytes[2] = count_bytes[2];
    bytes[3] = count_bytes[3];

    let result = DangerMap::from_bytes(&bytes);
    // expected_len = 8 + 100*16 = 1608, but data.len() = 8 + 16 = 24
    assert!(result.is_err(), "BUG: from_bytes with inflated count should error on short data");

    // Now try: create enough bytes with bogus content BUT set count properly in header
    let mut bogus_bytes = vec![0u8; 8 + 100 * 16];
    // Set count to 100
    bogus_bytes[0] = 100u8;
    let result2 = DangerMap::from_bytes(&bogus_bytes);
    assert!(result2.is_ok(), "from_bytes should succeed with enough bytes even if content is garbage");
    let parsed = result2.unwrap();
    assert_eq!(parsed.len(), 100, "Should read all 100 entries even if they contain garbage zeros");
}

#[test]
fn test_bug_shm_open_nonexistent_does_not_crash() {
    // Open a SHM segment that doesn't exist
    let result = danger_map_from_shm("/nonexistent_shm_for_bug_test_9999");
    assert!(result.is_err(), "Should return error, not crash");
}

#[test]
fn test_bug_from_bytes_truncated_header() {
    let result = DangerMap::from_bytes(&[0u8; 3]);
    assert!(result.is_err(), "from_bytes with truncated header should error");
}

#[test]
fn test_bug_from_bytes_correct_count_data_too_short() {
    // Claim 10 entries but provide data for only 1
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&10u32.to_le_bytes());  // count = 10
    bytes.extend_from_slice(&[0u8; 4]);              // reserved
    bytes.extend_from_slice(&0x1000u64.to_le_bytes()); 
    bytes.extend_from_slice(&0.5f32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 4]);              // padding for 1 entry

    let result = DangerMap::from_bytes(&bytes);
    assert!(result.is_err(), "BUG: from_bytes should error when data is too short for claimed count (expected {} bytes, got {})",
        "8+160", bytes.len());
}

#[test]
fn test_bug_serialization_desync_extra_bytes_after_data() {
    // from_bytes only reads `expected_len` bytes, silently ignoring trailing bytes
    let dm = DangerMap::from_pairs(vec![(0x1000, 0.5)]);
    let mut bytes = dm.to_bytes();
    // Append garbage
    bytes.extend_from_slice(&[0xFFu8; 100]);

    // from_bytes should succeed and only read the claimed entries
    let result = DangerMap::from_bytes(&bytes);
    assert!(result.is_ok(), "EXTRA_BYTES_ACCEPTED: from_bytes ignores trailing bytes — potential desync");
    assert_eq!(result.unwrap().len(), 1, "Should read exactly 1 entry despite extra bytes");
}

#[test]
fn test_bug_danger_scores_not_updated_in_record_crash() {
    // Verify that danger_score_min/max/p50 are never updated by record_crash
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            ..Default::default()
        },
        DedupConfig::default(),
    );
    ctrl.start().unwrap();
    ctrl.load_danger_map(vec![(0x1000, 0.2), (0x2000, 0.9)]);

    // Record crashes with varying danger scores
    ctrl.record_crash(vec![1], "#0 a".into(), 11, "".into(), 0x1000); // danger=0.2
    ctrl.record_crash(vec![2], "#0 b".into(), 11, "".into(), 0x2000); // danger=0.9
    ctrl.record_crash(vec![3], "#0 c".into(), 11, "".into(), 0x3000); // floor→0x2000, danger=0.9

    // FIXED: danger_score_min/max/p50 are now updated in record_crash()
    let stats = ctrl.stats();
    assert!(stats.danger_score_min > 0.0, "FIXED: min should reflect actual min danger score");
    assert!(stats.danger_score_max > 0.0, "FIXED: max should reflect actual max danger score");
    // avg_danger_score IS updated
    assert!(stats.avg_danger_score > 0.0, "avg_danger_score is updated correctly");
}

#[test]
fn test_bug_dangerconfig_defaults_vs_fuzzconfig_inconsistency() {
    // DangerConfig::default() has enabled=true
    // FuzzConfig::default() has danger_map_enabled=false
    // FuzzController::new() uses danger_map_enabled from FuzzConfig, NOT DangerConfig.enabled
    let ctrl1 = FuzzController::new(
        FuzzConfig {
            danger_map_enabled: true,
            danger_config: Some(DangerConfig { enabled: false, ..Default::default() }),
            ..Default::default()
        },
        DedupConfig::default(),
    );
    // DangerConfig.enabled = false, but FuzzConfig.danger_map_enabled = true
    // Which one wins? FuzzController uses danger_map_enabled from FuzzConfig.
    // But DangerFeed.enabled is set from FuzzController, not DangerConfig.
    // So DConfig.enabled is never actually checked by FuzzController.
    // This is a design flaw — DangerConfig.enabled is dead code in the FuzzController context.
    let stats = ctrl1.danger_stats();
    assert_eq!(stats.executions, 0, "No executions yet");
    // DangerConfig.enabled=false is ignored by FuzzController (uses config.danger_map_enabled instead)
    // DOCUMENTED: DangerConfig.enabled is never consulted by FuzzController. Only FuzzConfig.danger_map_enabled matters.
}

// ============================================================================
// 10. Memory and Resource Leaks (smoke tests)
// ============================================================================

#[test]
fn test_no_memory_leak_1000_fuzzcontrollers() {
    let controllers: Vec<FuzzController> = (0..1000)
        .map(|_| FuzzController::new(FuzzConfig::default(), DedupConfig::default()))
        .collect();
    assert_eq!(controllers.len(), 1000);
    drop(controllers);
    // If we got here without OOM, we passed the basic sanity check
}

#[test]
fn test_no_memory_leak_1000_dangermaps() {
    let maps: Vec<DangerMap> = (0..1000)
        .map(|i| {
            DangerMap::from_pairs(vec![
                (i + 0x1000, 0.1),
                (i + 0x2000, 0.5),
                (i + 0x3000, 0.9),
            ])
        })
        .collect();
    assert_eq!(maps.len(), 1000);
    drop(maps);
}

#[test]
fn test_no_memory_leak_1000_dangerfeeds() {
    let feeds: Vec<DangerFeed> = (0..1000)
        .map(|_| {
            DangerFeed::new(true, DangerConfig::default())
        })
        .collect();
    assert_eq!(feeds.len(), 1000);
    drop(feeds);
}

// ============================================================================
// Additional edge case tests
// ============================================================================

#[test]
fn test_danger_map_from_response_json_parsing() {
    let response = DangerMapResponse {
        num_entries: 3,
        sink_count: 1,
        decay_factor: 0.7,
        entries: vec![
            DangerMapEntryJson { address: "0x1000".into(), danger_score: 0.5 },
            DangerMapEntryJson { address: "0X2000".into(), danger_score: 0.8 },
            DangerMapEntryJson { address: "3000".into(), danger_score: 1.0 },
        ],
        timestamp: "2024-01-01T00:00:00Z".into(),
    };
    let dm = response.to_danger_map();
    assert_eq!(dm.len(), 3);
    assert!((dm.lookup(0x1000) - 0.5).abs() < 0.001);
    assert!((dm.lookup(0x2000) - 0.8).abs() < 0.001);
    assert!((dm.lookup(0x3000) - 1.0).abs() < 0.001);
}

#[test]
fn test_danger_map_from_response_invalid_hex_handled_gracefully() {
    let response = DangerMapResponse {
        num_entries: 2,
        sink_count: 0,
        decay_factor: 0.7,
        entries: vec![
            DangerMapEntryJson { address: "0xVALID".into(), danger_score: 0.5 },
            DangerMapEntryJson { address: "0x1000".into(), danger_score: 0.3 },
        ],
        timestamp: "2024-01-01T00:00:00Z".into(),
    };
    let dm = response.to_danger_map();
    // "0xVALID" → from_str_radix returns Err → unwrap_or(0) → address 0
    assert_eq!(dm.len(), 2);
    assert!((dm.lookup(0) - 0.5).abs() < 0.001, "Invalid hex defaults to address 0 with score 0.5");
    assert!((dm.lookup(0x1000) - 0.3).abs() < 0.001);
}

#[test]
fn test_dangerfeed_load_map_from_json() {
    let config = DangerConfig::default();
    let mut feed = DangerFeed::new(true, config);

    let response = DangerMapResponse {
        num_entries: 2,
        sink_count: 2,
        decay_factor: 0.8,
        entries: vec![
            DangerMapEntryJson { address: "0x4000".into(), danger_score: 0.6 },
            DangerMapEntryJson { address: "0x8000".into(), danger_score: 0.9 },
        ],
        timestamp: "2024-01-01T00:00:00Z".into(),
    };

    feed.load_map_from_json(&response);
    assert_eq!(feed.map.len(), 2);
    assert!((feed.map.lookup(0x4000) - 0.6).abs() < 0.001);
    assert!((feed.map.lookup(0x8000) - 0.9).abs() < 0.001);
}

#[test]
fn test_campaign_id_display() {
    let id = CampaignId(uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap());
    let display = format!("{}", id);
    assert_eq!(display, "550e8400-e29b-41d4-a716-446655440000");
}

#[test]
fn test_stop_transition_from_provisioning_fails() {
    let mut ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
    assert!(ctrl.stop().is_err(), "Should not be able to stop from Provisioning state");
    assert!(ctrl.pause().is_err(), "Should not be able to pause from Provisioning state");
    assert!(ctrl.resume().is_err(), "Should not be able to resume from Provisioning state");
}

#[test]
fn test_fuzzconfig_env_vars_default_empty() {
    let cfg = FuzzConfig::default();
    assert!(cfg.env_vars.is_empty());
}

#[test]
fn test_fuzzconfig_env_vars_custom() {
    let mut env = HashMap::new();
    env.insert("ASAN_OPTIONS".into(), "detect_leaks=1".into());
    let cfg = FuzzConfig {
        env_vars: env,
        ..Default::default()
    };
    assert_eq!(cfg.env_vars.get("ASAN_OPTIONS"), Some(&"detect_leaks=1".to_string()));
}

#[test]
fn test_compute_power_score_free_function() {
    let config = DangerConfig::default();
    let score = compute_power_score(0.9, 0.5, &config);
    // 0.5*0.3 + 0.9*0.7 = 0.15 + 0.63 = 0.78
    assert!((score - 0.78).abs() < 0.001);
}

#[test]
fn test_danger_map_from_pairs_autosort() {
    let map = DangerMap::from_pairs(vec![
        (0x5000, 0.5),
        (0x1000, 0.1),
        (0x3000, 0.3),
    ]);
    // from_pairs sorts — entry order should be 0x1000, 0x3000, 0x5000
    assert_eq!(map.pairs[0], (0x1000, 0.1));
    assert_eq!(map.pairs[1], (0x3000, 0.3));
    assert_eq!(map.pairs[2], (0x5000, 0.5));
}

#[test]
fn test_danger_map_sort_pairs() {
    let mut map = DangerMap::from_pairs(vec![(0x3000, 0.3), (0x1000, 0.1), (0x5000, 0.5)]);
    assert_eq!(map.pairs[0], (0x1000, 0.1));

    // Mess up and re-sort
    map.pairs = vec![(0x5000, 0.5), (0x1000, 0.1), (0x3000, 0.3)];
    map.sort_pairs();
    assert_eq!(map.pairs[0], (0x1000, 0.1));
    assert_eq!(map.pairs[1], (0x3000, 0.3));
    assert_eq!(map.pairs[2], (0x5000, 0.5));
}

#[test]
fn test_avg_danger_score_zero_crashes() {
    let ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            ..Default::default()
        },
        DedupConfig::default(),
    );
    assert!((ctrl.stats().avg_danger_score - 0.0).abs() < 0.001,
        "avg_danger_score should start at 0.0");
}

#[test]
fn test_power_scores_total_never_updated() {
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            ..Default::default()
        },
        DedupConfig::default(),
    );
    ctrl.start().unwrap();
    ctrl.load_danger_map(vec![(0x1000, 0.5)]);

    // FIXED: record_power_score now updates power_scores_total
    for _ in 0..5 {
        ctrl.record_power_score(0x1000, 0.3);
    }
    assert!(ctrl.stats().power_scores_total > 0.0,
        "FIXED: power_scores_total should now be updated by record_power_score()");
}

#[test]
fn test_danger_map_from_bytes_zero_entries() {
    let dm = DangerMap::new();
    let bytes = dm.to_bytes();
    let dm2 = DangerMap::from_bytes(&bytes).unwrap();
    assert!(dm2.is_empty());
    assert_eq!(dm2.len(), 0);
}

#[test]
fn test_danger_map_is_empty() {
    assert!(DangerMap::new().is_empty());
    assert!(!DangerMap::from_pairs(vec![(0x1000, 0.5)]).is_empty());
}

#[test]
fn test_controller_after_record_crash_stats() {
    let mut ctrl = FuzzController::new(
        FuzzConfig {
            target_path: "/bin/true".into(),
            danger_map_enabled: true,
            ..Default::default()
        },
        DedupConfig::default(),
    );
    ctrl.start().unwrap();
    ctrl.load_danger_map(vec![(0x1000, 0.85)]);

    // Record a crash with danger > 0.5
    ctrl.record_crash(vec![1, 2], "#0 test".into(), 11, "err".into(), 0x1000);

    // Check the danger feed stats
    let d_stats = ctrl.danger_stats();
    assert!(d_stats.executions > 0, "DangerFeed should record the execution");
}
