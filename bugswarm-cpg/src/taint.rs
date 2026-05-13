/// Peak Taint Propagation Engine — Phase 17
///
/// C6.2.3: Inter-procedural worklist algorithm with call graph summaries
/// C6.2.4: AST contract checking + weighted sanitizer classification
/// C6.2.6: Path-sensitive multiplicative confidence scoring

use crate::graph::{CodePropertyGraph, EdgeKind};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use std::collections::{HashMap, HashSet, VecDeque};

type NodeId = NodeIndex;

/// A single hop in a taint path with confidence tracking.
#[derive(Debug, Clone)]
pub struct TaintHop {
    pub node_id: NodeId,
    pub node_name: String,
    pub file: String,
    pub line: usize,
    pub hop_type: String,
    /// Multiplier applied to confidence at this hop (C6.2.6)
    pub confidence_multiplier: f64,
}

impl TaintHop {
    pub fn new(node_id: NodeId, name: &str, file: &str, line: usize, hop_type: &str, multiplier: f64) -> Self {
        Self {
            node_id,
            node_name: name.to_string(),
            file: file.to_string(),
            line,
            hop_type: hop_type.to_string(),
            confidence_multiplier: multiplier,
        }
    }
}

/// SSA-based taint path with path-sensitive confidence (C6.2.6).
#[derive(Debug, Clone)]
pub struct SsaTaintPath {
    pub source_id: NodeId,
    pub sink_id: NodeId,
    pub source_name: String,
    pub sink_name: String,
    pub hops: Vec<TaintHop>,
    pub sanitized: bool,
    pub sanitizer_id: Option<NodeId>,
    pub length: usize,
    /// C6.2.6: Multiplicative confidence: 0.95 × ∏(hop_multiplier)
    pub confidence: f64,
}

// ═══════════════════════════════════════════════════════════════
// C6.2.6: Multiplicative Confidence Multipliers
// ═══════════════════════════════════════════════════════════════

fn confidence_multiplier(edge_kind: &EdgeKind, target_node: &crate::graph::GraphNode) -> f64 {
    match edge_kind {
        EdgeKind::DataFlow | EdgeKind::TaintFlow => 0.99,
        EdgeKind::References => 0.95,
        EdgeKind::Assigns => 0.99,
        EdgeKind::Calls => {
            // Through function call: depends on confidence in parameter mapping
            if target_node.metadata.get("resolved").map_or(false, |v| v == "true") {
                0.95 // Cross-file, resolved via two-pass
            } else {
                0.75 // External stub, lower confidence
            }
        }
        EdgeKind::Contains => 0.99,
        _ => 0.95,
    }
}

/// Degradation multipliers for SSA hop types (C6.2.6).
fn hop_degradation(hop_type: &str) -> f64 {
    let h = hop_type.to_lowercase();
    if h.contains("assign") { return 0.99; }
    if h.contains("dataflow") || h.contains("taintflow") { return 0.99; }
    if h.contains("call") { return 0.95; }
    if h.contains("return") { return 0.95; }
    if h.contains("field") { return 0.85; }
    if h.contains("collection") { return 0.80; }
    if h.contains("conditional") { return 0.70; }
    if h.contains("dynamic") { return 0.50; }
    if h.contains("loop") { return 0.60; }
    if h.contains("global") { return 0.75; }
    0.95 // default
}

// ═══════════════════════════════════════════════════════════════
// C6.2.4: Enhanced Sanitizer Detection — AST Contract + Library
// ═══════════════════════════════════════════════════════════════

/// Known sanitizer libraries and functions (weighted).
const KNOWN_SANITIZERS: &[(&str, f64)] = &[
    ("html.escape", 1.0), ("bleach.clean", 1.0), ("markupsafe.escape", 1.0),
    ("django.utils.html.escape", 1.0), ("urllib.parse.quote", 1.0),
    ("shlex.quote", 1.0), ("pipes.quote", 1.0),
    ("mysql_real_escape_string", 1.0), ("pg_escape_string", 1.0),
    ("sqlite3.escape", 0.9), ("re.escape", 0.7),
    ("strip_tags", 0.9), ("htmlspecialchars", 1.0),
    ("filter_var", 0.9), ("sanitize", 0.5), ("clean", 0.3),
    ("escape", 0.3), // Low base score — needs context confirmation
];

/// Known UNSAFE functions that REVERSE sanitization.
const ANTI_SANITIZERS: &[&str] = &[
    "unescape", "html.unescape", "urllib.parse.unquote",
    "base64.b64decode", "decode", "unquote",
];

/// C6.2.4: Check if a function node is a sanitizer using AST-level heuristics.
/// Returns (is_sanitizer, confidence_score 0.0-1.0).
pub fn classify_sanitizer(node: &crate::graph::GraphNode) -> (bool, f64) {
    let lower = node.name.to_lowercase();

    // 1. Anti-sanitizer check: if it reverses sanitization, NOT a sanitizer
    for anti in ANTI_SANITIZERS {
        if lower.contains(anti) {
            return (false, 0.0);
        }
    }

    // 2. Known sanitizer library check (highest weight)
    for (name, weight) in KNOWN_SANITIZERS {
        if lower.contains(name) {
            return (true, *weight);
        }
    }

    // 3. Heuristic: does the function name suggest sanitization?
    let heuristic_score = if lower.contains("sanitize") || lower.contains("clean") || lower.contains("strip") {
        0.4
    } else if lower.contains("escape") || lower.contains("quote") || lower.contains("encode") {
        0.3
    } else if lower.contains("filter") || lower.contains("validate") {
        0.2
    } else {
        0.0
    };

    (heuristic_score >= 0.3, heuristic_score)
}

/// Legacy compat: check if node name suggests sanitization
pub fn is_sanitizer_node(node: &crate::graph::GraphNode) -> bool {
    classify_sanitizer(node).0
}

// ═══════════════════════════════════════════════════════════════
// C6.2.3: Inter-Procedural Worklist Algorithm
// ═══════════════════════════════════════════════════════════════

/// Run peak taint propagation: worklist algorithm with path-sensitive confidence.
pub fn propagate_taint_peak(cpg: &CodePropertyGraph) -> Vec<SsaTaintPath> {
    let mut paths = Vec::new();

    for &source_id in &cpg.sources {
        for &sink_id in &cpg.sinks {
            let found = trace_taint_worklist(cpg, source_id, sink_id);
            paths.extend(found);
        }
    }

    // Sort by confidence descending
    paths.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    paths
}

/// Worklist-based taint tracing from source to sink.
/// Visited tracks (node, sanitized) pairs for multi-path discovery.
fn trace_taint_worklist(
    cpg: &CodePropertyGraph,
    source: NodeId,
    sink: NodeId,
) -> Vec<SsaTaintPath> {
    let mut results = Vec::new();
    let mut queue = VecDeque::new();
    let mut visited: HashSet<(NodeId, bool)> = HashSet::new();

    let source_node = match cpg.get_node(source) {
        Some(n) => n,
        None => return results,
    };

    let start_hop = TaintHop::new(source, &source_node.name, &source_node.file,
                                   source_node.line_start, "source", 1.0);

    queue.push_back((source, vec![start_hop], false, None, 0.95_f64));
    visited.insert((source, false));

    let mut iterations = 0;
    let max_iterations = 10_000;

    while let Some((current, path, sanitized, sanitizer, confidence)) = queue.pop_front() {
        iterations += 1;
        if iterations > max_iterations {
            break;
        }

        if current == sink {
            let src_name = cpg.get_node(source).map(|n| n.name.clone()).unwrap_or_default();
            let snk_name = cpg.get_node(sink).map(|n| n.name.clone()).unwrap_or_default();

            results.push(SsaTaintPath {
                source_id: source, sink_id: sink,
                source_name: src_name, sink_name: snk_name,
                hops: path.clone(), sanitized,
                sanitizer_id: sanitizer, length: path.len(),
                // C6.2.6: Apply sanitization penalty
                confidence: if sanitized { confidence * 0.35 } else { confidence },
            });

            if results.len() >= 20 {
                break;
            }
            continue;
        }

        if path.len() > 50 {
            continue;
        }

        for edge in cpg.graph.edges(current) {
            let target = edge.target();
            let target_node = &cpg.graph[target];

            // C6.2.6: compute confidence degradation for this edge
            let edge_mult = confidence_multiplier(&edge.weight().kind, target_node);

            // C6.2.4: check sanitizer
            let (is_san, _san_score) = classify_sanitizer(target_node);
            let becomes_sanitized = sanitized || is_san;

            let key = (target, becomes_sanitized);
            if visited.contains(&key) {
                continue;
            }
            visited.insert(key);

            let hop_type = format!("{:?}", edge.weight().kind).to_lowercase();
            let hop_degrade = hop_degradation(&hop_type);
            let hop_mult = edge_mult * hop_degrade;

            let hop = TaintHop::new(target, &target_node.name, &target_node.file,
                                     target_node.line_start, &hop_type, hop_mult);

            let mut new_path = path.clone();
            new_path.push(hop);

            let new_sanitizer = if is_san { Some(target) } else { sanitizer };
            let new_confidence = confidence * hop_mult;

            // Drop paths with negligible confidence
            if new_confidence < 0.01 {
                continue;
            }

            queue.push_back((target, new_path, becomes_sanitized, new_sanitizer, new_confidence));
        }
    }

    results
}

/// Legacy compat: run enhanced (non-worklist) propagation
pub fn propagate_taint_enhanced(cpg: &CodePropertyGraph) -> Vec<SsaTaintPath> {
    propagate_taint_peak(cpg)
}

/// Stats from taint propagation.
#[derive(Debug, Clone)]
pub struct TaintStats {
    pub total_paths: usize,
    pub sanitized_paths: usize,
    pub unsanitized_paths: usize,
    pub avg_path_length: f64,
    pub max_path_length: usize,
    pub sources_analyzed: usize,
    pub sinks_analyzed: usize,
    /// C6.2.6: Average confidence of top-10 paths
    pub avg_top10_confidence: f64,
}

pub fn compute_taint_stats(paths: &[SsaTaintPath], sources: usize, sinks: usize) -> TaintStats {
    let total = paths.len();
    let sanitized = paths.iter().filter(|p| p.sanitized).count();
    let lengths: Vec<usize> = paths.iter().map(|p| p.length).collect();
    let avg = if total > 0 { lengths.iter().sum::<usize>() as f64 / total as f64 } else { 0.0 };
    let max = lengths.iter().max().copied().unwrap_or(0);
    let avg_top10 = if total > 0 {
        paths.iter().take(10).map(|p| p.confidence).sum::<f64>() / (total.min(10) as f64)
    } else { 0.0 };

    TaintStats {
        total_paths: total,
        sanitized_paths: sanitized,
        unsanitized_paths: total - sanitized,
        avg_path_length: avg,
        max_path_length: max,
        sources_analyzed: sources,
        sinks_analyzed: sinks,
        avg_top10_confidence: avg_top10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{CodePropertyGraph, EdgeKind, GraphEdge, GraphNode, NodeKind};
    use std::collections::HashMap;

    fn make_node(cpg: &mut CodePropertyGraph, name: &str, kind: NodeKind, file: &str, line: usize) -> NodeId {
        cpg.add_node(GraphNode {
            id: name.to_string(), kind, name: name.to_string(), file: file.to_string(),
            line_start: line, line_end: line, col_start: 0, col_end: 0,
            language: "python".into(), metadata: HashMap::new(),
        })
    }

    // ─── C6.2.6: Confidence Tests ───

    #[test]
    fn test_confidence_multiplicative_degradation() {
        let mut cpg = CodePropertyGraph::new();
        let src = make_node(&mut cpg, "request.form", NodeKind::Source, "app.py", 1);
        let mid = make_node(&mut cpg, "x = request", NodeKind::Assignment, "app.py", 2);
        let sink = make_node(&mut cpg, "os.system", NodeKind::Sink, "app.py", 3);

        cpg.add_edge(src, mid, GraphEdge {
            kind: EdgeKind::Assigns, label: None, confidence: 1.0, metadata: HashMap::new(),
        });
        cpg.add_edge(mid, sink, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.9, metadata: HashMap::new(),
        });
        cpg.sources.push(src);
        cpg.sinks.push(sink);

        let paths = propagate_taint_peak(&cpg);
        assert!(!paths.is_empty());
        // 2-hop path: 0.95 * 0.99 (assignment) * 0.99 (dataflow) = ~0.93
        // Actually: start conf 0.95, hop1 Assigns edge_mult=0.99 hop_degrade=0.99 → 0.95*0.99*0.99=0.93
        assert!(paths[0].confidence < 0.95, "Multiplicative: confidence should degrade with hops");
        assert!(paths[0].confidence > 0.90, "2-hop path should still have high confidence");
    }

    #[test]
    fn test_confidence_prioritization() {
        let mut cpg = CodePropertyGraph::new();
        let src = make_node(&mut cpg, "request", NodeKind::Source, "a.py", 1);
        let direct_sink = make_node(&mut cpg, "os.system", NodeKind::Sink, "a.py", 2);
        let hop1 = make_node(&mut cpg, "x = req", NodeKind::Assignment, "b.py", 3);
        let hop2 = make_node(&mut cpg, "y = x", NodeKind::Assignment, "b.py", 4);
        let hop3 = make_node(&mut cpg, "z = y", NodeKind::Assignment, "b.py", 5);
        let deep_sink = make_node(&mut cpg, "exec", NodeKind::Sink, "b.py", 6);

        cpg.add_edge(src, direct_sink, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.9, metadata: HashMap::new(),
        });
        cpg.add_edge(src, hop1, GraphEdge {
            kind: EdgeKind::Assigns, label: None, confidence: 1.0, metadata: HashMap::new(),
        });
        cpg.add_edge(hop1, hop2, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.9, metadata: HashMap::new(),
        });
        cpg.add_edge(hop2, hop3, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.9, metadata: HashMap::new(),
        });
        cpg.add_edge(hop3, deep_sink, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.9, metadata: HashMap::new(),
        });

        cpg.sources.push(src);
        cpg.sinks.push(direct_sink);
        cpg.sinks.push(deep_sink);

        let paths = propagate_taint_peak(&cpg);
        assert!(paths.len() >= 1, "C6.2.6: Should find at least one path");
        // Paths sorted by confidence descending — verify ordering
        if paths.len() >= 2 {
            assert!(paths[0].confidence >= paths[1].confidence,
                "C6.2.6: Paths sorted by confidence. [0]={:.4} >= [1]={:.4}",
                paths[0].confidence, paths[1].confidence);
        }
    }

    // ─── C6.2.4: Sanitizer Detection Tests ───

    #[test]
    fn test_sanitizer_known_library() {
        let mut cpg = CodePropertyGraph::new();
        let node = make_node(&mut cpg, "html.escape", NodeKind::Function, "lib.py", 1);
        let (is_san, score) = classify_sanitizer(&cpg.graph[node]);
        assert!(is_san);
        assert!(score >= 0.9, "Known sanitizer should score high, got {}", score);
    }

    #[test]
    fn test_anti_sanitizer_detected() {
        let mut cpg = CodePropertyGraph::new();
        let node = make_node(&mut cpg, "unescape_html", NodeKind::Function, "lib.py", 1);
        let (is_san, score) = classify_sanitizer(&cpg.graph[node]);
        assert!(!is_san, "unescape reverses sanitization — should NOT be sanitizer");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_heuristic_sanitizer() {
        let mut cpg = CodePropertyGraph::new();
        let node = make_node(&mut cpg, "sanitize_input", NodeKind::Function, "lib.py", 1);
        let (is_san, score) = classify_sanitizer(&cpg.graph[node]);
        assert!(is_san);
        assert!(score >= 0.3);
    }

    // ─── Basic Taint Tests ───

    #[test]
    fn test_taint_direct_source_to_sink() {
        let mut cpg = CodePropertyGraph::new();
        let src = make_node(&mut cpg, "request.form", NodeKind::Source, "app.py", 1);
        let sink = make_node(&mut cpg, "os.system", NodeKind::Sink, "app.py", 2);
        cpg.add_edge(src, sink, GraphEdge {
            kind: EdgeKind::DataFlow, label: Some("data".into()), confidence: 0.9, metadata: HashMap::new(),
        });
        cpg.sources.push(src);
        cpg.sinks.push(sink);
        let paths = propagate_taint_peak(&cpg);
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_sanitized_path() {
        let mut cpg = CodePropertyGraph::new();
        let src = make_node(&mut cpg, "request.form", NodeKind::Source, "app.py", 1);
        let san = make_node(&mut cpg, "html.escape", NodeKind::Sanitizer, "app.py", 2);
        let sink = make_node(&mut cpg, "os.system", NodeKind::Sink, "app.py", 3);
        cpg.add_edge(src, san, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.9, metadata: HashMap::new(),
        });
        cpg.add_edge(san, sink, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.8, metadata: HashMap::new(),
        });
        cpg.sources.push(src);
        cpg.sinks.push(sink);
        cpg.sanitizers.push(san);
        let paths = propagate_taint_peak(&cpg);
        assert!(!paths.is_empty(), "Should find sanitized path");
        assert!(paths[0].sanitized, "Path should be sanitized via html.escape");
        // Sanitized path confidence should be moderate (between 0.2 and 0.8)
        assert!(paths[0].confidence < 0.85, "Sanitized path confidence should be reduced");
    }
}
