/// Daemon mode — Unix socket server for the Evidence Graph.
///
/// Listens on /var/run/bugswarm/evidence.sock (configurable).
/// Methods: add_claim, add_sandbox_run, link_result, confirm_bug, stats, query,
///          score_agent, verify, health.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{error, info};

use crate::graph::EvidenceGraph;
use crate::trigger;
use crate::types::{EvidenceQuery, NodeKind};

#[derive(Debug, Deserialize)]
struct DaemonRequest {
    method: String,
    #[serde(default)]
    claim: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    severity: u8,
    #[serde(default)]
    label: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    receipt_json: String,
    #[serde(default)]
    prediction_id: usize,
    #[serde(default)]
    sandbox_run_id: usize,
    #[serde(default)]
    confirmed: bool,
    #[serde(default)]
    confidence: f64,
    #[serde(default)]
    claim_id: usize,
    #[serde(default)]
    sandbox_run_ids: Vec<usize>,
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    bug_id: String,
    #[serde(default)]
    dimension: String,
    #[serde(default)]
    layer: String,
}

#[derive(Debug, Serialize)]
struct DaemonResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn run_daemon(socket_path: PathBuf) -> anyhow::Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(&socket_path).ok();
    }
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let listener = UnixListener::bind(&socket_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)).ok();
    }

    info!("Evidence daemon listening on {}", socket_path.display());

    let graph = Arc::new(EvidenceGraph::new());

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let g = graph.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, g).await {
                        error!("Evidence connection error: {}", e);
                    }
                });
            }
            Err(e) => error!("Evidence accept error: {}", e),
        }
    }
}

async fn handle_connection(stream: UnixStream, graph: Arc<EvidenceGraph>) -> anyhow::Result<()> {
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
                writer.write_all(serde_json::to_string(&resp)?.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                continue;
            }
        };

        let response = process(&request, &graph);
        writer.write_all(serde_json::to_string(&response)?.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }
    Ok(())
}

fn process(req: &DaemonRequest, graph: &EvidenceGraph) -> DaemonResponse {
    match req.method.as_str() {
        "add_claim" => {
            let id = graph.add_claim(&req.claim, &req.author, &req.location, req.severity);
            DaemonResponse { success: true, data: Some(Value::Number(id.into())), error: None }
        }

        "add_sandbox_run" => {
            let id = graph.add_sandbox_run(&req.label, &req.description, &req.author, &req.receipt_json);
            DaemonResponse { success: true, data: Some(Value::Number(id.into())), error: None }
        }

        "link_result" => {
            graph.link_sandbox_result(req.sandbox_run_id, req.prediction_id, req.confirmed, req.confidence);
            DaemonResponse { success: true, data: None, error: None }
        }

        "confirm_bug" => {
            let result = graph.confirm_bug(req.claim_id, &req.sandbox_run_ids);
            DaemonResponse {
                success: result.is_some(),
                data: result.map(|id| Value::Number(id.into())),
                error: if result.is_none() { Some("Claim not found or not a claim".into()) } else { None },
            }
        }

        "stats" => {
            let s = graph.stats();
            DaemonResponse { success: true, data: serde_json::to_value(&s).ok(), error: None }
        }

        "query" => {
            let q = EvidenceQuery {
                author: if req.author.is_empty() { None } else { Some(req.author.clone()) },
                ..Default::default()
            };
            let results: Vec<Value> = graph.query(&q).iter()
                .map(|n| serde_json::to_value(n).unwrap_or_default())
                .collect();
            DaemonResponse { success: true, data: Some(Value::Array(results)), error: None }
        }

        "score_agent" => {
            let score = graph.score_agent(&req.agent_id);
            DaemonResponse { success: true, data: serde_json::to_value(&score).ok(), error: None }
        }

        "verify" => {
            let corrupted = graph.verify_integrity();
            DaemonResponse {
                success: corrupted.is_empty(),
                data: Some(Value::Array(corrupted.iter().map(|&id| Value::Number(id.into())).collect())),
                error: if corrupted.is_empty() { None } else { Some(format!("{} corrupted nodes", corrupted.len())) },
            }
        }

        "health" => {
            DaemonResponse { success: true, data: None, error: None }
        }

        "add_trigger_condition" => {
            let dimension = match parse_dimension(&req.dimension) {
                Ok(d) => d,
                Err(e) => return DaemonResponse { success: false, data: None, error: Some(e) },
            };
            let layer = parse_layer(&req.layer);
            let condition = trigger::TriggerCondition::new(
                &req.bug_id, dimension, &req.description, layer,
            );
            let mut tm = graph.trigger_manager.write();
            let is_new = tm.add_condition(condition.clone());
            graph.add_trigger_condition(&req.bug_id, condition);
            DaemonResponse {
                success: true,
                data: Some(serde_json::json!({"is_new": is_new})),
                error: None,
            }
        }
        "get_trigger_matrix" => {
            let tm = graph.trigger_manager.read();
            if let Some(matrix) = tm.get(&req.bug_id) {
                DaemonResponse {
                    success: true,
                    data: Some(serde_json::to_value(matrix).unwrap_or_default()),
                    error: None,
                }
            } else {
                let matrix = trigger::TriggerMatrix::new(&req.bug_id);
                DaemonResponse {
                    success: true,
                    data: Some(serde_json::to_value(&matrix).unwrap_or_default()),
                    error: None,
                }
            }
        }

        _ => DaemonResponse { success: false, data: None, error: Some(format!("Unknown method: {}", req.method)) },
    }
}

fn parse_dimension(s: &str) -> Result<trigger::TriggerDimension, String> {
    match s.to_lowercase().as_str() {
        "input" | "inputtype" => Ok(trigger::TriggerDimension::Input),
        "environment" => Ok(trigger::TriggerDimension::Environment),
        "timing" => Ok(trigger::TriggerDimension::Timing),
        "datastate" | "data_state" => Ok(trigger::TriggerDimension::DataState),
        "concurrency" => Ok(trigger::TriggerDimension::Concurrency),
        "configuration" | "config" => Ok(trigger::TriggerDimension::Configuration),
        "dependencyversion" | "dependency" => Ok(trigger::TriggerDimension::DependencyVersion),
        "osarch" | "os_arch" | "os" | "arch" => Ok(trigger::TriggerDimension::OsArch),
        _ => Err(format!("Unknown dimension: {}", s)),
    }
}

fn parse_layer(s: &str) -> trigger::ContributionLayer {
    match s.to_lowercase().as_str() {
        "agent" => trigger::ContributionLayer::Agent,
        "fuzzer" => trigger::ContributionLayer::Fuzzer,
        "concolic" => trigger::ContributionLayer::Concolic,
        "differential" => trigger::ContributionLayer::Differential,
        "sanitizer" => trigger::ContributionLayer::Sanitizer,
        "symbolic" => trigger::ContributionLayer::Symbolic,
        "delta" => trigger::ContributionLayer::Delta,
        "manual" | "human" => trigger::ContributionLayer::Manual,
        _ => trigger::ContributionLayer::Manual,
    }
}
