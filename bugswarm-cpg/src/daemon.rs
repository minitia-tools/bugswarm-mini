/// Daemon mode — Unix socket server for the Code Property Graph.
///
/// Listens on /var/run/bugswarm/cpg.sock (configurable).
/// Methods: stats, taint, call_path, index, test_file, health.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{error, info};
use uuid::Uuid;

use crate::graph::CodePropertyGraph;
use crate::parser;

#[derive(Debug, Deserialize)]
struct DaemonRequest {
    method: String,
    #[serde(default)]
    repo: String,
    #[serde(default)]
    file: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    from_func: String,
    #[serde(default)]
    to_func: String,
    #[serde(default)]
    prune: bool,
    #[serde(default = "default_decay")]
    decay: f32,
    #[serde(default)]
    request_id: Option<String>,
}

fn default_decay() -> f32 { 0.7 }

#[derive(Debug, Serialize)]
struct DaemonResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Cached CPG index per repository.
struct CpgCache {
    graphs: RwLock<std::collections::HashMap<String, CodePropertyGraph>>,
}

impl CpgCache {
    fn new() -> Self {
        Self { graphs: RwLock::new(std::collections::HashMap::new()) }
    }

    fn get_or_index(&self, repo: &str) -> Result<CodePropertyGraph, String> {
        {
            let cache = self.graphs.read();
            if let Some(cpg) = cache.get(repo) {
                return Ok(cpg.clone());
            }
        }

        let mut cpg = CodePropertyGraph::new();
        let path = std::path::Path::new(repo);
        parser::index_directory(&mut cpg, path).map_err(|e| format!("Index failed: {}", e))?;

        let mut cache = self.graphs.write();
        cache.insert(repo.to_string(), cpg.clone());
        Ok(cpg)
    }

    fn invalidate(&self, repo: &str) {
        self.graphs.write().remove(repo);
    }
}

pub async fn run_daemon(socket_path: PathBuf, http_port: Option<u16>) -> anyhow::Result<()> {
    if socket_path.exists() {
        match tokio::net::UnixStream::connect(&socket_path).await {
            Ok(_) => {
                tracing::error!("Another daemon instance is already running on {}. Refusing to start.", socket_path.display());
                return Err(anyhow::anyhow!("Daemon already running on {}", socket_path.display()));
            }
            Err(_) => {
                tracing::info!("Removing stale socket file: {}", socket_path.display());
                std::fs::remove_file(&socket_path)?;
            }
        }
    }
    if let Some(parent) = socket_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::error!("Failed to create socket parent directory {}: {}", parent.display(), e);
            return Err(anyhow::anyhow!("Failed to create socket parent directory {}: {}", parent.display(), e));
        }
    }

    let listener = UnixListener::bind(&socket_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!("Failed to set socket permissions for {}: {}", socket_path.display(), e);
        }
    }

    info!("CPG daemon listening on {}", socket_path.display());

    let cache = Arc::new(CpgCache::new());

    if let Some(port) = http_port {
        let metrics = Arc::new(crate::metrics::CpgMetrics::new());
        tokio::spawn(crate::metrics::spawn_http_server(port, metrics));
        info!("CPG HTTP health/metrics server on port {}", port);
    }

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let c = cache.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, c).await {
                                error!("CPG connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => error!("CPG accept error: {}", e),
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received SIGTERM, shutting down gracefully...");
                break Ok(());
            }
        }
    }
}

async fn handle_connection(stream: UnixStream, cache: Arc<CpgCache>) -> anyhow::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        if buf_reader.read_line(&mut line).await? == 0 { break; }

        let request: DaemonRequest = match serde_json::from_str(line.trim()) {
            Ok(r) => r,
            Err(e) => {
                let resp = DaemonResponse { success: false, data: None, error: Some(format!("Invalid JSON: {}", e)) };
                let json = serde_json::to_string(&resp).unwrap_or_default();
                writer.write_all(json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                continue;
            }
        };

        let request_id = request.request_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let span = tracing::info_span!("request", request_id = %request_id);
        let _guard = span.enter();

        let response = process_request(&request, &cache);
        let json = serde_json::to_string(&response).unwrap_or_default();
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }
    Ok(())
}

fn process_request(req: &DaemonRequest, cache: &CpgCache) -> DaemonResponse {
    match req.method.as_str() {
        "stats" | "index" => handle_stats_or_index(req, cache),
        "taint" => handle_taint(req, cache),
        "call_path" => handle_call_path(req, cache),
        "invalidate" => {
            cache.invalidate(&req.repo);
            DaemonResponse { success: true, data: None, error: None }
        }
        "health" => {
            DaemonResponse { success: true, data: None, error: None }
        }
        "danger_map" => handle_danger_map(req, cache),
        _ => DaemonResponse { success: false, data: None, error: Some(format!("Unknown method: {}", req.method)) },
    }
}

fn handle_stats_or_index(req: &DaemonRequest, cache: &CpgCache) -> DaemonResponse {
    match cache.get_or_index(&req.repo) {
        Ok(cpg) => {
            let stats = cpg.stats();
            DaemonResponse {
                success: true,
                data: Some(serde_json::to_value(&stats).unwrap_or_default()),
                error: None,
            }
        }
        Err(e) => DaemonResponse { success: false, data: None, error: Some(e) },
    }
}

fn handle_taint(req: &DaemonRequest, cache: &CpgCache) -> DaemonResponse {
    match cache.get_or_index(&req.repo) {
        Ok(cpg) => {
            let paths = cpg.find_taint_paths();
            let summary: Vec<serde_json::Value> = paths.iter().map(|p| {
                serde_json::json!({
                    "source": cpg.get_node(p.source).map(|n| n.name.clone()).unwrap_or_default(),
                    "sink": cpg.get_node(p.sink).map(|n| n.name.clone()).unwrap_or_default(),
                    "length": p.length,
                    "sanitized": p.sanitized,
                    "confidence": p.confidence,
                })
            }).collect();
            DaemonResponse { success: true, data: Some(serde_json::json!({"paths": summary})), error: None }
        }
        Err(e) => DaemonResponse { success: false, data: None, error: Some(e) },
    }
}

fn handle_call_path(req: &DaemonRequest, cache: &CpgCache) -> DaemonResponse {
    match cache.get_or_index(&req.repo) {
        Ok(cpg) => {
            let paths = cpg.find_call_paths(&req.from_func, &req.to_func);
            DaemonResponse {
                success: true,
                data: Some(serde_json::to_value(&paths).unwrap_or_default()),
                error: None,
            }
        }
        Err(e) => DaemonResponse { success: false, data: None, error: Some(e) },
    }
}

fn handle_danger_map(req: &DaemonRequest, cache: &CpgCache) -> DaemonResponse {
    match cache.get_or_index(&req.repo) {
        Ok(cpg) => {
            let danger_map = crate::danger_map::danger_map_from_graph(&cpg, req.decay);
            let entries: Vec<serde_json::Value> = danger_map.iter().map(|(addr, score)| {
                serde_json::json!({"address": format!("0x{:x}", addr), "danger_score": score})
            }).collect();
            let sinks_used: Vec<&str> = crate::danger_map::DEFAULT_SINKS.to_vec();
            let data = serde_json::json!({
                "num_entries": entries.len(),
                "sink_count": sinks_used.len(),
                "source_count": entries.len(),
                "decay_factor": req.decay,
                "entries": entries,
                "sinks": sinks_used,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            DaemonResponse { success: true, data: Some(data), error: None }
        }
        Err(e) => DaemonResponse { success: false, data: None, error: Some(e) },
    }
}
