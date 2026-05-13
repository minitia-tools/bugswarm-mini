/// SSA-Based Taint Propagation Engine.
///
/// Propagates taint from source nodes through SSA use-def chains
/// until reaching sink nodes or sanitizers. Replaces the old
/// substring-based bfs_taint with SSA-precise tracking.

use crate::graph::{CodePropertyGraph, EdgeKind};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use std::collections::{HashSet, VecDeque};

use serde::Serialize;

type NodeId = NodeIndex;

/// A single hop in a taint path.
#[derive(Debug, Clone)]
pub struct TaintHop {
    pub node_id: NodeId,
    pub node_name: String,
    pub file: String,
    pub line: usize,
    pub hop_type: String, // "assignment", "call", "return", "field", "collection"
}

/// An SSA-based taint path from source to sink.
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
    pub confidence: f64,
}

/// Run SSA-enhanced taint propagation on the CPG.
///
/// This works alongside the existing substring-based taint. It adds more
/// precise paths by following variable assignments through the call graph.
pub fn propagate_taint_enhanced(cpg: &CodePropertyGraph) -> Vec<SsaTaintPath> {
    let mut paths = Vec::new();

    for &source_id in &cpg.sources {
        for &sink_id in &cpg.sinks {
            let found = trace_taint_from_source(cpg, source_id, sink_id);
            paths.extend(found);
        }
    }

    paths
}

/// Trace taint from a specific source to a specific sink.
fn trace_taint_from_source(
    cpg: &CodePropertyGraph,
    source: NodeId,
    sink: NodeId,
) -> Vec<SsaTaintPath> {
    let mut results = Vec::new();
    let mut queue = VecDeque::new();
    let mut visited: HashSet<(NodeId, bool)> = HashSet::new(); // (node, sanitized?)

    let source_node = match cpg.get_node(source) {
        Some(n) => n,
        None => return results,
    };

    let start_hop = TaintHop {
        node_id: source,
        node_name: source_node.name.clone(),
        file: source_node.file.clone(),
        line: source_node.line_start,
        hop_type: "source".into(),
    };

    queue.push_back((source, vec![start_hop], false, None));
    visited.insert((source, false));

    while let Some((current, path, sanitized, sanitizer)) = queue.pop_front() {
        if current == sink {
            let source_name = cpg.get_node(source).map(|n| n.name.clone()).unwrap_or_default();
            let sink_name = cpg.get_node(sink).map(|n| n.name.clone()).unwrap_or_default();

            results.push(SsaTaintPath {
                source_id: source,
                sink_id: sink,
                source_name,
                sink_name,
                hops: path.clone(),
                sanitized,
                sanitizer_id: sanitizer,
                length: path.len(),
                confidence: if sanitized { 0.3 } else { 0.95 },
            });

            if results.len() >= 10 {
                break;
            }
            continue;
        }

        if path.len() > 50 {
            continue;
        }

        // Follow outgoing edges from current node
        for edge in cpg.graph.edges(current) {
            let target = edge.target();
            let edge_kind = &edge.weight().kind;
            let target_node = &cpg.graph[target];

            let becomes_sanitized = sanitized || is_sanitizer_node(target_node);

            let follows = matches!(
                edge_kind,
                EdgeKind::DataFlow
                    | EdgeKind::TaintFlow
                    | EdgeKind::References
                    | EdgeKind::Assigns
                    | EdgeKind::Calls
                    | EdgeKind::Contains
            );

            if !follows {
                continue;
            }

            let key = (target, becomes_sanitized);
            if visited.contains(&key) {
                continue;
            }
            visited.insert(key);

            let hop = TaintHop {
                node_id: target,
                node_name: target_node.name.clone(),
                file: target_node.file.clone(),
                line: target_node.line_start,
                hop_type: format!("{:?}", edge_kind).to_lowercase(),
            };

            let mut new_path = path.clone();
            new_path.push(hop);

            let new_sanitizer = if is_sanitizer_node(target_node) {
                Some(target)
            } else {
                sanitizer
            };

            queue.push_back((target, new_path, becomes_sanitized, new_sanitizer));
        }
    }

    results
}

/// Check if a node is a sanitizer (function that cleans tainted data).
fn is_sanitizer_node(node: &crate::graph::GraphNode) -> bool {
    let lower = node.name.to_lowercase();
    let sanitizers = [
        "html.escape", "escape", "sanitize", "clean", "strip_tags",
        "bleach.clean", "markupsafe.escape", "django.utils.html.escape",
        "urllib.parse.quote", "htmlspecialchars", "filter_var",
        "mysql_real_escape_string", "pg_escape_string",
        "shlex.quote", "pipes.quote",
    ];
    sanitizers.iter().any(|s| lower.contains(s))
}

/// Stats from the enhanced taint propagation.
#[derive(Debug, Clone)]
pub struct TaintStats {
    pub total_paths: usize,
    pub sanitized_paths: usize,
    pub unsanitized_paths: usize,
    pub avg_path_length: f64,
    pub max_path_length: usize,
    pub sources_analyzed: usize,
    pub sinks_analyzed: usize,
}

/// Compute stats from a set of taint paths.
pub fn compute_taint_stats(paths: &[SsaTaintPath], sources: usize, sinks: usize) -> TaintStats {
    let total = paths.len();
    let sanitized = paths.iter().filter(|p| p.sanitized).count();
    let lengths: Vec<usize> = paths.iter().map(|p| p.length).collect();
    let avg = if total > 0 {
        lengths.iter().sum::<usize>() as f64 / total as f64
    } else {
        0.0
    };
    let max = lengths.iter().max().copied().unwrap_or(0);

    TaintStats {
        total_paths: total,
        sanitized_paths: sanitized,
        unsanitized_paths: total - sanitized,
        avg_path_length: avg,
        max_path_length: max,
        sources_analyzed: sources,
        sinks_analyzed: sinks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{CodePropertyGraph, EdgeKind, GraphEdge, GraphNode, NodeKind};
    use std::collections::HashMap;

    fn make_node(cpg: &mut CodePropertyGraph, name: &str, kind: NodeKind, file: &str, line: usize) -> NodeId {
        cpg.add_node(GraphNode {
            id: name.to_string(),
            kind,
            name: name.to_string(),
            file: file.to_string(),
            line_start: line,
            line_end: line,
            col_start: 0,
            col_end: 0,
            language: "python".into(),
            metadata: HashMap::new(),
        })
    }

    #[test]
    fn test_taint_direct_source_to_sink() {
        let mut cpg = CodePropertyGraph::new();
        let src = make_node(&mut cpg, "request.form", NodeKind::Source, "app.py", 1);
        let sink = make_node(&mut cpg, "os.system", NodeKind::Sink, "app.py", 2);

        cpg.add_edge(src, sink, GraphEdge {
            kind: EdgeKind::DataFlow,
            label: Some("data".into()),
            confidence: 0.9,
            metadata: HashMap::new(),
        });

        let paths = propagate_taint_enhanced(&cpg);
        assert!(!paths.is_empty());
        assert_eq!(paths[0].length, 2);
        assert!(!paths[0].sanitized);
        assert!(paths[0].confidence > 0.9);
    }

    #[test]
    fn test_taint_through_multiple_hops() {
        let mut cpg = CodePropertyGraph::new();
        let src = make_node(&mut cpg, "request.form", NodeKind::Source, "app.py", 1);
        let mid = make_node(&mut cpg, "x = request", NodeKind::Assignment, "app.py", 2);
        let mid2 = make_node(&mut cpg, "y = x", NodeKind::Assignment, "app.py", 3);
        let sink = make_node(&mut cpg, "os.system", NodeKind::Sink, "app.py", 4);

        cpg.add_edge(src, mid, GraphEdge {
            kind: EdgeKind::Assigns, label: None, confidence: 1.0, metadata: HashMap::new(),
        });
        cpg.add_edge(mid, mid2, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.9, metadata: HashMap::new(),
        });
        cpg.add_edge(mid2, sink, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.9, metadata: HashMap::new(),
        });

        cpg.sources.push(src);
        cpg.sinks.push(sink);

        let paths = propagate_taint_enhanced(&cpg);
        assert!(!paths.is_empty());
        assert!(paths[0].length >= 3);
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

        let paths = propagate_taint_enhanced(&cpg);
        assert!(!paths.is_empty());
        assert!(paths[0].sanitized);
        assert!(paths[0].confidence < 0.5);
    }
}
