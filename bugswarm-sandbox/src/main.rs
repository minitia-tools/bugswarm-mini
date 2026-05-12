use std::collections::HashMap;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use bugswarm_sandbox::config::{
    CausalIntervention, ExecutionReceipt, SandboxConfig,
};
use bugswarm_sandbox::error::SandboxError;
use bugswarm_sandbox::container::ContainerManager;
use bugswarm_sandbox::error::SandboxResult;
use bugswarm_sandbox::seccomp::SeccompProfile;

/// Bug Swarm Sandbox Daemon — Isolated Proof-of-Concept Execution Engine
///
/// Executes agent-generated PoC scripts in highly constrained Docker containers
/// and returns cryptographically verifiable execution receipts.
#[derive(Parser)]
#[command(name = "bugswarm-sandbox", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to sandbox config YAML
    #[arg(short, long, default_value = "/etc/bugswarm/sandbox.yaml")]
    config: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute a single PoC and return an execution receipt.
    Execute {
        /// Path to the PoC script file, or '-' for stdin.
        #[arg(short, long)]
        poc: PathBuf,

        /// Environment variables in KEY=VALUE format (whitelist-validated).
        #[arg(short, long)]
        env: Vec<String>,

        /// Output file for the receipt JSON. Defaults to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Run with flaky detection annotation.
        #[arg(long)]
        flaky: bool,
    },

    /// Run statistical re-execution (N runs) for flaky bug detection.
    ExecuteStatistical {
        /// Path to the PoC script file.
        #[arg(short, long)]
        poc: PathBuf,

        /// Environment variables in KEY=VALUE format.
        #[arg(short, long)]
        env: Vec<String>,

        /// Number of runs (default from config, usually 100).
        #[arg(short, long)]
        count: Option<u32>,

        /// Output file for the statistical result JSON.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Apply a causal intervention and re-execute.
    CausalIntervention {
        /// Path to the PoC script file.
        #[arg(short, long)]
        poc: PathBuf,

        /// Environment variables in KEY=VALUE format.
        #[arg(short, long)]
        env: Vec<String>,

        /// File path to surgically modify.
        #[arg(long)]
        file: String,

        /// Line number to replace (1-indexed).
        #[arg(long)]
        line: u32,

        /// Original line content.
        #[arg(long)]
        original: String,

        /// Replacement line content.
        #[arg(long)]
        replacement: String,

        /// Output file for the intervention result JSON.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Independently re-execute a PoC to verify a previous receipt.
    VerifyReceipt {
        /// Path to the PoC script file.
        #[arg(short, long)]
        poc: PathBuf,

        /// Environment variables in KEY=VALUE format.
        #[arg(short, long)]
        env: Vec<String>,

        /// Output file for the verification receipt.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Validate environment variables against the whitelist.
    ValidateEnv {
        /// Environment variables in KEY=VALUE format.
        #[arg(short, long)]
        env: Vec<String>,
    },

    /// Generate and validate the seccomp profile.
    SeccompProfile {
        /// Output the default profile JSON.
        #[arg(long)]
        output_default: bool,

        /// Validate a custom seccomp profile file.
        #[arg(short, long)]
        validate: Option<PathBuf>,
    },

    /// Run as a daemon listening on a Unix socket.
    RunServer {
        /// Unix socket path.
        #[arg(short, long, default_value = "/var/run/bugswarm/sandbox.sock")]
        socket: PathBuf,
    },

    /// Print the default configuration.
    DefaultConfig,
}

fn parse_env_vars(env_args: &[String]) -> HashMap<String, String> {
    env_args
        .iter()
        .filter_map(|arg| {
            let parts: Vec<&str> = arg.splitn(2, '=').collect();
            if parts.len() == 2 {
                Some((parts[0].to_string(), parts[1].to_string()))
            } else {
                None
            }
        })
        .collect()
}

async fn run_command(cli: Cli) -> SandboxResult<()> {
    let config = if cli.config.exists() {
        let content = std::fs::read_to_string(&cli.config)?;
        serde_yaml::from_str(&content).unwrap_or_else(|e| {
            tracing::warn!("Failed to parse config file {}: {}. Using defaults.", cli.config.display(), e);
            SandboxConfig::default()
        })
    } else {
        SandboxConfig::default()
    };
    let manager = ContainerManager::connect(config.clone()).await?;

    // Ensure image is available (skip for meta-commands that don't need it)
    if !matches!(cli.command, Commands::ValidateEnv { .. } | Commands::SeccompProfile { .. } | Commands::DefaultConfig) {
        manager.ensure_image().await?;
    }

    match cli.command {
        Commands::Execute { poc, env, output, flaky } => {
            let poc_content = if poc == PathBuf::from("-") {
                std::io::read_to_string(std::io::stdin())?
            } else {
                std::fs::read_to_string(&poc)?
            };

            let env_vars = parse_env_vars(&env);
            let receipt = manager.execute(&poc_content, &env_vars, flaky).await?;

            output_receipt(&receipt, output.as_ref())?;
        }

        Commands::ExecuteStatistical { poc, env, count, output } => {
            let poc_content = std::fs::read_to_string(&poc)?;
            let env_vars = parse_env_vars(&env);

            let mut stat_config = config.clone();
            if let Some(n) = count {
                stat_config.rerun_count = n;
            }

            let stats = manager.execute_statistical(&poc_content, &env_vars).await?;
            let json = serde_json::to_string_pretty(&stats)?;

            match output {
                Some(path) => std::fs::write(&path, json)?,
                None => println!("{}", json),
            }

            if stats.is_significant {
                info!("Flaky bug confirmed: failure_rate={:.2}%, CI=[{:.4}, {:.4}]",
                    stats.failure_rate * 100.0,
                    stats.confidence_interval_lower,
                    stats.confidence_interval_upper);
            } else {
                info!("Flaky bug NOT confirmed: failure_rate={:.2}% below threshold",
                    stats.failure_rate * 100.0);
            }
        }

        Commands::CausalIntervention { poc, env, file, line, original, replacement, output } => {
            let poc_content = std::fs::read_to_string(&poc)?;
            let env_vars = parse_env_vars(&env);

            let intervention = CausalIntervention {
                original_line: original,
                replacement_line: replacement,
                file_path: file,
                line_number: line,
            };

            let result = manager.execute_causal_intervention(
                &poc_content,
                &env_vars,
                intervention,
            ).await?;

            let json = serde_json::to_string_pretty(&result)?;
            match output {
                Some(path) => std::fs::write(&path, json)?,
                None => println!("{}", json),
            }

            if result.causality_confirmed {
                info!("Causality CONFIRMED: crash resolved by surgical fix at {}:{}",
                    result.intervention.file_path, result.intervention.line_number);
            } else {
                info!("Causality NOT confirmed: crash persists despite surgical fix");
            }
        }

        Commands::VerifyReceipt { poc, env, output } => {
            let poc_content = std::fs::read_to_string(&poc)?;
            let env_vars = parse_env_vars(&env);

            let receipt = manager.independent_reexecute(&poc_content, &env_vars).await?;
            output_receipt(&receipt, output.as_ref())?;
        }

        Commands::ValidateEnv { env } => {
            let env_vars = parse_env_vars(&env);
            match manager.validate_env_vars(&env_vars) {
                Ok(validated) => {
                    println!("All environment variables valid:");
                    for (k, v) in &validated {
                        println!("  {}={}", k, v);
                    }
                }
                Err(e) => {
                    error!("{}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::SeccompProfile { output_default, validate } => {
            if output_default {
                let profile = SeccompProfile::default_profile()?;
                println!("{}", serde_json::to_string_pretty(&profile)?);
                return Ok(());
            }

            if let Some(path) = validate {
                let profile = SeccompProfile::from_file(
                    path.to_str().ok_or_else(|| SandboxError::Other(format!("Non-UTF8 path: {}", path.display())))?
                )?;
                profile.validate()?;
                println!("Seccomp profile is valid. {} syscalls allowed, {} blocked.",
                    profile.allowed_count(),
                    profile.blocked_syscalls().len());
                return Ok(());
            }

            // Default: validate built-in profile
            let profile = SeccompProfile::default_profile()?;
            profile.validate()?;
            println!("Default seccomp profile is valid. {} syscalls allowed.",
                profile.allowed_count());
        }

        Commands::RunServer { socket } => {
            info!("Starting sandbox daemon on {}", socket.display());
            bugswarm_sandbox::daemon::run_daemon(socket, config).await?;
        }

        Commands::DefaultConfig => {
            let config = SandboxConfig::default();
            let output = serde_yaml::to_string(&config)
                .unwrap_or_else(|e| format!("Error serializing config: {}\n{}", e,
                    serde_json::to_string_pretty(&config).unwrap_or_else(|e2| format!("JSON fallback also failed: {}", e2))));
        }
    }

    Ok(())
}

fn output_receipt(receipt: &ExecutionReceipt, output: Option<&PathBuf>) -> SandboxResult<()> {
    let json = serde_json::to_string_pretty(receipt)?;

    match output {
        Some(path) => {
            std::fs::write(path, &json)?;
            println!("Receipt written to {}", path.display());
        }
        None => println!("{}", json),
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let cli = Cli::parse();

    if let Err(e) = run_command(cli).await {
        error!("{}", e);
        std::process::exit(1);
    }
}
