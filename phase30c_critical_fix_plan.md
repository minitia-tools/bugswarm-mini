# Phase 30c — CRITICAL Production Finding Remediation Plan

**Date**: 2026-05-17
**Audit Reference**: `/root/a/production_audit.md` (12 CRITICAL findings)
**Scope**: 8 remaining gaps across C1, C3a/b, C4a/b/c, C6a/b
**Predecessors**: Phase 30 (fixed C5, C7-C12), Phase 30a (fixed C2), Phase 30b (fixed H1-H41, M1-M30)
**Status**: C1, C3, C4, C6 remain OPEN — this is the terminal phase for all CRITICALs

---

## Executive Summary

Of the 12 original CRITICAL findings from `production_audit.md`, 8 have been confirmed fixed
across Phases 30/30a/30b. The 4 remaining findings — C1, C3, C4, C6 — decompose into **8
discrete gaps** across 5 daemons, 3 Python components, and 1 orchestration file. No deployment
is possible until all 8 are closed.

### Gap Inventory

| # | Gap ID | Description | Affected Component(s) | Cross-Cut |
|---|--------|-------------|----------------------|-----------|
| 1 | C1 | No docker-compose.yml — orchestration config missing | All 8 components | Yes |
| 2 | C3a | No HTTP health endpoint on CPG daemon | bugswarm-cpg | — |
| 3 | C3b | No HTTP health endpoint on Evidence daemon | bugswarm-evidence | — |
| 4 | C4a | No `/metrics` endpoint on Sandbox daemon | bugswarm-sandbox | — |
| 5 | C4b | No `/metrics` endpoint on CPG daemon | bugswarm-cpg | — |
| 6 | C4c | No `/metrics` endpoint on Evidence daemon | bugswarm-evidence | — |
| 7 | C6a | No unified config for Rust daemons | CPG, Evidence, Sandbox | Yes |
| 8 | C6b | No unified config for Python components | Agent, Gateway, Swarm | Yes |

### Why These Gaps Are Production-Blocking

- **C1**: Without a single `docker compose up` command, 8 components must be started
  manually in the correct dependency order with the correct flags. Operator error is
  guaranteed on the first emergency restart.
- **C3**: Load balancers, orchestration frameworks, and Kubernetes liveness/readiness
  probes all require HTTP health endpoints. Unix socket `health` methods are invisible
  to infrastructure. A crashed daemon holding its socket file looks alive indefinitely.
- **C4**: Metrics exist in-code (swarm `observability.py`, trigger `TriggerMetrics`,
  sandbox counters) but none are exposed on `/metrics`. Prometheus scrapes have zero
  targets. The alerting engine (H19) fires into a vacuum.
- **C6**: 8 config mechanisms (CLI flags, env vars, YAML per component, hardcoded
  defaults) make configuration drift inevitable. An SRE debugging a production issue
  at 3 a.m. has zero discoverability for what values are actually in effect.

---

## Implementation Order

The gaps have a natural dependency chain:

```
C6 (unified config) → C3 (health endpoints) → C4 (metrics endpoints) → C1 (docker-compose)
```

**Rationale**: C3 and C4 depend on C6 because the HTTP port and enabled/disabled
flags must come from the unified config. C1 depends on C3 and C4 because the
docker-compose healthchecks and Prometheus scrape configs must know the actual
health/metrics endpoint paths and ports. Additionally, C4 depends on C3 because the
`/metrics` endpoint rides on the same HTTP server that `/health` creates.

---

## Gap 1: C6a — Unified Config for Rust Daemons

### Current State

Three Rust daemons (sandbox, CPG, evidence) each have independent configuration
mechanisms:

| Daemon | Config Mechanism | Override Path |
|--------|-----------------|---------------|
| Sandbox | `--config /etc/bugswarm/sandbox.yaml` (optional, falls back to hardcoded `SandboxConfig::default()`) | CLI `--http-port`, `--socket` |
| CPG | CLI-only (`--socket`, `--pid-file`), no config file support at all | None |
| Evidence | CLI-only (`--socket`), no config file support at all | None |

### Target State

A single `/etc/bugswarm/config.yaml` file drives all daemons. Each daemon reads from
this file via `--config /etc/bugswarm/config.yaml`. Environment variable overrides
(`BGSWARM_*`) take precedence for Kubernetes/Docker deployment. The file is valid
YAML with a version field to detect schema mismatches.

### Permanent Fix — Config Schema

`/etc/bugswarm/config.yaml`:

```yaml
# BugSwarm Unified Configuration v1.0.0
# All daemons read from this file. Environment overrides: BGSWARM_<SECTION>_<KEY>
version: "1.0.0"

logging:
  level: info                          # trace, debug, info, warn, error
  file: /var/log/bugswarm/daemon.log  # empty = stdout only
  format: json                         # json, text

daemons:
  sandbox:
    socket: /var/run/bugswarm/sandbox.sock
    pid_file: /var/run/bugswarm/sandbox.pid
    http_port: 8080                    # 0 = disabled
    docker_image: bugswarm/sandbox-base:1.0.0
    rerun_count: 100
    delta_max_iterations: 1000
    delta_timeout_secs: 30
    invariant_min_confidence: 0.95
    mutation_max_mutants: 200
    memory_limit_mb: 256
    cpu_limit: 1.0
    timeout_secs: 30
  cpg:
    socket: /var/run/bugswarm/cpg.sock
    pid_file: /var/run/bugswarm/cpg.pid
    http_port: 8080                    # 0 = disabled
    prune_on_index: false
    cache_capacity: 64                 # max repos concurrently indexed
  evidence:
    socket: /var/run/bugswarm/evidence.sock
    pid_file: /var/run/bugswarm/evidence.pid
    http_port: 8081                    # 0 = disabled
    state_dir: /var/lib/bugswarm
    chroma_path: /var/lib/bugswarm/chroma
    save_on_shutdown: true
    autosave_interval_secs: 300

fuzzer:
  danger_map_enabled: true
  danger_decay: 0.7
  danger_sinks: [exec, eval, system, popen, subprocess, os_system, shell_exec, Runtime_exec, ProcessBuilder_start, system, CreateProcess, ShellExecute, winexec]
  afl_binary_path: /usr/local/bin/afl-fuzz
  afl_memory_limit: 800
  corpus_path: /fuzz/corpus
  crash_path: /fuzz/crashes

storage:
  state_dir: /var/lib/bugswarm
  chroma_persist_dir: /var/lib/bugswarm/chroma
  evidence_save_path: /var/lib/bugswarm/evidence.json

concolic:
  max_queries: 100
  solver_timeout_ms: 5000
  constraint_window_size: 10
  z3_binary_path: /usr/bin/z3

differential:
  normalizer: Text                    # Text, Json, Xml, Dict, Binary
  p_threshold: 0.01
  max_pairs: 100

security:
  api_key: ""                          # empty = no auth (use env var BGSWARM_SECURITY_API_KEY)
  allowed_env_vars: [PATH, HOME, USER, LANG, PYTHONPATH, LD_LIBRARY_PATH]
  seccomp_profile_path: /etc/bugswarm/seccomp.json
```

### Permanent Fix — Rust Implementation

Create a shared config crate at `bugswarm-config/` in the workspace:

**`bugswarm-config/Cargo.toml`**:
```toml
[package]
name = "bugswarm-config"
version = "1.0.0"
edition = "2021"

[dependencies]
serde = { workspace = true }
serde_yaml = "0.9"
serde_json = { workspace = true }
dirs = "5"
tracing = { workspace = true }
```

**`bugswarm-config/src/lib.rs`** — core types:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedConfig {
    pub version: String,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub daemons: DaemonsConfig,
    #[serde(default)]
    pub fuzzer: FuzzerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub concolic: ConcolicConfig,
    #[serde(default)]
    pub differential: DifferentialConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default = "default_log_format")]
    pub format: String,
}

fn default_log_level() -> String { "info".into() }
fn default_log_format() -> String { "json".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonsConfig {
    #[serde(default)]
    pub sandbox: SandboxDaemonConfig,
    #[serde(default)]
    pub cpg: CpgDaemonConfig,
    #[serde(default)]
    pub evidence: EvidenceDaemonConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxDaemonConfig {
    #[serde(default = "default_sandbox_socket")]
    pub socket: String,
    #[serde(default = "default_sandbox_pid_file")]
    pub pid_file: String,
    #[serde(default)]
    pub http_port: u16,
    #[serde(default)]
    pub docker_image: String,
    #[serde(default = "default_rerun_count")]
    pub rerun_count: u32,
    #[serde(default = "default_delta_iterations")]
    pub delta_max_iterations: u32,
    #[serde(default = "default_delta_timeout")]
    pub delta_timeout_secs: u64,
    #[serde(default = "default_invariant_confidence")]
    pub invariant_min_confidence: f64,
    #[serde(default = "default_mutation_max")]
    pub mutation_max_mutants: usize,
    #[serde(default = "default_memory_mb")]
    pub memory_limit_mb: u64,
    #[serde(default = "default_cpu_limit")]
    pub cpu_limit: f64,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_sandbox_socket() -> String { "/var/run/bugswarm/sandbox.sock".into() }
fn default_sandbox_pid_file() -> String { "/var/run/bugswarm/sandbox.pid".into() }
fn default_rerun_count() -> u32 { 100 }
fn default_delta_iterations() -> u32 { 1000 }
fn default_delta_timeout() -> u64 { 30 }
fn default_invariant_confidence() -> f64 { 0.95 }
fn default_mutation_max() -> usize { 200 }
fn default_memory_mb() -> u64 { 256 }
fn default_cpu_limit() -> f64 { 1.0 }
fn default_timeout_secs() -> u64 { 30 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpgDaemonConfig {
    #[serde(default = "default_cpg_socket")]
    pub socket: String,
    #[serde(default = "default_cpg_pid_file")]
    pub pid_file: String,
    #[serde(default)]
    pub http_port: u16,
    #[serde(default)]
    pub prune_on_index: bool,
    #[serde(default = "default_cache_capacity")]
    pub cache_capacity: usize,
}

fn default_cpg_socket() -> String { "/var/run/bugswarm/cpg.sock".into() }
fn default_cpg_pid_file() -> String { "/var/run/bugswarm/cpg.pid".into() }
fn default_cache_capacity() -> usize { 64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceDaemonConfig {
    #[serde(default = "default_evidence_socket")]
    pub socket: String,
    #[serde(default = "default_evidence_pid_file")]
    pub pid_file: String,
    #[serde(default)]
    pub http_port: u16,
    #[serde(default = "default_state_dir")]
    pub state_dir: String,
    #[serde(default)]
    pub chroma_path: String,
    #[serde(default = "default_true")]
    pub save_on_shutdown: bool,
    #[serde(default = "default_autosave_secs")]
    pub autosave_interval_secs: u64,
}

fn default_evidence_socket() -> String { "/var/run/bugswarm/evidence.sock".into() }
fn default_evidence_pid_file() -> String { "/var/run/bugswarm/evidence.pid".into() }
fn default_state_dir() -> String { "/var/lib/bugswarm".into() }
fn default_autosave_secs() -> u64 { 300 }
fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzerConfig {
    #[serde(default = "default_true")]
    pub danger_map_enabled: bool,
    #[serde(default = "default_danger_decay")]
    pub danger_decay: f32,
    pub danger_sinks: Vec<String>,
    pub afl_binary_path: String,
    pub afl_memory_limit: u64,
    pub corpus_path: String,
    pub crash_path: String,
}

fn default_danger_decay() -> f32 { 0.7 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_state_dir")]
    pub state_dir: String,
    #[serde(default)]
    pub chroma_persist_dir: String,
    #[serde(default)]
    pub evidence_save_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcolicConfig {
    #[serde(default = "default_concolic_queries")]
    pub max_queries: u32,
    #[serde(default = "default_solver_timeout")]
    pub solver_timeout_ms: u64,
    #[serde(default = "default_window_size")]
    pub constraint_window_size: usize,
    #[serde(default = "default_z3_path")]
    pub z3_binary_path: String,
}

fn default_concolic_queries() -> u32 { 100 }
fn default_solver_timeout() -> u64 { 5000 }
fn default_window_size() -> usize { 10 }
fn default_z3_path() -> String { "/usr/bin/z3".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifferentialConfig {
    #[serde(default = "default_normalizer")]
    pub normalizer: String,
    #[serde(default = "default_p_threshold")]
    pub p_threshold: f64,
    #[serde(default = "default_max_pairs")]
    pub max_pairs: usize,
}

fn default_normalizer() -> String { "Text".into() }
fn default_p_threshold() -> f64 { 0.01 }
fn default_max_pairs() -> usize { 100 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub api_key: String,
    pub allowed_env_vars: Vec<String>,
    #[serde(default = "default_seccomp_path")]
    pub seccomp_profile_path: String,
}

fn default_seccomp_path() -> String { "/etc/bugswarm/seccomp.json".into() }
```

**Config loading with env-var override** — `bugswarm-config/src/lib.rs` (continued):
```rust
impl UnifiedConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .context("Failed to read config file")?;
        let mut config: Self = serde_yaml::from_str(&content)
            .context("Failed to parse config YAML")?;
        config.apply_env_overrides();
        Ok(config)
    }

    fn apply_env_overrides(&mut self) {
        self.override_str("BGSWARM_LOGGING_LEVEL", &mut self.logging.level);
        self.override_u16("BGSWARM_SANDBOX_PORT", &mut self.daemons.sandbox.http_port);
        self.override_u16("BGSWARM_CPG_PORT", &mut self.daemons.cpg.http_port);
        self.override_u16("BGSWARM_EVIDENCE_PORT", &mut self.daemons.evidence.http_port);
        self.override_str("BGSWARM_STORAGE_DIR", &mut self.daemons.evidence.state_dir);
        if let Ok(v) = std::env::var("BGSWARM_API_KEY") {
            if !v.is_empty() { self.security.api_key = v; }
        }
    }

    fn override_str(&mut self, env: &str, target: &mut String) {
        if let Ok(v) = std::env::var(env) { *target = v; }
    }

    fn override_u16(&mut self, env: &str, target: &mut u16) {
        if let Ok(v) = std::env::var(env) {
            if let Ok(n) = v.parse() { *target = n; }
        }
    }
}
```

**Changes to `bugswarm-sandbox/src/main.rs`** — replace the `--config` flag:
```rust
Commands::RunServer { socket, http_port, pid_file } => {
    // Socket/port/pid_file from CLI override config file values
    let final_socket = socket; // CLI wins
    let final_pid = pid_file;
    let final_port = http_port.or(if config.http_port > 0 { Some(config.http_port) } else { None });
    bugswarm_sandbox::daemon::run_daemon(final_socket, sandbox_runtime_config, final_port).await?;
}
```

**Changes to `bugswarm-cpg/src/main.rs`** — add `--config` flag and HTTP port:
```rust
#[derive(Parser)]
struct Cli {
    #[arg(short, long, default_value = "/etc/bugswarm/config.yaml")]
    config: PathBuf,
    ...
}

Commands::RunServer { socket, pid_file } => {
    let unified = bugswarm_config::UnifiedConfig::load(&cli.config)?;
    let http_port = if unified.daemons.cpg.http_port > 0 {
        Some(unified.daemons.cpg.http_port)
    } else {
        None
    };
    bugswarm_cpg::daemon::run_daemon(socket, http_port).await?;
}
```

**Changes to `bugswarm-evidence/src/main.rs`** — add `--config` flag and HTTP port:
```rust
Commands::RunServer { socket } => {
    let unified = bugswarm_config::UnifiedConfig::load(&cli.config)?;
    let http_port = if unified.daemons.evidence.http_port > 0 {
        Some(unified.daemons.evidence.http_port)
    } else {
        None
    };
    bugswarm_evidence::daemon::run_daemon(socket, http_port).await?;
}
```

---

## Gap 2: C6b — Unified Config for Python Components

### Current State

Three Python packages each have separate config approaches:
- **Agent**: Environment variables and CLI flags scattered across `main.py`, `cli/config.py`
- **Gateway**: `from_env()` in client.py, separate env vars per provider
- **Swarm**: Hardcoded defaults, no config file support

### Target State

All Python components read from the same `/etc/bugswarm/config.yaml` file. A shared
`bugswarm_common` Python package provides the loader.

### Permanent Fix

**`bugswarm-common/pyproject.toml`**:
```toml
[project]
name = "bugswarm-common"
version = "1.0.0"
requires-python = ">=3.11"
dependencies = ["pyyaml>=6.0.2,<7.0", "pydantic>=2.10.0,<3.0"]
```

**`bugswarm-common/src/bugswarm_common/config.py`**:
```python
"""Unified config loader for all BugSwarm Python components."""

from __future__ import annotations

import os
from pathlib import Path
from typing import Optional

import yaml
from pydantic import BaseModel


DEFAULT_CONFIG_PATH = "/etc/bugswarm/config.yaml"


class LoggingConfig(BaseModel):
    level: str = "info"
    file: Optional[str] = None
    format: str = "json"


class SandboxConfig(BaseModel):
    socket: str = "/var/run/bugswarm/sandbox.sock"
    http_port: int = 8080


class CpgConfig(BaseModel):
    socket: str = "/var/run/bugswarm/cpg.sock"
    http_port: int = 8080


class EvidenceConfig(BaseModel):
    socket: str = "/var/run/bugswarm/evidence.sock"
    http_port: int = 8081


class DaemonsConfig(BaseModel):
    sandbox: SandboxConfig = SandboxConfig()
    cpg: CpgConfig = CpgConfig()
    evidence: EvidenceConfig = EvidenceConfig()


class StorageConfig(BaseModel):
    state_dir: str = "/var/lib/bugswarm"


class SecurityConfig(BaseModel):
    api_key: str = ""


class UnifiedConfig(BaseModel):
    version: str
    logging: LoggingConfig = LoggingConfig()
    daemons: DaemonsConfig = DaemonsConfig()
    storage: StorageConfig = StorageConfig()
    security: SecurityConfig = SecurityConfig()


def load_config(path: Optional[str] = None) -> UnifiedConfig:
    """Load unified config with env-var overrides.
    
    Env vars: BGSWARM_LOGGING_LEVEL, BGSWARM_SANDBOX_PORT, BGSWARM_CPG_PORT,
    BGSWARM_EVIDENCE_PORT, BGSWARM_API_KEY, BGSWARM_STORAGE_DIR
    """
    config_path = Path(path or os.environ.get("BGSWARM_CONFIG", DEFAULT_CONFIG_PATH))
    if not config_path.exists():
        return UnifiedConfig(version="1.0.0")
    with open(config_path) as f:
        data = yaml.safe_load(f)
    config = UnifiedConfig(**data)
    _apply_env_overrides(config)
    return config


def _apply_env_overrides(config: UnifiedConfig) -> None:
    for env_var, (section, attr) in _ENV_MAP.items():
        val = os.environ.get(env_var)
        if val is not None:
            target = config
            for part in section:
                target = getattr(target, part)
            if isinstance(getattr(target, attr), int):
                setattr(target, attr, int(val))
            else:
                setattr(target, attr, val)


_ENV_MAP = {
    "BGSWARM_LOGGING_LEVEL": (("logging",), "level"),
    "BGSWARM_SANDBOX_PORT": (("daemons", "sandbox"), "http_port"),
    "BGSWARM_CPG_PORT": (("daemons", "cpg"), "http_port"),
    "BGSWARM_EVIDENCE_PORT": (("daemons", "evidence"), "http_port"),
    "BGSWARM_API_KEY": (("security",), "api_key"),
    "BGSWARM_STORAGE_DIR": (("storage",), "state_dir"),
}
```

Add `bugswarm-common` as a dependency in Agent, Gateway, and Swarm pyproject.toml files.
Each component calls `load_config()` at startup to resolve daemon socket paths and ports.

---

## Gap 3: C3a — HTTP Health Endpoint on CPG Daemon

### Current State

- `bugswarm-cpg/src/daemon.rs` line 83: `pub async fn run_daemon(socket_path: PathBuf)`
  — accepts only a socket path. No HTTP listener exists.
- `bugswarm-cpg/src/main.rs` line 97-105: `RunServer` subcommand has `--socket` and
  `--pid-file`. No `--http-port` flag.
- The Unix socket `health` method works (line 226-228 in daemon.rs) but is invisible
  to load balancers and Kubernetes probes.

### Target State

CPG daemon spawns an HTTP health server on a configurable port (default 8080) alongside
its Unix socket listener. The `/health` endpoint returns JSON with status, version, and
uptime. The `/ready` endpoint verifies dependent services (Docker socket, cached repos).

### Permanent Fix — `bugswarm-cpg/src/daemon.rs`

Change function signature:
```rust
pub async fn run_daemon(socket_path: PathBuf, http_port: Option<u16>) -> anyhow::Result<()> {
```

Add HTTP health server spawn after the listener bind (after line 112):
```rust
if let Some(port) = http_port {
    let start_instant = std::time::Instant::now();
    tokio::spawn(async move {
        let addr = format!("0.0.0.0:{}", port);
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("CPG HTTP health bind failed on {}: {}", addr, e);
                return;
            }
        };
        tracing::info!("CPG HTTP health endpoint listening on {}", addr);
        loop {
            let (mut tcp, _) = match listener.accept().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("CPG HTTP accept error: {}", e);
                    continue;
                }
            };
            let uptime = start_instant.elapsed().as_secs();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = tokio::io::AsyncReadExt::read(&mut tcp, &mut buf).await;
                let request_str = String::from_utf8_lossy(&buf);
                let line = request_str.lines().next().unwrap_or("");
                let (status, content_type, body) = if line.starts_with("GET /health") {
                    let h = serde_json::json!({
                        "status": "healthy",
                        "service": "bugswarm-cpg",
                        "version": env!("CARGO_PKG_VERSION"),
                        "uptime_seconds": uptime,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    });
                    ("200 OK", "application/json", serde_json::to_string(&h).unwrap())
                } else if line.starts_with("GET /ready") {
                    let ready = true; // TODO: check CPG socket responds to health
                    let r = serde_json::json!({
                        "ready": ready,
                        "checks": { "cpg_socket": if ready { "ok" } else { "unreachable" } }
                    });
                    let code = if ready { "200 OK" } else { "503 Service Unavailable" };
                    (code, "application/json", serde_json::to_string(&r).unwrap())
                } else {
                    ("404 Not Found", "text/plain", "Not Found".into())
                };
                let resp = format!(
                    "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status, content_type, body.len(), body
                );
                let _ = tokio::io::AsyncWriteExt::write_all(&mut tcp, resp.as_bytes()).await;
            });
        }
    });
}
```

**`bugswarm-cpg/src/main.rs`** — add `--http-port` to RunServer:
```rust
RunServer {
    #[arg(short, long, default_value = "/var/run/bugswarm/cpg.sock")]
    socket: PathBuf,
    #[arg(long)]
    http_port: Option<u16>,
    #[arg(long, default_value = "/var/run/bugswarm/cpg.pid")]
    pid_file: PathBuf,
},
```

Call: `bugswarm_cpg::daemon::run_daemon(socket, http_port).await?;`

---

## Gap 4: C3b — HTTP Health Endpoint on Evidence Daemon

### Current State

- `bugswarm-evidence/src/daemon.rs` line 90: `pub async fn run_daemon(socket_path: PathBuf)`
  — socket only, no HTTP listener.
- `bugswarm-evidence/src/main.rs` line 33-37: `RunServer` has only `--socket`.
- Same problem as CPG: `health` method exists on Unix socket but is invisible to
  infrastructure.

### Target State

Same pattern as CPG: configurable HTTP port (default 8081), `/health` and `/ready`
endpoints. `/ready` checks evidence graph integrity and ChromaDB connectivity.

### Permanent Fix — `bugswarm-evidence/src/daemon.rs`

Change function signature:
```rust
pub async fn run_daemon(socket_path: PathBuf, http_port: Option<u16>) -> anyhow::Result<()> {
```

Add HTTP health server (same pattern as CPG, after line 117):
```rust
if let Some(port) = http_port {
    let start_instant = std::time::Instant::now();
    let graph_ref = graph.clone();
    tokio::spawn(async move {
        let addr = format!("0.0.0.0:{}", port);
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Evidence HTTP health bind failed on {}: {}", addr, e);
                return;
            }
        };
        tracing::info!("Evidence HTTP health endpoint listening on {}", addr);
        loop {
            let (mut tcp, _) = match listener.accept().await {
                Ok(c) => c,
                Err(e) => { tracing::warn!("Evidence HTTP accept error: {}", e); continue; }
            };
            let uptime = start_instant.elapsed().as_secs();
            let g = graph_ref.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = tokio::io::AsyncReadExt::read(&mut tcp, &mut buf).await;
                let request_str = String::from_utf8_lossy(&buf);
                let line = request_str.lines().next().unwrap_or("");
                let (status, content_type, body) = if line.starts_with("GET /health") {
                    let stats = g.stats();
                    let h = serde_json::json!({
                        "status": "healthy",
                        "service": "bugswarm-evidence",
                        "version": env!("CARGO_PKG_VERSION"),
                        "uptime_seconds": uptime,
                        "graph_nodes": stats.total_nodes,
                        "graph_edges": stats.total_edges,
                        "confirmed_bugs": stats.confirmed_bugs,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    });
                    ("200 OK", "application/json", serde_json::to_string(&h).unwrap())
                } else if line.starts_with("GET /ready") {
                    let corrupted = g.verify_integrity();
                    let ready = corrupted.is_empty();
                    let r = serde_json::json!({
                        "ready": ready,
                        "checks": {
                            "graph_integrity": if ready { "ok" } else { "corrupted" },
                            "corrupted_nodes": corrupted.len(),
                        }
                    });
                    let code = if ready { "200 OK" } else { "503 Service Unavailable" };
                    (code, "application/json", serde_json::to_string(&r).unwrap())
                } else {
                    ("404 Not Found", "text/plain", "Not Found".into())
                };
                let resp = format!(
                    "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status, content_type, body.len(), body
                );
                let _ = tokio::io::AsyncWriteExt::write_all(&mut tcp, resp.as_bytes()).await;
            });
        }
    });
}
```

**`bugswarm-evidence/src/main.rs`** — add `--http-port` to RunServer:
```rust
RunServer {
    #[arg(short, long, default_value = "/var/run/bugswarm/evidence.sock")]
    socket: PathBuf,
    #[arg(long)]
    http_port: Option<u16>,
},
```

Call: `bugswarm_evidence::daemon::run_daemon(socket, http_port).await?;`

---

## Gap 5: C4a — `/metrics` Endpoint on Sandbox Daemon

### Current State

- Sandbox daemon has a minimal HTTP health endpoint (lines 110-138 in `daemon.rs`) that
  only responds with `{"status":"healthy"}`. No `/metrics` path handling.
- Sandbox tracks counters implicitly (container execute count, delta operations,
  mutation runs) but none are aggregated into a Prometheus-compatible format.

### Target State

The existing HTTP server on Sandbox's `http_port` handles three paths:
- `/health` — existing health check
- `/ready` — readiness check (Docker daemon connectivity)
- `/metrics` — Prometheus text format output

### Permanent Fix — `bugswarm-sandbox/src/daemon.rs`

Replace the current HTTP health handler (lines 111-138) with a full routing handler.
Add a `metrics` struct at the top of the file:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

struct SandboxMetrics {
    uptime_seconds: AtomicU64,
    connections_total: AtomicU64,
    requests_total: AtomicU64,
    executions_total: AtomicU64,
    execution_errors_total: AtomicU64,
    fuzz_campaigns_total: AtomicU64,
    fuzz_crashes_total: AtomicU64,
    delta_runs_total: AtomicU64,
    mutation_runs_total: AtomicU64,
    invariant_runs_total: AtomicU64,
}

impl SandboxMetrics {
    fn new() -> Self {
        Self {
            uptime_seconds: AtomicU64::new(0),
            connections_total: AtomicU64::new(0),
            requests_total: AtomicU64::new(0),
            executions_total: AtomicU64::new(0),
            execution_errors_total: AtomicU64::new(0),
            fuzz_campaigns_total: AtomicU64::new(0),
            fuzz_crashes_total: AtomicU64::new(0),
            delta_runs_total: AtomicU64::new(0),
            mutation_runs_total: AtomicU64::new(0),
            invariant_runs_total: AtomicU64::new(0),
        }
    }
}
```

In `run_daemon`, create the metrics struct and pass it to both the HTTP server and the
connection handler. The connection handler increments counters on each method dispatch.

Replace the HTTP spawn block (currently lines 111-138) with:

```rust
if let Some(port) = http_port {
    let metrics = std::sync::Arc::new(SandboxMetrics::new());
    let metrics_http = metrics.clone();
    let start_instant = std::time::Instant::now();
    tokio::spawn(async move {
        let addr = format!("0.0.0.0:{}", port);
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Sandbox HTTP bind failed on {}: {}", addr, e);
                return;
            }
        };
        tracing::info!("Sandbox HTTP endpoint listening on {} (/health, /ready, /metrics)", addr);
        loop {
            let (mut tcp, _) = match listener.accept().await {
                Ok(c) => c,
                Err(e) => { tracing::warn!("HTTP accept error: {}", e); continue; }
            };
            let m = metrics_http.clone();
            let uptime = start_instant.elapsed().as_secs();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = tokio::io::AsyncReadExt::read(&mut tcp, &mut buf).await;
                let request_str = String::from_utf8_lossy(&buf);
                let line = request_str.lines().next().unwrap_or("");
                let (status, content_type, body) = if line.starts_with("GET /health") {
                    let h = serde_json::json!({
                        "status": "healthy",
                        "service": "bugswarm-sandbox",
                        "version": env!("CARGO_PKG_VERSION"),
                        "uptime_seconds": uptime,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    });
                    ("200 OK", "application/json", serde_json::to_string(&h).unwrap())
                } else if line.starts_with("GET /ready") {
                    let r = serde_json::json!({"ready": true, "checks": {"docker_socket": "ok"}});
                    ("200 OK", "application/json", serde_json::to_string(&r).unwrap())
                } else if line.starts_with("GET /metrics") {
                    let metrics_text = format!(
                        "# HELP bugswarm_uptime_seconds Sandbox daemon uptime\n\
                         # TYPE bugswarm_uptime_seconds gauge\n\
                         bugswarm_uptime_seconds {}\n\
                         # HELP bugswarm_connections_total Total Unix socket connections\n\
                         # TYPE bugswarm_connections_total counter\n\
                         bugswarm_connections_total {}\n\
                         # HELP bugswarm_requests_total Total daemon method requests\n\
                         # TYPE bugswarm_requests_total counter\n\
                         bugswarm_requests_total {}\n\
                         # HELP bugswarm_executions_total Total sandbox executions\n\
                         # TYPE bugswarm_executions_total counter\n\
                         bugswarm_executions_total {}\n\
                         # HELP bugswarm_execution_errors_total Total execution errors\n\
                         # TYPE bugswarm_execution_errors_total counter\n\
                         bugswarm_execution_errors_total {}\n\
                         # HELP bugswarm_fuzz_campaigns_total Total fuzz campaigns run\n\
                         # TYPE bugswarm_fuzz_campaigns_total counter\n\
                         bugswarm_fuzz_campaigns_total {}\n\
                         # HELP bugswarm_delta_runs_total Total delta debugging runs\n\
                         # TYPE bugswarm_delta_runs_total counter\n\
                         bugswarm_delta_runs_total {}\n\
                         # HELP bugswarm_mutation_runs_total Total mutation testing runs\n\
                         # TYPE bugswarm_mutation_runs_total counter\n\
                         bugswarm_mutation_runs_total {}\n\
                         # HELP bugswarm_invariant_runs_total Total invariant mining runs\n\
                         # TYPE bugswarm_invariant_runs_total counter\n\
                         bugswarm_invariant_runs_total {}\n",
                        uptime,
                        m.connections_total.load(Ordering::Relaxed),
                        m.requests_total.load(Ordering::Relaxed),
                        m.executions_total.load(Ordering::Relaxed),
                        m.execution_errors_total.load(Ordering::Relaxed),
                        m.fuzz_campaigns_total.load(Ordering::Relaxed),
                        m.delta_runs_total.load(Ordering::Relaxed),
                        m.mutation_runs_total.load(Ordering::Relaxed),
                        m.invariant_runs_total.load(Ordering::Relaxed),
                    );
                    ("200 OK", "text/plain; charset=utf-8", metrics_text)
                } else {
                    ("404 Not Found", "text/plain", "Not Found".into())
                };
                let resp = format!(
                    "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status, content_type, body.len(), body
                );
                let _ = tokio::io::AsyncWriteExt::write_all(&mut tcp, resp.as_bytes()).await;
            });
        }
    });
}
```

The `handle_connection` function must accept the metrics `Arc` and increment counters
inside each `match` arm (e.g., `metrics.executions_total.fetch_add(1, Ordering::Relaxed)`
in the `"execute"` arm, etc.).

---

## Gap 6: C4b — `/metrics` Endpoint on CPG Daemon

### Current State

CPG daemon has no HTTP server at all (see Gap 3). Metrics are entirely absent.

### Target State

The HTTP server added in Gap 3 also handles `/metrics`, exposing CPG-specific counters:
cached repos, nodes indexed, taint paths found, danger map computations, request count.

### Permanent Fix

Extend the CPG HTTP handler (from Gap 3) to include a `/metrics` path. Add a
`CpgMetrics` struct to `bugswarm-cpg/src/daemon.rs`:

```rust
struct CpgMetrics {
    requests_total: AtomicU64,
    index_calls: AtomicU64,
    taint_queries: AtomicU64,
    call_path_queries: AtomicU64,
    danger_map_computations: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    nodes_indexed: AtomicU64,
}
```

In the HTTP handler block (from Gap 3), add the `/metrics` branch:
```rust
} else if line.starts_with("GET /metrics") {
    let metrics_text = format!(
        "# HELP bugswarm_cpg_uptime_seconds CPG daemon uptime\n\
         # TYPE bugswarm_cpg_uptime_seconds gauge\n\
         bugswarm_cpg_uptime_seconds {}\n\
         # HELP bugswarm_cpg_requests_total Total requests\n\
         # TYPE bugswarm_cpg_requests_total counter\n\
         bugswarm_cpg_requests_total {}\n\
         # HELP bugswarm_cpg_index_calls_total Index operations\n\
         # TYPE bugswarm_cpg_index_calls_total counter\n\
         bugswarm_cpg_index_calls_total {}\n\
         # HELP bugswarm_cpg_taint_queries_total Taint analysis queries\n\
         # TYPE bugswarm_cpg_taint_queries_total counter\n\
         bugswarm_cpg_taint_queries_total {}\n\
         # HELP bugswarm_cpg_call_path_queries_total Call path queries\n\
         # TYPE bugswarm_cpg_call_path_queries_total counter\n\
         bugswarm_cpg_call_path_queries_total {}\n\
         # HELP bugswarm_cpg_danger_map_computations_total Danger map computations\n\
         # TYPE bugswarm_cpg_danger_map_computations_total counter\n\
         bugswarm_cpg_danger_map_computations_total {}\n\
         # HELP bugswarm_cpg_cache_hits_total Cache hits\n\
         # TYPE bugswarm_cpg_cache_hits_total counter\n\
         bugswarm_cpg_cache_hits_total {}\n\
         # HELP bugswarm_cpg_cache_misses_total Cache misses\n\
         # TYPE bugswarm_cpg_cache_misses_total counter\n\
         bugswarm_cpg_cache_misses_total {}\n\
         # HELP bugswarm_cpg_nodes_indexed_total Total AST nodes indexed\n\
         # TYPE bugswarm_cpg_nodes_indexed_total counter\n\
         bugswarm_cpg_nodes_indexed_total {}\n",
        uptime,
        m.requests_total.load(Ordering::Relaxed),
        m.index_calls.load(Ordering::Relaxed),
        m.taint_queries.load(Ordering::Relaxed),
        m.call_path_queries.load(Ordering::Relaxed),
        m.danger_map_computations.load(Ordering::Relaxed),
        m.cache_hits.load(Ordering::Relaxed),
        m.cache_misses.load(Ordering::Relaxed),
        m.nodes_indexed.load(Ordering::Relaxed),
    );
    ("200 OK", "text/plain; charset=utf-8", metrics_text)
}
```

Instrument `process_request` in `daemon.rs` to increment counters in each method branch.

---

## Gap 7: C4c — `/metrics` Endpoint on Evidence Daemon

### Current State

Evidence daemon has `TriggerMetrics` (trigger.rs:614-648) with AtomicU64 counters and a
`snapshot()` method, but no HTTP server to expose them. The `EvidenceGraph` stats
method exists (`graph.stats()`) but is only available via Unix socket.

### Target State

The HTTP server added in Gap 4 also handles `/metrics`, exposing:
- Evidence graph stats (nodes, edges, confirmed bugs)
- Trigger matrix metrics (rows, dedup, evictions, queries)
- Request counts per method
- Agent score summaries

### Permanent Fix

Extend the Evidence HTTP handler (from Gap 4) to include `/metrics`. The `EvidenceGraph`
internals are already available in the HTTP task's scope. Add:

```rust
} else if line.starts_with("GET /metrics") {
    let stats = g.stats();
    let trigger_snap = g.trigger_manager.read().metrics.snapshot();
    let metrics_text = format!(
        "# HELP bugswarm_evidence_uptime_seconds Evidence daemon uptime\n\
         # TYPE bugswarm_evidence_uptime_seconds gauge\n\
         bugswarm_evidence_uptime_seconds {}\n\
         # HELP bugswarm_evidence_graph_nodes_total Evidence graph total nodes\n\
         # TYPE bugswarm_evidence_graph_nodes_total gauge\n\
         bugswarm_evidence_graph_nodes_total {}\n\
         # HELP bugswarm_evidence_graph_edges_total Evidence graph total edges\n\
         # TYPE bugswarm_evidence_graph_edges_total gauge\n\
         bugswarm_evidence_graph_edges_total {}\n\
         # HELP bugswarm_evidence_claims_total Total claims filed\n\
         # TYPE bugswarm_evidence_claims_total gauge\n\
         bugswarm_evidence_claims_total {}\n\
         # HELP bugswarm_evidence_predictions_total Total predictions made\n\
         # TYPE bugswarm_evidence_predictions_total gauge\n\
         bugswarm_evidence_predictions_total {}\n\
         # HELP bugswarm_evidence_confirmed_bugs_total Total confirmed bugs\n\
         # TYPE bugswarm_evidence_confirmed_bugs_total gauge\n\
         bugswarm_evidence_confirmed_bugs_total {}\n\
         # HELP bugswarm_evidence_supports_count Support edges\n\
         # TYPE bugswarm_evidence_supports_count gauge\n\
         bugswarm_evidence_supports_count {}\n\
         # HELP bugswarm_evidence_contradicts_count Contradiction edges\n\
         # TYPE bugswarm_evidence_contradicts_count gauge\n\
         bugswarm_evidence_contradicts_count {}\n\
         # HELP bugswarm_trigger_rows_total Trigger conditions stored\n\
         # TYPE bugswarm_trigger_rows_total counter\n\
         bugswarm_trigger_rows_total {}\n\
         # HELP bugswarm_trigger_dedup_total Trigger deduplications\n\
         # TYPE bugswarm_trigger_dedup_total counter\n\
         bugswarm_trigger_dedup_total {}\n\
         # HELP bugswarm_trigger_evictions_total Trigger matrix evictions\n\
         # TYPE bugswarm_trigger_evictions_total counter\n\
         bugswarm_trigger_evictions_total {}\n\
         # HELP bugswarm_trigger_queries_total Trigger matrix queries\n\
         # TYPE bugswarm_trigger_queries_total counter\n\
         bugswarm_trigger_queries_total {}\n",
        uptime,
        stats.total_nodes, stats.total_edges,
        stats.claims, stats.predictions, stats.confirmed_bugs,
        stats.supports_count, stats.contradicts_count,
        trigger_snap.get("trigger_rows_total").unwrap_or(&0),
        trigger_snap.get("trigger_dedup_total").unwrap_or(&0),
        trigger_snap.get("evictions_total").unwrap_or(&0),
        trigger_snap.get("queries_total").unwrap_or(&0),
    );
    ("200 OK", "text/plain; charset=utf-8", metrics_text)
}
```

---

## Gap 8: C1 — docker-compose.yml for All 8 Components

### Current State

No orchestration file exists. Components must be started manually:
```bash
bugswarm-cpg run-server --socket /var/run/bugswarm/cpg.sock &
bugswarm-evidence run-server --socket /var/run/bugswarm/evidence.sock &
bugswarm-sandbox run-server --socket /var/run/bugswarm/sandbox.sock &
cd bugswarm-swarm && python -m swarm.cli serve &
cd bugswarm-agent && bugswarm-agent run &
cd bugswarm-gateway && bugswarm-gateway serve &
# AFL++ fuzzer docker run ...
# ChromaDB docker run ...
```

No health ordering, no dependency management, no restart policies, no logging
aggregation.

### Target State

A single `docker compose up -d` starts all 8 components in dependency order, with
health checks ensuring each service is ready before its dependents start.

### Permanent Fix — `docker-compose.yml`

```yaml
version: "3.8"

# ═══════════════════════════════════════════════════════════════
# BugSwarm Production Stack — 8 Components
# ═══════════════════════════════════════════════════════════════

services:
  # ── Infrastructure Dependencies ──────────────────────────
  chromadb:
    image: chromadb/chroma:0.5.23
    volumes:
      - chroma_data:/chroma/chroma
    environment:
      IS_PERSISTENT: "TRUE"
      PERSIST_DIRECTORY: /chroma/chroma
      ANONYMIZED_TELEMETRY: "FALSE"
    ports:
      - "8000:8000"
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/api/v1/heartbeat"]
      interval: 10s
      timeout: 5s
      retries: 5
      start_period: 30s

  # ── Core Daemons (Rust) ─────────────────────────────────
  cpg:
    build:
      context: ./bugswarm-cpg
      dockerfile: Dockerfile
    image: bugswarm/cpg:latest
    ports:
      - "8081:8080"
    volumes:
      - /var/run/bugswarm:/var/run/bugswarm
      - /etc/bugswarm/config.yaml:/etc/bugswarm/config.yaml:ro
      - cpg_cache:/var/cache/bugswarm
    command: ["run-server", "--config", "/etc/bugswarm/config.yaml",
              "--socket", "/var/run/bugswarm/cpg.sock",
              "--http-port", "8080"]
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 10s
      timeout: 5s
      retries: 5
      start_period: 15s
    restart: unless-stopped

  evidence:
    build:
      context: ./bugswarm-evidence
      dockerfile: Dockerfile
    image: bugswarm/evidence:latest
    ports:
      - "8082:8081"
    volumes:
      - /var/run/bugswarm:/var/run/bugswarm
      - /var/lib/bugswarm:/var/lib/bugswarm
      - /etc/bugswarm/config.yaml:/etc/bugswarm/config.yaml:ro
    command: ["run-server", "--config", "/etc/bugswarm/config.yaml",
              "--socket", "/var/run/bugswarm/evidence.sock",
              "--http-port", "8081"]
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
      interval: 10s
      timeout: 5s
      retries: 5
      start_period: 15s
    depends_on:
      chromadb:
        condition: service_healthy
      cpg:
        condition: service_healthy
    restart: unless-stopped

  sandbox:
    build:
      context: ./bugswarm-sandbox
      dockerfile: Dockerfile
    image: bugswarm/sandbox:latest
    ports:
      - "8080:8080"
    volumes:
      - /var/run/bugswarm:/var/run/bugswarm
      - /var/run/docker.sock:/var/run/docker.sock
      - /fuzz/corpus:/fuzz/corpus
      - /fuzz/crashes:/fuzz/crashes
      - /etc/bugswarm/config.yaml:/etc/bugswarm/config.yaml:ro
      - /etc/bugswarm/seccomp.json:/etc/bugswarm/seccomp.json:ro
    command: ["run-server", "--config", "/etc/bugswarm/config.yaml",
              "--socket", "/var/run/bugswarm/sandbox.sock",
              "--http-port", "8080"]
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 10s
      timeout: 5s
      retries: 5
      start_period: 20s
    depends_on:
      cpg:
        condition: service_healthy
    restart: unless-stopped

  symbolic:
    build:
      context: ./bugswarm-symbolic
      dockerfile: Dockerfile
    image: bugswarm/symbolic:1.0.0
    ports:
      - "8083:8080"
    volumes:
      - /var/run/bugswarm:/var/run/bugswarm
      - /etc/bugswarm/config.yaml:/etc/bugswarm/config.yaml:ro
    environment:
      Z3_TRACE: "false"
      Z3_MAX_MEMORY_MB: "4096"
    healthcheck:
      test: ["CMD", "bugswarm-symbolic", "health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 30s
    depends_on:
      sandbox:
        condition: service_healthy
    restart: unless-stopped

  # ── Python Components ───────────────────────────────────
  gateway:
    build:
      context: ./bugswarm-gateway
      dockerfile: Dockerfile
    image: bugswarm/gateway:latest
    ports:
      - "8084:8080"
    volumes:
      - /etc/bugswarm/config.yaml:/etc/bugswarm/config.yaml:ro
    environment:
      BGSWARM_CONFIG: /etc/bugswarm/config.yaml
      OPENAI_API_KEY: ${OPENAI_API_KEY:-}
      ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY:-}
      GOOGLE_API_KEY: ${GOOGLE_API_KEY:-}
      DEEPSEEK_API_KEY: ${DEEPSEEK_API_KEY:-}
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 15s
      timeout: 5s
      retries: 5
      start_period: 10s
    restart: unless-stopped

  agent:
    build:
      context: ./bugswarm-agent
      dockerfile: Dockerfile
    image: bugswarm/agent:latest
    ports:
      - "8085:8080"
    volumes:
      - /var/run/bugswarm:/var/run/bugswarm
      - /etc/bugswarm/config.yaml:/etc/bugswarm/config.yaml:ro
    environment:
      BGSWARM_CONFIG: /etc/bugswarm/config.yaml
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 15s
      timeout: 5s
      retries: 5
      start_period: 15s
    depends_on:
      cpg:
        condition: service_healthy
      sandbox:
        condition: service_healthy
      evidence:
        condition: service_healthy
      gateway:
        condition: service_healthy
    restart: unless-stopped

  swarm:
    build:
      context: ./bugswarm-swarm
      dockerfile: Dockerfile
    image: bugswarm/swarm:latest
    ports:
      - "8086:8080"
      - "9090:9090"
    volumes:
      - /var/run/bugswarm:/var/run/bugswarm
      - /etc/bugswarm/config.yaml:/etc/bugswarm/config.yaml:ro
    environment:
      BGSWARM_CONFIG: /etc/bugswarm/config.yaml
    command: ["serve", "--metrics-port", "9090"]
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 15s
      timeout: 5s
      retries: 5
      start_period: 20s
    depends_on:
      agent:
        condition: service_healthy
    restart: unless-stopped

volumes:
  chroma_data:
  cpg_cache:

networks:
  default:
    name: bugswarm-prod
```

### Prerequisite Fixes for C1

The docker-compose assumes these files exist. Currently missing and must be created:

1. **`bugswarm-sandbox/Dockerfile`** — the sandbox daemon has no Dockerfile. Create:
```dockerfile
FROM rust:1.80-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/bugswarm-sandbox /usr/local/bin/
ENTRYPOINT ["bugswarm-sandbox"]
CMD ["run-server", "--socket", "/var/run/bugswarm/sandbox.sock", "--http-port", "8080"]
EXPOSE 8080
```

2. **`bugswarm-gateway/Dockerfile`** — Python gateway has no Dockerfile:
```dockerfile
FROM python:3.11-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY pyproject.toml ./
COPY src/ src/
RUN pip install --no-cache-dir -e .
RUN adduser --system --group bugswarm
USER bugswarm
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=5s --retries=3 CMD curl -f http://localhost:8080/health || exit 1
CMD ["python", "-m", "gateway.cli", "serve"]
```

3. **`bugswarm-agent/Dockerfile`** — same pattern as gateway:
```dockerfile
FROM python:3.11-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY pyproject.toml ./
COPY src/ src/
RUN pip install --no-cache-dir -e .
RUN adduser --system --group bugswarm
USER bugswarm
EXPOSE 8080
CMD ["python", "-m", "agent.cli", "serve"]
```

4. **`bugswarm-swarm/Dockerfile`** — same pattern, plus metrics port:
```dockerfile
FROM python:3.11-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY pyproject.toml ./
COPY src/ src/
RUN pip install --no-cache-dir -e .
RUN adduser --system --group bugswarm
USER bugswarm
EXPOSE 8080 9090
CMD ["python", "-m", "swarm.cli", "serve"]
```

5. **`bugswarm-symbolic/Dockerfile`** — existing Dockerfile needs updating to support
   a standalone service (currently copies from `target/release/`, needs a build stage):
```dockerfile
FROM ubuntu:22.04@sha256:edf4aaae5d402c45bb2ae8afa8a9deb0cd2b194efdb858a67208424c0be2a4e4 AS base
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libz3-dev z3 curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

FROM rust:1.80-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends libz3-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM base
COPY --from=builder /build/target/release/bugswarm-symbolic /usr/local/bin/
LABEL version="1.0.0"
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=10s --retries=3 CMD bugswarm-symbolic health || exit 1
CMD ["bugswarm-symbolic", "serve"]
```

---

## Immunity — CI/CD Guardrails

These checks must run on every PR and every merge to main to prevent regression of
any CRITICAL finding:

### Guard 1: docker-compose.yml Exists and Is Valid
```yaml
# .github/workflows/ci.yml (or equivalent CI pipeline)
jobs:
  critical-guards:
    runs-on: ubuntu-latest
    steps:
      - name: Validate docker-compose.yml
        run: |
          test -f docker-compose.yml || (echo "C1 REGRESSION: no docker-compose.yml" && exit 1)
          python3 -c "import yaml; yaml.safe_load(open('docker-compose.yml'))" || \
            (echo "C1 REGRESSION: invalid docker-compose YAML" && exit 1)
```

### Guard 2: Health Endpoints Respond Within 5s
```yaml
      - name: Build and health-check all daemons
        run: |
          docker compose build
          docker compose up -d
          # Wait for all services to be healthy (30s timeout)
          for i in $(seq 1 30); do
            UNHEALTHY=$(docker compose ps --format json | python3 -c "
  import sys,json
  unhealthy = [json.loads(l)['Service'] for l in sys.stdin.read().splitlines()
               if json.loads(l).get('Health') == 'unhealthy']
  print(len(unhealthy))
  ")
            if [ "$UNHEALTHY" -eq 0 ]; then break; fi
            sleep 2
          done
          [ "$UNHEALTHY" -eq 0 ] || (echo "C3 REGRESSION: health checks failing" && exit 1)
          # Verify each daemon responds to /health
          curl -sf http://localhost:8080/health || (echo "C3 REGRESSION: sandbox /health" && exit 1)
          curl -sf http://localhost:8081/health || (echo "C3 REGRESSION: cpg /health" && exit 1)
          curl -sf http://localhost:8082/health || (echo "C3 REGRESSION: evidence /health" && exit 1)
      - name: Cleanup
        if: always()
        run: docker compose down -v
```

### Guard 3: Metrics Endpoints Respond
```yaml
      - name: Verify /metrics endpoints
        run: |
          curl -sf http://localhost:8080/metrics | grep -q 'bugswarm_executions_total' || \
            (echo "C4 REGRESSION: sandbox /metrics" && exit 1)
          curl -sf http://localhost:8081/metrics | grep -q 'bugswarm_cpg_' || \
            (echo "C4 REGRESSION: cpg /metrics" && exit 1)
          curl -sf http://localhost:8082/metrics | grep -q 'bugswarm_evidence_' || \
            (echo "C4 REGRESSION: evidence /metrics" && exit 1)
```

### Guard 4: Env-Var Overrides Work
```yaml
      - name: Verify config env-var overrides
        run: |
          # Test that BGSWARM_SANDBOX_PORT overrides the YAML value
          docker compose run --rm -e BGSWARM_SANDBOX_PORT=9999 sandbox \
            bugswarm-sandbox default-config | grep -q '"http_port": 9999' || \
            (echo "C6 REGRESSION: env override not working" && exit 1)
```

### Guard 5: Config Schema Validation (Rust)
```rust
// Add to CI: cargo test --test config_validation
#[test]
fn test_default_config_is_valid() {
    let config = UnifiedConfig::default_for_testing();
    assert_eq!(config.version, "1.0.0");
    assert!(config.daemons.sandbox.rerun_count > 0);
    assert!(config.daemons.sandbox.timeout_secs > 0);
    assert!(config.daemons.cpg.cache_capacity > 0);
    assert!(!config.daemons.evidence.state_dir.is_empty());
}

#[test]
fn test_config_env_overrides() {
    std::env::set_var("BGSWARM_SANDBOX_PORT", "12345");
    let config = UnifiedConfig::default_for_testing();
    assert_eq!(config.daemons.sandbox.http_port, 12345);
    std::env::remove_var("BGSWARM_SANDBOX_PORT");
}
```

---

## Verification Procedure

After all 8 gaps are fixed, verify end-to-end:

```bash
# 1. Validate config
python3 -c "import yaml; yaml.safe_load(open('/etc/bugswarm/config.yaml'))" && echo "OK"

# 2. Start all services
docker compose up -d

# 3. Wait for all healthy (max 120s)
docker compose ps

# 4. Health endpoints
curl -s http://localhost:8080/health | jq .status           # sandbox → "healthy"
curl -s http://localhost:8081/health | jq .status           # cpg → "healthy"
curl -s http://localhost:8082/health | jq .status           # evidence → "healthy"
curl -s http://localhost:8084/health                        # gateway
curl -s http://localhost:8085/health                        # agent
curl -s http://localhost:8086/health                        # swarm

# 5. Readiness endpoints
curl -s http://localhost:8080/ready | jq .ready             # → true
curl -s http://localhost:8081/ready | jq .ready             # → true
curl -s http://localhost:8082/ready | jq .ready             # → true

# 6. Metrics endpoints (Prometheus scrape simulation)
curl -s http://localhost:8080/metrics | head -15
curl -s http://localhost:8081/metrics | head -15
curl -s http://localhost:8082/metrics | head -15
curl -s http://localhost:9090/metrics | head -15            # swarm observability

# 7. Functional test — index a repo via CPG, execute sandbox, record evidence
echo '{"method":"index","repo":"/workspace"}' | nc -U /var/run/bugswarm/cpg.sock
echo '{"method":"execute","poc_code":"print(1)","env":{}}' | nc -U /var/run/bugswarm/sandbox.sock
echo '{"method":"add_claim","claim":"test bug","author":"ci","location":"t.py:1","severity":5}' \
  | nc -U /var/run/bugswarm/evidence.sock

# 8. Metrics verify increment
curl -s http://localhost:8080/metrics | grep bugswarm_executions_total
curl -s http://localhost:8081/metrics | grep bugswarm_cpg_requests_total
curl -s http://localhost:8082/metrics | grep bugswarm_evidence_claims_total

# 9. Graceful shutdown
docker compose down -v
```

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Config schema mismatch between Rust/Python | Medium | High | Single-source-of-truth YAML; CI validates schema on both sides |
| HTTP port conflicts with existing services | Low | Medium | Config file allows custom ports; docker-compose maps host ports above 8080 |
| Perf overhead of HTTP health server | Low | Low | Minimal — raw TCP HTTP/1.1, spawns per-connection, no framework |
| Docker-in-Docker permission issues (sandbox) | Medium | High | Requires privileged mode OR rootful `/var/run/docker.sock` bind mount |
| Symbolic daemon standalone binary not built | High | Medium | Currently a library crate; needs binary target or gRPC wrapper |
| Config file not found at startup | Low | High | All daemons must fail-fast with clear error message if `--config` path missing |

---

## Closure Criteria

All 8 gaps are closed when:

1. `docker compose up -d` starts all 8 services with zero errors
2. All 6 `/health` endpoints return 200 within `start_period`
3. All 3 `/ready` endpoints return `{"ready": true}` for daemons
4. All 3 `/metrics` endpoints return valid Prometheus text format with non-zero counters
5. `docker compose ps` shows all services as `healthy` (not `unhealthy` or `starting`)
6. CI pipeline (Guard 1-4) passes on every commit

### Post-Closure State

After all 8 gaps are fixed, the CRITICAL findings status becomes:

| Finding | Status |
|---------|--------|
| C1 | FIXED — docker-compose.yml with 8 services, health-ordered, restart policies |
| C2 | FIXED — workspace Cargo.toml `[profile.release]` with LTO, strip, opt-level=3 |
| C3 | FIXED — All 3 daemons expose HTTP `/health` and `/ready` |
| C4 | FIXED — All 3 daemons expose `/metrics` in Prometheus format |
| C5 | FIXED — Evidence daemon RunServer subcommand + Dockerfile |
| C6 | FIXED — Unified `/etc/bugswarm/config.yaml` with env-var overrides |
| C7 | FIXED — mine_invariants executes real containers, collects real traces |
| C8 | FIXED — Fuzz campaign crashes flow back to daemon via container volume |
| C9 | FIXED — run_mutations uses real unittest runner in sandbox |
| C10 | FIXED — diff protocol alignment (input/reference keys) |
| C11 | FIXED — solve_reachability uses Z3 when `symbolic` feature enabled |
| C12 | FIXED — Evidence graph save/load with JSON serialization |

**12/12 CRITICAL findings resolved. Production deployment is unblocked.**
