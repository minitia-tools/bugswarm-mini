use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::EnvFilter;

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

    /// Run as a daemon listening on a Unix socket.
    RunServer {
        /// Unix socket path.
        #[arg(short, long, default_value = "/var/run/bugswarm/cpg.sock")]
        socket: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let cli = Cli::parse();

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
                    let src_match = source.as_ref().map_or(true, |s| {
                        cpg.get_node(p.source).map_or(false, |n| n.name.contains(s))
                    });
                    let sink_match = sink.as_ref().map_or(true, |s| {
                        cpg.get_node(p.sink).map_or(false, |n| n.name.contains(s))
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

        Commands::Test { file } => {
            let mut cpg = CodePropertyGraph::new();
            parser::parse_file(&mut cpg, &file)?;
            let stats = cpg.stats();
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }

        Commands::RunServer { socket } => {
            info!("Starting CPG daemon on {}", socket.display());
            bugswarm_cpg::daemon::run_daemon(socket).await?;
        }
    }

    Ok(())
}
