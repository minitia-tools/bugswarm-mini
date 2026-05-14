// Phase 21E: Binary format compatibility tests between Rust DangerMap
// and the C danger_power_schedule.c plugin.
//
// Binary format (both sides agree):
//   - 4 bytes:  u32 count, little-endian
//   - 4 bytes:  u32 reserved (zeros)
//   - N × 16 bytes:  u64 addr (LE) + f32 score (LE) + u32 padding (zeros)

use bugswarm_sandbox::danger_map::DangerMap;
use std::io::Write;
use std::process::Command;

// ---------------------------------------------------------------------------
// Test 1: Binary Format Verification — byte-by-byte manual validation
// ---------------------------------------------------------------------------

#[test]
fn test_binary_format_offset_by_offset_5_entries() {
    let dm = DangerMap::from_pairs(vec![
        (0x1000, 0.5_f32),
        (0x2000, 0.75_f32),
        (0x3000, 1.0_f32),
        (0x4000, 0.125_f32),
        (0x5000, 0.0_f32),
    ]);
    let bytes = dm.to_bytes();

    // Header: 8 bytes total (4 count + 4 reserved)
    assert_eq!(bytes.len(), 8 + 5 * 16); // 8 + 80 = 88
    assert_eq!(&bytes[0..4], &5u32.to_le_bytes());
    assert_eq!(&bytes[4..8], &[0u8, 0, 0, 0]); // reserved zeros

    // Entry 0 at offset 8
    let off = 8;
    assert_eq!(&bytes[off..off + 8], &0x1000u64.to_le_bytes(), "entry 0 addr");
    assert_eq!(&bytes[off + 8..off + 12], &0.5_f32.to_le_bytes(), "entry 0 score");
    assert_eq!(&bytes[off + 12..off + 16], &[0u8, 0, 0, 0], "entry 0 padding");

    // Entry 1 at offset 24
    let off = 8 + 16;
    assert_eq!(&bytes[off..off + 8], &0x2000u64.to_le_bytes(), "entry 1 addr");
    assert_eq!(&bytes[off + 8..off + 12], &0.75_f32.to_le_bytes(), "entry 1 score");
    assert_eq!(&bytes[off + 12..off + 16], &[0u8, 0, 0, 0], "entry 1 padding");

    // Entry 2 at offset 40
    let off = 8 + 32;
    assert_eq!(&bytes[off..off + 8], &0x3000u64.to_le_bytes(), "entry 2 addr");
    assert_eq!(&bytes[off + 8..off + 12], &1.0_f32.to_le_bytes(), "entry 2 score");

    // Entry 3 at offset 56
    let off = 8 + 48;
    assert_eq!(&bytes[off..off + 8], &0x4000u64.to_le_bytes(), "entry 3 addr");
    assert_eq!(&bytes[off + 8..off + 12], &0.125_f32.to_le_bytes(), "entry 3 score");

    // Entry 4 at offset 72
    let off = 8 + 64;
    assert_eq!(&bytes[off..off + 8], &0x5000u64.to_le_bytes(), "entry 4 addr");
    assert_eq!(&bytes[off + 8..off + 12], &0.0_f32.to_le_bytes(), "entry 4 score");
}

#[test]
fn test_binary_format_empty_map() {
    let dm = DangerMap::new();
    let bytes = dm.to_bytes();
    assert_eq!(bytes.len(), 8);
    assert_eq!(&bytes[0..4], &0u32.to_le_bytes(), "count must be 0");
    assert_eq!(&bytes[4..8], &[0u8, 0, 0, 0], "reserved must be zero");
}

#[test]
fn test_binary_format_single_entry() {
    let dm = DangerMap::from_pairs(vec![(0xDEADBEEFCAFE, 0.42_f32)]);
    let bytes = dm.to_bytes();
    assert_eq!(bytes.len(), 8 + 16);
    assert_eq!(&bytes[0..4], &1u32.to_le_bytes());
    assert_eq!(&bytes[8..16], &0xDEADBEEFCAFE_u64.to_le_bytes());
    assert_eq!(&bytes[16..20], &0.42_f32.to_le_bytes());
}

// ---------------------------------------------------------------------------
// Test 2: Endianness — explicitly verify little-endian encoding on x86_64
// ---------------------------------------------------------------------------

#[test]
fn test_little_endian_u32_count() {
    let dm = DangerMap::from_pairs(vec![(0x1000, 0.1), (0x2000, 0.2)]);
    let bytes = dm.to_bytes();
    // On little-endian, byte 0 is LSB of the count value
    assert_eq!(bytes[0], 2, "LSB of count=2 must be 2");
    assert_eq!(bytes[1], 0);
    assert_eq!(bytes[2], 0);
    assert_eq!(bytes[3], 0);
}

#[test]
fn test_little_endian_u64_addr() {
    let addr: u64 = 0x0102030405060708;
    let dm = DangerMap::from_pairs(vec![(addr, 0.5)]);
    let bytes = dm.to_bytes();
    let addr_bytes = &bytes[8..16];
    assert_eq!(addr_bytes[0], 0x08, "byte 0 (LSB)");
    assert_eq!(addr_bytes[1], 0x07);
    assert_eq!(addr_bytes[2], 0x06);
    assert_eq!(addr_bytes[3], 0x05);
    assert_eq!(addr_bytes[4], 0x04);
    assert_eq!(addr_bytes[5], 0x03);
    assert_eq!(addr_bytes[6], 0x02);
    assert_eq!(addr_bytes[7], 0x01, "byte 7 (MSB)");
}

#[test]
fn test_little_endian_u64_max() {
    let dm = DangerMap::from_pairs(vec![(u64::MAX, 1.0)]);
    let bytes = dm.to_bytes();
    assert_eq!(&bytes[8..16], &[0xFFu8; 8]);
}

#[test]
fn test_little_endian_f32_score() {
    let dm = DangerMap::from_pairs(vec![(0x1000, 1.0_f32)]);
    let bytes = dm.to_bytes();
    // 1.0f32 in IEEE 754 LE = 0x3F800000 → bytes: 00 00 80 3F
    let expected = 1.0_f32.to_le_bytes();
    assert_eq!(&bytes[16..20], &expected);
    assert_eq!(bytes[16], 0x00);
    assert_eq!(bytes[17], 0x00);
    assert_eq!(bytes[18], 0x80);
    assert_eq!(bytes[19], 0x3F);
}

// ---------------------------------------------------------------------------
// Test 3: Round-Trip Through File (uses tempfile)
// ---------------------------------------------------------------------------

fn roundtrip_via_file(dm: &DangerMap) {
    let bytes = dm.to_bytes();
    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&bytes).expect("write to temp file");
    tmp.flush().expect("flush temp file");

    let buf = std::fs::read(tmp.path()).expect("read temp file");
    let dm2 = DangerMap::from_bytes(&buf).expect("from_bytes should succeed");

    assert_eq!(dm2.pairs.len(), dm.pairs.len(), "entry count mismatch");
    for (i, ((a1, s1), (a2, s2))) in dm.pairs.iter().zip(dm2.pairs.iter()).enumerate() {
        assert_eq!(a1, a2, "addr mismatch at index {}", i);
        let diff = (*s1 - *s2).abs();
        assert!(diff < 1e-6, "score mismatch at index {}: {} vs {}, diff={}", i, s1, s2, diff);
    }
}

#[test]
fn test_roundtrip_zero_entries() {
    roundtrip_via_file(&DangerMap::new());
}

#[test]
fn test_roundtrip_one_entry() {
    roundtrip_via_file(&DangerMap::from_pairs(vec![(0x1234, 0.5)]));
}

#[test]
fn test_roundtrip_100_entries() {
    let pairs: Vec<(u64, f32)> = (0..100)
        .map(|i| (i as u64 * 0x1000, (i as f32) / 100.0))
        .collect();
    roundtrip_via_file(&DangerMap::from_pairs(pairs));
}

#[test]
fn test_roundtrip_1000_entries() {
    let pairs: Vec<(u64, f32)> = (0..1000)
        .map(|i| (i as u64 * 0x100, (i as f32) / 1000.0))
        .collect();
    roundtrip_via_file(&DangerMap::from_pairs(pairs));
}

#[test]
fn test_roundtrip_negative_and_extreme_scores() {
    let dm = DangerMap::from_pairs(vec![
        (0x1000, -0.0_f32),
        (0x2000, -1.0_f32),
        (0x3000, 3.4028235e38_f32),   // near f32::MAX
        (0x4000, -3.4028235e38_f32),  // near f32::MIN
        (0x5000, 1.1754944e-38_f32),  // near f32::MIN_POSITIVE
    ]);
    roundtrip_via_file(&dm);
}

// ---------------------------------------------------------------------------
// Test 4: Corrupted Input
// ---------------------------------------------------------------------------

#[test]
fn test_corrupted_truncated_empty() {
    assert!(DangerMap::from_bytes(&[]).is_err());
}

#[test]
fn test_corrupted_truncated_byte_3() {
    assert!(DangerMap::from_bytes(&[0u8; 3]).is_err());
    assert!(DangerMap::from_bytes(&[0u8; 7]).is_err());
}

#[test]
fn test_corrupted_count_zero_valid() {
    // count=0 is valid and produces an empty map
    let mut bytes = vec![0u8; 8];
    bytes[0..4].copy_from_slice(&0u32.to_le_bytes());
    let dm = DangerMap::from_bytes(&bytes).expect("count=0 should be valid");
    assert_eq!(dm.pairs.len(), 0);
    assert!(dm.is_empty());
}

#[test]
fn test_corrupted_wrong_count_says_5_has_2() {
    // Header says 5 entries but only 2 entries worth of data provided
    let mut bytes = vec![0u8; 8 + 2 * 16]; // 40 bytes: header + 2 entries
    bytes[0..4].copy_from_slice(&5u32.to_le_bytes());
    assert!(DangerMap::from_bytes(&bytes).is_err());
}

#[test]
fn test_corrupted_wrong_count_says_3_has_0() {
    // Header says 3 entries, only header present
    let mut bytes = vec![0u8; 8];
    bytes[0..4].copy_from_slice(&3u32.to_le_bytes());
    assert!(DangerMap::from_bytes(&bytes).is_err());
}

#[test]
fn test_corrupted_count_u32_max() {
    // count=u32::MAX should fail because data length is insufficient
    let mut bytes = vec![0u8; 8];
    bytes[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
    let result = DangerMap::from_bytes(&bytes);
    assert!(result.is_err(), "u32::MAX count should fail (no way to have enough data)");
}

#[test]
fn test_corrupted_count_large_but_not_max() {
    // count = 100_000_000 should also fail gracefully
    let mut bytes = vec![0u8; 8];
    bytes[0..4].copy_from_slice(&100_000_000u32.to_le_bytes());
    assert!(DangerMap::from_bytes(&bytes).is_err());
}

#[test]
fn test_corrupted_no_padding_between_entries() {
    // Build valid binary and then truncate to miss the last 4 padding bytes
    let dm = DangerMap::from_pairs(vec![(0x1000, 0.5)]);
    let mut bytes = dm.to_bytes();
    // Valid state: give the right count but trim the padding
    // Actually, from_bytes doesn't validate padding, just reads 16 bytes per entry.
    // So even without padding bytes, it would error because data is too short.
    bytes.truncate(bytes.len() - 3); // cut into the padding
    assert!(DangerMap::from_bytes(&bytes).is_err());
}

// ---------------------------------------------------------------------------
// NaN and Infinity float values — these should serialize/deserialize fine
// since we're doing raw byte copies, not arithmetic
// ---------------------------------------------------------------------------

#[test]
fn test_nan_score_roundtrip() {
    let dm = DangerMap::from_pairs(vec![(0x1000, f32::NAN)]);
    let bytes = dm.to_bytes();
    let dm2 = DangerMap::from_bytes(&bytes).expect("NAN should deserialize");
    let score = dm2.pairs[0].1;
    assert!(score.is_nan(), "score should be NaN after roundtrip");
    assert_eq!(dm2.pairs[0].0, 0x1000);
}

#[test]
fn test_infinity_score_roundtrip() {
    let dm = DangerMap::from_pairs(vec![
        (0x1000, f32::INFINITY),
        (0x2000, f32::NEG_INFINITY),
    ]);
    let bytes = dm.to_bytes();
    let dm2 = DangerMap::from_bytes(&bytes).expect("infinity should deserialize");
    assert!(dm2.pairs[0].1.is_infinite());
    assert!(dm2.pairs[0].1.is_sign_positive());
    assert!(dm2.pairs[1].1.is_infinite());
    assert!(dm2.pairs[1].1.is_sign_negative());
}

// ---------------------------------------------------------------------------
// Test 5: C Plugin Loader Compatibility
// ---------------------------------------------------------------------------

const C_TEST_SOURCE: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

typedef struct __attribute__((packed)) {
    uint64_t address;
    float    score;
    uint32_t _pad;
} DangerEntry;

static uint8_t *g_map_data = NULL;
static uint32_t g_entry_count = 0;

/* Binary search with floor semantics — exact copy from danger_power_schedule.c */
float lookup_danger(uint64_t addr) {
    if (!g_map_data || g_entry_count == 0) return 0.0f;
    const DangerEntry *entries = (const DangerEntry *)(g_map_data + 8);
    int32_t lo = 0, hi = (int32_t)g_entry_count - 1;
    while (lo <= hi) {
        int32_t mid = lo + (hi - lo) / 2;
        if (entries[mid].address == addr) return entries[mid].score;
        if (entries[mid].address < addr) lo = mid + 1;
        else                             hi = mid - 1;
    }
    if (hi >= 0) return entries[hi].score;
    return 0.0f;
}

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: %s <binary-file>\n", argv[0]); return 1; }
    FILE *f = fopen(argv[1], "rb");
    if (!f) { perror("fopen"); return 1; }
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    g_map_data = (uint8_t *)malloc((size_t)sz);
    fread(g_map_data, 1, (size_t)sz, f);
    fclose(f);

    memcpy(&g_entry_count, g_map_data, 4);
    printf("ENTRIES=%u\n", g_entry_count);

    const DangerEntry *entries = (const DangerEntry *)(g_map_data + 8);
    for (uint32_t i = 0; i < g_entry_count; i++) {
        printf("ENTRY[%u]=addr=%lu score=%.8f\n", i,
               (unsigned long)entries[i].address, (double)entries[i].score);
    }

    /* Verify lookup_danger with floor semantics */
    if (g_entry_count > 0) {
        printf("LOOKUP_0x0000=%.8f\n", (double)lookup_danger(0x0000));
        printf("LOOKUP_0x%04lx=%.8f\n", (unsigned long)entries[0].address,
               (double)lookup_danger(entries[0].address));
        if (g_entry_count >= 2) {
            uint64_t between = entries[0].address + (entries[1].address - entries[0].address) / 2;
            printf("LOOKUP_BETWEEN=%.8f\n", (double)lookup_danger(between));
        }
        uint64_t above = entries[g_entry_count - 1].address + 0x10000;
        printf("LOOKUP_ABOVE=%.8f\n", (double)lookup_danger(above));
    }
    printf("LOOKUP_EMPTY=%.8f\n", (double)lookup_danger(0x9999));

    free(g_map_data);
    return 0;
}
"#;

/// Compile the C test harness, feed it DangerMap binary from Rust, verify
/// the C side parses the same entries.
fn run_c_compat_test(dm: &DangerMap, test_label: &str) {
    // Write Rust DangerMap to a temp binary file
    let bytes = dm.to_bytes();
    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&bytes).expect("write binary");
    tmp.flush().expect("flush");

    // Write C source to a temp file with .c extension so gcc recognizes it
    let mut c_src = tempfile::Builder::new()
        .suffix(".c")
        .tempfile()
        .expect("create C source temp file");
    c_src.write_all(C_TEST_SOURCE.as_bytes()).expect("write C source");
    c_src.flush().expect("flush");

    // Compile the C program; output to a temp path
    let bin_path = format!("{}.bin", c_src.path().to_string_lossy());
    let compile = Command::new("gcc")
        .args([
            "-O0",
            "-o",
            &bin_path,
            c_src.path().to_str().unwrap(),
            "-lm",
        ])
        .output()
        .expect("gcc must be available");

    if !compile.status.success() {
        let stderr = String::from_utf8_lossy(&compile.stderr);
        panic!(
            "[{}] Failed to compile C test harness:\n{}",
            test_label, stderr
        );
    }

    // Run the C binary, pass the DangerMap binary file as argument
    let run = Command::new(&bin_path)
        .arg(tmp.path().to_str().unwrap())
        .output()
        .expect("failed to run C test harness");

    // Clean up
    let _ = std::fs::remove_file(&bin_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    if !run.status.success() {
        let stderr = String::from_utf8_lossy(&run.stderr);
        panic!(
            "[{}] C harness exited with error:\nstdout:\n{}\nstderr:\n{}",
            test_label, stdout, stderr
        );
    }

    // Parse C output and verify against Rust expectations
    let lines: Vec<&str> = stdout.lines().collect();

    // Verify entry count
    let count_str = lines
        .iter()
        .find(|l| l.starts_with("ENTRIES="))
        .expect("missing ENTRIES line");
    let c_count: usize = count_str
        .strip_prefix("ENTRIES=")
        .unwrap()
        .parse()
        .expect("parse count");
    assert_eq!(c_count, dm.pairs.len(), "[{}] entry count mismatch", test_label);

    // Verify each entry
    for (i, &(expected_addr, expected_score)) in dm.pairs.iter().enumerate() {
        let prefix = format!("ENTRY[{}]=", i);
        let entry_line = lines
            .iter()
            .find(|l| l.starts_with(&prefix))
            .unwrap_or_else(|| panic!("[{}] missing line for entry {}", test_label, i));

        // Parse "ENTRY[i]=addr=N score=N.NNNNN"
        let rest = entry_line.strip_prefix(&prefix).unwrap();
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        let addr_part = parts[0].strip_prefix("addr=").expect("addr= prefix");
        let c_addr: u64 = addr_part.parse().expect("parse addr");
        assert_eq!(c_addr, expected_addr, "[{}] entry {} addr mismatch", test_label, i);

        if parts.len() >= 2 {
            let score_part = parts[1].strip_prefix("score=").expect("score= prefix");
            let c_score: f64 = score_part.parse().expect("parse score");
            let diff = (c_score as f32 - expected_score).abs();
            assert!(diff < 1e-6, "[{}] entry {} score mismatch: C={}, Rust={}", test_label, i, c_score, expected_score);
        }
    }

    // Verify lookup_danger results if we have entries
    if !dm.pairs.is_empty() {
        // Check lookup for an address below all entries
        let below_line = lines
            .iter()
            .find(|l| l.starts_with("LOOKUP_0x0000="))
            .expect("missing LOOKUP_0x0000");
        let below_score: f64 = below_line.strip_prefix("LOOKUP_0x0000=").unwrap().parse().unwrap();
        assert!((below_score as f32 - 0.0f32).abs() < 1e-6,
            "[{}] lookup below all should be 0.0, got {}", test_label, below_score);

        // Check exact match for first entry
        let first_addr = dm.pairs[0].0;
        let first_prefix = format!("LOOKUP_0x{:04x}=", first_addr);
        let first_line = lines
            .iter()
            .find(|l| l.starts_with(&first_prefix))
            .unwrap_or_else(|| panic!("[{}] missing LOOKUP for addr 0x{:x}", test_label, first_addr));
        let c_score: f64 = first_line
            .strip_prefix(&first_prefix)
            .unwrap()
            .parse()
            .unwrap();
        let expected = dm.pairs[0].1 as f64;
        let diff = (c_score - expected).abs();
        assert!(diff < 1e-6, "[{}] lookup exact mismatch: C={}, expected={}", test_label, c_score, expected);

        // Check above all entries (unless last address is near u64::MAX and +0x10000 wraps)
        let above_line = lines
            .iter()
            .find(|l| l.starts_with("LOOKUP_ABOVE="))
            .expect("missing LOOKUP_ABOVE");
        let above_score: f64 = above_line.strip_prefix("LOOKUP_ABOVE=").unwrap().parse().unwrap();
        let last_addr = dm.pairs.last().unwrap().0;
        let above_addr = last_addr.wrapping_add(0x10000); // same as C code
        let rust_floor_score = dm.pairs.iter()
            .rev()
            .find(|&&(a, _)| a <= above_addr)
            .map(|&(_, s)| s)
            .unwrap_or(0.0);
        let expected_above = rust_floor_score as f64;
        let diff = (above_score - expected_above).abs();
        assert!(diff < 1e-6, "[{}] lookup above={} should match Rust floor={}: C={}, expected={}",
            test_label, above_addr, expected_above, above_score, expected_above);
    }

    // LOOKUP_EMPTY should always be 0.0 since g_map_data is freed before that call
    // (in the C harness we free after printing all lookups)
}

#[test]
fn test_c_compat_empty_map() {
    run_c_compat_test(&DangerMap::new(), "empty");
}

#[test]
fn test_c_compat_3_entries() {
    run_c_compat_test(
        &DangerMap::from_pairs(vec![
            (0x1000, 0.25),
            (0x2000, 0.75),
            (0x3000, 1.0),
        ]),
        "3_entries",
    );
}

#[test]
fn test_c_compat_5_entries_sorted() {
    run_c_compat_test(
        &DangerMap::from_pairs(vec![
            (0x0A00, 0.1),
            (0x0B00, 0.3),
            (0x0C00, 0.5),
            (0x1000, 0.7),
            (0xFFFF_FFFF, 0.999),
        ]),
        "5_sorted",
    );
}

#[test]
fn test_c_compat_large_addresses() {
    run_c_compat_test(
        &DangerMap::from_pairs(vec![
            (0x7FFF_FFFF_FFFF_FFFF, 0.5),
            (0xFFFF_FFFF_FFFF_FFFF, 1.0),
        ]),
        "large_addrs",
    );
}

#[test]
fn test_c_compat_single_entry() {
    run_c_compat_test(
        &DangerMap::from_pairs(vec![(0xDEADBEEF, 0.42)]),
        "single",
    );
}

// ---------------------------------------------------------------------------
// Test: C plugin struct size matches Rust's entry size
// ---------------------------------------------------------------------------

#[test]
fn test_c_struct_size_matches() {
    // Compile a tiny C program that prints sizeof(DangerEntry)
    let sizeof_src = r#"
#include <stdint.h>
#include <stdio.h>
typedef struct __attribute__((packed)) {
    uint64_t address;
    float    score;
    uint32_t _pad;
} DangerEntry;
int main(void) {
    printf("%zu\n", sizeof(DangerEntry));
    return 0;
}
"#;
    let mut c_src = tempfile::Builder::new()
        .suffix(".c")
        .tempfile()
        .expect("tempfile");
    c_src.write_all(sizeof_src.as_bytes()).expect("write");
    c_src.flush().expect("flush");

    let bin_path = format!("{}.sizeof_bin", c_src.path().to_string_lossy());
    let compile = Command::new("gcc")
        .args(["-O0", "-o", &bin_path, c_src.path().to_str().unwrap()])
        .output()
        .expect("compile gcc");
    if !compile.status.success() {
        panic!("gcc failed: {}", String::from_utf8_lossy(&compile.stderr));
    }

    let output = Command::new(&bin_path).output().expect("run");
    let _ = std::fs::remove_file(&bin_path);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let c_sizeof: usize = stdout.trim().parse().expect("parse sizeof");
    assert_eq!(c_sizeof, 16, "C DangerEntry must be 16 bytes (packed)");
}

// ---------------------------------------------------------------------------
// Test: verify DangerConfig JSON roundtrip doesn't affect binary format
// ---------------------------------------------------------------------------

#[test]
fn test_danger_config_does_not_alter_binary() {
    // DangerConfig is a separate concept, but verify it's not in to_bytes()
    let dm = DangerMap::from_pairs(vec![(0x1000, 0.5)]);
    let bytes = dm.to_bytes();
    // Should only contain count + reserved + entries, no config fields
    assert_eq!(bytes.len(), 8 + 16); // no extra bytes for config
}
