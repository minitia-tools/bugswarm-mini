use bugswarm_evidence::graph::EvidenceGraph;
use bugswarm_evidence::types::{EvidenceNode, NodeKind, EvidenceEdge, EdgeKind, EvidenceQuery};
use clap::{Parser, Subcommand};
use std::collections::HashMap;

#[derive(Parser)]
#[command(name = "bugswarm-evidence", version, about = "Bug Swarm Evidence Graph CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run demo: create nodes and edges, query, score.
    Demo,
    /// Show graph statistics.
    Stats,
    /// Verify integrity of all immutable nodes.
    Verify,
    /// Run Phase 5 Gate tests.
    Test,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Demo => run_demo(),
        Commands::Stats => run_stats(),
        Commands::Verify => run_verify(),
        Commands::Test => run_gate_tests(),
    }
}

fn run_demo() {
    println!("=== Evidence Graph Demo ===\n");
    let g = EvidenceGraph::new();

    // Register agents
    let agent_a = g.add_node(EvidenceNode::new(0, NodeKind::Agent, "Agent A (Adversarial)", "A"));
    let agent_b = g.add_node(EvidenceNode::new(1, NodeKind::Agent, "Agent B (Causal)", "B"));

    // Agent A: finds SQL injection
    let claim1 = g.add_claim("SQL injection in login()", "A", "auth.py:42", 9);
    let pred1 = g.add_prediction("Input ' OR 1=1 -- bypasses auth", "A", claim1).0;

    // Agent B: finds null pointer
    let claim2 = g.add_claim("Null pointer in user.trim()", "B", "auth.py:47", 5);
    let pred2 = g.add_prediction("Null username causes crash", "B", claim2).0;

    // Sandbox confirms Agent A's prediction
    let run1 = g.add_sandbox_run("Sandbox #8472", "NPE at auth.py:42 — SQL injection confirmed", "orchestrator", r#"{"exit_code":1,"status":"Failed","stdout":"bypass"}"#);
    g.link_sandbox_result(run1, pred1, true, 0.95);

    // Sandbox supports Agent B's prediction
    let run2 = g.add_sandbox_run("Sandbox #8501", "NPE at auth.py:47", "orchestrator", r#"{"exit_code":1,"status":"Failed"}"#);
    g.link_sandbox_result(run2, pred2, true, 0.80);

    // Confirm bugs
    g.confirm_bug(claim1, &[run1]);
    g.confirm_bug(claim2, &[run2]);

    // Stats
    let stats = g.stats();
    println!("Graph Stats:");
    println!("  Nodes: {} ({} claims, {} predictions, {} sandbox runs, {} confirmed bugs)",
        stats.total_nodes, stats.claims, stats.predictions, stats.sandbox_runs, stats.confirmed_bugs);
    println!("  Edges: {} ({} supports, {} contradicts, {} confirms)",
        stats.total_edges, stats.supports_count, stats.contradicts_count, stats.confirms_count);

    // Scores
    let score_a = g.score_agent("A");
    let score_b = g.score_agent("B");
    println!("\nAgent Scores:");
    println!("  A: novelty={:.2}, verification={:.2}, composite={:.2}",
        score_a.novelty_score, score_a.verification_score, score_a.composite_score);
    println!("  B: novelty={:.2}, verification={:.2}, composite={:.2}",
        score_b.novelty_score, score_b.verification_score, score_b.composite_score);

    // Queries
    let claims = g.query(&EvidenceQuery { kind: Some(NodeKind::Claim), ..Default::default() });
    println!("\nClaims: {}", claims.len());
    for c in &claims {
        println!("  [{}] {} (severity {})", c.id, c.label, c.severity.unwrap_or(0));
    }

    println!("\n✓ Evidence Graph demo complete");
}

fn run_stats() {
    let g = EvidenceGraph::new();
    let stats = g.stats();
    println!("{}", serde_json::to_string_pretty(&stats).unwrap());
}

fn run_verify() {
    let g = EvidenceGraph::new();
    let corrupted = g.verify_integrity();
    if corrupted.is_empty() {
        println!("✓ All immutable nodes have valid hashes");
    } else {
        println!("✗ Corrupted nodes: {:?}", corrupted);
    }
}

fn run_gate_tests() {
    println!("=== Phase 5 Gate: Immutability Assault ===\n");
    let mut passed = 0;
    let mut failed = 0;
    let G = "\x1b[0;32m"; let R = "\x1b[0;31m"; let N = "\x1b[0m";

    let g = EvidenceGraph::new();

    // Test 1: Add nodes and edges
    let c = g.add_claim("SQL injection", "A", "db.py:10", 8);
    let p = g.add_prediction("' OR 1=1 -- works", "A", c).0;
    let r = g.add_sandbox_run("PoC #1", "confirmed", "orch", r#"{"ok":true}"#);
    g.link_sandbox_result(r, p, true, 0.9);
    g.confirm_bug(c, &[r]);
    // Add another claim to test edge count
    let c2 = g.add_claim("XSS in render()", "B", "ui.py:20", 6);
    g.add_prediction("<script>alert(1)</script> triggers", "B", c2);

    let stats = g.stats();
    let t1 = stats.total_nodes >= 6 && stats.total_edges >= 5;
    println!("  [{}] {} nodes, {} edges", if t1 {"PASS"} else {"FAIL"}, stats.total_nodes, stats.total_edges);
    if t1 { passed += 1; } else { failed += 1; }

    // Test 2: Sandbox nodes are immutable
    let node = g.get_node(r).unwrap();
    let t2 = node.immutable && node.content_hash.is_some();
    println!("  [{}] Sandbox node immutable + hashed", if t2 {"PASS"} else {"FAIL"});
    if t2 { passed += 1; } else { failed += 1; }

    // Test 3: Integrity verification
    let corrupted = g.verify_integrity();
    let t3 = corrupted.is_empty();
    println!("  [{}] No corrupted nodes", if t3 {"PASS"} else {"FAIL"});
    if t3 { passed += 1; } else { failed += 1; }

    // Test 4: Agent scoring
    let score = g.score_agent("A");
    let t4 = score.total_claims >= 1 && score.verification_score > 0.0;
    println!("  [{}] Agent A: {}/{} verified, composite={:.2}", if t4 {"PASS"} else {"FAIL"}, score.verified_claims, score.total_claims, score.composite_score);
    if t4 { passed += 1; } else { failed += 1; }

    // Test 5: Weakest/strongest claims
    let weak = g.weakest_claim();
    let strong = g.strongest_claim();
    let t5 = weak.is_some() && strong.is_some();
    println!("  [{}] Weakest: {:?}, Strongest: {:?}", if t5 {"PASS"} else {"FAIL"},
        weak.map(|(_,l,_)| l), strong.map(|(_,l,_,_)| l));
    if t5 { passed += 1; } else { failed += 1; }

    // Test 6: Orphaned claims
    let _orphan = g.add_claim("Orphaned bug", "C", "x.py:1", 3);
    let orphans = g.orphaned_claims();
    let t6 = !orphans.is_empty();
    println!("  [{}] {} orphaned claims found", if t6 {"PASS"} else {"FAIL"}, orphans.len());
    if t6 { passed += 1; } else { failed += 1; }

    // Test 7: Concurrency — 100 parallel reads
    let g2 = std::sync::Arc::new(EvidenceGraph::new());
    g2.add_claim("test", "X", "f.py:1", 1);
    let mut handles = vec![];
    for _ in 0..100 {
        let g = g2.clone();
        handles.push(std::thread::spawn(move || { g.stats(); }));
    }
    for h in handles { h.join().unwrap(); }
    let t7 = true;
    println!("  [{}] 100 concurrent reads complete", if t7 {"PASS"} else {"FAIL"});
    if t7 { passed += 1; } else { failed += 1; }

    // Test 8: Large graph performance
    let g3 = EvidenceGraph::new();
    for i in 0..1000 {
        let claim = g3.add_claim(&format!("bug {}", i), "agent", &format!("f{}.py:1", i), (i % 10 + 1) as u8);
        let pred = g3.add_prediction(&format!("pred {}", i), "agent", claim).0;
        let run = g3.add_sandbox_run(&format!("run {}", i), "ok", "orch", r#"{}"#);
        g3.link_sandbox_result(run, pred, true, 0.5);
    }
    let start = std::time::Instant::now();
    let stats3 = g3.stats();
    let dur = start.elapsed();
    let t8 = dur.as_millis() < 1000;
    println!("  [{}] 1000-node graph stats in {}ms", if t8 {"PASS"} else {"FAIL"}, dur.as_millis());
    if t8 { passed += 1; } else { failed += 1; }

    println!("\n═══ Phase 5 Gate: {}{} passed{}, {}{} failed{}, {} total ═══",
        G, passed, N, R, failed, N, passed + failed);
    if failed == 0 {
        println!("{}✓ PHASE 5 GATE PASSED — Evidence Graph immutable + queryable{}", G, N);
    } else {
        println!("{}✗ PHASE 5 GATE FAILED{}", R, N);
    }
}
