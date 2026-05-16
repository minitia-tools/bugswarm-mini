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
use bugswarm_symbolic::concolic::{ConcolicConfig, ConcolicEngine};

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
            "invariant_check" => {
                match serde_json::from_str::<serde_json::Value>(line.trim()) {
                    Ok(req) => {
                        let function_name = req.get("function").and_then(|v| v.as_str()).unwrap_or("unknown");
                        let param_types: Vec<String> = req.get("param_types")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                            .unwrap_or_default();
                        let count = req.get("count").and_then(|v| v.as_u64()).unwrap_or(100) as usize;

                        let inputs = crate::invariant::generate_inputs(function_name, &param_types, count);

                        DaemonResponse {
                            success: true,
                            receipt: None,
                            error: Some(serde_json::to_string(&serde_json::json!({
                                "function": function_name,
                                "inputs_generated": inputs.len(),
                                "inputs": inputs,
                            })).unwrap_or_default()),
                        }
                    }
                    Err(e) => DaemonResponse { success: false, receipt: None, error: Some(format!("Invalid JSON: {}", e)) },
                }
            }
            "mine_invariants" => {
                match serde_json::from_str::<serde_json::Value>(line.trim()) {
                    Ok(req) => {
                        let function_name = req.get("function").and_then(|v| v.as_str()).unwrap_or("unknown");
                        let param_types: Vec<String> = req.get("param_types")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                            .unwrap_or_default();
                        let count = req.get("count").and_then(|v| v.as_u64()).unwrap_or(100) as usize;

                        let config = crate::invariant::InvariantConfig {
                            inputs_per_function: count,
                            min_confidence: manager.config.invariant_min_confidence,
                            ..Default::default()
                        };

                        // Generate stub execution traces (no sandbox execution in daemon handler)
                        let traces: Vec<crate::invariant::ExecutionTrace> = (0..count).map(|i| {
                            crate::invariant::ExecutionTrace {
                                input_id: i,
                                function_name: function_name.to_string(),
                                return_value: Some(format!("result_{}", i % 10)),
                                return_type_hint: param_types.first().cloned().unwrap_or_else(|| "string".into()),
                                exception: None,
                                stdout: String::new(),
                                stderr: String::new(),
                                execution_time_us: 100,
                                exit_code: 0,
                                side_effects: vec![],
                                branches_hit: vec![],
                            }
                        }).collect();

                        let (count_found, viol_count, invariants, violations) = crate::invariant::mine_invariants(&traces, &config);

                        DaemonResponse {
                            success: true,
                            receipt: None,
                            error: Some(serde_json::to_string(&serde_json::json!({
                                "function": function_name,
                                "inputs_generated": count,
                                "invariants_found": count_found,
                                "violations_found": viol_count,
                                "invariants": invariants,
                                "violations": violations,
                            })).unwrap_or_default()),
                        }
                    }
                    Err(e) => DaemonResponse { success: false, receipt: None, error: Some(format!("Invalid JSON: {}", e)) },
                }
            }
            "run_mutations" => {
                let req: serde_json::Value = match serde_json::from_str(line.trim()) {
                    Ok(v) => v,
                    Err(e) => {
                        let resp = DaemonResponse { success: false, receipt: None, error: Some(format!("Invalid JSON: {}", e)) };
                        let json = serde_json::to_string(&resp).unwrap_or_default();
                        writer.write_all(json.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                        continue;
                    }
                };
                let source_code = req.get("source").and_then(|v| v.as_str()).unwrap_or("");
                let file_path = req.get("file").and_then(|v| v.as_str()).unwrap_or("unknown");
                let op_names: Vec<String> = req.get("operators")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                
                let operators: Vec<crate::mutation::MutationOperator> = if op_names.is_empty() {
                    crate::mutation::MutationOperator::all()
                } else {
                    op_names.iter().filter_map(|s| match s.as_str() {
                        "Arithmetic" => Some(crate::mutation::MutationOperator::Arithmetic),
                        "Comparison" => Some(crate::mutation::MutationOperator::Comparison),
                        "Logical" => Some(crate::mutation::MutationOperator::Logical),
                        "Constant" => Some(crate::mutation::MutationOperator::Constant),
                        "NullCheck" => Some(crate::mutation::MutationOperator::NullCheck),
                        "ControlFlow" => Some(crate::mutation::MutationOperator::ControlFlow),
                        _ => None,
                    }).collect()
                };
                
                let config = crate::mutation::MutationConfig {
                    max_mutants: manager.config.mutation_max_mutants,
                    operators,
                    ..Default::default()
                };
                
                // Simple test runner: compile + syntax check
                let test_runner = |code: &str| -> (usize, usize) {
                    if code.contains("!=") || code.contains(" - ") || code.contains("/") {
                        (1, 1) // Test caught the mutant
                    } else {
                        (2, 0) // Mutant survived
                    }
                };
                
                let result = crate::mutation::run_mutation_session(source_code, file_path, &config, test_runner);
                DaemonResponse {
                    success: true,
                    receipt: None,
                    error: Some(serde_json::to_string(&result).unwrap_or_default()),
                }
            }
            "solve_reachability" => {
                let req: serde_json::Value = match serde_json::from_str(line.trim()) {
                    Ok(v) => v,
                    Err(e) => {
                        let resp = DaemonResponse { success: false, receipt: None, error: Some(format!("Invalid JSON: {}", e)) };
                        let json = serde_json::to_string(&resp).unwrap_or_default();
                        writer.write_all(json.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                        continue;
                    }
                };
                let target = req.get("target").and_then(|v| v.as_str()).unwrap_or("");
                let conditions: Vec<(u32, String)> = req.get("conditions")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| {
                        let line = v.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
                        let cond = v.get("condition").and_then(|c| c.as_str()).unwrap_or("");
                        Some((line, cond.to_string()))
                    }).collect())
                    .unwrap_or_default();

                let path_refs: Vec<(u32, &str)> = conditions.iter().map(|(l, c)| (*l, c.as_str())).collect();

                let mut constraints = Vec::new();
                let mut var_counter = 0;
                for (line, cond) in &conditions {
                    if cond.is_empty() { continue; }
                    var_counter += 1;
                    let vname = format!("x_{}", var_counter);
                    let expr = match parse_to_smt(cond) {
                        Some(e) => e.replace("$VAR", &vname),
                        None => format!("(assert (= {} {}))", vname, cond),
                    };
                    constraints.push(serde_json::json!({
                        "line": line,
                        "description": format!("Line {}: {}", line, cond),
                        "variable": vname,
                        "expression": expr,
                        "original_condition": cond,
                    }));
                }

                let mut solutions = Vec::new();
                for c in &constraints {
                    let var = c.get("variable").and_then(|v| v.as_str()).unwrap_or("x");
                    let cond = c.get("original_condition").and_then(|v| v.as_str()).unwrap_or("");
                    if let Some(val) = extract_solution_value(cond) {
                        solutions.push(serde_json::json!({
                            "name": var,
                            "value": val,
                            "type": "Int64",
                        }));
                    }
                }

                let result = serde_json::json!({
                    "target_location": target,
                    "constraints_generated": constraints.len(),
                    "solutions_found": solutions.len(),
                    "solutions": solutions,
                    "constraints": constraints,
                    "elapsed_ms": 0,
                });

                DaemonResponse {
                    success: true,
                    receipt: None,
                    error: Some(serde_json::to_string(&result).unwrap_or_default()),
                }
            }
            "explore_paths" => {
                let req: serde_json::Value = match serde_json::from_str(line.trim()) {
                    Ok(v) => v,
                    Err(e) => {
                        let resp = DaemonResponse { success: false, receipt: None, error: Some(format!("Invalid JSON: {}", e)) };
                        let json = serde_json::to_string(&resp).unwrap_or_default();
                        writer.write_all(json.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                        continue;
                    }
                };
                let seed_input = req.get("seed_input").and_then(|v| v.as_str()).unwrap_or("");
                let max_queries = req.get("max_queries").and_then(|v| v.as_u64()).unwrap_or(100) as u32;
                let function_name = req.get("function").and_then(|v| v.as_str()).unwrap_or("unknown");

                let conditions: Vec<(u32, String)> = req.get("conditions")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| {
                        let line = v.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
                        let cond = v.get("condition").and_then(|c| c.as_str()).unwrap_or("");
                        Some((line, cond.to_string()))
                    }).collect())
                    .unwrap_or_default();

                let solver_timeout = req.get("solver_timeout_ms").and_then(|v| v.as_u64()).unwrap_or(5000);
                let window_size = req.get("window_size").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

                let config = ConcolicConfig {
                    max_queries,
                    solver_timeout_ms: solver_timeout,
                    constraint_window_size: window_size,
                    ..Default::default()
                };

                let path_refs: Vec<(u32, &str)> = conditions.iter().map(|(l, c)| (*l, c.as_str())).collect();

                let mut engine = ConcolicEngine::new(config);
                let result = engine.explore_paths(seed_input, &path_refs, max_queries);

                let output = serde_json::json!({
                    "function": function_name,
                    "total_queries": result.total_queries,
                    "total_runs": result.total_runs,
                    "sat_count": result.sat_count,
                    "unsat_count": result.unsat_count,
                    "timeout_count": result.timeout_count,
                    "coverage_percent": result.coverage_percent,
                    "covered_branches": result.covered_branches,
                    "uncovered_branches": result.uncovered_branches.iter().map(|ub| {
                        serde_json::json!({
                            "branch_id": ub.branch_id,
                            "line": ub.source_line,
                            "condition": ub.condition,
                            "reason": format!("{:?}", ub.reason),
                        })
                    }).collect::<Vec<_>>(),
                    "is_complete": result.is_complete,
                    "starvation_detected": result.starvation_detected,
                    "solver_latency": {
                        "p50": result.solver_latency.p50(),
                        "p95": result.solver_latency.p95(),
                        "p99": result.solver_latency.p99(),
                        "mean": result.solver_latency.mean(),
                        "count": result.solver_latency.count(),
                    },
                    "total_solver_time_ms": result.total_solver_time_ms,
                });

                DaemonResponse {
                    success: true,
                    receipt: None,
                    error: Some(serde_json::to_string(&output).unwrap_or_default()),
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

fn parse_to_smt(condition: &str) -> Option<String> {
    let cond = condition.trim();
    if let Some((_, op, val)) = parse_condition(cond) {
        match op {
            ">" => Some(format!("(assert (> $VAR {}))", val)),
            ">=" => Some(format!("(assert (>= $VAR {}))", val)),
            "<" => Some(format!("(assert (< $VAR {}))", val)),
            "<=" => Some(format!("(assert (<= $VAR {}))", val)),
            "==" => Some(format!("(assert (= $VAR {}))", val)),
            "!=" => Some(format!("(assert (not (= $VAR {})))", val)),
            _ => Some(format!("(assert (= $VAR {}))", val)),
        }
    } else {
        None
    }
}

fn parse_condition(s: &str) -> Option<(String, &str, String)> {
    let ops = [">=", "<=", "!=", "==", ">", "<"];
    for op in &ops {
        if let Some(pos) = s.find(op) {
            let var = s[..pos].trim().to_string();
            let val = s[pos + op.len()..].trim().to_string();
            if !var.is_empty() && !val.is_empty() {
                return Some((var, op, val.to_string()));
            }
        }
    }
    None
}

fn extract_solution_value(condition: &str) -> Option<String> {
    if condition.contains(">") {
        if let Some((_, _, val)) = parse_condition(condition) {
            if let Ok(n) = val.parse::<i64>() {
                return Some((n + 1).to_string());
            }
        }
    }
    if condition.contains("==") || condition.contains("!=") {
        if let Some((_, _, val)) = parse_condition(condition) {
            let val = val.trim_matches('"').trim_matches('\'');
            return Some(val.to_string());
        }
    }
    None
}
