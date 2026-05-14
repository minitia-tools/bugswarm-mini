/// Fuzz Crucible Gate Test — ALL 8 attack vectors must pass.
/// This test is designed to BREAK the fuzzer implementation.

#[cfg(test)]
mod fuzz_crucible {
    use bugswarm_sandbox::fuzzer::*;

    #[test]
    fn av1_dedup_identical_traces() {
        // AV1: Two identical stack traces produce same hash
        let config = DedupConfig { normalize_addresses: true, ..Default::default() };
        let mut d1 = DedupEngine::new(config.clone());
        let mut d2 = DedupEngine::new(config);

        let trace = "#0 parse_input\n#1 main\n#2 _start";
        let hash1 = d1.normalize_stack_trace(trace);
        let hash2 = d2.normalize_stack_trace(trace);
        assert_eq!(hash1, hash2, "AV1 FAIL: Identical traces must produce identical hashes");
    }

    #[test]
    fn av2_dedup_aslr_resistance() {
        // AV2: Same crash at different addresses (ASLR) → same hash
        let config = DedupConfig { normalize_addresses: true, ..Default::default() };
        let mut dedup = DedupEngine::new(config);
        let h1 = dedup.normalize_stack_trace("#0 func at 0x7fff12345678");
        let h2 = dedup.normalize_stack_trace("#0 func at 0x7fff87654321");
        assert_eq!(h1, h2, "AV2 FAIL: ASLR-normalized traces must match");
    }

    #[test]
    fn av3_campaign_lifecycle_full() {
        // AV3: Full campaign lifecycle: Provisioning → Running → Completed
        let mut ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
        assert_eq!(ctrl.state(), CampaignState::Provisioning);
        ctrl.start().unwrap();
        assert_eq!(ctrl.state(), CampaignState::Running);
        ctrl.stop().unwrap();
        assert_eq!(ctrl.state(), CampaignState::Completed);
    }

    #[test]
    fn av4_stats_parse_complete() {
        // AV4: All stats fields parse correctly from AFL output
        let raw = "execs_done : 50000\nexecs_per_sec : 2000.5\nunique_crashes : 5\nunique_hangs : 3\ncorpus_count : 120\ncorpus_favored : 25\ncycles_done : 10\nbitmap_cvg : 75.0%\npending_favs : 8\npending_total : 30\nstability : 95.5%\nrun_time : 30\n";
        let stats = parse_fuzzer_stats(raw).unwrap();
        assert_eq!(stats.total_execs, 50000);
        assert_eq!(stats.execs_per_sec, 2000.5);
        assert_eq!(stats.unique_crashes, 5);
        assert_eq!(stats.corpus_entries, 120);
    }

    #[test]
    fn av5_crash_record_dedup_accuracy() {
        // AV5: Crash recording deduplicates correctly — 0 false duplicates
        let mut ctrl = FuzzController::new(
            FuzzConfig { target_path: "/bin/true".into(), ..Default::default() },
            DedupConfig::default(),
        );
        ctrl.start().unwrap();
        for i in 0..10 {
            let r = ctrl.record_crash(b"x".to_vec(), "#0 crash".into(), libc::SIGSEGV, "err".into(), i);
            if i == 0 { assert!(r.is_some(), "First crash must be unique"); }
            else { assert!(r.is_none(), "Duplicate {} must be None", i); }
        }
        assert_eq!(ctrl.stats().unique_crashes, 1);
        assert_eq!(ctrl.stats().duplicate_crashes, 9);
    }

    #[test]
    fn av6_max_crashes_stopping() {
        // AV6: Campaign stops at max_crashes threshold
        let mut ctrl = FuzzController::new(
            FuzzConfig { target_path: "/bin/true".into(), max_crashes: 5, max_duration_secs: 99999, ..Default::default() },
            DedupConfig::default(),
        );
        ctrl.start().unwrap();
        for i in 0..5 {
            ctrl.record_crash(b"x".to_vec(), format!("#0 crash_{}", i), libc::SIGSEGV, "e".into(), i as u64);
        }
        assert!(ctrl.is_stopping_condition(), "AV6 FAIL: Must stop after max_crashes");
    }

    #[test]
    fn av7_command_building_correctness() {
        // AV7: AFL++ command built correctly with all flags
        let config = FuzzConfig {
            target_path: "/target".into(),
            target_args: vec!["@@".into()],
            exec_timeout_ms: 2000,
            memory_limit_mb: 1024,
            dictionary_path: Some("/dict".into()),
            ..Default::default()
        };
        let ctrl = FuzzController::new(config, DedupConfig::default());
        let cmd = ctrl.build_afl_command("/in", "/out");
        assert!(cmd.contains(&"afl-fuzz".to_string()));
        assert!(cmd.contains(&"-t".to_string()));
        assert!(cmd.contains(&"2000".to_string()));
        assert!(cmd.contains(&"-m".to_string()));
        assert!(cmd.contains(&"1024".to_string()));
        assert!(cmd.contains(&"-x".to_string()));
        assert!(cmd.contains(&"/dict".to_string()));
    }

    #[test]
    fn av8_error_graceful_handling() {
        // AV8: All errors are proper types (no panics)
        let mut ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
        // Can't stop before starting
        assert!(ctrl.stop().is_err());
        // Can't pause before starting
        assert!(ctrl.pause().is_err());
        // Start, then valid transitions
        ctrl.start().unwrap();
        ctrl.pause().unwrap();
        // Can't stop while paused
        assert!(ctrl.stop().is_err());
        // Resume and stop
        ctrl.resume().unwrap();
        ctrl.stop().unwrap();
        assert!(ctrl.stop().is_err()); // Already stopped
    }
}
