use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

use bugswarm_cpg::graph::CodePropertyGraph;
use bugswarm_cpg::parser;

/// Bug Swarm CPG — Code Property Graph Builder
///
/// Parses codebases into a queryable graph of functions, calls, data flows, and taint paths.
#[derive(Parser)]
#[command(name = "bugswarm-cpg", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Write logs to a file (in addition to stdout).
    #[arg(long)]
    log_file: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Index a codebase and build the CPG.
    Index {
        /// Root directory of the codebase.
        #[arg(short, long)]
        repo: PathBuf,

        /// Output JSON file for CPG stats. Defaults to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Prune unreachable nodes after indexing.
        #[arg(long)]
        prune: bool,
    },

    /// Query the CPG for taint paths.
    Taint {
        /// Path to a previously indexed CPG JSON (not yet supported; use index first).
        #[arg(short, long)]
        repo: PathBuf,

        /// Filter by source name pattern.
        #[arg(long)]
        source: Option<String>,

        /// Filter by sink name pattern.
        #[arg(long)]
        sink: Option<String>,
    },

    /// Find call paths between two functions.
    CallPath {
        /// Root directory of the codebase.
        #[arg(short, long)]
        repo: PathBuf,

        /// Starting function name.
        #[arg(long)]
        from: String,

        /// Target function name.
        #[arg(long)]
        to: String,
    },

    /// Print CPG statistics.
    Stats {
        /// Root directory of the codebase.
        #[arg(short, long)]
        repo: PathBuf,
    },

    /// Test the parser on a single file and print detected nodes.
    Test {
        /// Path to a single file to parse.
        #[arg(short, long)]
        file: PathBuf,
    },

    /// Compute a danger map for taint-guided fuzzing prioritization.
    DangerMap {
        #[arg(short, long)]
        repo: PathBuf,

        #[arg(long, default_value = "0.7")]
        decay: f32,
    },

    /// Run as a daemon listening on a Unix socket.
    RunServer {
        /// Unified config file path.
        #[arg(short, long, default_value = "/etc/bugswarm/config.yaml")]
        config: PathBuf,

        /// Unix socket path (overrides config file).
        #[arg(short, long)]
        socket: Option<PathBuf>,

        /// PID file path (overrides config file).
        #[arg(long)]
        pid_file: Option<PathBuf>,

        /// HTTP health/metrics port (overrides config file).
        #[arg(long)]
        http_port: Option<u16>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = fmt::layer().with_target(true).with_thread_ids(true).json();
    let subscriber = tracing_subscriber::registry().with(env_filter).with(fmt_layer);

    if let Some(ref path) = cli.log_file {
        let file = match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to open log file {}: {}", path, e);
                subscriber.try_init().ok();
                return Err(anyhow::anyhow!("Failed to open log file {}: {}", path, e));
            }
        };
        let file_layer = fmt::layer()
            .with_writer(std::sync::Mutex::new(file))
            .json();
        subscriber.with(file_layer).try_init().ok();
    } else {
        subscriber.try_init().ok();
    }

    match cli.command {
        Commands::Index { repo, output, prune } => {
            let mut cpg = CodePropertyGraph::new();
            info!("Indexing: {}", repo.display());
            parser::index_directory(&mut cpg, &repo)?;

            if prune {
                info!("Pruning unreachable nodes...");
                cpg.prune_unreachable();
            }

            let stats = cpg.stats();
            let json = serde_json::to_string_pretty(&stats)?;

            match output {
                Some(path) => std::fs::write(&path, json)?,
                None => println!("{}", json),
            }

            info!("Indexed {} files, {} functions, {} nodes",
                stats.total_files, stats.total_functions, stats.total_nodes);
        }

        Commands::Taint { repo, source, sink } => {
            let mut cpg = CodePropertyGraph::new();
            parser::index_directory(&mut cpg, &repo)?;

            let paths = cpg.find_taint_paths();

            let filtered: Vec<_> = paths.iter()
                .filter(|p| {
                    let src_match = source.as_ref().is_none_or(|s| {
                        cpg.get_node(p.source).is_some_and(|n| n.name.contains(s))
                    });
                    let sink_match = sink.as_ref().is_none_or(|s| {
                        cpg.get_node(p.sink).is_some_and(|n| n.name.contains(s))
                    });
                    src_match && sink_match
                })
                .collect();

            println!("Found {} taint paths ({} after filter)", paths.len(), filtered.len());
            for (i, path) in filtered.iter().enumerate() {
                let src = cpg.get_node(path.source).map(|n| n.name.clone()).unwrap_or_default();
                let snk = cpg.get_node(path.sink).map(|n| n.name.clone()).unwrap_or_default();
                println!("\nPath {}: {} → {} (length={}, sanitized={}, confidence={:.2})",
                    i + 1, src, snk, path.length, path.sanitized, path.confidence);
                for node_id in &path.path {
                    if let Some(node) = cpg.get_node(*node_id) {
                        println!("  {}:{} ({})", node.file, node.line_start, node.name);
                    }
                }
            }
        }

        Commands::CallPath { repo, from, to } => {
            let mut cpg = CodePropertyGraph::new();
            parser::index_directory(&mut cpg, &repo)?;

            let paths = cpg.find_call_paths(&from, &to);
            println!("Found {} call paths from {} to {}", paths.len(), from, to);
            for (i, path) in paths.iter().enumerate() {
                println!("\nPath {} (length={}):", i + 1, path.length);
                for (j, func) in path.path.iter().enumerate() {
                    println!("  {} → {}", func, path.path.get(j + 1).unwrap_or(&"?".to_string()));
                }
            }
        }

        Commands::Stats { repo } => {
            let mut cpg = CodePropertyGraph::new();
            parser::index_directory(&mut cpg, &repo)?;
            let stats = cpg.stats();
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }

        Commands::DangerMap { repo, decay } => {
            let mut cpg = CodePropertyGraph::new();
            parser::index_directory(&mut cpg, &repo)?;
            let danger_map = bugswarm_cpg::danger_map::danger_map_from_graph(&cpg, decay);
            let sinks_used: Vec<&str> = bugswarm_cpg::danger_map::DEFAULT_SINKS.to_vec();
            let entries: Vec<serde_json::Value> = danger_map.iter().map(|(addr, score)| {
                serde_json::json!({"address": format!("0x{:x}", addr), "danger_score": score})
            }).collect();
            let result = serde_json::json!({
                "num_entries": entries.len(),
                "sink_count": sinks_used.len(),
                "decay_factor": decay,
                "entries": entries,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            println!("{}", serde_json::to_string(&result)?);
        }

        Commands::Test { file } => {
            let mut cpg = CodePropertyGraph::new();
            parser::parse_file(&mut cpg, &file)?;
            let stats = cpg.stats();
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }

        Commands::RunServer { config, socket, pid_file, http_port } => {
            // Read unified config with schema_version validation
            let yaml_opt: Option<serde_yaml::Value> = if config.exists() {
                let content = std::fs::read_to_string(&config)?;
                let yaml: serde_yaml::Value = match serde_yaml::from_str(&content) {
                    Ok(y) => y,
                    Err(e) => {
                        tracing::error!("Failed to parse config file {}: {}", config.display(), e);
                        std::process::exit(1);
                    }
                };
                let sv = yaml["schema_version"].as_u64().unwrap_or(0);
                if sv != 1 {
                    tracing::error!(
                        "Unsupported schema_version {} in {}. Expected 1. Please migrate or update your config.",
                        sv,
                        config.display()
                    );
                    std::process::exit(1);
                }
                Some(yaml)
            } else {
                None
            };

            let yaml_ref = yaml_opt.as_ref();

            let socket_path = socket.unwrap_or_else(|| {
                PathBuf::from(config_or_env(yaml_ref, "/daemons/cpg/socket", "BGSWARM_CPG_SOCKET", "/var/run/bugswarm/cpg.sock"))
            });
            let pid_path = pid_file.unwrap_or_else(|| {
                PathBuf::from(config_or_env(yaml_ref, "/daemons/cpg/pid_file", "BGSWARM_CPG_PID_FILE", "/var/run/bugswarm/cpg.pid"))
            });
            let port = http_port.unwrap_or_else(|| {
                config_or_env(yaml_ref, "/daemons/cpg/http_port", "BGSWARM_CPG_HTTP_PORT", "8081").parse::<u16>().unwrap_or(8081)
            });

            // Log effective config (secrets redacted)
            let log_level = config_or_env(yaml_ref, "/logging/level", "BGSWARM_LOG_LEVEL", "info");
            info!(
                effective_config = %serde_json::json!({
                    "schema_version": 1,
                    "log_level": log_level,
                    "cpg_socket": socket_path.display().to_string(),
                    "cpg_pid_file": pid_path.display().to_string(),
                    "cpg_http_port": port,
                }),
                "Effective configuration loaded"
            );

            info!("Starting CPG daemon on {}", socket_path.display());

            #[cfg(unix)]
            {
                let sp = socket_path.clone();
                tokio::spawn(async move {
                    use tokio::signal::unix::{signal, SignalKind};
                    let mut sighup = match signal(SignalKind::hangup()) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!("Failed to register SIGHUP handler: {}", e);
                            return;
                        }
                    };
                    loop {
                        sighup.recv().await;
                        tracing::info!("Received SIGHUP on CPG daemon — runtime config reload is limited to mutable fields");
                    }
                });
                let _ = sp;
            }

            // Create PID file (cleaned up on drop)
            struct PidGuard(std::path::PathBuf);
            impl Drop for PidGuard {
                fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
            }
            if let Some(parent) = pid_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&pid_path, std::process::id().to_string())?;
            let _pid = PidGuard(pid_path);
            bugswarm_cpg::daemon::run_daemon(socket_path, Some(port)).await?;
        }
    }

    Ok(())
}

fn config_or_env(yaml: Option<&serde_yaml::Value>, path: &str, env_name: &str, default: &str) -> String {
    std::env::var(env_name).unwrap_or_else(|_| {
        let mut current = yaml;
        for key in path.split('/').skip(1) {
            current = current.and_then(|v| v.get(key));
        }
        current.and_then(|v| v.as_str()).unwrap_or(default).to_string()
    })
}
