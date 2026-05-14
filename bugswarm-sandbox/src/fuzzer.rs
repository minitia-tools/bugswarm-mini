use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Unique identifier for a fuzzing campaign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CampaignId(pub Uuid);

/// Hash algorithm selection for deduplication (currently only SHA-256 is
/// wired; the field is retained for future configuration transport).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HashAlgorithm {
    #[default]
    Sha256,
    Blake3,
    Xxh3,
}

/// Configuration for a single fuzzing campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FuzzConfig {
    /// Path to the target binary inside the container.
    pub target_path: String,
    /// Arguments passed to the target binary (use `@@` for file input).
    pub target_args: Vec<String>,
    /// Timeout per execution in milliseconds.
    pub exec_timeout_ms: u64,
    /// Memory limit for the target in MB.
    pub memory_limit_mb: u64,
    /// Maximum number of *unique* crashes before stopping.
    pub max_crashes: u64,
    /// Maximum campaign duration in seconds.
    pub max_duration_secs: u64,
    /// Optional path to an AFL dictionary file.
    pub dictionary_path: Option<String>,
    /// Optional directory containing seed corpus inputs.
    pub seed_corpus_dir: Option<String>,
    /// Number of parallel AFL instances (auto-detected by default).
    pub parallel_instances: u32,
    /// Enable deterministic fuzzing stage.
    pub enable_deterministic: bool,
    /// Enable havoc stage.
    pub enable_havoc: bool,
    /// Enable splice stage.
    pub enable_splice: bool,
    /// Additional environment variables injected into the container.
    pub env_vars: HashMap<String, String>,
    /// Enable taint-guided prioritization via danger map.
    #[serde(default)]
    pub danger_map_enabled: bool,
    /// Configuration for blending danger scores into the power schedule.
    #[serde(default)]
    pub danger_config: Option<crate::danger_map::DangerConfig>,
    /// Shared memory name for the danger map segment.
    #[serde(default)]
    pub danger_map_shm_name: Option<String>,
}

impl Default for FuzzConfig {
    fn default() -> Self {
        Self {
            target_path: String::new(),
            target_args: Vec::new(),
            exec_timeout_ms: 1000,
            memory_limit_mb: 2048,
            max_crashes: 100,
            max_duration_secs: 3600,
            dictionary_path: None,
            seed_corpus_dir: None,
            parallel_instances: num_cpus::get() as u32,
            enable_deterministic: true,
            enable_havoc: true,
            enable_splice: true,
            env_vars: HashMap::new(),
            danger_map_enabled: false,
            danger_config: None,
            danger_map_shm_name: None,
        }
    }
}

/// High-level lifecycle state of a fuzzing campaign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CampaignState {
    Provisioning,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

/// Real-time statistics collected from a running AFL++ instance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CampaignStats {
    /// Total number of executions.
    pub total_execs: u64,
    /// Current executions per second.
    pub execs_per_sec: f64,
    /// Number of unique crashes discovered so far.
    pub unique_crashes: u64,
    /// Number of duplicate crashes that were filtered.
    pub duplicate_crashes: u64,
    /// Number of hangs observed.
    pub hangs: u64,
    /// Total number of corpus entries.
    pub corpus_entries: u64,
    /// Number of favoured corpus entries.
    pub corpus_favored: u64,
    /// Number of edges found.
    pub edges_found: u64,
    /// Total number of edges in the map (if known).
    pub edges_total: u64,
    /// Current bitmap coverage percentage.
    pub coverage_pct: f64,
    /// Number of completed fuzzing cycles.
    pub cycles_done: u64,
    /// Elapsed seconds since campaign start.
    pub elapsed_secs: u64,
    /// Number of pending favoured paths.
    pub pending_favs: u64,
    /// Total number of pending paths.
    pub pending_total: u64,
    /// AFL stability percentage.
    pub stability_pct: f64,
    /// Number of mutations that reached a security-sensitive sink (danger > 0.5).
    pub sink_mutations_count: u64,
    /// Rolling average of danger scores across all unique crashes.
    pub avg_danger_score: f32,
}

/// High-level crash classification derived from the terminating signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrashClassification {
    Segfault,
    Abort,
    IllegalInstruction,
    ArithmeticException,
    BusError,
    NonZeroExit(i32),
    Timeout,
    Unknown,
}

/// A single deduplicated crash artefact produced during a campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzCrash {
    /// Unique identifier of this crash.
    pub crash_id: Uuid,
    /// Campaign that produced the crash.
    pub campaign_id: CampaignId,
    /// SHA-256 of the normalized stack trace (used for deduplication).
    pub stack_hash: String,
    /// The input that triggered the crash.
    pub crashing_input: Vec<u8>,
    /// Byte-length of the crashing input.
    pub input_size: usize,
    /// POSIX signal number that terminated the process.
    pub signal: i32,
    /// Human-readable signal name.
    pub signal_name: String,
    /// Raw, unnormalized stack trace captured from the target.
    pub stack_trace: String,
    /// Standard error output captured during the crash.
    pub stderr_output: String,
    /// Automatic classification based on the terminating signal.
    pub classification: CrashClassification,
    /// Address where the crash occurred (if known).
    pub crash_address: u64,
    /// UTC timestamp when the crash was first discovered.
    pub discovered_at: DateTime<Utc>,
    /// Filesystem path where the crash artefact is stored.
    pub artifact_path: String,
}

/// Deduplication configuration controlling how stack-traces are normalized.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DedupConfig {
    /// Which hash algorithm to use for fingerprinting.
    pub hash_algorithm: HashAlgorithm,
    /// Maximum number of stack frames to consider.
    pub max_stack_frames: usize,
    /// Replace hex addresses (0x…) with a constant placeholder.
    pub normalize_addresses: bool,
    /// Strip leading frame numbers (`#0`, `#1`, …) from each line.
    pub ignore_frame_numbers: bool,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            hash_algorithm: HashAlgorithm::default(),
            max_stack_frames: 30,
            normalize_addresses: true,
            ignore_frame_numbers: true,
        }
    }
}

/// Request payload to start (or resume) a fuzzing campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzRequest {
    pub config: FuzzConfig,
    pub resume_from: Option<CampaignId>,
    pub dedup_config: Option<DedupConfig>,
}

/// Response returned immediately after accepting a fuzz request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzResponse {
    pub campaign_id: CampaignId,
    pub state: CampaignState,
    pub stream_id: String,
}

/// Periodic status update pushed to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzStatusUpdate {
    pub campaign_id: CampaignId,
    pub state: CampaignState,
    pub stats: CampaignStats,
    pub new_crashes: Vec<FuzzCrash>,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error conditions that may arise during fuzzing operations.
#[derive(Debug, thiserror::Error)]
pub enum FuzzerError {
    #[error("AFL++ binary not found at {0}")]
    AflNotFound(String),

    #[error("Target binary not found at {0}")]
    TargetNotFound(String),

    #[error("Campaign {0} is not running (state: {1:?})")]
    CampaignNotRunning(CampaignId, CampaignState),

    #[error("Container error: {0}")]
    ContainerError(String),

    #[error("Stats parse error: {0}")]
    StatsParseError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, FuzzerError>;

// ---------------------------------------------------------------------------
// CampaignId display
// ---------------------------------------------------------------------------

impl std::fmt::Display for CampaignId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// DedupEngine
// ---------------------------------------------------------------------------

/// Stack-trace deduplication engine that normalizes, hashes, and tracks
/// unique crash fingerprints.
pub struct DedupEngine {
    config: DedupConfig,
    seen_hashes: HashSet<String>,
    crash_count: u64,
}

impl DedupEngine {
    /// Create a new deduplication engine with the supplied configuration.
    pub fn new(config: DedupConfig) -> Self {
        Self {
            config,
            seen_hashes: HashSet::new(),
            crash_count: 0,
        }
    }

    /// Normalize a raw stack trace according to the engine's configuration.
    ///
    /// Applies address stripping, frame-number removal, and frame-count
    /// truncation in that order.
    pub fn normalize_stack_trace(&self, raw: &str) -> String {
        let mut result = raw.to_string();

        if self.config.normalize_addresses {
            result = Self::strip_addresses(&result);
        }
        if self.config.ignore_frame_numbers {
            result = Self::strip_frame_numbers(&result);
        }
        result = Self::take_top_frames(&result, self.config.max_stack_frames);

        result
    }

    /// Check whether the given stack trace represents a new unique crash.
    ///
    /// Returns `true` if the normalized hash has not been seen before (and
    /// therefore represents a genuinely new crash). Returns `false` for
    /// duplicates.
    pub fn is_unique(&mut self, stack_trace: &str) -> bool {
        let normalized = self.normalize_stack_trace(stack_trace);
        let hash = Self::compute_sha256(normalized.as_bytes());

        if self.seen_hashes.insert(hash) {
            self.crash_count += 1;
            true
        } else {
            false
        }
    }

    /// Total number of unique crashes processed.
    pub fn crash_count(&self) -> u64 {
        self.crash_count
    }

    // -- private helpers ----------------------------------------------------

    fn compute_sha256(data: &[u8]) -> String {
        format!("{:x}", Sha256::digest(data))
    }

    fn strip_addresses(text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < chars.len() {
            if i + 2 < chars.len() && chars[i] == '0' && chars[i + 1] == 'x' {
                out.push_str("0x0");
                i += 2;
                while i < chars.len() && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        out
    }

    fn strip_frame_numbers(text: &str) -> String {
        text.lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if let Some(stripped) = trimmed.strip_prefix('#') {
                    if let Some(space_idx) = stripped.find(|c: char| c.is_whitespace()) {
                        return stripped[space_idx..].trim_start().to_string();
                    }
                }
                line.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn take_top_frames(text: &str, max: usize) -> String {
        if max == 0 {
            return String::new();
        }
        text.lines().take(max).collect::<Vec<_>>().join("\n")
    }
}

// ---------------------------------------------------------------------------
// FuzzController
// ---------------------------------------------------------------------------

/// Core controller that orchestrates a single fuzzing campaign.
pub struct FuzzController {
    campaign_id: CampaignId,
    config: FuzzConfig,
    state: CampaignState,
    stats: CampaignStats,
    dedup: DedupEngine,
    crashes: Vec<FuzzCrash>,
    started_at: DateTime<Utc>,
    danger_map: crate::danger_map::DangerMap,
    danger_config: crate::danger_map::DangerConfig,
}

impl FuzzController {
    /// Create a new controller, generate a fresh campaign ID, and enter
    /// the `Provisioning` state.
    pub fn new(config: FuzzConfig, dedup_config: DedupConfig) -> Self {
        let campaign_id = CampaignId(Uuid::new_v4());
        let danger_config = config.danger_config.clone().unwrap_or_default();
        Self {
            campaign_id,
            config,
            state: CampaignState::Provisioning,
            stats: CampaignStats {
                total_execs: 0,
                execs_per_sec: 0.0,
                unique_crashes: 0,
                duplicate_crashes: 0,
                hangs: 0,
                corpus_entries: 0,
                corpus_favored: 0,
                edges_found: 0,
                edges_total: 0,
                coverage_pct: 0.0,
                cycles_done: 0,
                elapsed_secs: 0,
                pending_favs: 0,
                pending_total: 0,
                stability_pct: 0.0,
                sink_mutations_count: 0,
                avg_danger_score: 0.0,
            },
            dedup: DedupEngine::new(dedup_config),
            crashes: Vec::new(),
            started_at: Utc::now(),
            danger_map: crate::danger_map::DangerMap::new(),
            danger_config,
        }
    }

    /// Transition the campaign into `Running` state.
    pub fn start(&mut self) -> Result<()> {
        self.state = CampaignState::Running;
        log::info!("Campaign {} started", self.campaign_id);
        Ok(())
    }

    /// Replace the current statistics snapshot with an updated one (typically
    /// parsed from the AFL++ `fuzzer_stats` file).
    pub fn update_stats(&mut self, new_stats: CampaignStats) {
        self.stats = new_stats;
    }

    /// Record a potential crash.
    ///
    /// If the crash is unique (per deduplication rules) a `FuzzCrash` struct
    /// is created, appended to the internal crash list, and returned.
    /// Duplicate crashes silently increment the duplicate counter.
    pub fn record_crash(
        &mut self,
        input: Vec<u8>,
        stack_trace: String,
        signal: i32,
        stderr: String,
        crash_addr: u64,
    ) -> Option<FuzzCrash> {
        let danger_score = self.danger_map.lookup(crash_addr);

        if self.dedup.is_unique(&stack_trace) {
            let normalized = self.dedup.normalize_stack_trace(&stack_trace);
            let stack_hash = format!("{:x}", Sha256::digest(normalized.as_bytes()));
            let input_size = input.len();
            let crash_id = Uuid::new_v4();

            let classification = match signal {
                11 => CrashClassification::Segfault,
                6 => CrashClassification::Abort,
                4 => CrashClassification::IllegalInstruction,
                8 => CrashClassification::ArithmeticException,
                7 => CrashClassification::BusError,
                _ => CrashClassification::Unknown,
            };

            let signal_name = match signal {
                11 => "SIGSEGV".to_string(),
                6 => "SIGABRT".to_string(),
                4 => "SIGILL".to_string(),
                8 => "SIGFPE".to_string(),
                7 => "SIGBUS".to_string(),
                other => format!("SIGNAL({})", other),
            };

            let crash = FuzzCrash {
                crash_id,
                campaign_id: self.campaign_id,
                stack_hash,
                crashing_input: input,
                input_size,
                signal,
                signal_name,
                stack_trace,
                stderr_output: stderr,
                classification,
                crash_address: crash_addr,
                discovered_at: Utc::now(),
                artifact_path: format!("crashes/{}", crash_id),
            };

            if danger_score > 0.5 {
                self.stats.sink_mutations_count += 1;
            }
            let n_before = self.stats.unique_crashes as f32;
            self.stats.avg_danger_score = if n_before > 0.0 {
                (self.stats.avg_danger_score * n_before + danger_score) / (n_before + 1.0)
            } else {
                danger_score
            };

            self.stats.unique_crashes += 1;
            self.crashes.push(crash.clone());
            Some(crash)
        } else {
            self.stats.duplicate_crashes += 1;
            None
        }
    }

    /// Load (or replace) the danger map from raw (address, score) pairs.
    /// The scores are automatically normalized.
    pub fn load_danger_map(&mut self, pairs: Vec<(u64, f32)>) {
        let mut map = crate::danger_map::DangerMap::from_pairs(pairs);
        map.normalize();
        self.danger_map = map;
    }

    /// Compute a combined power score from coverage rarity and the danger
    /// score at `address`.
    pub fn compute_power_score(&self, address: u64, coverage_rarity: f32) -> f32 {
        let danger_score = self.danger_map.lookup(address);
        self.danger_config
            .compute_power_schedule(coverage_rarity, danger_score)
    }

    /// Mark the campaign as `Completed` (normal termination).
    pub fn stop(&mut self) -> Result<()> {
        if self.state != CampaignState::Running {
            return Err(FuzzerError::CampaignNotRunning(self.campaign_id, self.state));
        }
        self.state = CampaignState::Completed;
        log::info!("Campaign {} completed", self.campaign_id);
        Ok(())
    }

    /// Mark the campaign as `Cancelled` (user-requested termination).
    pub fn cancel(&mut self) -> Result<()> {
        self.state = CampaignState::Cancelled;
        log::info!("Campaign {} cancelled", self.campaign_id);
        Ok(())
    }

    /// Mark the campaign as `Failed` with a human-readable reason.
    pub fn fail(&mut self, reason: &str) -> Result<()> {
        self.state = CampaignState::Failed;
        log::error!("Campaign {} failed: {}", self.campaign_id, reason);
        Ok(())
    }

    /// Returns `true` when any stop condition has been met (max crashes
    /// reached, or campaign duration exceeded).
    pub fn is_stopping_condition(&self) -> bool {
        self.stats.unique_crashes >= self.config.max_crashes
            || self.stats.elapsed_secs >= self.config.max_duration_secs
    }

    /// Build the `afl-fuzz` command-line arguments that will be executed
    /// inside the container.
    pub fn generate_afl_command(&self) -> Vec<String> {
        let mut cmd: Vec<String> = Vec::new();

        cmd.push("afl-fuzz".to_string());

        // Input directory
        let input_dir = self
            .config
            .seed_corpus_dir
            .as_deref()
            .unwrap_or("/corpus/in");
        cmd.push("-i".to_string());
        cmd.push(input_dir.to_string());

        // Output directory
        cmd.push("-o".to_string());
        cmd.push("/corpus/out".to_string());

        // Timeout
        cmd.push("-t".to_string());
        cmd.push(format!("{}", self.config.exec_timeout_ms));

        // Memory limit
        cmd.push("-m".to_string());
        cmd.push(format!("{}", self.config.memory_limit_mb));

        // Dictionary
        if let Some(ref dict) = self.config.dictionary_path {
            cmd.push("-x".to_string());
            cmd.push(dict.clone());
        }

        // Deterministic fuzzing
        if !self.config.enable_deterministic {
            cmd.push("-d".to_string());
        }

        // Separator
        cmd.push("--".to_string());

        // Target binary
        cmd.push(self.config.target_path.clone());

        // Target arguments
        for arg in &self.config.target_args {
            cmd.push(arg.clone());
        }

        cmd
    }

    /// Return the campaign identifier.
    pub fn campaign_id(&self) -> CampaignId {
        self.campaign_id
    }

    /// Return the current campaign state.
    pub fn state(&self) -> CampaignState {
        self.state
    }

    /// Return a reference to the current statistics snapshot.
    pub fn stats(&self) -> &CampaignStats {
        &self.stats
    }

    /// Return the list of unique crashes discovered so far.
    pub fn crashes(&self) -> &[FuzzCrash] {
        &self.crashes
    }

    /// Produce a response describing the current state of the campaign.
    pub fn to_response(&self, stream_id: String) -> FuzzResponse {
        FuzzResponse {
            campaign_id: self.campaign_id,
            state: self.state,
            stream_id,
        }
    }

    /// Alias for generate_afl_command (test compatibility).
    pub fn build_afl_command(&self, _input_dir: &str, _output_dir: &str) -> Vec<String> {
        self.generate_afl_command()
    }

    /// Pause an active campaign (state Running → Paused).
    pub fn pause(&mut self) -> Result<()> {
        if self.state != CampaignState::Running {
            return Err(FuzzerError::CampaignNotRunning(self.campaign_id, self.state));
        }
        self.state = CampaignState::Paused;
        Ok(())
    }

    /// Resume a paused campaign (state Paused → Running).
    pub fn resume(&mut self) -> Result<()> {
        if self.state != CampaignState::Paused {
            return Err(FuzzerError::CampaignNotRunning(self.campaign_id, self.state));
        }
        self.state = CampaignState::Running;
        Ok(())
    }
}

/// Free function wrapper for AFLStatsParser::parse_fuzzer_stats.
pub fn parse_fuzzer_stats(raw: &str) -> Result<CampaignStats> {
    AFLStatsParser::parse_fuzzer_stats(raw)
}

// ---------------------------------------------------------------------------
// AFLStatsParser
// ---------------------------------------------------------------------------

/// Parses the AFL++ `fuzzer_stats` key-value file format and converts it
/// into a [`CampaignStats`] snapshot.
pub struct AFLStatsParser;

impl AFLStatsParser {
    /// Parse raw `fuzzer_stats` content into a [`CampaignStats`] struct.
    ///
    /// Unknown keys are silently ignored. Missing numeric fields default to
    /// zero.
    pub fn parse_fuzzer_stats(raw: &str) -> Result<CampaignStats> {
        let mut start_time: Option<u64> = None;
        let mut last_update: Option<u64> = None;
        let mut execs_done: Option<u64> = None;
        let mut execs_per_sec: Option<f64> = None;
        let mut paths_total: Option<u64> = None;
        let mut paths_favored: Option<u64> = None;
        let mut cycles_done: Option<u64> = None;
        let mut bitmap_cvg: Option<f64> = None;
        let mut unique_crashes: Option<u64> = None;
        let mut unique_hangs: Option<u64> = None;
        let mut pending_favs: Option<u64> = None;
        let mut pending_total: Option<u64> = None;
        let mut stability: Option<f64> = None;

        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let (key, value) = match line.split_once(':') {
                Some((k, v)) => (k.trim(), v.trim()),
                None => continue,
            };

            match key {
                "start_time" => start_time = parse_u64(value),
                "last_update" => last_update = parse_u64(value),
                "execs_done" => execs_done = parse_u64(value),
                "execs_per_sec" => execs_per_sec = parse_f64(value),
                "paths_total" | "corpus_count" => paths_total = parse_u64(value),
                "paths_favored" | "corpus_favored" => paths_favored = parse_u64(value),
                "cycles_done" => cycles_done = parse_u64(value),
                "bitmap_cvg" => bitmap_cvg = parse_percentage(value),
                "unique_crashes" => unique_crashes = parse_u64(value),
                "unique_hangs" => unique_hangs = parse_u64(value),
                "pending_favs" => pending_favs = parse_u64(value),
                "pending_total" => pending_total = parse_u64(value),
                "stability" => stability = parse_percentage(value),
                "run_time" => {
                    if let Some(secs) = parse_u64(value) {
                        if start_time.is_none() && last_update.is_none() {
                            last_update = Some(secs);
                            start_time = Some(0);
                        }
                    }
                }
                _ => {}
            }
        }

        let elapsed_secs = match (start_time, last_update) {
            (Some(start), Some(last)) => last.saturating_sub(start),
            _ => 0,
        };

        Ok(CampaignStats {
            total_execs: execs_done.unwrap_or(0),
            execs_per_sec: execs_per_sec.unwrap_or(0.0),
            unique_crashes: unique_crashes.unwrap_or(0),
            duplicate_crashes: 0,
            hangs: unique_hangs.unwrap_or(0),
            corpus_entries: paths_total.unwrap_or(0),
            corpus_favored: paths_favored.unwrap_or(0),
            edges_found: 0,
            edges_total: 0,
            coverage_pct: bitmap_cvg.unwrap_or(0.0),
            cycles_done: cycles_done.unwrap_or(0),
            elapsed_secs,
            pending_favs: pending_favs.unwrap_or(0),
            pending_total: pending_total.unwrap_or(0),
            stability_pct: stability.unwrap_or(0.0),
            sink_mutations_count: 0,
            avg_danger_score: 0.0,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal helpers for stats parsing
// ---------------------------------------------------------------------------

fn parse_u64(s: &str) -> Option<u64> {
    s.parse().ok()
}

fn parse_f64(s: &str) -> Option<f64> {
    s.parse().ok()
}

fn parse_percentage(s: &str) -> Option<f64> {
    let s = s.trim_end_matches('%');
    s.parse().ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // DedupEngine
    // ------------------------------------------------------------------

    #[test]
    fn test_dedup_is_unique() {
        let mut engine = DedupEngine::new(DedupConfig::default());
        assert!(engine.is_unique("frame 0: 0xdeadbeef main"));
        // Same normalized trace (addresses stripped) -> duplicate
        assert!(!engine.is_unique("frame 0: 0xcafebabe main"));
        // Same as first trace (address stripped) -> duplicate
        assert!(!engine.is_unique("frame 0: 0xdeadbeef main"));
        assert_eq!(engine.crash_count(), 1);
    }

    #[test]
    fn test_normalize_stack_trace_strips_addresses() {
        let engine = DedupEngine::new(DedupConfig {
            normalize_addresses: true,
            ignore_frame_numbers: false,
            max_stack_frames: 30,
            ..Default::default()
        });
        let raw = "frame 1: libc.so+0x12345\nframe 2: main+0xabc";
        let norm = engine.normalize_stack_trace(raw);
        assert!(!norm.contains("0x12345"));
        assert!(!norm.contains("0xabc"));
        assert!(norm.contains("0x0"));
    }

    #[test]
    fn test_normalize_stack_trace_strips_frame_numbers() {
        let engine = DedupEngine::new(DedupConfig {
            normalize_addresses: false,
            ignore_frame_numbers: true,
            max_stack_frames: 30,
            ..Default::default()
        });
        let raw = "#0  main\n#1  foo\n#2  bar";
        let norm = engine.normalize_stack_trace(raw);
        assert!(!norm.contains("#0"));
        assert!(!norm.contains("#1"));
        assert!(!norm.contains("#2"));
    }

    #[test]
    fn test_normalize_stack_trace_truncates_frames() {
        let engine = DedupEngine::new(DedupConfig {
            normalize_addresses: false,
            ignore_frame_numbers: false,
            max_stack_frames: 2,
            ..Default::default()
        });
        let raw = "a\nb\nc\nd";
        let norm = engine.normalize_stack_trace(raw);
        assert_eq!(norm.lines().count(), 2);
    }

    // ------------------------------------------------------------------
    // FuzzController
    // ------------------------------------------------------------------

    #[test]
    fn test_new_controller_is_provisioning() {
        let ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
        assert_eq!(ctrl.state(), CampaignState::Provisioning);
    }

    #[test]
    fn test_start_transitions_to_running() {
        let mut ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
        ctrl.start().unwrap();
        assert_eq!(ctrl.state(), CampaignState::Running);
    }

    #[test]
    fn test_record_crash_and_duplicate() {
        let mut ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
        ctrl.start().unwrap();

        let c1 = ctrl.record_crash(
            vec![1, 2, 3],
            "SIGSEGV at 0xdead".into(),
            11,
            "error".into(),
            0xdead,
        );
        assert!(c1.is_some());
        assert_eq!(ctrl.stats().unique_crashes, 1);

        // Duplicate
        let c2 = ctrl.record_crash(
            vec![4, 5, 6],
            "SIGSEGV at 0xbeef".into(),
            11,
            "error".into(),
            0xbeef,
        );
        assert!(c2.is_none());
        assert_eq!(ctrl.stats().duplicate_crashes, 1);
        assert_eq!(ctrl.crashes().len(), 1);
    }

    #[test]
    fn test_is_stopping_condition_crashes() {
        let mut ctrl = FuzzController::new(
            FuzzConfig {
                max_crashes: 2,
                max_duration_secs: 999_999,
                ..Default::default()
            },
            DedupConfig::default(),
        );
        ctrl.start().unwrap();

        assert!(!ctrl.is_stopping_condition());
        ctrl.record_crash(vec![1], "trace1".into(), 11, "".into(), 0);
        assert!(!ctrl.is_stopping_condition());
        ctrl.record_crash(vec![2], "trace2".into(), 11, "".into(), 0);
        assert!(ctrl.is_stopping_condition());
    }

    #[test]
    fn test_is_stopping_condition_duration() {
        let mut ctrl = FuzzController::new(
            FuzzConfig {
                max_crashes: 999_999,
                max_duration_secs: 1,
                ..Default::default()
            },
            DedupConfig::default(),
        );
        ctrl.start().unwrap();

        assert!(!ctrl.is_stopping_condition());

        let mut over_limit = CampaignStats::default();
        over_limit.elapsed_secs = 3600;
        ctrl.update_stats(over_limit);

        assert!(ctrl.is_stopping_condition());
    }

    #[test]
    fn test_crash_classification_from_signal() {
        let mut ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
        ctrl.start().unwrap();

        let crash = ctrl
            .record_crash(vec![], "sigsegv".into(), 11, "".into(), 0)
            .unwrap();
        assert_eq!(crash.classification, CrashClassification::Segfault);

        let crash = ctrl
            .record_crash(vec![], "sigabrt".into(), 6, "".into(), 0)
            .unwrap();
        assert_eq!(crash.classification, CrashClassification::Abort);

        let crash = ctrl
            .record_crash(vec![], "sigill".into(), 4, "".into(), 0)
            .unwrap();
        assert_eq!(crash.classification, CrashClassification::IllegalInstruction);

        let crash = ctrl
            .record_crash(vec![], "sigfpe".into(), 8, "".into(), 0)
            .unwrap();
        assert_eq!(crash.classification, CrashClassification::ArithmeticException);

        let crash = ctrl
            .record_crash(vec![], "sigbus".into(), 7, "".into(), 0)
            .unwrap();
        assert_eq!(crash.classification, CrashClassification::BusError);

        let crash = ctrl
            .record_crash(vec![], "unknown_signal".into(), 99, "".into(), 0)
            .unwrap();
        assert_eq!(crash.classification, CrashClassification::Unknown);
    }

    #[test]
    fn test_generate_afl_command_minimal() {
        let ctrl = FuzzController::new(
            FuzzConfig {
                target_path: "/bin/target".into(),
                ..Default::default()
            },
            DedupConfig::default(),
        );
        let cmd = ctrl.generate_afl_command();
        let joined = cmd.join(" ");
        assert!(joined.contains("afl-fuzz"));
        assert!(joined.contains("-i /corpus/in"));
        assert!(joined.contains("-o /corpus/out"));
        assert!(joined.contains("-t 1000"));
        assert!(joined.contains("-m 2048"));
        assert!(joined.contains("-- /bin/target"));
    }

    #[test]
    fn test_generate_afl_command_with_dict_and_seed() {
        let ctrl = FuzzController::new(
            FuzzConfig {
                target_path: "/bin/target".into(),
                dictionary_path: Some("/dict/target.dict".into()),
                seed_corpus_dir: Some("/seeds".into()),
                enable_deterministic: false,
                target_args: vec!["-f".into(), "@@".into()],
                ..Default::default()
            },
            DedupConfig::default(),
        );
        let cmd = ctrl.generate_afl_command();
        let joined = cmd.join(" ");
        assert!(joined.contains("-i /seeds"));
        assert!(joined.contains("-x /dict/target.dict"));
        assert!(joined.contains("-d"));
        assert!(joined.contains("-- /bin/target -f @@"));
    }

    #[test]
    fn test_stop_cancel_fail() {
        let mut ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
        ctrl.start().unwrap();
        ctrl.stop().unwrap();
        assert_eq!(ctrl.state(), CampaignState::Completed);

        let mut ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
        ctrl.cancel().unwrap();
        assert_eq!(ctrl.state(), CampaignState::Cancelled);

        let mut ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
        ctrl.fail("out of memory").unwrap();
        assert_eq!(ctrl.state(), CampaignState::Failed);
    }

    #[test]
    fn test_to_response() {
        let ctrl = FuzzController::new(FuzzConfig::default(), DedupConfig::default());
        let resp = ctrl.to_response("stream-1".into());
        assert_eq!(resp.stream_id, "stream-1");
        assert_eq!(resp.state, CampaignState::Provisioning);
    }

    // ------------------------------------------------------------------
    // AFLStatsParser
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_fuzzer_stats() {
        let raw = "\
start_time        : 1700000000
last_update       : 1700003600
fuzzer_pid        : 42
cycles_done       : 10
execs_done        : 500000
execs_per_sec     : 250.5
paths_total       : 120
paths_favored     : 15
paths_found       : 100
paths_imported    : 20
max_depth         : 12
cur_path          : 8
pending_favs      : 4
pending_total     : 30
variable_paths    : 5
stability         : 99.80%
bitmap_cvg        : 12.34%
unique_crashes    : 3
unique_hangs      : 1
last_path         : 1700003500
last_crash        : 1700003400
last_hang         : 1700003300
";
        let stats = AFLStatsParser::parse_fuzzer_stats(raw).unwrap();

        assert_eq!(stats.total_execs, 500000);
        assert_eq!(stats.execs_per_sec, 250.5);
        assert_eq!(stats.unique_crashes, 3);
        assert_eq!(stats.duplicate_crashes, 0);
        assert_eq!(stats.hangs, 1);
        assert_eq!(stats.corpus_entries, 120);
        assert_eq!(stats.corpus_favored, 15);
        assert_eq!(stats.edges_found, 0);
        assert_eq!(stats.edges_total, 0);
        assert_eq!(stats.coverage_pct, 12.34);
        assert_eq!(stats.cycles_done, 10);
        assert_eq!(stats.elapsed_secs, 3600);
        assert_eq!(stats.pending_favs, 4);
        assert_eq!(stats.pending_total, 30);
    }

    #[test]
    fn test_parse_fuzzer_stats_empty() {
        let stats = AFLStatsParser::parse_fuzzer_stats("").unwrap();
        assert_eq!(stats.total_execs, 0);
        assert_eq!(stats.elapsed_secs, 0);
    }

    // ------------------------------------------------------------------
    // CampaignId round-trip
    // ------------------------------------------------------------------

    #[test]
    fn test_campaign_id_serde_roundtrip() {
        let id = CampaignId(Uuid::new_v4());
        let json = serde_json::to_string(&id).unwrap();
        let parsed: CampaignId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_crash_serde_roundtrip() {
        let crash = FuzzCrash {
            crash_id: Uuid::new_v4(),
            campaign_id: CampaignId(Uuid::new_v4()),
            stack_hash: "abcdef".into(),
            crashing_input: vec![1, 2, 3],
            input_size: 3,
            signal: 11,
            signal_name: "SIGSEGV".into(),
            stack_trace: "trace".into(),
            stderr_output: "err".into(),
            classification: CrashClassification::Segfault,
            crash_address: 0x1000,
            discovered_at: Utc::now(),
            artifact_path: "crashes/uuid".into(),
        };
        let json = serde_json::to_string(&crash).unwrap();
        let parsed: FuzzCrash = serde_json::from_str(&json).unwrap();
        assert_eq!(crash.stack_hash, parsed.stack_hash);
    }

    // ------------------------------------------------------------------
    // Defaults
    // ------------------------------------------------------------------

    #[test]
    fn test_fuzz_config_defaults() {
        let cfg = FuzzConfig::default();
        assert_eq!(cfg.exec_timeout_ms, 1000);
        assert_eq!(cfg.memory_limit_mb, 2048);
        assert_eq!(cfg.max_crashes, 100);
        assert_eq!(cfg.max_duration_secs, 3600);
        assert!(cfg.enable_deterministic);
        assert!(cfg.enable_havoc);
        assert!(cfg.enable_splice);
        assert!(cfg.dictionary_path.is_none());
        assert!(cfg.seed_corpus_dir.is_none());
        assert!(cfg.parallel_instances > 0);
    }

    #[test]
    fn test_dedup_config_defaults() {
        let cfg = DedupConfig::default();
        assert_eq!(cfg.hash_algorithm, HashAlgorithm::Sha256);
        assert_eq!(cfg.max_stack_frames, 30);
        assert!(cfg.normalize_addresses);
        assert!(cfg.ignore_frame_numbers);
    }
}

// Ensure `CampaignStats` implements `Default` for tests.
impl Default for CampaignStats {
    fn default() -> Self {
        Self {
            total_execs: 0,
            execs_per_sec: 0.0,
            unique_crashes: 0,
            duplicate_crashes: 0,
            hangs: 0,
            corpus_entries: 0,
            corpus_favored: 0,
            edges_found: 0,
            edges_total: 0,
            coverage_pct: 0.0,
            cycles_done: 0,
            elapsed_secs: 0,
            pending_favs: 0,
            pending_total: 0,
            stability_pct: 0.0,
            sink_mutations_count: 0,
            avg_danger_score: 0.0,
        }
    }
}
