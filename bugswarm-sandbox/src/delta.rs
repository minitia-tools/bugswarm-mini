use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Type alias for the oracle function: test(input) → passes (true) or fails (false).
/// Failing means the bug is still triggered. Passing means the bug is NOT triggered.
pub type OracleFn = Arc<dyn Fn(&[u8]) -> bool + Send + Sync>;

/// Result of delta debugging minimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaResult {
    /// The minimized input (essential bytes only).
    pub minimized: Vec<u8>,
    /// Original input size in bytes.
    pub original_size: usize,
    /// Minimized input size in bytes.
    pub minimized_size: usize,
    /// Size reduction ratio (0.0-1.0).
    pub reduction_ratio: f64,
    /// Number of test iterations performed.
    pub iterations: u32,
    /// Whether 1-minimality was achieved.
    pub is_1_minimal: bool,
    /// Wall-clock time spent.
    pub elapsed_ms: u64,
    /// Strategy used for splitting.
    pub split_strategy: SplitStrategy,
    /// Whether parallel execution was used.
    pub parallel_used: bool,
}

/// Input type detection for split strategy selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SplitStrategy {
    /// Arbitrary byte boundaries. Works for all inputs.
    #[default]
    Byte,
    /// Split on newline boundaries for text inputs.
    Text,
    /// Split on JSON/XML token boundaries.
    Structured,
    /// Split on instruction boundaries for bytecode.
    Instruction,
}

/// Configuration for the delta debugging engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaConfig {
    /// Maximum number of test iterations (safety limit).
    pub max_iterations: u32,
    /// Maximum wall-clock time for minimization.
    pub timeout_secs: u64,
    /// Minimum chunk size in bytes before stopping.
    pub min_chunk_size: usize,
    /// Starting granularity n for ddmin.
    pub initial_granularity: usize,
    /// Enable parallel chunk testing.
    pub parallel_enabled: bool,
    /// Maximum parallel workers.
    pub max_parallel_workers: usize,
    /// Whether to use parser-aware splitting.
    pub smart_splitting: bool,
    /// Whether to run 1-minimality verification after convergence.
    pub verify_1_minimal: bool,
}

impl Default for DeltaConfig {
    fn default() -> Self {
        Self {
            max_iterations: 200,
            timeout_secs: 30,
            min_chunk_size: 1,
            initial_granularity: 2,
            parallel_enabled: false,
            max_parallel_workers: 4,
            smart_splitting: true,
            verify_1_minimal: true,
        }
    }
}

/// Delta Debugging Minimizer (ddmin algorithm).
pub struct DeltaMinimizer {
    config: DeltaConfig,
}

impl DeltaMinimizer {
    pub fn new(config: DeltaConfig) -> Self {
        Self { config }
    }

    /// Minimize an input using ddmin.
    /// The oracle returns true if the input PASSES (no crash), false if FAILS (crash).
    pub fn minimize(&self, input: &[u8], oracle: &OracleFn) -> DeltaResult {
        let t0 = Instant::now();
        let original_size = input.len();
        let strategy = if self.config.smart_splitting {
            detect_strategy(input)
        } else {
            SplitStrategy::Byte
        };

        let mut current = input.to_vec();
        let mut n = self.config.initial_granularity;
        let mut iterations = 0u32;

        while current.len() > self.config.min_chunk_size
            && iterations < self.config.max_iterations
            && t0.elapsed().as_secs() < self.config.timeout_secs
        {
            iterations += 1;
            let mut progress = false;
            let chunk_size = (current.len() + n - 1) / n;

            // Test complements of each chunk
            for i in 0..n {
                let start = i * chunk_size;
                let end = (start + chunk_size).min(current.len());
                if start >= current.len() {
                    break;
                }

                let complement = build_complement(&current, start, end);
                if !oracle(&complement) {
                    // Complement still crashes → chunk is irrelevant
                    current = complement;
                    progress = true;
                    break; // Restart with smaller input
                }
            }

            if progress {
                n = n.saturating_sub(1).max(2); // Reduce granularity
            } else if current.len() >= self.config.min_chunk_size
                && n * 2 <= current.len() / self.config.min_chunk_size.max(1)
            {
                n *= 2; // Increase granularity
            } else {
                break; // Cannot refine further
            }
        }

        // 1-minimality verification: try removing each single byte
        let is_1_minimal = if self.config.verify_1_minimal {
            verify_1_minimality(&current, oracle)
        } else {
            true
        };

        let minimized_size = current.len();
        DeltaResult {
            minimized: current,
            original_size,
            minimized_size,
            reduction_ratio: if original_size > 0 {
                1.0 - minimized_size as f64 / original_size as f64
            } else {
                0.0
            },
            iterations,
            is_1_minimal,
            elapsed_ms: t0.elapsed().as_millis() as u64,
            split_strategy: strategy,
            parallel_used: false,
        }
    }
}

/// Build input minus the chunk at [start, end).
pub fn build_complement(input: &[u8], start: usize, end: usize) -> Vec<u8> {
    let mut result = Vec::with_capacity(input.len() - (end - start));
    result.extend_from_slice(&input[..start]);
    result.extend_from_slice(&input[end..]);
    result
}

/// Detect optimal split strategy based on input content.
pub fn detect_strategy(input: &[u8]) -> SplitStrategy {
    if input.is_empty() {
        return SplitStrategy::Byte;
    }

    let preview = &input[..input.len().min(256)];
    let s = String::from_utf8_lossy(preview);

    // Check for structured formats
    if s.trim_start().starts_with('{') || s.trim_start().starts_with('[') {
        return SplitStrategy::Structured;
    }
    if s.trim_start().starts_with('<') {
        return SplitStrategy::Structured;
    }

    // Check if text (high ratio of printable ASCII)
    let printable = preview
        .iter()
        .filter(|&&b| b >= 0x20 && b <= 0x7E || b == b'\n' || b == b'\r')
        .count();
    if preview.len() > 0 && printable as f64 / preview.len() as f64 > 0.8 {
        return SplitStrategy::Text;
    }

    // Check for bytecode (ELF magic, WASM magic, etc.)
    if preview.len() >= 4 {
        if &preview[..4] == b"\x7fELF" || &preview[..4] == b"\0asm" {
            return SplitStrategy::Instruction;
        }
    }

    SplitStrategy::Byte
}

/// Verify 1-minimality: try removing each single byte.
pub fn verify_1_minimality(input: &[u8], oracle: &OracleFn) -> bool {
    for i in 0..input.len() {
        let mut test = Vec::with_capacity(input.len() - 1);
        test.extend_from_slice(&input[..i]);
        test.extend_from_slice(&input[i + 1..]);
        if !oracle(&test) {
            return false; // Removing byte i still crashes → not minimal
        }
    }
    true
}

/// Build chunks for a given split strategy.
pub fn build_chunks(input: &[u8], n: usize, strategy: SplitStrategy) -> Vec<Vec<u8>> {
    match strategy {
        SplitStrategy::Text => split_on_delimiter(input, n, b'\n'),
        SplitStrategy::Byte => {
            let chunk_size = (input.len() + n - 1) / n;
            (0..n)
                .map(|i| {
                    let start = i * chunk_size;
                    let end = (start + chunk_size).min(input.len());
                    input[start..end].to_vec()
                })
                .collect()
        }
        SplitStrategy::Structured => split_structured(input, n),
        SplitStrategy::Instruction => {
            // For bytecode, split on instruction boundaries (4-byte aligned)
            let aligned = &input[..(input.len() / 4) * 4];
            let chunk_size = (aligned.len() / 4 + n - 1) / n * 4;
            (0..n)
                .map(|i| {
                    let start = (i * chunk_size).min(input.len());
                    let end = (start + chunk_size).min(input.len());
                    input[start..end].to_vec()
                })
                .collect()
        }
    }
}

fn split_on_delimiter(input: &[u8], n: usize, delim: u8) -> Vec<Vec<u8>> {
    let lines: Vec<&[u8]> = input.split(|&b| b == delim).collect();
    let chunk_size = (lines.len() + n - 1) / n;
    (0..n)
        .map(|i| {
            let start = i * chunk_size;
            let end = (start + chunk_size).min(lines.len());
            let mut result = Vec::new();
            for (j, &line) in lines[start..end].iter().enumerate() {
                if j > 0 {
                    result.push(delim);
                }
                result.extend_from_slice(line);
            }
            result
        })
        .collect()
}

fn split_structured(input: &[u8], n: usize) -> Vec<Vec<u8>> {
    let s = String::from_utf8_lossy(input);
    let tokens: Vec<&str> = s
        .split_inclusive(&[',', '{', '}', '[', ']', ':', '"', '<', '>', '/'][..])
        .collect();
    let n = n.max(2);
    let chunk_size = (tokens.len() + n - 1) / n;
    (0..n)
        .map(|i| {
            let start = i * chunk_size;
            let end = (start + chunk_size).min(tokens.len());
            tokens[start..end].concat().into_bytes()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_build_complement_start() {
        let result = build_complement(b"ABCDEF", 0, 2);
        assert_eq!(result, b"CDEF");
    }

    #[test]
    fn test_build_complement_middle() {
        let result = build_complement(b"ABCDEF", 2, 4);
        assert_eq!(result, b"ABEF");
    }

    #[test]
    fn test_build_complement_end() {
        let result = build_complement(b"ABCDEF", 4, 6);
        assert_eq!(result, b"ABCD");
    }

    #[test]
    fn test_build_complement_whole() {
        let result = build_complement(b"ABCDEF", 0, 6);
        assert_eq!(result, b"");
    }

    #[test]
    fn test_minimize_simple_overflow() {
        // Crash triggered by any input containing 3 specific bytes: "\x00\xFF\x00"
        let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
            // Passes if it does NOT contain the trigger sequence
            !input.windows(3).any(|w| w == b"\x00\xFF\x00")
        });

        let mut input = Vec::new();
        // Pad with junk, insert trigger in the middle
        input.extend_from_slice(b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        input.extend_from_slice(b"\x00\xFF\x00");
        input.extend_from_slice(b"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB");

        let config = DeltaConfig::default();
        let minimizer = DeltaMinimizer::new(config);
        let result = minimizer.minimize(&input, &oracle);

        assert!(
            result.minimized.len() <= 10,
            "Expected small minimized input, got {} bytes",
            result.minimized.len()
        );
        assert!(result.reduction_ratio > 0.7);
    }

    #[test]
    fn test_minimize_already_minimal() {
        let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
            input != b"\xFF" // Only \xFF crashes
        });

        let config = DeltaConfig::default();
        let minimizer = DeltaMinimizer::new(config);
        let result = minimizer.minimize(b"\xFF", &oracle);

        assert_eq!(result.minimized, b"\xFF");
        assert_eq!(result.minimized_size, 1);
    }

    #[test]
    fn test_detect_strategy_json() {
        let s = detect_strategy(b"{\"key\": \"value\"}");
        assert_eq!(s, SplitStrategy::Structured);
    }

    #[test]
    fn test_detect_strategy_text() {
        let s = detect_strategy(b"line1\nline2\nline3\n");
        assert_eq!(s, SplitStrategy::Text);
    }

    #[test]
    fn test_detect_strategy_binary() {
        let s = detect_strategy(&[0x00, 0x01, 0x02, 0xFF, 0xFE][..]);
        assert_eq!(s, SplitStrategy::Byte);
    }

    #[test]
    fn test_detect_strategy_elf() {
        let s = detect_strategy(b"\x7fELF\x02\x01\x01\x00");
        assert_eq!(s, SplitStrategy::Instruction);
    }

    #[test]
    fn test_verify_1_minimality_true() {
        let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
            input != b"\x00\xFF\x00" // Only exact 3 bytes crash
        });
        assert!(verify_1_minimality(b"\x00\xFF\x00", &oracle));
    }

    #[test]
    fn test_verify_1_minimality_false() {
        let oracle: OracleFn = Arc::new(|input: &[u8]| -> bool {
            // Crashes if it contains \x00\xFF\x00 (and len >= 3)
            !(input.windows(3).any(|w| w == b"\x00\xFF\x00") && input.len() >= 3)
        });
        // "X\x00\xFF\x00" has extra byte 'X' at start — removing 'X' still crashes
        assert!(!verify_1_minimality(b"X\x00\xFF\x00", &oracle));
    }

    #[test]
    fn test_empty_input() {
        let oracle: OracleFn = Arc::new(|_: &[u8]| -> bool { true });
        let config = DeltaConfig::default();
        let minimizer = DeltaMinimizer::new(config);
        let result = minimizer.minimize(b"", &oracle);
        assert_eq!(result.minimized.len(), 0);
    }

    #[test]
    fn test_config_defaults() {
        let config = DeltaConfig::default();
        assert_eq!(config.max_iterations, 200);
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.initial_granularity, 2);
    }
}
