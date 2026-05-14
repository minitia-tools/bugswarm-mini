use bugswarm_sandbox::fuzzer::*;
use std::collections::HashMap;

#[test]
fn test_dedup_unique_crashes() {
    let config = DedupConfig::default();
    let mut dedup = DedupEngine::new(config);

    // First crash — should be unique
    assert!(dedup.is_unique("#0 func1 at 0x7fff1234\n#1 func2 at 0x7fff5678"));
    assert_eq!(dedup.crash_count(), 1);

    // Same stack trace — should be duplicate
    assert!(!dedup.is_unique("#0 func1 at 0x7fff1234\n#1 func2 at 0x7fff5678"));
    assert_eq!(dedup.crash_count(), 1);

    // Different stack trace — should be unique
    assert!(dedup.is_unique("#0 func3 at 0x7fffabcd\n#1 func4 at 0x7fffef01"));
    assert_eq!(dedup.crash_count(), 2);
}

#[test]
fn test_dedup_address_normalization() {
    let config = DedupConfig { normalize_addresses: true, ..Default::default() };
    let mut dedup = DedupEngine::new(config);

    // Same crash at different addresses (ASLR) — should dedup
    dedup.is_unique("#0 do_thing at 0x7fff11112222\n#1 main at 0x55554444");
    assert!(!dedup.is_unique("#0 do_thing at 0x7fff99998888\n#1 main at 0x55556666"),
            "Address normalization should deduplicate ASLR-variant crashes");
}

#[test]
fn test_dedup_frame_number_stripping() {
    let config = DedupConfig { ignore_frame_numbers: true, ..Default::default() };
    let mut dedup = DedupEngine::new(config);

    dedup.is_unique("#0 func_a at 0x1234\n#1 func_b at 0x5678");
    // Same frames, different numbers (recursive call) — should dedup if ignore_frame_numbers=true
    // Note: normalization strips frame numbers, so the hashes should match
    assert!(!dedup.is_unique("#5 func_a at 0x1234\n#10 func_b at 0x5678"),
            "Frame number stripping should deduplicate recursive-variant crashes");
}

#[test]
fn test_fuzz_config_defaults() {
    let config = FuzzConfig::default();
    assert_eq!(config.exec_timeout_ms, 1000);
    assert_eq!(config.memory_limit_mb, 2048);
    assert_eq!(config.max_crashes, 100);
    assert_eq!(config.max_duration_secs, 3600);
    assert!(config.enable_deterministic);
    assert!(config.enable_havoc);
    assert!(config.enable_splice);
    assert!(config.target_path.is_empty());
}

#[test]
fn test_campaign_lifecycle() {
    let config = FuzzConfig {
        target_path: "/bin/true".into(),
        ..Default::default()
    };
    let dedup = DedupConfig::default();
    let mut ctrl = FuzzController::new(config, dedup);

    assert_eq!(ctrl.state(), CampaignState::Provisioning);
    ctrl.start().unwrap();
    assert_eq!(ctrl.state(), CampaignState::Running);
    ctrl.stop().unwrap();
    assert_eq!(ctrl.state(), CampaignState::Completed);
}

#[test]
fn test_record_crash() {
    let config = FuzzConfig { target_path: "/bin/true".into(), max_crashes: 5, ..Default::default() };
    let dedup = DedupConfig::default();
    let mut ctrl = FuzzController::new(config, dedup);
    ctrl.start().unwrap();

    let crash = ctrl.record_crash(
        b"AAAA".to_vec(),
        "#0 parse_input\n#1 main".to_string(),
        libc::SIGSEGV,
        "buffer overflow".to_string(),
        0x41414141,
    );
    assert!(crash.is_some());
    let c = crash.unwrap();
    assert_eq!(c.classification, CrashClassification::Segfault);
    assert_eq!(c.signal_name, "SIGSEGV");
    assert_eq!(c.input_size, 4);

    // Duplicate should return None
    let crash2 = ctrl.record_crash(
        b"BBBB".to_vec(),
        "#0 parse_input\n#1 main".to_string(),
        libc::SIGSEGV,
        "buffer overflow".to_string(),
        0x41414141,
    );
    assert!(crash2.is_none());
    assert_eq!(ctrl.stats().duplicate_crashes, 1);
}

#[test]
fn test_parse_fuzzer_stats() {
    let raw = r#"
start_time        : 1700000000
last_update       : 1700000060
fuzzer_pid        : 12345
cycles_done       : 42
execs_done        : 100000
execs_per_sec     : 1534.56
paths_total       : 50
paths_favored     : 10
paths_found       : 45
paths_imported    : 5
max_depth         : 12
cur_path          : 3
pending_favs      : 7
pending_total     : 15
variable_paths    : 8
stability         : 99.50%
bitmap_cvg        : 67.89%
unique_crashes    : 3
unique_hangs      : 2
last_path         : 1699999990
last_crash        : 1699999995
last_hang         : 1699999998
"#;
    let stats = parse_fuzzer_stats(raw).unwrap();
    assert_eq!(stats.total_execs, 100000);
    assert_eq!(stats.execs_per_sec, 1534.56);
    assert_eq!(stats.unique_crashes, 3);
    assert_eq!(stats.hangs, 2);
    assert_eq!(stats.corpus_entries, 50);
    assert_eq!(stats.corpus_favored, 10);
    assert_eq!(stats.cycles_done, 42);
    assert!((stats.coverage_pct - 67.89).abs() < 0.01);
    assert!((stats.stability_pct - 99.50).abs() < 0.01);
}

#[test]
fn test_build_afl_command() {
    let config = FuzzConfig {
        target_path: "/fuzz/target_bin".into(),
        target_args: vec!["--mode".into(), "@@".into()],
        exec_timeout_ms: 1000,
        memory_limit_mb: 2048,
        dictionary_path: Some("/fuzz/dict.txt".into()),
        ..Default::default()
    };
    let dedup = DedupConfig::default();
    let ctrl = FuzzController::new(config, dedup);

    let cmd = ctrl.build_afl_command("/corpus/in", "/corpus/out");
    assert!(cmd.contains(&"afl-fuzz".to_string()));
    assert!(cmd.contains(&"-i".to_string()));
    assert!(cmd.contains(&"-o".to_string()));
    assert!(cmd.contains(&"-t".to_string()));
    assert!(cmd.contains(&"-m".to_string()));
    assert!(cmd.contains(&"--".to_string()));
    assert!(cmd.contains(&"/fuzz/target_bin".to_string()));
    assert!(cmd.contains(&"-x".to_string()));
    assert!(cmd.contains(&"/fuzz/dict.txt".to_string()));
}

#[test]
fn test_stopping_condition_max_crashes() {
    let config = FuzzConfig { target_path: "/bin/true".into(), max_crashes: 3, max_duration_secs: 99999, ..Default::default() };
    let dedup = DedupConfig::default();
    let mut ctrl = FuzzController::new(config, dedup);
    ctrl.start().unwrap();

    // Record 3 crashes
    for i in 0..3 {
        ctrl.record_crash(
            vec![i as u8; 4],
            format!("#0 crash{}", i),
            libc::SIGSEGV, "crash".into(), 0x400000 + i as u64,
        );
    }
    assert!(ctrl.is_stopping_condition());
}

#[test]
fn test_signal_classification() {
    // Test through record_crash
    let config = FuzzConfig { target_path: "/bin/true".into(), ..Default::default() };
    let dedup = DedupConfig::default();
    let mut ctrl = FuzzController::new(config, dedup);
    ctrl.start().unwrap();

    let c = ctrl.record_crash(b"x".to_vec(), "#0 main".into(), libc::SIGABRT, "abort".into(), 0x1).unwrap();
    assert_eq!(c.classification, CrashClassification::Abort);

    let c = ctrl.record_crash(b"y".to_vec(), "#0 other".into(), libc::SIGFPE, "div0".into(), 0x2).unwrap();
    assert_eq!(c.classification, CrashClassification::ArithmeticException);
}

#[test]
fn test_campaign_pause_resume() {
    let config = FuzzConfig { target_path: "/bin/true".into(), ..Default::default() };
    let dedup = DedupConfig::default();
    let mut ctrl = FuzzController::new(config, dedup);
    ctrl.start().unwrap();
    ctrl.pause().unwrap();
    assert_eq!(ctrl.state(), CampaignState::Paused);
    ctrl.resume().unwrap();
    assert_eq!(ctrl.state(), CampaignState::Running);
}

#[test]
fn test_campaign_cancel() {
    let config = FuzzConfig { target_path: "/bin/true".into(), ..Default::default() };
    let dedup = DedupConfig::default();
    let mut ctrl = FuzzController::new(config, dedup);
    ctrl.start().unwrap();
    ctrl.cancel().unwrap();
    assert_eq!(ctrl.state(), CampaignState::Cancelled);
}

#[test]
fn test_campaign_fail() {
    let config = FuzzConfig { target_path: "/bin/true".into(), ..Default::default() };
    let dedup = DedupConfig::default();
    let mut ctrl = FuzzController::new(config, dedup);
    ctrl.start().unwrap();
    ctrl.fail("OOM in container").unwrap();
    assert_eq!(ctrl.state(), CampaignState::Failed);
}

// ── Phase 21D Danger Config Tests ──────────────────────────────────────────

#[test]
fn test_danger_config_defaults() {
    let config = bugswarm_sandbox::danger_map::DangerConfig::default();
    assert!(config.enabled);
    assert!((config.taint_weight - 0.7).abs() < 0.001);
    assert!((config.coverage_weight - 0.3).abs() < 0.001);
    assert!((config.decay_factor - 0.7).abs() < 0.001);
}

#[test]
fn test_danger_feed_creation() {
    let config = bugswarm_sandbox::danger_map::DangerConfig::default();
    let feed = bugswarm_sandbox::fuzzer::DangerFeed::new(true, config);
    assert!(feed.enabled);
    assert_eq!(feed.map.len(), 0);
    assert!(feed.danger_scores.is_empty());
}

#[test]
fn test_danger_feed_disabled() {
    let config = bugswarm_sandbox::danger_map::DangerConfig::default();
    let feed = bugswarm_sandbox::fuzzer::DangerFeed::new(false, config);
    assert!(!feed.enabled);
}

#[test]
fn test_danger_feed_load_map() {
    let config = bugswarm_sandbox::danger_map::DangerConfig::default();
    let mut feed = bugswarm_sandbox::fuzzer::DangerFeed::new(true, config);
    feed.load_map(vec![(0x1000, 0.5), (0x2000, 0.8), (0x3000, 1.0)]);
    assert_eq!(feed.map.len(), 3);
    assert!((feed.map.lookup(0x2000) - 0.8).abs() < 0.001);
}

#[test]
fn test_danger_feed_record_execution() {
    let config = bugswarm_sandbox::danger_map::DangerConfig::default();
    let mut feed = bugswarm_sandbox::fuzzer::DangerFeed::new(true, config);
    feed.load_map(vec![(0x1000, 0.5), (0x2000, 0.9)]);
    let score = feed.record_execution(0x1000, 0.3);
    assert!(score > 0.0);
    let stats = feed.stats();
    assert_eq!(stats.executions, 1);
    // Verify the danger feed recorded the execution and computed stats
    assert!(stats.avg_danger_score > 0.0, "avg_danger should be non-zero after execution");
}

#[test]
fn test_danger_feed_stats_empty() {
    let config = bugswarm_sandbox::danger_map::DangerConfig::default();
    let feed = bugswarm_sandbox::fuzzer::DangerFeed::new(true, config);
    let stats = feed.stats();
    assert_eq!(stats.executions, 0);
}

#[test]
fn test_danger_feed_stats_with_data() {
    let config = bugswarm_sandbox::danger_map::DangerConfig::default();
    let mut feed = bugswarm_sandbox::fuzzer::DangerFeed::new(true, config);
    feed.load_map(vec![
        (0x1000, 0.2), (0x2000, 0.5), (0x3000, 0.8), (0x4000, 1.0)
    ]);
    feed.record_execution(0x1000, 0.1);
    feed.record_execution(0x2000, 0.1);
    feed.record_execution(0x3000, 0.1);
    feed.record_execution(0x4000, 0.1);
    let stats = feed.stats();
    assert_eq!(stats.executions, 4);
    assert!(stats.danger_p50 > 0.0);
    assert!(stats.danger_p95 > stats.danger_p50);
    // Sink mutations: danger > 0.5 counts
    assert!(stats.sink_mutations >= 2); // 0.8 and 1.0 are > 0.5
}

#[test]
fn test_fuzz_config_danger_fields_default() {
    let config = FuzzConfig::default();
    assert!(!config.danger_map_enabled);
    assert!(config.danger_config.is_none());
    assert!(config.danger_map_shm_name.is_none());
}

#[test]
fn test_fuzz_config_danger_enabled() {
    let config = FuzzConfig {
        danger_map_enabled: true,
        danger_config: Some(bugswarm_sandbox::danger_map::DangerConfig::default()),
        danger_map_shm_name: Some("/bugswarm_danger_test".into()),
        ..Default::default()
    };
    assert!(config.danger_map_enabled);
    assert!(config.danger_config.is_some());
    assert_eq!(config.danger_map_shm_name, Some("/bugswarm_danger_test".into()));
}

#[test]
fn test_sandbox_config_danger_defaults() {
    let config = bugswarm_sandbox::config::SandboxConfig::default();
    assert!(!config.fuzz_danger_map_enabled);
    assert!((config.fuzz_danger_decay - 0.7).abs() < 0.001);
    assert!((config.fuzz_danger_taint_weight - 0.7).abs() < 0.001);
    assert!((config.fuzz_danger_coverage_weight - 0.3).abs() < 0.001);
}
