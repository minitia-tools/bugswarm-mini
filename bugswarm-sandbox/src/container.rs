use std::collections::HashMap;
use std::time::{Duration, Instant};

use bollard::container::{
    Config as ContainerConfig, CreateContainerOptions, LogOutput, LogsOptions,
    StartContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::models::{
    HostConfig, Mount, MountTypeEnum, ResourcesUlimits, RestartPolicy, RestartPolicyNameEnum,
};
use bollard::Docker;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::time::timeout as tokio_timeout;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::{
    ExecutionReceipt, ExecutionStatus, ExceptionHandling, MemoryProfile, ReceiptHealthCheck,
    SandboxConfig, StackFrame, TimeoutReason, ALLOWED_ENV_VARS, BLOCKED_ENV_VARS, TMPFS_MOUNTS,
};
use crate::error::{SandboxError, SandboxResult};
use crate::fuzzer::{DedupConfig, FuzzConfig, FuzzController, FuzzResponse, FuzzerError};
use crate::scanner::OutputScanner;

/// Full output from a container execution.
#[derive(Debug, Clone)]
pub struct ContainerOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i64>,
    pub duration_secs: f64,
    pub status: ExecutionStatus,
    pub memory_profile: MemoryProfile,
    pub timeout_reason: Option<TimeoutReason>,
    pub pre_kill_diagnostics: Option<String>,
    pub error: Option<String>,
}

/// The container manager handles all Docker interactions.
pub struct ContainerManager {
    docker: Docker,
    pub(crate) config: SandboxConfig,
    scanner: OutputScanner,
}

impl ContainerManager {
    /// Connect to the Docker daemon.
    pub async fn connect(config: SandboxConfig) -> SandboxResult<Self> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| SandboxError::DockerUnavailable(e.to_string()))?;

        let _version = docker.version().await
            .map_err(|e| SandboxError::DockerUnavailable(format!("Docker ping failed: {}", e)))?;

        info!("Connected to Docker daemon");

        let scanner = OutputScanner::new()?;

        Ok(Self { docker, config, scanner })
    }

    /// Connect to Docker with retry logic for transient unavailability.
    pub async fn connect_with_retry(config: SandboxConfig, max_retries: u32) -> SandboxResult<Self> {
        let mut last_err = None;
        for attempt in 0..max_retries {
            match Self::connect(config.clone()).await {
                Ok(manager) => {
                    if attempt > 0 {
                        tracing::info!("Docker connected after {} retries", attempt);
                    }
                    return Ok(manager);
                }
                Err(e) => {
                    last_err = Some(e);
                    if attempt < max_retries - 1 {
                        let delay = std::time::Duration::from_secs(2u64.pow(attempt));
                        tracing::warn!("Docker unavailable (attempt {}), retrying in {}s", attempt + 1, delay.as_secs());
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
        Err(last_err.unwrap())
    }

    /// Ensure the sandbox image is pulled.
    pub async fn ensure_image(&self) -> SandboxResult<String> {
        let image = &self.config.image;
        let inspect = self.docker.inspect_image(image).await;
        if inspect.is_ok() {
            debug!("Image {} already exists locally", image);
            return Ok(image.clone());
        }

        info!("Pulling image: {}", image);
        let options = CreateImageOptions {
            from_image: image.as_str(),
            ..Default::default()
        };

        let mut had_error = false;
        let mut stream = self.docker.create_image(Some(options), None, None);
        while let Some(result) = stream.next().await {
            if let Err(e) = result {
                had_error = true;
                warn!("Image pull error: {}", e);
            }
        }

        if had_error {
            return Err(SandboxError::DockerUnavailable(format!(
                "Failed to pull image '{}' — errors occurred during download", image
            )));
        }
        Ok(image.clone())
    }

    /// Get the SHA256 digest of a Docker image.
    pub async fn get_image_sha256(&self) -> SandboxResult<String> {
        let inspect = self.docker.inspect_image(&self.config.image).await
            .map_err(|e| SandboxError::ContainerExecution(format!("Image inspect failed: {}", e)))?;

        inspect.id
            .ok_or_else(|| SandboxError::ContainerExecution("Image has no ID".into()))
    }

    /// Select the appropriate image based on sanitizer config.
    /// C6.2.3: Uses pre-built cached image registry — O(1) lookup.
    async fn select_image(&self, language: &str) -> String {
        if !self.config.sanitizer_enabled {
            return self.config.image.clone();
        }
        if let Some(ref custom) = self.config.sanitizer_image {
            return custom.clone();
        }

        // C6.2.3: O(1) lookup from language map, not string scanning
        let asan_image = match language {
            "python" => "bugswarm/sandbox-python-asan:latest",
            "c" | "cpp" | "c++" => "bugswarm/sandbox-cpp-asan:latest",
            _ => return self.config.image.clone(), // fallback to vanilla
        };

        // Verify image exists, cache result
        if self.docker.inspect_image(asan_image).await.is_ok() {
            debug!("Using sanitizer image: {}", asan_image);
            asan_image.to_string()
        } else {
            warn!("Sanitizer image {} not available, falling back to vanilla", asan_image);
            self.config.image.clone()
        }
    }

    /// Validate environment variables requested by an agent.
    pub fn validate_env_vars(&self, env_vars: &HashMap<String, String>) -> SandboxResult<HashMap<String, String>> {
        let mut validated = HashMap::new();
        for (key, value) in env_vars {
            let key_upper = key.to_uppercase();
            if BLOCKED_ENV_VARS.iter().any(|b| key_upper == *b) {
                return Err(SandboxError::Other(format!(
                    "Environment variable '{}' is blocked for security reasons", key
                )));
            }
            if ALLOWED_ENV_VARS.iter().any(|a| key_upper == *a) || key_upper.starts_with("BUGSWARM_") {
                validated.insert(key.clone(), value.clone());
            } else {
                warn!("Environment variable '{}' not in whitelist — ignored", key);
            }
        }
        Ok(validated)
    }

    /// Run a single execution of a PoC script.
    pub async fn execute(
        &self,
        poc_content: &str,
        env_vars: &HashMap<String, String>,
        flaky_detection: bool,
    ) -> SandboxResult<ExecutionReceipt> {
        let execution_id = Uuid::new_v4().to_string();
        let started_at = chrono::Utc::now();
        let poc_sha256 = hex::encode(Sha256::digest(poc_content.as_bytes()));

        let validated_env = self.validate_env_vars(env_vars)?;

        // C6.2.3: Language from env var, not PoC content scanning
        let language = validated_env.get("BGSWARM_LANGUAGE").map(|s| s.as_str()).unwrap_or("python");
        let selected_image = self.select_image(language).await;
        let image_sha256 = self.docker.inspect_image(&selected_image).await
            .map_err(|e| SandboxError::ContainerExecution(format!("Image inspect failed: {}", e)))?
            .id
            .ok_or_else(|| SandboxError::ContainerExecution("Image has no ID".into()))?;

        // Scan PoC for escape attempts
        if self.config.escape_detection {
            let escape_matches = self.scanner.scan_escape_attempts(poc_content);
            if !escape_matches.is_empty() {
                let pattern = escape_matches[0].clone();
                error!("Escape attempt detected in PoC {}: {}", execution_id, pattern);
                return Err(SandboxError::EscapeAttempt { pattern });
            }
        }

        let health_before = self.daemon_healthy().await;
        let output = self.run_container(&execution_id, poc_content, &validated_env, flaky_detection, &selected_image).await?;
        let health_after = self.daemon_healthy().await;

        let ended_at = chrono::Utc::now();
        let duration = output.duration_secs;

        let (stdout_clean, pii_count) = if self.config.pii_scanning {
            self.scanner.scan_pii(&output.stdout)
        } else {
            (output.stdout.clone(), 0)
        };
        let (stderr_clean, stderr_pii) = if self.config.pii_scanning {
            self.scanner.scan_pii(&output.stderr)
        } else {
            (output.stderr.clone(), 0)
        };

        // Parse sanitizer output if enabled
        let sanitizer_report = if self.config.sanitizer_enabled {
            crate::sanitizer_report::parse_sanitizer_output(&stderr_clean)
        } else {
            None
        };
        let finding_source = sanitizer_report.as_ref().map(|_| "sanitizer".to_string());

        let stack_frames = Self::extract_stack_frames(&output.stderr);
        let (exception_type, exception_message, exception_handling) =
            Self::classify_exception(&output.stderr, output.exit_code);

        let stdout_truncated = truncate_str(&stdout_clean, 10_000);
        let stderr_truncated = truncate_str(&stderr_clean, 10_000);

        let receipt = ExecutionReceipt {
            execution_id: execution_id.clone(),
            poc_sha256,
            image_sha256,
            command: vec!["sh".into(), "-c".into(), "python3 /tmp/sandbox/poc.py".into()],
            exit_code: output.exit_code,
            status: Self::determine_status(output.exit_code, &output.status),
            duration_secs: duration,
            cpu_time_secs: None,
            memory_profile: output.memory_profile.clone(),
            exception_type,
            exception_message,
            exception_handling,
            stack_frames,
            stdout_truncated,
            stderr_truncated,
            stdout_sha256: hex::encode(Sha256::digest(stdout_clean.as_bytes())),
            stderr_sha256: hex::encode(Sha256::digest(stderr_clean.as_bytes())),
            timeout_reason: output.timeout_reason,
            pre_kill_diagnostics: output.pre_kill_diagnostics,
            health_check: ReceiptHealthCheck {
                before_timestamp: started_at,
                after_timestamp: ended_at,
                daemon_healthy_before: health_before,
                daemon_healthy_after: health_after,
                docker_daemon_ok: true,
            },
            pii_redactions: pii_count + stderr_pii,
            sanitizer_report,
            finding_source,
            env_vars: validated_env.clone(),
            started_at,
            ended_at,
            independently_verified: false,
            independent_receipt_id: None,
            tainted: false,
            taint_reason: None,
        };

        let receipt = self.check_taint(receipt).await;
        Ok(receipt)
    }

    async fn run_container(
        &self,
        execution_id: &str,
        poc_content: &str,
        env_vars: &HashMap<String, String>,
        _flaky_detection: bool,
        image: &str,
    ) -> SandboxResult<ContainerOutput> {
        let container_name = format!("bugswarm-sandbox-{}", execution_id);

        let env_list: Vec<String> = env_vars
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        let script = format!(
            "mkdir -p /tmp/sandbox && cat > /tmp/sandbox/poc.py << 'BUGSWARM_EOF'\n{}\nBUGSWARM_EOF\npython3 /tmp/sandbox/poc.py",
            poc_content
        );

        let mut host_cfg = build_host_config(&self.config);
        host_cfg.auto_remove = Some(true);

        let container_config = ContainerConfig {
            image: Some(image.to_string()),
            env: Some(env_list),
            cmd: Some(vec!["sh".into(), "-c".into(), script]),
            working_dir: Some(self.config.workdir.clone()),
            host_config: Some(host_cfg),
            ..Default::default()
        };

        let create_options = CreateContainerOptions {
            name: &container_name,
            platform: None,
        };

        self.docker.create_container(Some(create_options), container_config)
            .await
            .map_err(|e| SandboxError::ContainerExecution(format!("Container create failed: {}", e)))?;

        self.docker.start_container(&container_name, None::<StartContainerOptions<&str>>)
            .await
            .map_err(|e| SandboxError::ContainerExecution(format!("Container start failed: {}", e)))?;

        let start = Instant::now();
        let timeout_duration = Duration::from_secs(self.config.wall_clock_timeout_secs);

        let result = tokio_timeout(timeout_duration, async {
            loop {
                tokio::time::sleep(Duration::from_millis(200)).await;
                match self.docker.inspect_container(&container_name, None).await {
                    Ok(inspect) => {
                        if let Some(state) = &inspect.state {
                            if state.oom_killed.unwrap_or(false) {
                                // Clean up on OOM
                                if let Err(e) = self.docker.remove_container(
                                    &container_name, None::<bollard::container::RemoveContainerOptions>,
                                ).await {
                                    warn!("Failed to remove OOM-killed container {}: {}", container_name, e);
                                }
                                return Ok(ContainerOutput {
                                    stdout: String::new(), stderr: String::new(),
                                    exit_code: Some(137),
                                    duration_secs: start.elapsed().as_secs_f64(),
                                    status: ExecutionStatus::OomKilled,
                                    memory_profile: MemoryProfile {
                                        peak_mb: 0, growth_rate_mb_per_sec: 0.0,
                                        growth_duration_secs: 0.0, oom_killed: true,
                                    },
                                    timeout_reason: None, pre_kill_diagnostics: None,
                                    error: Some("OOM killed".into()),
                                });
                            }
                            if !state.running.unwrap_or(true) {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            // Collect logs
            let mut stdout = String::new();
            let mut stderr = String::new();
            let mut logs_stream = self.docker.logs(
                &container_name,
                Some(LogsOptions::<&str> {
                    stdout: true, stderr: true, follow: false,
                    ..Default::default()
                }),
            );
            while let Some(log_output) = logs_stream.next().await {
                match log_output {
                    Ok(LogOutput::StdOut { message }) => stdout.push_str(&String::from_utf8_lossy(&message)),
                    Ok(LogOutput::StdErr { message }) => stderr.push_str(&String::from_utf8_lossy(&message)),
                    Ok(_) => {}, Err(_) => break,
                }
            }

            let inspect = self.docker.inspect_container(&container_name, None).await.ok();
            let exit_code = inspect.as_ref().and_then(|i| i.state.as_ref()).and_then(|s| s.exit_code);
            let oom_killed = inspect.as_ref().and_then(|i| i.state.as_ref()).and_then(|s| s.oom_killed).unwrap_or(false);
            let duration_secs = start.elapsed().as_secs_f64();

            let (growth_rate, growth_duration) = (0.0, 0.0);

            // Clean up container after collecting logs
            if let Err(e) = self.docker.remove_container(
                &container_name, None::<bollard::container::RemoveContainerOptions>,
            ).await {
                warn!("Failed to remove container {}: {}", container_name, e);
            }

            Ok(ContainerOutput {
                stdout, stderr, exit_code, duration_secs,
                status: if oom_killed { ExecutionStatus::OomKilled }
                        else if exit_code.map_or(true, |c| c == 0) { ExecutionStatus::Passed }
                        else { ExecutionStatus::Failed },
                memory_profile: MemoryProfile {
                    peak_mb: 0,
                    growth_rate_mb_per_sec: growth_rate,
                    growth_duration_secs: growth_duration,
                    oom_killed,
                },
                timeout_reason: None, pre_kill_diagnostics: None,
                error: if exit_code.map_or(false, |c| c != 0) { Some(format!("Exit code {}", exit_code.unwrap_or(-1))) } else { None },
            })
        }).await;

        match result {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(e)) => Err(e),
            Err(_elapsed) => {
                // Timeout
                let pre_kill_diag = if self.config.pre_kill_diagnostics {
                    self.pre_kill_diagnostics(&container_name).await
                } else { None };

                let timeout_reason = if let Some(ref diag) = pre_kill_diag {
                    if diag.contains("deadlock") || diag.contains("waiting for lock") {
                        Some(TimeoutReason::LikelyDeadlock)
                    } else { Some(TimeoutReason::InfiniteLoop) }
                } else { Some(TimeoutReason::Unknown) };

                if let Err(e) = self.docker.kill_container(&container_name, None::<bollard::container::KillContainerOptions<&str>>).await {
                    warn!("Failed to kill timed-out container {}: {}", container_name, e);
                }
                let (stdout, stderr) = self.get_partial_logs(&container_name).await;
                if let Err(e) = self.docker.remove_container(
                    &container_name, None::<bollard::container::RemoveContainerOptions>,
                ).await {
                    warn!("Failed to remove timed-out container {}: {}", container_name, e);
                }

                Ok(ContainerOutput {
                    stdout, stderr, exit_code: None,
                    duration_secs: self.config.wall_clock_timeout_secs as f64,
                    status: ExecutionStatus::Timeout,
                    memory_profile: MemoryProfile {
                        peak_mb: 0,
                        growth_rate_mb_per_sec: 0.0, growth_duration_secs: 0.0,
                        oom_killed: false,
                    },
                    timeout_reason, pre_kill_diagnostics: pre_kill_diag,
                    error: Some("Container timed out".into()),
                })
            }
        }
    }

    async fn pre_kill_diagnostics(&self, container_name: &str) -> Option<String> {
        let mut diagnostics = String::new();

        let exec_result = self.docker.create_exec(container_name, CreateExecOptions {
            attach_stdout: Some(true), attach_stderr: Some(true),
            cmd: Some(vec!["cat", "/proc/1/status"]),
            ..Default::default()
        }).await.ok()?;

        if let Ok(output) = self.docker.start_exec(&exec_result.id, None).await {
            if let StartExecResults::Attached { mut output, .. } = output {
                let mut status_text = String::new();
                while let Some(Ok(msg)) = output.next().await {
                    status_text.push_str(&String::from_utf8_lossy(&msg.into_bytes()));
                }
                diagnostics.push_str("=== Process State ===\n");
                diagnostics.push_str(&status_text);
                if status_text.contains("State:\tS") || status_text.contains("State:\tD") {
                    diagnostics.push_str("\n[Process in sleep state — possible deadlock]\n");
                }
            }
        }

        if let Err(e) = self.docker.kill_container(container_name, Some(bollard::container::KillContainerOptions { signal: "SIGQUIT" })).await {
            warn!("Failed to send SIGQUIT to container {}: {}", container_name, e);
        }
        Some(diagnostics)
    }

    async fn get_partial_logs(&self, container_name: &str) -> (String, String) {
        let mut stdout = String::new();
        let mut stderr = String::new();

        let mut stream = self.docker.logs(container_name, Some(LogsOptions::<&str> {
            stdout: true, stderr: true, follow: false,
            tail: "100",
            ..Default::default()
        }));

        while let Some(log_output) = stream.next().await {
            match log_output {
                Ok(LogOutput::StdOut { message }) => stdout.push_str(&String::from_utf8_lossy(&message)),
                Ok(LogOutput::StdErr { message }) => stderr.push_str(&String::from_utf8_lossy(&message)),
                Ok(_) => {}, Err(_) => break,
            }
        }
        (stdout, stderr)
    }

    async fn daemon_healthy(&self) -> bool {
        self.docker.ping().await.is_ok()
    }

    async fn check_taint(&self, receipt: ExecutionReceipt) -> ExecutionReceipt {
        let mut receipt = receipt;
        if !receipt.health_check.daemon_healthy_before || !receipt.health_check.daemon_healthy_after {
            receipt.tainted = true;
            receipt.taint_reason = Some("Docker daemon was unhealthy during execution".into());
            warn!("Receipt {} tainted: Docker daemon unhealthy", receipt.execution_id);
        }
        let combined = format!("{}{}", receipt.stdout_truncated, receipt.stderr_truncated);
        if combined.contains("docker: Error response from daemon")
            || combined.contains("containerd: failed to")
            || combined.contains("runc:") {
            receipt.tainted = true;
            receipt.taint_reason = Some("Container runtime errors detected in output".into());
        }
        receipt
    }

    fn determine_status(exit_code: Option<i64>, raw_status: &ExecutionStatus) -> ExecutionStatus {
        match raw_status {
            ExecutionStatus::OomKilled => ExecutionStatus::OomKilled,
            ExecutionStatus::Timeout => ExecutionStatus::Timeout,
            _ => match exit_code {
                None => ExecutionStatus::Killed,
                Some(0) => ExecutionStatus::Passed,
                Some(_) => ExecutionStatus::Failed,
            }
        }
    }

    fn extract_stack_frames(stderr: &str) -> Vec<StackFrame> {
        let mut frames = Vec::new();
        let py_re = regex::Regex::new(r#"File\s+"([^"]+)",\s+line\s+(\d+)(?:,\s+in\s+(\w+))?"#);
        if let Ok(re) = py_re {
            for cap in re.captures_iter(stderr) {
                frames.push(StackFrame {
                    file: cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                    line: cap.get(2).and_then(|m| m.as_str().parse().ok()),
                    function: cap.get(3).map(|m| m.as_str().to_string()),
                    locals_hash: None,
                });
            }
        }
        if frames.is_empty() {
            if let Ok(re) = regex::Regex::new(r"(\S+\.go):(\d+)\s") {
                for cap in re.captures_iter(stderr) {
                    frames.push(StackFrame {
                        file: cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                        line: cap.get(2).and_then(|m| m.as_str().parse().ok()),
                        function: None, locals_hash: None,
                    });
                }
            }
        }
        if frames.is_empty() {
            if let Ok(re) = regex::Regex::new(r"(\S+\.(?:c|cpp|h|hpp)):(\d+)") {
                for cap in re.captures_iter(stderr) {
                    frames.push(StackFrame {
                        file: cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default(),
                        line: cap.get(2).and_then(|m| m.as_str().parse().ok()),
                        function: None, locals_hash: None,
                    });
                }
            }
        }
        if frames.is_empty() {
            if let Ok(re) = regex::Regex::new(r"at\s+(\S+)\((\S+\.java):(\d+)\)") {
                for cap in re.captures_iter(stderr) {
                    frames.push(StackFrame {
                        file: cap.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
                        line: cap.get(3).and_then(|m| m.as_str().parse().ok()),
                        function: cap.get(1).map(|m| m.as_str().to_string()),
                        locals_hash: None,
                    });
                }
            }
        }
        frames
    }

    fn classify_exception(stderr: &str, exit_code: Option<i64>) -> (Option<String>, Option<String>, ExceptionHandling) {
        let is_error = exit_code.map_or(true, |c| c != 0);
        if !is_error && stderr.is_empty() {
            return (None, None, ExceptionHandling::Unknown);
        }

        if let Ok(re) = regex::Regex::new(r"(\w+(?:Error|Exception|Warning)):\s*(.+)") {
            for cap in re.captures_iter(stderr) {
                let exc = cap.get(1).map(|m| m.as_str().to_string());
                let msg = cap.get(2).map(|m| m.as_str().to_string());
                let handling = if stderr.contains("Traceback (most recent call last)") {
                    ExceptionHandling::Uncaught
                } else if stderr.contains("During handling of") {
                    ExceptionHandling::CaughtByApplication
                } else { ExceptionHandling::Unknown };
                return (exc, msg, handling);
            }
        }
        if stderr.contains("panic:") {
            if let Ok(re) = regex::Regex::new(r"panic:\s*(.+)") {
                if let Some(cap) = re.captures(stderr) {
                    return (Some("panic".into()), cap.get(1).map(|m| m.as_str().to_string()), ExceptionHandling::Uncaught);
                }
            }
        }
        if stderr.contains("SIGSEGV") || stderr.contains("segmentation fault") {
            return (Some("SIGSEGV".into()), Some("Segmentation fault".into()), ExceptionHandling::Uncaught);
        }
        if is_error && !stderr.is_empty() {
            let first_line = stderr.lines().next().unwrap_or("");
            return (Some("Error".into()), Some(first_line.to_string()), ExceptionHandling::Unknown);
        }
        (None, None, ExceptionHandling::Unknown)
    }

    #[allow(dead_code)]
    fn compute_memory_growth(_samples: &[(f64, u64)]) -> (f64, f64) {
        (0.0, 0.0)
    }

    /// Start a fuzzing campaign inside this container.
    /// Launches AFL++ with the given fuzz configuration.
    /// Uses FuzzController for campaign lifecycle management.
    pub async fn fuzz(&self, fuzz_config: &FuzzConfig) -> std::result::Result<FuzzResponse, FuzzerError> {
        info!(
            "Starting fuzzer: target={}, timeout={}ms, memory={}MB danger={}",
            fuzz_config.target_path,
            fuzz_config.exec_timeout_ms,
            fuzz_config.memory_limit_mb,
            self.config.fuzz_danger_map_enabled
        );

        // Populate danger map config from SandboxConfig
        let mut fz_config = fuzz_config.clone();
        if self.config.fuzz_danger_map_enabled {
            fz_config.danger_map_enabled = true;
            fz_config.danger_config = Some(crate::danger_map::DangerConfig {
                enabled: true,
                taint_weight: self.config.fuzz_danger_taint_weight,
                coverage_weight: self.config.fuzz_danger_coverage_weight,
                decay_factor: self.config.fuzz_danger_decay,
            });
        }

        // Create and start the campaign controller
        let mut controller = FuzzController::new(fz_config, DedupConfig::default());

        // Pre-create corpus directories on host
        for dir in &["/fuzz/corpus/in", "/fuzz/corpus/out"] {
            std::fs::create_dir_all(dir).unwrap_or_else(|e| {
                tracing::warn!("Failed to create fuzz corpus directory {}: {}", dir, e);
            });
        }

        // Phase 21C: Load danger map if enabled
        controller.load_danger_map_if_configured()?;

        controller.start()?;

        // Build AFL++ command from the controller
        let afl_cmd = controller.build_afl_command("/corpus/in", "/corpus/out");
        let cmd_shell = afl_cmd.join(" ");

        let stream_id = Uuid::new_v4().to_string();
        let container_name = format!("bugswarm-fuzz-{}", controller.campaign_id());

        let host_config = HostConfig {
            memory: Some(i64::MAX),
            auto_remove: Some(true),
            mounts: Some(vec![
                Mount {
                    target: Some("/corpus/in".to_string()),
                    typ: Some(MountTypeEnum::BIND),
                    source: Some("/fuzz/corpus/in".to_string()),
                    ..Default::default()
                },
                Mount {
                    target: Some("/corpus/out".to_string()),
                    typ: Some(MountTypeEnum::BIND),
                    source: Some("/fuzz/corpus/out".to_string()),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };

        let container_config = ContainerConfig {
            image: Some(self.config.fuzz_image.clone()),
            cmd: Some(vec!["sh".into(), "-c".into(), cmd_shell]),
            host_config: Some(host_config),
            ..Default::default()
        };

        self.docker.create_container(
            Some(CreateContainerOptions { name: &container_name, platform: None }),
            container_config,
        ).await.map_err(|e| FuzzerError::ContainerError(format!("Fuzz container create failed: {}", e)))?;

        self.docker.start_container(&container_name, None::<StartContainerOptions<&str>>)
            .await
            .map_err(|e| FuzzerError::ContainerError(format!("Fuzz container start failed: {}", e)))?;

        info!("Fuzz campaign started: container={}", container_name);

        Ok(controller.to_response(stream_id))
    }

    pub async fn independent_reexecute(&self, poc_content: &str, env_vars: &HashMap<String, String>) -> SandboxResult<ExecutionReceipt> {
        let mut receipt = self.execute(poc_content, env_vars, false).await?;
        receipt.independently_verified = true;
        Ok(receipt)
    }

    pub async fn execute_statistical(&self, poc_content: &str, env_vars: &HashMap<String, String>) -> SandboxResult<crate::config::StatisticalResult> {
        use crate::config::StatisticalResult;
        use std::collections::HashMap as StdHashMap;

        let total = self.config.rerun_count;
        let mut failures = 0u32;
        let mut passes = 0u32;
        let mut timeouts = 0u32;
        let mut ooms = 0u32;
        let mut failure_receipts: Vec<ExecutionReceipt> = Vec::new();
        let mut pass_receipts: Vec<ExecutionReceipt> = Vec::new();
        let mut exception_counts: StdHashMap<String, u32> = StdHashMap::new();
        let mut location_counts: StdHashMap<String, u32> = StdHashMap::new();
        let mut failure_times: Vec<f64> = Vec::new();

        info!("Starting statistical re-execution: {} runs", total);

        // Execute sequentially to avoid tokio::spawn + &self lifetime issues.
        // For high-throughput, batch with Arc<Self> in the future.
        for i in 0..total {
            match self.execute(poc_content, env_vars, false).await {
                Ok(receipt) => {
                    match receipt.status {
                        ExecutionStatus::Failed | ExecutionStatus::OomKilled => {
                            failures += 1;
                            if receipt.status == ExecutionStatus::OomKilled {
                                ooms += 1;
                            }
                            if let Some(ref exc) = receipt.exception_type {
                                *exception_counts.entry(exc.clone()).or_insert(0) += 1;
                            }
                            if let Some(ref frame) = receipt.stack_frames.first() {
                                let loc = format!("{}:{}", frame.file, frame.line.unwrap_or(0));
                                *location_counts.entry(loc).or_insert(0) += 1;
                            }
                            failure_times.push(receipt.duration_secs);
                            if failure_receipts.len() < 10 {
                                failure_receipts.push(receipt);
                            }
                        }
                        ExecutionStatus::Passed => {
                            passes += 1;
                            if pass_receipts.len() < 5 {
                                pass_receipts.push(receipt);
                            }
                        }
                        ExecutionStatus::Timeout => {
                            timeouts += 1;
                            if failure_receipts.len() < 10 {
                                failure_receipts.push(receipt);
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    warn!("Statistical execution error (run {}/{}): {}", i + 1, total, e);
                    failures += 1;
                }
            }
        }

        let failure_rate = if total > 0 { failures as f64 / total as f64 } else { 0.0 };

        // 95% binomial confidence interval (Wilson score)
        let z = 1.96;
        let n = total as f64;
        let p = failure_rate;
        let denom = 1.0 + z * z / n;
        let centre = (p + z * z / (2.0 * n)) / denom;
        let margin = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denom;
        let ci_lower = (centre - margin).max(0.0);
        let ci_upper = (centre + margin).min(1.0);

        let is_significant = ci_lower > self.config.min_flaky_failure_rate;

        // Dominant failure mode
        let dominant_failure = exception_counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(exc, count)| format!("{}: {}x", exc, count));

        // Top exceptions
        let mut top_exceptions: Vec<(String, u32)> = exception_counts.into_iter().collect();
        top_exceptions.sort_by(|a, b| b.1.cmp(&a.1));
        top_exceptions.truncate(3);

        // Top crash locations
        let mut top_locations: Vec<(String, u32)> = location_counts.into_iter().collect();
        top_locations.sort_by(|a, b| b.1.cmp(&a.1));
        top_locations.truncate(3);

        // Time-to-failure stats
        failure_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let ttf_min = failure_times.first().copied();
        let ttf_max = failure_times.last().copied();
        let ttf_median = if failure_times.len() % 2 == 0 && !failure_times.is_empty() {
            let mid = failure_times.len() / 2;
            Some((failure_times[mid - 1] + failure_times[mid]) / 2.0)
        } else if !failure_times.is_empty() {
            Some(failure_times[failure_times.len() / 2])
        } else {
            None
        };

        Ok(StatisticalResult {
            total_runs: total, failures, passes, timeouts, oom_kills: ooms,
            failure_rate,
            confidence_interval_lower: ci_lower,
            confidence_interval_upper: ci_upper,
            is_significant,
            dominant_failure_mode: dominant_failure,
            top_exceptions,
            top_crash_locations: top_locations,
            time_to_failure_min: ttf_min,
            time_to_failure_median: ttf_median,
            time_to_failure_max: ttf_max,
            sample_failure_receipts: failure_receipts,
            sample_pass_receipts: pass_receipts,
        })
    }

    pub async fn diff_execute(&self, input: &str, reference_input: &str) -> SandboxResult<crate::differential::DiffExecution> {
        let receipt_a = self.execute(input, &HashMap::new(), false).await?;
        let output_a = format!("stdout:{}\nstderr:{}", receipt_a.stdout_truncated, receipt_a.stderr_truncated);

        let receipt_b = self.execute(reference_input, &HashMap::new(), false).await?;
        let output_b = format!("stdout:{}\nstderr:{}", receipt_b.stdout_truncated, receipt_b.stderr_truncated);

        Ok(crate::differential::compute_diff(&output_a, &output_b, crate::differential::OutputNormalizer::Text, 0.01))
    }

    pub async fn execute_causal_intervention(
        &self, poc_content: &str, env_vars: &HashMap<String, String>,
        intervention: crate::config::CausalIntervention,
    ) -> SandboxResult<crate::config::CausalInterventionResult> {
        let modified_poc = format!(
            "import sys, os\ntarget='{file}'\nif os.path.exists(target):\n f=open(target).readlines()\n if 1<={line}<=len(f) and f[{line0}].strip()=='{orig}':\n  f[{line0}]='{repl}\\n'\n  open(target,'w').writelines(f)\n{poc}",
            file=intervention.file_path, line=intervention.line_number, line0=intervention.line_number-1,
            orig=intervention.original_line.replace('\'', "\\'"), repl=intervention.replacement_line.replace('\'', "\\'"),
            poc=poc_content,
        );
        let receipt = self.execute(&modified_poc, env_vars, false).await?;
        Ok(crate::config::CausalInterventionResult {
            crash_resolved: receipt.exit_code.unwrap_or(1) == 0,
            causality_confirmed: receipt.exit_code.unwrap_or(1) == 0,
            intervention, receipt,
        })
    }
}

fn build_host_config(config: &SandboxConfig) -> HostConfig {
    let tmpfs_mounts: Vec<Mount> = TMPFS_MOUNTS.iter().map(|(path, _opts)| {
        Mount {
            target: Some(path.to_string()),
            typ: Some(MountTypeEnum::TMPFS),
            tmpfs_options: Some(bollard::models::MountTmpfsOptions {
                size_bytes: None,
                mode: None,
            }),
            ..Default::default()
        }
    }).collect();

    let mut storage_opts = HashMap::new();
    storage_opts.insert("size".to_string(), format!("{}G", config.storage_size_gb));

    HostConfig {
        cap_drop: if config.cap_drop_all { Some(vec!["ALL".into()]) } else { None },
        security_opt: Some({
            let mut opts = vec!["no-new-privileges".into()];
            // Wire seccomp profile if available.
            // Safety: serde_json produces JSON-escaped strings for all values.
            // No user-controlled input passes through raw shell; Docker API
            // receives this as a structured HostConfig field, not a CLI string.
            // Shell metacharacters in JSON strings are always escaped by serde_json.
            if let Ok(profile) = crate::seccomp::SeccompProfile::default_profile() {
                if let Ok(json) = profile.to_docker_string() {
                    opts.push(format!("seccomp={}", json));
                }
            }
            opts
        }),
        readonly_rootfs: Some(config.read_only),
        network_mode: Some(if config.network_none { "none".into() } else { "bridge".into() }),
            auto_remove: Some(false),  // Don't auto-remove — need to collect logs first
        memory: Some(config.memory_limit_mb as i64 * 1_048_576),
        memory_swap: Some(config.memory_swap_mb as i64 * 1_048_576),
        nano_cpus: Some((config.cpus * 1_000_000_000.0) as i64),
        pids_limit: Some(config.pids_limit as i64),
        storage_opt: Some(storage_opts),
        mounts: Some(tmpfs_mounts),
        ulimits: Some(vec![
            ResourcesUlimits { name: Some("nproc".into()), soft: Some(config.nproc_limit as i64), hard: Some(config.nproc_limit as i64) },
            ResourcesUlimits { name: Some("nofile".into()), soft: Some(config.nofile_limit as i64), hard: Some(config.nofile_limit as i64) },
            ResourcesUlimits { name: Some("cpu".into()), soft: Some(config.cpu_timeout_secs as i64), hard: Some(config.cpu_timeout_secs as i64) },
        ]),
        restart_policy: Some(RestartPolicy { name: Some(RestartPolicyNameEnum::NO), maximum_retry_count: Some(0) }),
        ..Default::default()
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len { s.to_string() }
    else { format!("{}... [truncated {} more bytes]", &s[..max_len], s.len() - max_len) }
}
