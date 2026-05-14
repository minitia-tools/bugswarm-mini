use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Maximum wall-clock execution time for a single PoC.
pub const DEFAULT_WALL_CLOCK_TIMEOUT_SECS: u64 = 120;

/// Maximum CPU time for a single PoC.
pub const DEFAULT_CPU_TIMEOUT_SECS: u64 = 60;

/// Memory limit per container in MB.
pub const DEFAULT_MEMORY_LIMIT_MB: u64 = 512;

/// Max PIDs per container.
pub const DEFAULT_PIDS_LIMIT: u64 = 100;

/// Max writable layer size in GB.
pub const DEFAULT_STORAGE_SIZE_GB: u64 = 1;

/// Default number of statistical re-runs for flaky detection.
pub const DEFAULT_RERUN_COUNT: u32 = 100;

/// Minimum failure rate to consider a flaky bug confirmed.
pub const MIN_FLAKY_FAILURE_RATE: f64 = 0.05;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Wall-clock timeout in seconds.
    #[serde(default = "default_wall_timeout")]
    pub wall_clock_timeout_secs: u64,

    /// CPU time limit in seconds.
    #[serde(default = "default_cpu_timeout")]
    pub cpu_timeout_secs: u64,

    /// Memory limit in MB.
    #[serde(default = "default_memory_limit")]
    pub memory_limit_mb: u64,

    /// Swap limit in MB (set equal to memory to disable swap).
    #[serde(default = "default_memory_limit")]
    pub memory_swap_mb: u64,

    /// Max PIDs in container.
    #[serde(default = "default_pids_limit")]
    pub pids_limit: u64,

    /// Max processes (ulimit nproc).
    #[serde(default = "default_nproc")]
    pub nproc_limit: u64,

    /// Max open files (ulimit nofile).
    #[serde(default = "default_nofile")]
    pub nofile_limit: u64,

    /// Max writable layer size in GB.
    #[serde(default = "default_storage_size")]
    pub storage_size_gb: u64,

    /// CPU cores allocated.
    #[serde(default = "default_cpus")]
    pub cpus: f64,

    /// Whether to use network isolation.
    #[serde(default = "default_true")]
    pub network_none: bool,

    /// Whether to use read-only root filesystem.
    #[serde(default = "default_true")]
    pub read_only: bool,

    /// Whether to drop all capabilities.
    #[serde(default = "default_true")]
    pub cap_drop_all: bool,

    /// Whether to use no-new-privileges.
    #[serde(default = "default_true")]
    pub no_new_privileges: bool,

    /// Docker image to use for the sandbox.
    #[serde(default = "default_image")]
    pub image: String,

    /// Working directory inside the container.
    #[serde(default = "default_workdir")]
    pub workdir: String,

    /// Number of statistical re-runs.
    #[serde(default = "default_reruns")]
    pub rerun_count: u32,

    /// Minimum failure rate for flaky confirmation.
    #[serde(default = "default_flaky_rate")]
    pub min_flaky_failure_rate: f64,

    /// Whether to run pre-kill diagnostics (thread dump) on timeout.
    #[serde(default = "default_true")]
    pub pre_kill_diagnostics: bool,

    /// Whether to enable PII scanning on outputs.
    #[serde(default = "default_true")]
    pub pii_scanning: bool,

    /// Whether to detect escape attempts in PoC code.
    #[serde(default = "default_true")]
    pub escape_detection: bool,

    /// Path to seccomp profile JSON. If None, uses built-in.
    #[serde(default)]
    pub seccomp_profile_path: Option<String>,

    /// Whether to enable sanitizer-compiled images.
    #[serde(default = "default_true")]
    pub sanitizer_enabled: bool,

    /// Custom sanitizer image. If None, auto-selects based on language.
    #[serde(default)]
    pub sanitizer_image: Option<String>,

    /// Whether the fuzzer subsystem is enabled.
    #[serde(default = "default_true")]
    pub fuzzer_enabled: bool,

    /// Docker image for fuzzing containers (includes AFL++).
    #[serde(default = "default_fuzz_image")]
    pub fuzz_image: String,

    /// Fuzzer execution timeout per input (ms).
    #[serde(default = "default_fuzz_timeout")]
    pub fuzz_exec_timeout_ms: u64,

    /// Fuzzer memory limit per target (MB).
    #[serde(default = "default_fuzz_memory")]
    pub fuzz_memory_limit_mb: u64,

    /// Default fuzz campaign duration (seconds).
    #[serde(default = "default_fuzz_duration")]
    pub fuzz_max_duration_secs: u64,

    /// Maximum unique crashes before auto-stop.
    #[serde(default = "default_fuzz_max_crashes")]
    pub fuzz_max_crashes: u64,

    /// Enable taint-guided fuzzing (Phase 21 danger map).
    #[serde(default)]
    pub fuzz_danger_map_enabled: bool,

    /// Decay factor for danger score propagation (multiplicative per call-graph edge).
    #[serde(default = "default_fuzz_danger_decay")]
    pub fuzz_danger_decay: f32,

    /// Weight for danger score in power schedule (0.0-1.0, remainder = coverage weight).
    #[serde(default = "default_fuzz_danger_taint_weight")]
    pub fuzz_danger_taint_weight: f32,

    /// Weight for coverage rarity in power schedule (computed as 1.0 - taint_weight).
    #[serde(default = "default_fuzz_danger_coverage_weight")]
    pub fuzz_danger_coverage_weight: f32,
}

fn default_fuzz_danger_decay() -> f32 { 0.7 }
fn default_fuzz_danger_taint_weight() -> f32 { 0.7 }
fn default_fuzz_danger_coverage_weight() -> f32 { 0.3 }

fn default_wall_timeout() -> u64 { DEFAULT_WALL_CLOCK_TIMEOUT_SECS }
fn default_cpu_timeout() -> u64 { DEFAULT_CPU_TIMEOUT_SECS }
fn default_memory_limit() -> u64 { DEFAULT_MEMORY_LIMIT_MB }
fn default_pids_limit() -> u64 { DEFAULT_PIDS_LIMIT }
fn default_nproc() -> u64 { 50 }
fn default_nofile() -> u64 { 256 }
fn default_storage_size() -> u64 { DEFAULT_STORAGE_SIZE_GB }
fn default_cpus() -> f64 { 1.0 }
fn default_true() -> bool { true }
fn default_image() -> String { "bugswarm/sandbox-python:latest".into() }
fn default_workdir() -> String { "/sandbox".into() }
fn default_reruns() -> u32 { DEFAULT_RERUN_COUNT }
fn default_flaky_rate() -> f64 { MIN_FLAKY_FAILURE_RATE }
fn default_fuzz_image() -> String { "bugswarm/sandbox-fuzz:latest".into() }
fn default_fuzz_timeout() -> u64 { 1000 }
fn default_fuzz_memory() -> u64 { 2048 }
fn default_fuzz_duration() -> u64 { 3600 }
fn default_fuzz_max_crashes() -> u64 { 100 }

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            wall_clock_timeout_secs: default_wall_timeout(),
            cpu_timeout_secs: default_cpu_timeout(),
            memory_limit_mb: default_memory_limit(),
            memory_swap_mb: default_memory_limit(),
            pids_limit: default_pids_limit(),
            nproc_limit: default_nproc(),
            nofile_limit: default_nofile(),
            storage_size_gb: default_storage_size(),
            cpus: default_cpus(),
            network_none: default_true(),
            read_only: default_true(),
            cap_drop_all: default_true(),
            no_new_privileges: default_true(),
            image: default_image(),
            workdir: default_workdir(),
            rerun_count: default_reruns(),
            min_flaky_failure_rate: default_flaky_rate(),
            pre_kill_diagnostics: default_true(),
            pii_scanning: default_true(),
            escape_detection: default_true(),
            seccomp_profile_path: None,
            sanitizer_enabled: default_true(),
            sanitizer_image: None,
            fuzzer_enabled: default_true(),
            fuzz_image: default_fuzz_image(),
            fuzz_exec_timeout_ms: default_fuzz_timeout(),
            fuzz_memory_limit_mb: default_fuzz_memory(),
            fuzz_max_duration_secs: default_fuzz_duration(),
            fuzz_max_crashes: default_fuzz_max_crashes(),
            fuzz_danger_map_enabled: false,
            fuzz_danger_decay: default_fuzz_danger_decay(),
            fuzz_danger_taint_weight: default_fuzz_danger_taint_weight(),
            fuzz_danger_coverage_weight: default_fuzz_danger_coverage_weight(),
        }
    }
}

/// Allowed environment variables that agents can request.
pub const ALLOWED_ENV_VARS: &[&str] = &[
    "DEBUG",
    "LOG_LEVEL",
    "VERBOSE",
    "RUST_LOG",
    "PYTHONUNBUFFERED",
    "PYTHONDONTWRITEBYTECODE",
    "NODE_ENV",
    "GO_ENV",
];

/// Blocked environment variables (would alter execution semantics).
pub const BLOCKED_ENV_VARS: &[&str] = &[
    "PATH",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "PYTHONPATH",
    "PYTHONHOME",
    "GEM_PATH",
    "GEM_HOME",
    "GOPATH",
    "GOROOT",
    "JAVA_HOME",
    "CLASSPATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "PERL5LIB",
    "RUBYLIB",
];

/// TMPFS mounts inside the container.
pub const TMPFS_MOUNTS: &[(&str, &str)] = &[
    ("/tmp", "noexec,nosuid,nodev,size=100M"),
    ("/run", "noexec,nosuid,nodev,size=10M"),
    ("/var/tmp", "noexec,nosuid,nodev,size=10M"),
];

/// Escape attempt patterns to scan PoC code for.
pub const ESCAPE_PATTERNS: &[&str] = &[
    "chroot",
    "nsenter",
    "unshare",
    "setns",
    "/proc/1/ns",
    "/proc/self/ns",
    "cgroup notify_on_release",
    "/var/run/docker.sock",
    "/run/docker.sock",
    "docker.sock",
    "pivot_root",
    "kexec",
    "mount -t cgroup",
    "mount -t proc",
    "mount -t sysfs",
    "insmod",
    "modprobe",
    "rmmod",
    "kexec_load",
    "finit_module",
    "init_module",
    "delete_module",
];

/// PII regex patterns for output scanning.
pub const PII_PATTERNS: &[(&str, &str)] = &[
    ("email", r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}"),
    ("credit_card", r"\b(?:\d{4}[ -]?){3}\d{4}\b"),
    ("ssn", r"\b\d{3}-\d{2}-\d{4}\b"),
    ("phone", r"\b\+?\d{1,3}?[- .]?\(?\d{3}\)?[- .]?\d{3}[- .]?\d{4}\b"),
    ("ipv4", r"\b(?:\d{1,3}\.){3}\d{1,3}\b"),
    ("aws_key", r"\bAKIA[0-9A-Z]{16}\b"),
    ("aws_secret", r"\b[0-9a-zA-Z/+]{40}\b"),
    ("github_token", r"\bgh[pousr]_[A-Za-z0-9_]{36,}\b"),
    ("jwt", r"\beyJ[A-Za-z0-9\-_=]+\.[A-Za-z0-9\-_=]+\.?[A-Za-z0-9\-_.+/=]*\b"),
    ("private_key_header", r"-----BEGIN (RSA|EC|DSA|OPENSSH|PGP) PRIVATE KEY-----"),
];

/// Sensitive file path patterns.
pub const SENSITIVE_PATH_PATTERNS: &[&str] = &[
    ".env",
    ".pem",
    ".key",
    "credentials",
    "id_rsa",
    "id_ed25519",
    "id_ecdsa",
    ".pfx",
    ".p12",
    "secrets",
    "secret",
    ".token",
    "service-account",
];

/// Sensitive variable name patterns for probe filtering.
pub const SENSITIVE_VAR_PATTERNS: &[&str] = &[
    "password", "passwd", "secret", "token", "key",
    "credential", "private_key", "api_key", "auth_token",
    "ssn", "credit_card", "access_key", "secret_key",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    Passed,
    Failed,
    Timeout,
    OomKilled,
    Tainted,
    Killed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimeoutReason {
    InfiniteLoop,
    LikelyDeadlock,
    ExcessiveComputation,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExceptionHandling {
    Uncaught,
    CaughtByApplication,
    CaughtByFramework,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptHealthCheck {
    pub before_timestamp: DateTime<Utc>,
    pub after_timestamp: DateTime<Utc>,
    pub daemon_healthy_before: bool,
    pub daemon_healthy_after: bool,
    pub docker_daemon_ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProfile {
    pub peak_mb: u64,
    pub growth_rate_mb_per_sec: f64,
    pub growth_duration_secs: f64,
    pub oom_killed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    pub file: String,
    pub line: Option<u32>,
    pub function: Option<String>,
    pub locals_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// Unique ID for this execution.
    pub execution_id: String,

    /// SHA256 of the PoC script content.
    pub poc_sha256: String,

    /// SHA256 of the Docker image.
    pub image_sha256: String,

    /// Full command executed inside the container.
    pub command: Vec<String>,

    /// Exit code (None if killed before exit).
    pub exit_code: Option<i64>,

    /// Execution status.
    pub status: ExecutionStatus,

    /// Wall-clock duration in seconds.
    pub duration_secs: f64,

    /// CPU time in seconds.
    pub cpu_time_secs: Option<f64>,

    /// Memory profile.
    pub memory_profile: MemoryProfile,

    /// Exception type if the process crashed.
    pub exception_type: Option<String>,

    /// Exception message.
    pub exception_message: Option<String>,

    /// How the exception was handled.
    pub exception_handling: ExceptionHandling,

    /// Extracted stack frames.
    pub stack_frames: Vec<StackFrame>,

    /// Raw stdout (truncated to first 10KB for the receipt).
    pub stdout_truncated: String,

    /// Raw stderr (truncated to first 10KB).
    pub stderr_truncated: String,

    /// SHA256 of full stdout.
    pub stdout_sha256: String,

    /// SHA256 of full stderr.
    pub stderr_sha256: String,

    /// Timeout reason (if status is Timeout).
    pub timeout_reason: Option<TimeoutReason>,

    /// Pre-kill diagnostic output (thread dump, process state).
    pub pre_kill_diagnostics: Option<String>,

    /// Health check data.
    pub health_check: ReceiptHealthCheck,

    /// PII redaction count.
    pub pii_redactions: usize,

    /// Environment variables used (whitelisted only).
    pub env_vars: std::collections::HashMap<String, String>,

    /// Timestamp of execution start.
    pub started_at: DateTime<Utc>,

    /// Timestamp of execution end.
    pub ended_at: DateTime<Utc>,

    /// Whether this receipt has been independently re-executed.
    pub independently_verified: bool,

    /// Receipt of the independent verification run (if any).
    pub independent_receipt_id: Option<String>,

    /// Whether the execution was tainted (daemon crash, config change mid-execution).
    pub tainted: bool,

    /// Reason for taint if tainted.
    pub taint_reason: Option<String>,

    /// Sanitizer report if the execution was run with sanitizer instrumentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sanitizer_report: Option<crate::sanitizer_report::SanitizerReport>,

    /// Source of the finding: "agent", "sanitizer", "fuzzer", "differential", etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalResult {
    /// Total runs.
    pub total_runs: u32,

    /// Number of failures.
    pub failures: u32,

    /// Number of passes.
    pub passes: u32,

    /// Number of timeouts.
    pub timeouts: u32,

    /// Number of OOM kills.
    pub oom_kills: u32,

    /// Failure rate.
    pub failure_rate: f64,

    /// 95% binomial confidence interval lower bound.
    pub confidence_interval_lower: f64,

    /// 95% binomial confidence interval upper bound.
    pub confidence_interval_upper: f64,

    /// Whether the result is statistically significant.
    pub is_significant: bool,

    /// Dominant failure mode (most common exception + location).
    pub dominant_failure_mode: Option<String>,

    /// Top 3 exception types with frequencies.
    pub top_exceptions: Vec<(String, u32)>,

    /// Top 3 crash locations with frequencies.
    pub top_crash_locations: Vec<(String, u32)>,

    /// Min/median/max time-to-failure in seconds.
    pub time_to_failure_min: Option<f64>,
    pub time_to_failure_median: Option<f64>,
    pub time_to_failure_max: Option<f64>,

    /// Receipts for each failure (up to 10 stored).
    pub sample_failure_receipts: Vec<ExecutionReceipt>,

    /// Receipts for each pass (up to 5 stored).
    pub sample_pass_receipts: Vec<ExecutionReceipt>,
}

/// A causal intervention: surgically modify code and re-run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalIntervention {
    /// Original line.
    pub original_line: String,

    /// Replacement line.
    pub replacement_line: String,

    /// File path.
    pub file_path: String,

    /// Line number.
    pub line_number: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalInterventionResult {
    /// The intervention applied.
    pub intervention: CausalIntervention,

    /// Receipt from running with the intervention.
    pub receipt: ExecutionReceipt,

    /// Whether the crash disappeared after the intervention.
    pub crash_resolved: bool,

    /// Causality confirmed (crash disappears when predicted buggy line is fixed).
    pub causality_confirmed: bool,
}
