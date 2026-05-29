//! Daemon mode — Unix socket server for the Evidence Graph.
//!
//! Listens on /var/run/bugswarm/evidence.sock (configurable).
//! Methods: add_claim, add_sandbox_run, link_result, confirm_bug, stats, query,
//!          score_agent, verify, health.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinSet;
use tracing::{error, info, warn};

use crate::chain;
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
    #[serde(default)]
    bug_ids: Vec<String>,
    #[serde(default)]
    max_hops: Option<usize>,
    #[serde(default)]
    original_line: String,
    #[serde(default)]
    replacement_line: String,
    #[serde(default)]
    function: String,
    #[serde(default)]
    line_number: Option<u32>,
    #[serde(default)]
    language: String,
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    request_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct DaemonResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn run_daemon(socket_path: PathBuf, http_port: Option<u16>, state_path: Option<PathBuf>) -> anyhow::Result<()> {
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
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create socket directory: {}", parent.display()))?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!("Failed to set socket permissions for {}: {}", socket_path.display(), e);
        }
    }

    info!("Evidence daemon listening on {}", socket_path.display());

    let graph = Arc::new(EvidenceGraph::new());

    // Restore graph state from disk
    if let Some(ref path) = state_path {
        if path.exists() {
            match graph.load_graph(path) {
                Ok(n) => info!("Restored {} evidence nodes from {}", n, path.display()),
                Err(e) => warn!("Failed to load graph from {}: {} — starting fresh", path.display(), e),
            }
        }
    }

    if let Some(port) = http_port {
        let metrics = Arc::new(crate::metrics::EvidenceMetrics::new());
        tokio::spawn(crate::metrics::spawn_http_server(port, metrics));
        info!("Evidence HTTP health/metrics server on port {}", port);
    }

    let mut join_set: JoinSet<anyhow::Result<()>> = JoinSet::new();

    let state_path2 = state_path.clone();
    let graph2 = graph.clone();
    let mut autosave_interval = tokio::time::interval(Duration::from_secs(30));
    autosave_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let shutdown = async {
        loop {
            tokio::select! {
                biased;
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Received SIGTERM, shutting down gracefully...");
                    break;
                }
                _ = autosave_interval.tick() => {
                    if let Some(ref sp) = state_path2 {
                        if let Err(e) = graph2.save_graph(sp) {
                            warn!("Autosave failed: {}", e);
                        }
                    }
                }
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let g = graph.clone();
                            join_set.spawn(async move {
                                if let Err(e) = handle_connection(stream, g).await {
                                    error!("Evidence connection error: {}", e);
                                }
                                Ok(())
                            });
                        }
                        Err(e) => error!("Evidence accept error: {}", e),
                    }
                }
            }
        }
    };

    shutdown.await;

    // Save evidence graph on shutdown
    if let Some(ref path) = state_path {
        info!("Shutting down — saving evidence graph to {}", path.display());
        match graph.save_graph(path) {
            Ok(n) => info!("Saved {} evidence nodes to {}", n, path.display()),
            Err(e) => error!("Failed to save graph: {}", e),
        }
    }

    tracing::info!("Draining {} active evidence connections...", join_set.len());
    let drain_timeout = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(drain_timeout);
    loop {
        tokio::select! {
            _ = &mut drain_timeout => {
                tracing::warn!("Evidence drain timeout reached — {} connections still active", join_set.len());
                break;
            }
            result = join_set.join_next() => {
                match result {
                    Some(Ok(Ok(()))) => continue,
                    Some(Ok(Err(e))) => warn!("Evidence connection task error during drain: {}", e),
                    Some(Err(e)) => warn!("Evidence connection task panicked during drain: {}", e),
                    None => break,
                }
            }
        }
    }

    tracing::info!("Evidence daemon shut down complete");
    Ok(())
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

        let request_id = request.request_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let span = tracing::info_span!("request", request_id = %request_id);
        let _guard = span.enter();

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
            handle_add_trigger_condition(req, graph)
        }
        "get_trigger_matrix" => {
            handle_get_trigger_matrix(req, graph)
        }

        "suggest_chain" => {
            handle_suggest_chain(req, graph)
        }

        "predict_fix_impact" => {
            handle_predict_fix_impact(req, graph)
        }

        _ => DaemonResponse { success: false, data: None, error: Some(format!("Unknown method: {}", req.method)) },
    }
}

fn handle_add_trigger_condition(req: &DaemonRequest, graph: &EvidenceGraph) -> DaemonResponse {
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

fn handle_get_trigger_matrix(req: &DaemonRequest, graph: &EvidenceGraph) -> DaemonResponse {
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

fn handle_suggest_chain(req: &DaemonRequest, graph: &EvidenceGraph) -> DaemonResponse {
    let bug_ids: Vec<String> = req.bug_ids.clone();
    let max_hops = req.max_hops.unwrap_or(10);

    let nodes = graph.nodes.read();
    let mut severities = HashMap::new();
    let mut effects = Vec::new();
    let mut preconditions = Vec::new();

    for id in &bug_ids {
        for node in nodes.iter() {
            if node.label == *id && node.kind == NodeKind::ConfirmedBug {
                let sev = node.severity.unwrap_or(5);
                severities.insert(id.clone(), sev);
                let desc = &node.description;
                effects.extend(chain::extract_effects(id, desc, sev));
                preconditions.extend(chain::extract_preconditions(id, desc, sev));
            }
        }
    }
    drop(nodes);

    let mut matcher = chain::ChainSemanticMatcher::new(0.3);
    let matches = matcher.find_matches(&effects, &preconditions);

    let chain_graph = chain::build_chain_graph(&matches);
    let chains = chain::detect_chains(&chain_graph, &severities, max_hops);
    let escalated = chain::escalate_severities(&chains, &severities);

    DaemonResponse {
        success: true,
        data: Some(serde_json::json!({
            "matches_found": matches.len(),
            "chains_found": chains.len(),
            "chains": chains,
            "original_severities": severities,
            "escalated_severities": escalated,
        })),
        error: None,
    }
}

fn handle_predict_fix_impact(req: &DaemonRequest, graph: &EvidenceGraph) -> DaemonResponse {
    let fix = match serde_json::from_value::<crate::fix_predict::FixProposal>(
        serde_json::json!({
            "bug_id": req.bug_id,
            "file_path": req.file_path.clone().unwrap_or_default(),
            "function_name": req.function.clone(),
            "original_line": req.original_line.clone(),
            "replacement_line": req.replacement_line.clone(),
            "line_number": req.line_number.unwrap_or(0),
            "description": req.description.clone(),
            "language": if req.language.is_empty() { "python".into() } else { req.language.clone() },
        })
    ) {
        Ok(f) => f,
        Err(e) => {
            return DaemonResponse { success: false, data: None, error: Some(format!("Invalid FixProposal: {}", e)) };
        }
    };
    
    let call_graph: std::collections::HashMap<String, Vec<String>> = {
        let nodes = graph.nodes.read();
        let mut cg: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for node in nodes.iter() {
            if node.kind == NodeKind::Agent || node.kind == NodeKind::Claim {
                cg.entry(node.label.clone()).or_default();
            }
        }
        cg.entry(fix.function_name.clone()).or_default();
        let out_edges = graph.out_edges.read();
        for (source_id, edges) in out_edges.iter() {
            for (_, target_id) in edges {
                if let Some(source_node) = nodes.get(*source_id) {
                    if let Some(target_node) = nodes.get(*target_id) {
                        cg.entry(source_node.label.clone()).or_default().push(target_node.label.clone());
                    }
                }
            }
        }
        cg
    };
    
    let priors = crate::fix_predict::LanguagePriors::default();
    let report = crate::fix_predict::predict_fix_impact(fix, &call_graph, &priors, 5);
    
    DaemonResponse {
        success: true,
        data: Some(serde_json::to_value(&report).unwrap_or_default()),
        error: None,
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
