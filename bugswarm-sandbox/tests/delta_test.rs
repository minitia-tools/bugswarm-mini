use bugswarm_sandbox::delta::*;
use std::sync::Arc;
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════════════
// Category 1: Core ddmin Correctness
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_ddmin_100byte_3_essential_reduced_to_10_or_less() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(3).any(|w| w == b"\x00\xFF\x00")
    });
    let mut input = Vec::with_capacity(100);
    input.extend_from_slice(b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    input.extend_from_slice(b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    input.extend_from_slice(b"\x00\xFF\x00");
    input.extend_from_slice(b"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB");

    let config = DeltaConfig {
        verify_1_minimal: true,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(&input, &oracle);

    assert!(
        result.minimized.len() <= 10,
        "Expected minimized ≤10 bytes, got {} bytes: {:?}",
        result.minimized.len(),
        String::from_utf8_lossy(&result.minimized)
    );
    assert!(result.reduction_ratio > 0.7);
    assert!(result.is_1_minimal);
    assert!(result.iterations > 0);
}

#[test]
fn test_already_minimal_input_unchanged() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        input != b"\xFF"
    });
    let config = DeltaConfig::default();
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(b"\xFF", &oracle);

    assert_eq!(result.minimized, b"\xFF");
    assert_eq!(result.minimized_size, 1);
    assert_eq!(result.original_size, 1);
    assert_eq!(result.reduction_ratio, 0.0);
    assert!(result.is_1_minimal);
}

#[test]
fn test_always_passes_no_progress_made() {
    let oracle: OracleFn = Arc::new(|_: &[u8]| -> bool { true });
    let config = DeltaConfig {
        max_iterations: 500,
        min_chunk_size: 1,
        initial_granularity: 2,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let input = b"IRRELEVANT_DATA";
    let result = minimizer.minimize(input, &oracle);

    assert_eq!(result.original_size, input.len());
    assert_eq!(result.minimized.len(), input.len());
    assert_eq!(result.minimized, input);
    assert!(result.iterations > 0);
    assert_eq!(result.reduction_ratio, 0.0);
}

#[test]
fn test_all_bytes_essential_handled_correctly() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !(input.contains(&0xAA) && input.contains(&0xBB) && input.contains(&0xCC))
    });
    let config = DeltaConfig::default();
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(b"\xAA\xBB\xCC", &oracle);

    assert_eq!(result.minimized, b"\xAA\xBB\xCC");
    assert_eq!(result.minimized_size, 3);
    assert_eq!(result.reduction_ratio, 0.0);
    assert!(result.is_1_minimal);
}

#[test]
fn test_1_minimality_verification_works() {
    let oracle_always_fails: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !(input.contains(&0x01) && input.contains(&0x02) && input.contains(&0x03))
    });

    assert!(verify_1_minimality(b"\x01\x02\x03", &oracle_always_fails));

    let oracle_extra: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(3).any(|w| w == b"\xDE\xAD\xBE")
    });
    assert!(!verify_1_minimality(b"X\xDE\xAD\xBE", &oracle_extra),
        "Extra byte X before trigger should make it not 1-minimal");
    assert!(verify_1_minimality(b"\xDE\xAD\xBE", &oracle_extra));

    let mut padded = Vec::new();
    padded.extend_from_slice(b"\xDE\xAD\xBE");
    padded.push(b'Z');
    assert!(!verify_1_minimality(&padded, &oracle_extra),
        "Trailing byte Z after trigger should make it not 1-minimal");
}

#[test]
fn test_reduction_ratio_computation() {
    let oracle_pass: OracleFn = Arc::new(|_: &[u8]| -> bool { true });
    let config = DeltaConfig {
        max_iterations: 500,
        min_chunk_size: 1,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);

    let r100 = minimizer.minimize(&[0u8; 100], &oracle_pass);
    assert_eq!(r100.original_size, 100);
    assert_eq!(r100.minimized_size, 100);
    assert_eq!(r100.reduction_ratio, 0.0);

    let oracle_exact: OracleFn = Arc::new(|input: &[u8]| -> bool { input != b"\xFF" });
    let r1 = minimizer.minimize(b"\xFF", &oracle_exact);
    assert_eq!(r1.reduction_ratio, 0.0);

    let oracle_partial: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !(input.contains(&0xAB) && input.contains(&0xCD))
    });
    let mut input = vec![0xAB, 0xCD];
    input.extend_from_slice(&[0x00; 98]);
    let r_partial = minimizer.minimize(&input, &oracle_partial);
    assert!(r_partial.original_size >= 100);
    assert!(r_partial.reduction_ratio > 0.5, "Should reduce >50%, got {}", r_partial.reduction_ratio);
    assert!(r_partial.reduction_ratio <= 1.0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Category 2: Split Strategies
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_byte_strategy_on_random_binary() {
    let input: Vec<u8> = (0..64).map(|i| (i * 37 + 13) as u8).collect();
    let chunks = build_chunks(&input, 4, SplitStrategy::Byte);

    assert_eq!(chunks.len(), 4);
    assert_eq!(chunks[0].len(), 16);
    assert_eq!(chunks[1].len(), 16);
    assert_eq!(chunks[2].len(), 16);
    assert_eq!(chunks[3].len(), 16);

    let total: usize = chunks.iter().map(|c| c.len()).sum();
    assert_eq!(total, input.len());

    let concatenated: Vec<u8> = chunks.iter().flatten().copied().collect();
    assert_eq!(&concatenated, &input);

    let single = build_chunks(&input, 1, SplitStrategy::Byte);
    assert_eq!(single.len(), 1);
    assert_eq!(single[0], input);
}

#[test]
fn test_text_strategy_on_newline_delimited() {
    let input = b"line1\nline2\nline3\nline4\nline5\nline6";
    let chunks = build_chunks(input, 3, SplitStrategy::Text);

    assert_eq!(chunks.len(), 3);
    assert!(!chunks.iter().all(|c| c.is_empty()));

    // Verify first chunk contains line1 and line2
    let first = String::from_utf8_lossy(&chunks[0]);
    assert!(first.contains("line1"));
    assert!(first.contains("line2"));

    // Verify middle chunk contains line3 and line4
    let middle = String::from_utf8_lossy(&chunks[1]);
    assert!(middle.contains("line3"));
    assert!(middle.contains("line4"));

    // Verify last chunk contains line5 and line6
    let last = String::from_utf8_lossy(&chunks[2]);
    assert!(last.contains("line5"));
    assert!(last.contains("line6"));
}

#[test]
fn test_structured_strategy_on_json() {
    let input = b"{\"key1\":\"val1\",\"key2\":[1,2,3],\"key3\":true}";
    let chunks = build_chunks(input, 4, SplitStrategy::Structured);

    assert_eq!(chunks.len(), 4);
    let total_len: usize = chunks.iter().map(|c| c.len()).sum();
    assert!(total_len <= input.len());
    assert!(!chunks.iter().all(|c| c.is_empty()));
}

#[test]
fn test_detect_strategy_identifies_each_type() {
    assert_eq!(detect_strategy(b""), SplitStrategy::Byte);

    assert_eq!(detect_strategy(b"{\"a\":1}"), SplitStrategy::Structured);
    assert_eq!(detect_strategy(b"[1,2,3]"), SplitStrategy::Structured);
    assert_eq!(detect_strategy(b"<root><child/></root>"), SplitStrategy::Structured);

    assert_eq!(detect_strategy(b"line1\nline2\nline3\n"), SplitStrategy::Text);
    assert_eq!(detect_strategy(b"hello world from rust"), SplitStrategy::Text);

    assert_eq!(detect_strategy(b"\x7fELF\x02\x01\x01\x00"), SplitStrategy::Instruction);
    assert_eq!(detect_strategy(b"\0asm\x01\x00\x00\x00"), SplitStrategy::Instruction);

    let binary: Vec<u8> = (0..16).map(|i| (i * 19) as u8).collect();
    assert_eq!(detect_strategy(&binary), SplitStrategy::Byte);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Category 3: Config / Edge Cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_max_iterations_limit_respected() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(3).any(|w| w == b"\xAB\xCD\xEF")
    });
    let mut input = vec![0u8; 500];
    input[200] = 0xAB;
    input[201] = 0xCD;
    input[202] = 0xEF;

    let config = DeltaConfig {
        max_iterations: 10,
        min_chunk_size: 1,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(&input, &oracle);

    assert!(result.iterations <= 10, "iterations {} > 10 limit", result.iterations);
    assert!(result.minimized.len() <= input.len());
}

#[test]
fn test_timeout_respected() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(4).any(|w| w == b"CRSH")
    });
    let mut input = vec![b'A'; 2000];
    input[1000..1004].copy_from_slice(b"CRSH");

    let config = DeltaConfig {
        timeout_secs: 1,
        max_iterations: 10_000,
        min_chunk_size: 1,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(&input, &oracle);

    assert!(result.elapsed_ms < 2000, "Timeout 1s but took {}ms", result.elapsed_ms);
    assert!(result.iterations < 10_000);
}

#[test]
fn test_min_chunk_size_stops_refinement() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(2).any(|w| w == b"\xAA\xBB")
    });
    let mut input = vec![0x00u8; 100];
    input[50] = 0xAA;
    input[51] = 0xBB;

    let config_small = DeltaConfig {
        min_chunk_size: 1,
        max_iterations: 500,
        ..Default::default()
    };
    let minimizer_small = DeltaMinimizer::new(config_small);
    let r_small = minimizer_small.minimize(&input, &oracle);

    let config_big = DeltaConfig {
        min_chunk_size: 20,
        ..Default::default()
    };
    let minimizer_big = DeltaMinimizer::new(config_big);
    let r_big = minimizer_big.minimize(&input, &oracle);

    assert!(
        r_small.minimized_size <= r_big.minimized_size,
        "Small min_chunk_size should produce <= result vs big min_chunk_size"
    );
    assert!(r_big.minimized_size >= 2); // at least the 2 essential bytes remain
}

#[test]
fn test_empty_input_returns_empty_result() {
    let oracle: OracleFn = Arc::new(|_: &[u8]| -> bool { false });
    let config = DeltaConfig::default();
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(b"", &oracle);

    assert_eq!(result.minimized.len(), 0);
    assert_eq!(result.original_size, 0);
    assert_eq!(result.minimized_size, 0);
    assert_eq!(result.reduction_ratio, 0.0);
    assert_eq!(result.iterations, 0);
}

#[test]
fn test_one_byte_input() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        input != b"\x01"
    });
    let config = DeltaConfig::default();
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(b"\x01", &oracle);

    assert_eq!(result.minimized, b"\x01");
    assert_eq!(result.original_size, 1);
    assert!(result.is_1_minimal);

    let oracle_pass: OracleFn = Arc::new(|_: &[u8]| -> bool { true });
    let result2 = minimizer.minimize(b"\x01", &oracle_pass);

    assert_eq!(result2.minimized, b"\x01");
    assert_eq!(result2.reduction_ratio, 0.0);
    assert_eq!(result2.original_size, 1);
}

#[test]
fn test_large_input_10kb_performance() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(4).any(|w| w == b"TARG")
    });
    let mut input = vec![b'A'; 10000];
    input[5000..5004].copy_from_slice(b"TARG");

    let config = DeltaConfig {
        max_iterations: 300,
        timeout_secs: 10,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let t0 = std::time::Instant::now();
    let result = minimizer.minimize(&input, &oracle);

    assert!(result.minimized.len() <= 10, "10KB with 4 essential bytes should reduce to ≤10 bytes");
    assert!(result.minimized.len() >= 4);
    assert!(result.reduction_ratio > 0.9);
    assert!(t0.elapsed() < Duration::from_secs(10), "Performance: 10KB input took too long");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Category 4: Complement Building
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_complement_from_start() {
    let result = build_complement(b"ABCDEFGHIJ", 0, 3);
    assert_eq!(result, b"DEFGHIJ");
}

#[test]
fn test_complement_from_middle() {
    let result = build_complement(b"ABCDEFGHIJ", 3, 6);
    assert_eq!(result, b"ABCGHIJ");
}

#[test]
fn test_complement_from_end() {
    let result = build_complement(b"ABCDEFGHIJ", 7, 10);
    assert_eq!(result, b"ABCDEFG");
}

#[test]
fn test_complement_entire_input_empty() {
    let result = build_complement(b"ABCDEF", 0, 6);
    assert_eq!(result, b"");
    assert!(result.is_empty());
}

#[test]
fn test_complement_zero_length_chunk() {
    let result = build_complement(b"ABCDEF", 3, 3);
    assert_eq!(result, b"ABCDEF");
    assert_eq!(result.len(), 6);

    let result2 = build_complement(b"XYZ", 0, 0);
    assert_eq!(result2, b"XYZ");

    let result3 = build_complement(b"XYZ", 3, 3);
    assert_eq!(result3, b"XYZ");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Category 5: Gate Attack Vectors
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn av1_ddmin_padded_500byte_3_essential_reduces() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(3).any(|w| w == b"KEY")
    });
    let mut input = vec![b'P'; 500];
    input[250..253].copy_from_slice(b"KEY");

    let config = DeltaConfig::default();
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(&input, &oracle);

    assert!(result.minimized.len() <= 10);
    assert!(result.reduction_ratio > 0.95);
    assert!(result.is_1_minimal);
}

#[test]
fn av2_already_1_minimal_unchanged() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        input != b"\x01\x02\x03\x04"
    });
    let config = DeltaConfig {
        verify_1_minimal: true,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(b"\x01\x02\x03\x04", &oracle);

    assert_eq!(result.minimized, b"\x01\x02\x03\x04");
    assert!(result.is_1_minimal);
    assert_eq!(result.reduction_ratio, 0.0);
}

#[test]
fn av3_empty_input_no_crash() {
    let oracle: OracleFn = Arc::new(|_: &[u8]| -> bool { false });
    let config = DeltaConfig::default();
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(b"", &oracle);

    assert_eq!(result.original_size, 0);
    assert_eq!(result.minimized_size, 0);
    assert_eq!(result.iterations, 0);
    assert_eq!(result.elapsed_ms, 0, "Empty input should process in <1ms (got {}ms)", result.elapsed_ms);

    let oracle_true: OracleFn = Arc::new(|_: &[u8]| -> bool { true });
    let result2 = minimizer.minimize(b"", &oracle_true);
    assert_eq!(result2.minimized_size, 0);
}

#[test]
fn av4_structured_json_detected_and_minimized() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        // Crash if input contains the substring "CRASH_HERE"
        let s = String::from_utf8_lossy(input);
        !s.contains("CRASH_HERE")
    });

    let json = b"{\"padding\":\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\",\"key\":\"CRASH_HERE\",\"more\":\"BBBBBBBBBBBBBBBB\"}";
    let config = DeltaConfig {
        smart_splitting: true,
        verify_1_minimal: true,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(json, &oracle);

    assert_eq!(result.split_strategy, SplitStrategy::Structured);
    assert!(result.minimized.len() < json.len());
    assert!(result.reduction_ratio > 0.2);
    assert!(result.iterations > 0);
}

#[test]
fn av5_text_newlines_correctly_split() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(7).any(|w| w == b"PANIC!!")
    });
    let text = b"line1\nline2\nline3\nPANIC!!\nline5\nline6\nline7\n";
    let config = DeltaConfig {
        smart_splitting: true,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(text, &oracle);

    assert_eq!(result.split_strategy, SplitStrategy::Text);
    assert!(result.minimized.len() <= 20);
    assert!(result.reduction_ratio > 0.5);
}

#[test]
fn av6_max_iterations_10_stops_early() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(2).any(|w| w == b"xx")
    });
    let mut input = vec![b'y'; 1000];
    input[500] = b'x';
    input[501] = b'x';

    let config = DeltaConfig {
        max_iterations: 10,
        min_chunk_size: 1,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(&input, &oracle);

    assert!(result.iterations <= 10);
    assert!(!result.is_1_minimal || result.iterations < 200,
        "With only 10 iterations, should either not be 1-minimal or exit early");
}

#[test]
fn av7_timeout_1s_stops_early() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(3).any(|w| w == b"BUG")
    });
    let mut input = vec![b'F'; 5000];
    input[2500..2503].copy_from_slice(b"BUG");

    let config = DeltaConfig {
        timeout_secs: 1,
        max_iterations: 100_000,
        min_chunk_size: 1,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(&input, &oracle);

    assert!(result.elapsed_ms < 2000,
        "Expected <2s with 1s timeout, got {}ms", result.elapsed_ms);
    assert!(result.iterations < 100_000);
}

#[test]
fn av8_result_fields_populated_and_consistent() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(4).any(|w| w == b"CRIT")
    });
    let mut input = vec![b'-'; 200];
    input[100..104].copy_from_slice(b"CRIT");

    let config = DeltaConfig {
        verify_1_minimal: true,
        smart_splitting: true,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(&input, &oracle);

    assert_eq!(result.original_size, 200);
    assert!(!result.minimized.is_empty());
    assert!(result.minimized_size <= 200);
    assert!((result.reduction_ratio - (1.0 - result.minimized_size as f64 / result.original_size as f64)).abs() < 0.001);
    assert!(result.iterations > 0);
    assert_eq!(result.is_1_minimal, true);
    assert!(result.elapsed_ms < 5000);
    assert!(!result.parallel_used);
    assert!(matches!(result.split_strategy, SplitStrategy::Byte | SplitStrategy::Text | SplitStrategy::Structured | SplitStrategy::Instruction));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Category 6: Integration — Oracle Function Correctness
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_oracle_crash_on_specific_byte_sequence() {
    let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
        !input.windows(4).any(|w| w == b"DEAD")
    });
    let mut input = vec![b'\x00'; 64];
    input[30..34].copy_from_slice(b"DEAD");

    let config = DeltaConfig::default();
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(&input, &oracle);

    assert!(result.minimized.len() <= 8);
    assert!(result.reduction_ratio > 0.8);
    assert!(!oracle(&result.minimized), "Minimized input must still crash the oracle");
}

#[test]
fn test_oracle_never_crashes() {
    let oracle: OracleFn = Arc::new(|_: &[u8]| -> bool { true });
    let config = DeltaConfig {
        max_iterations: 500,
        min_chunk_size: 1,
        ..Default::default()
    };
    let minimizer = DeltaMinimizer::new(config);
    let input = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let result = minimizer.minimize(input, &oracle);

    assert_eq!(result.minimized, input);
    assert_eq!(result.original_size, 26);
    assert!(oracle(&result.minimized), "Output input should still pass the oracle");
}

#[test]
fn test_oracle_crash_on_exact_match_only() {
    let target: Vec<u8> = b"\xAB\xCD\xEF\x01".to_vec();
    let target_clone = target.clone();
    let oracle: OracleFn = Arc::new(move |input: &[u8]| -> bool {
        input != target_clone.as_slice()
    });
    let config = DeltaConfig::default();
    let minimizer = DeltaMinimizer::new(config);
    let result = minimizer.minimize(&target, &oracle);

    assert_eq!(result.minimized, target);
    assert_eq!(result.reduction_ratio, 0.0);
    assert!(result.is_1_minimal);
    assert!(!oracle(&result.minimized), "Output must still crash the oracle");

    let target_for_lax = target.clone();
    let oracle_lax: OracleFn = Arc::new(move |input: &[u8]| -> bool {
        !input.windows(4).any(|w| w == target_for_lax.as_slice())
    });
    let padded: Vec<u8> = {
        let mut v = vec![0u8; 50];
        v.extend_from_slice(&target);
        v.extend_from_slice(&[0u8; 50]);
        v
    };
    let result2 = minimizer.minimize(&padded, &oracle_lax);
    assert!(result2.minimized.len() <= 8);
    assert!(!oracle_lax(&result2.minimized), "Reduced input must still trigger crash");
}
