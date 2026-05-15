/// Daemon mode — Unix socket server for the Execution Sandbox.
///
/// Listens on /var/run/bugswarm/sandbox.sock (configurable).
/// Accepts JSON request lines, returns JSON receipt lines.
/// Supports concurrent connections via tokio.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{error, info, warn};

use crate::config::{ExecutionReceipt, ExecutionStatus, SandboxConfig};
use crate::container::ContainerManager;
use crate::delta::{DeltaConfig, DeltaMinimizer, OracleFn};
use crate::error::SandboxResult;
use crate::fuzzer::{FuzzRequest, FuzzResponse};

#[derive(Debug, Deserialize)]
struct DaemonRequest {
    method: String,
    #[serde(default)]
    poc_code: String,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    flaky: bool,
    #[serde(default)]
    count: u32,
}

#[derive(Debug, Deserialize)]
struct DeltaRequest {
    input: String,
    #[serde(default)]
    max_iterations: u32,
    #[serde(default)]
    timeout_secs: u64,
}

#[derive(Debug, Serialize)]
struct DeltaResponse {
    success: bool,
    minimized: String,
    original_size: usize,
    minimized_size: usize,
    reduction_ratio: f64,
    iterations: u32,
    is_1_minimal: bool,
    elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct DaemonResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<ExecutionReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Start the sandbox daemon on a Unix socket.
pub async fn run_daemon(socket_path: PathBuf, config: SandboxConfig) -> SandboxResult<()> {
    // Remove stale socket file if it exists
    if socket_path.exists() {
        std::fs::remove_file(&socket_path).ok();
    }

    // Ensure parent directory exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let listener = UnixListener::bind(&socket_path)
        .map_err(|e| crate::error::SandboxError::Other(format!("Failed to bind socket: {}", e)))?;

    // Restrict socket permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)).ok();
    }

    info!("Sandbox daemon listening on {}", socket_path.display());

    let manager = std::sync::Arc::new(ContainerManager::connect(config).await?);
    manager.ensure_image().await?;

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let mgr = manager.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, mgr).await {
                        error!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Accept error: {}", e);
            }
        }
    }
}

async fn handle_connection(stream: UnixStream, manager: std::sync::Arc<ContainerManager>) -> SandboxResult<()> {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = buf_reader.read_line(&mut line).await?;
        if n == 0 {
            break; // EOF
        }

        let request: DaemonRequest = match serde_json::from_str(line.trim()) {
            Ok(req) => req,
            Err(e) => {
                let resp = DaemonResponse { success: false, receipt: None, error: Some(format!("Invalid JSON: {}", e)) };
                let json = serde_json::to_string(&resp).unwrap_or_default();
                writer.write_all(json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                continue;
            }
        };

        let response = match request.method.as_str() {
            "execute" => {
                match manager.execute(&request.poc_code, &request.env, request.flaky).await {
                    Ok(receipt) => DaemonResponse { success: true, receipt: Some(receipt), error: None },
                    Err(e) => DaemonResponse { success: false, receipt: None, error: Some(e.to_string()) },
                }
            }
            "execute_statistical" => {
                let count = if request.count > 0 { request.count } else { manager.config.rerun_count };
                match manager.execute_statistical(&request.poc_code, &request.env).await {
                    Ok(result) => {
                        let json = serde_json::to_value(&result).unwrap_or_default();
                        // Wrap statistical result as a receipt-like response
                        DaemonResponse {
                            success: true,
                            receipt: None,
                            error: Some(format!("STATISTICAL: {}", json)),
                        }
                    }
                    Err(e) => DaemonResponse { success: false, receipt: None, error: Some(e.to_string()) },
                }
            }
            "health" => {
                DaemonResponse { success: true, receipt: None, error: None }
            }
            "fuzz" => {
                match serde_json::from_str::<FuzzRequest>(line.trim()) {
                    Ok(fuzz_req) => {
                        match manager.fuzz(&fuzz_req.config).await {
                            Ok(fuzz_resp) => {
                                let json = serde_json::to_string(&fuzz_resp).unwrap_or_default();
                                DaemonResponse {
                                    success: true,
                                    receipt: None,
                                    error: Some(json),
                                }
                            }
                            Err(e) => DaemonResponse { success: false, receipt: None, error: Some(e.to_string()) },
                        }
                    }
                    Err(e) => DaemonResponse { success: false, receipt: None, error: Some(format!("Invalid FuzzRequest: {}", e)) },
                }
            }
            "diff" => {
                match serde_json::from_str::<serde_json::Value>(line.trim()) {
                    Ok(req) => {
                        let output_a = req.get("input").and_then(|v| v.as_str()).unwrap_or("");
                        let output_b = req.get("reference").and_then(|v| v.as_str()).unwrap_or("");
                        let normalizer = match req.get("normalizer").and_then(|v| v.as_str()).unwrap_or("Text") {
                            "Json" => crate::differential::OutputNormalizer::Json,
                            "Xml" => crate::differential::OutputNormalizer::Xml,
                            "Dict" => crate::differential::OutputNormalizer::Dict,
                            "Text" => crate::differential::OutputNormalizer::Text,
                            "Binary" => crate::differential::OutputNormalizer::Binary,
                            _ => crate::differential::OutputNormalizer::Text,
                        };
                        let result = crate::differential::compute_diff(output_a, output_b, normalizer, 0.01);
                        DaemonResponse {
                            success: true,
                            receipt: None,
                            error: Some(serde_json::to_string(&result).unwrap_or_default()),
                        }
                    }
                    Err(e) => DaemonResponse { success: false, receipt: None, error: Some(format!("Invalid JSON: {}", e)) },
                }
            }
            "generate_pairs" => {
                match serde_json::from_str::<serde_json::Value>(line.trim()) {
                    Ok(req) => {
                        let inputs: Vec<String> = req.get("inputs")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                            .unwrap_or_default();
                        let max_pairs = req.get("max_pairs").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
                        let pairs = crate::differential::generate_pairs(&inputs, &[crate::differential::InvariantType::Commutative], max_pairs);
                        DaemonResponse {
                            success: true,
                            receipt: None,
                            error: Some(serde_json::to_string(&pairs).unwrap_or_default()),
                        }
                    }
                    Err(e) => DaemonResponse { success: false, receipt: None, error: Some(format!("Invalid JSON: {}", e)) },
                }
            }
            "delta" => {
                match serde_json::from_str::<DeltaRequest>(line.trim()) {
                    Ok(delta_req) => {
                        let input_bytes = hex::decode(&delta_req.input).unwrap_or_default();
                        let max_iterations = if delta_req.max_iterations > 0 {
                            delta_req.max_iterations
                        } else {
                            manager.config.delta_max_iterations
                        };
                        let timeout_secs = if delta_req.timeout_secs > 0 {
                            delta_req.timeout_secs
                        } else {
                            manager.config.delta_timeout_secs
                        };
                        let config = DeltaConfig {
                            max_iterations,
                            timeout_secs,
                            ..DeltaConfig::default()
                        };
                        let minimizer = DeltaMinimizer::new(config);

                        let mgr = manager.clone();
                        let oracle: OracleFn = std::sync::Arc::new(move |test_input: &[u8]| -> bool {
                            let poc_str = String::from_utf8_lossy(test_input).to_string();
                            let handle = tokio::runtime::Handle::current();
                            match handle.block_on(mgr.execute(&poc_str, &std::collections::HashMap::new(), false)) {
                                Ok(receipt) => receipt.status == ExecutionStatus::Passed,
                                Err(_) => false,
                            }
                        });

                        let result = minimizer.minimize(&input_bytes, &oracle);

                        let resp = DeltaResponse {
                            success: true,
                            minimized: hex::encode(&result.minimized),
                            original_size: result.original_size,
                            minimized_size: result.minimized_size,
                            reduction_ratio: result.reduction_ratio,
                            iterations: result.iterations,
                            is_1_minimal: result.is_1_minimal,
                            elapsed_ms: result.elapsed_ms,
                            error: None,
                        };

                        let json = serde_json::to_string(&resp).unwrap_or_default();
                        DaemonResponse {
                            success: true,
                            receipt: None,
                            error: Some(json),
                        }
                    }
                    Err(e) => DaemonResponse {
                        success: false,
                        receipt: None,
                        error: Some(format!("Invalid DeltaRequest: {}", e)),
                    },
                }
            }
            _ => {
                DaemonResponse { success: false, receipt: None, error: Some(format!("Unknown method: {}", request.method)) }
            }
        };

        let json = serde_json::to_string(&response).unwrap_or_default();
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}
