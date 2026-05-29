use std::collections::HashMap;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};

/// Unique identifier for a graph node.
pub type NodeId = NodeIndex;

/// The Code Property Graph — a directed graph of code entities and their relationships.
#[derive(Debug, Clone)]
pub struct CodePropertyGraph {
    pub graph: DiGraph<GraphNode, GraphEdge>,
    pub files: HashMap<String, FileInfo>,
    pub functions: HashMap<String, FunctionInfo>,
    pub classes: HashMap<String, ClassInfo>,
    /// Index: function name → NodeId
    pub function_index: HashMap<String, NodeId>,
    /// Index: file path → list of NodeIds in that file
    pub file_index: HashMap<String, Vec<NodeId>>,
    /// Source nodes (taint sources like HTTP params, user input)
    pub sources: Vec<NodeId>,
    /// Sink nodes (dangerous functions like exec, eval, sql execute)
    pub sinks: Vec<NodeId>,
    /// Sanitizer nodes (functions that clean input)
    pub sanitizers: Vec<NodeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeKind {
    File,
    Module,
    Class,
    Function,
    Method,
    Variable,
    Parameter,
    CallSite,
    Return,
    Literal,
    Import,
    Source,      // Taint source (e.g., request.body)
    Sink,        // Taint sink (e.g., os.system, exec)
    Sanitizer,   // Data sanitizer (e.g., html.escape)
    Assignment,
    Condition,
    Loop,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: NodeKind,
    pub name: String,
    pub file: String,
    pub line_start: usize,
    pub line_end: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub language: String,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdgeKind {
    Contains,        // File contains Function, Function contains Variable
    Calls,           // Function A calls Function B
    Inherits,        // Class A inherits Class B
    Imports,         // File imports Module
    DataFlow,        // Data flows from node A to node B
    ControlFlow,     // Control flows from node A to node B
    TaintFlow,       // Tainted data flows from source to sink
    Sanitized,       // Taint is sanitized at this node
    CrossLanguage,   // Cross-language boundary (JS → Python API call)
    References,      // Variable references another
    Assigns,         // Assignment
    Returns,         // Function returns value
    Defines,         // Definition
    Overrides,       // Method override
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub kind: EdgeKind,
    pub label: Option<String>,
    pub confidence: f64,  // 0.0–1.0, 1.0 = static certainty
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub language: String,
    pub lines: usize,
    pub functions: Vec<String>,
    pub imports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub file: String,
    pub line_start: usize,
    pub line_end: usize,
    pub params: Vec<String>,
    pub return_type: Option<String>,
    pub calls: Vec<String>,
    pub called_by: Vec<String>,
    pub is_async: bool,
    pub is_exported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassInfo {
    pub name: String,
    pub file: String,
    pub line_start: usize,
    pub methods: Vec<String>,
    pub parent: Option<String>,
    pub interfaces: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TaintPath {
    pub source: NodeId,
    pub sink: NodeId,
    pub path: Vec<NodeId>,
    pub sanitized: bool,
    pub sanitizer: Option<NodeId>,
    pub length: usize,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallPath {
    pub caller: String,
    pub callee: String,
    pub path: Vec<String>,
    pub length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossLanguageEdge {
    pub source_file: String,
    pub source_function: String,
    pub target_file: String,
    pub target_function: String,
    pub how_detected: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpgStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_files: usize,
    pub total_functions: usize,
    pub total_classes: usize,
    pub sources: usize,
    pub sinks: usize,
    pub sanitizers: usize,
    pub taint_paths: usize,
    pub untracked_calls: usize,
    pub cross_language_edges: usize,
    pub by_language: HashMap<String, LanguageStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageStats {
    pub files: usize,
    pub functions: usize,
    pub classes: usize,
    pub lines: usize,
}

impl CodePropertyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            files: HashMap::new(),
            functions: HashMap::new(),
            classes: HashMap::new(),
            function_index: HashMap::new(),
            file_index: HashMap::new(),
            sources: Vec::new(),
            sinks: Vec::new(),
            sanitizers: Vec::new(),
        }
    }

    /// Add a node and return its index.
    pub fn add_node(&mut self, node: GraphNode) -> NodeId {
        let idx = self.graph.add_node(node.clone());
        self.file_index
            .entry(node.file.clone())
            .or_default()
            .push(idx);
        if node.kind == NodeKind::Function || node.kind == NodeKind::Method {
            self.function_index.insert(node.name.clone(), idx);
        }
        if node.kind == NodeKind::Source {
            self.sources.push(idx);
        }
        if node.kind == NodeKind::Sink {
            self.sinks.push(idx);
        }
        if node.kind == NodeKind::Sanitizer {
            self.sanitizers.push(idx);
        }
        idx
    }

    /// Remove a node.
    pub fn remove_node(&mut self, idx: NodeId) -> Option<GraphNode> {
        self.graph.remove_node(idx)
    }

    /// Add an edge between two nodes.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId, edge: GraphEdge) {
        self.graph.add_edge(from, to, edge);
    }

    /// Get a node by index.
    pub fn get_node(&self, idx: NodeId) -> Option<&GraphNode> {
        self.graph.node_weight(idx)
    }

    /// Find nodes by name and optional kind.
    pub fn find_nodes(&self, name: &str, kind: Option<NodeKind>) -> Vec<NodeId> {
        self.graph
            .node_indices()
            .filter(|idx| {
                let node = &self.graph[*idx];
                let name_match = node.name.contains(name);
                let kind_match = kind.as_ref().is_none_or(|k| node.kind == *k);
                name_match && kind_match
            })
            .collect()
    }

    /// Find a function node by name. Tries exact match, then suffix match.
    pub fn find_function(&self, name: &str) -> Option<NodeId> {
        // Exact match
        if let Some(id) = self.function_index.get(name) {
            return Some(*id);
        }
        // Suffix match: find any function whose name ends with `:name`
        let suffix = format!(":{}", name);
        for (key, id) in &self.function_index {
            if key.ends_with(&suffix) {
                return Some(*id);
            }
        }
        // Also try matching just the function name without file prefix
        for (key, id) in &self.function_index {
            if key.ends_with(name) {
                return Some(*id);
            }
        }
        None
    }

    /// Get all nodes in a file.
    pub fn get_file_nodes(&self, file: &str) -> Vec<NodeId> {
        self.file_index.get(file).cloned().unwrap_or_default()
    }

    /// Find call paths between two functions.
    pub fn find_call_paths(&self, from: &str, to: &str) -> Vec<CallPath> {
        let mut paths = Vec::new();
        let from_id = match self.find_function(from) {
            Some(id) => id,
            None => return paths,
        };
        let to_id = match self.find_function(to) {
            Some(id) => id,
            None => return paths,
        };

        // BFS through CALLS edges
        use std::collections::{HashSet, VecDeque};
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back((from_id, vec![from.to_string()]));
        visited.insert(from_id);

        while let Some((current, path)) = queue.pop_front() {
            if current == to_id {
                paths.push(CallPath {
                    caller: from.to_string(),
                    callee: to.to_string(),
                    path: path.clone(),
                    length: path.len(),
                });
                if paths.len() >= 5 { break; }
                continue;
            }
            if path.len() > 10 { continue; }

            for edge in self.graph.edges(current) {
                if edge.weight().kind == EdgeKind::Calls {
                    let target = edge.target();
                    if !visited.contains(&target) {
                        visited.insert(target);
                        let node = &self.graph[target];
                        let mut new_path = path.clone();
                        new_path.push(node.name.clone());
                        queue.push_back((target, new_path));
                    }
                }
            }
        }
        paths
    }

    /// Find all taint paths from sources to sinks using SSA-based propagation.
    /// C6.2.3 PEAK: Wired to propagate_taint_peak() from Phase 17.
    pub fn find_taint_paths(&self) -> Vec<TaintPath> {
        // Try SSA propagation first
        let ssa_paths = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::taint::propagate_taint_peak(self)
        }));

        match ssa_paths {
            Ok(ssa_paths) if !ssa_paths.is_empty() => {
                // Convert SsaTaintPath → TaintPath
                ssa_paths.into_iter().map(|p| TaintPath {
                    source: p.source_id,
                    sink: p.sink_id,
                    path: p.hops.iter().map(|h| h.node_id).collect(),
                    sanitized: p.sanitized,
                    sanitizer: p.sanitizer_id,
                    length: p.length,
                    confidence: p.confidence,
                }).collect()
            }
            _ => {
                // Fallback to old substring-based bfs_taint
                let mut paths = Vec::new();
                for &source in &self.sources {
                    for &sink in &self.sinks {
                        let found = self.bfs_taint(source, sink);
                        paths.extend(found);
                    }
                }
                paths
            }
        }
    }

    fn bfs_taint(&self, source: NodeId, sink: NodeId) -> Vec<TaintPath> {
        use std::collections::{HashSet, VecDeque};
        let mut results = Vec::new();
        let mut queue = VecDeque::new();
        // Track visited as (node, sanitized) pairs so tainted paths
        // can revisit nodes that were only seen on sanitized paths.
        let mut visited: HashSet<(NodeId, bool)> = HashSet::new();
        queue.push_back((source, vec![source], false, None));
        visited.insert((source, false));

        while let Some((current, path, sanitized, sanitizer)) = queue.pop_front() {
            if current == sink {
                results.push(TaintPath {
                    source, sink,
                    path: path.clone(),
                    sanitized,
                    sanitizer,
                    length: path.len(),
                    confidence: if sanitized { 0.3 } else { 0.95 },
                });
                if results.len() >= 10 { break; }
                continue;
            }
            if path.len() > 20 { continue; }

            for edge in self.graph.edges(current) {
                let target = edge.target();
                let edge_kind = &edge.weight().kind;
                let becomes_sanitized = sanitized || matches!(edge_kind, EdgeKind::Sanitized);

                let follows = matches!(edge_kind,
                    EdgeKind::DataFlow | EdgeKind::TaintFlow | EdgeKind::References | EdgeKind::Assigns | EdgeKind::Sanitized
                );

                if follows {
                    let key = (target, becomes_sanitized);
                    if !visited.contains(&key) {
                        visited.insert(key);
                        let mut new_path = path.clone();
                        new_path.push(target);
                        let new_sanitizer = if edge_kind == &EdgeKind::Sanitized { Some(current) } else { sanitizer };
                        queue.push_back((target, new_path, becomes_sanitized, new_sanitizer));
                    }
                }
            }
        }
        results
    }

    /// Get statistics about the CPG.
    pub fn stats(&self) -> CpgStats {
        let mut by_language = HashMap::new();
        let mut untracked = 0;

        for info in self.files.values() {
            let entry = by_language
                .entry(info.language.clone())
                .or_insert(LanguageStats {
                    files: 0, functions: 0, classes: 0, lines: 0,
                });
            entry.files += 1;
            entry.functions += info.functions.len();
            entry.lines += info.lines;
        }

        for info in self.functions.values() {
            for call in &info.calls {
                if !self.function_index.contains_key(call) {
                    untracked += 1;
                }
            }
        }

        CpgStats {
            total_nodes: self.graph.node_count(),
            total_edges: self.graph.edge_count(),
            total_files: self.files.len(),
            total_functions: self.functions.len(),
            total_classes: self.classes.len(),
            sources: self.sources.len(),
            sinks: self.sinks.len(),
            sanitizers: self.sanitizers.len(),
            taint_paths: self.find_taint_paths().len(),
            untracked_calls: untracked,
            cross_language_edges: 0,
            by_language,
        }
    }

    /// Remove nodes that are unreachable from any source or sink.
    pub fn prune_unreachable(&mut self) {
        // Keep nodes reachable from sources and nodes that reach sinks
        use std::collections::HashSet;
        let mut keep = HashSet::new();

        for &source in &self.sources {
            let reachable = self.reachable_from(source);
            keep.extend(reachable);
        }
        for &sink in &self.sinks {
            let reaching = self.reaching_to(sink);
            keep.extend(reaching);
        }

        let to_remove: Vec<NodeId> = self.graph.node_indices()
            .filter(|idx| !keep.contains(idx))
            .collect();

        for idx in to_remove {
            self.graph.remove_node(idx);
        }
    }

    fn reachable_from(&self, start: NodeId) -> Vec<NodeId> {
        use std::collections::{HashSet, VecDeque};
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        visited.insert(start);

        while let Some(current) = queue.pop_front() {
            result.push(current);
            for edge in self.graph.edges(current) {
                let target = edge.target();
                if !visited.contains(&target) {
                    visited.insert(target);
                    queue.push_back(target);
                }
            }
        }
        result
    }

    fn reaching_to(&self, target: NodeId) -> Vec<NodeId> {
        use std::collections::{HashSet, VecDeque};
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(target);
        visited.insert(target);

        while let Some(current) = queue.pop_front() {
            result.push(current);
            // Walk backwards efficiently using incoming edges
            for edge in self.graph.edges_directed(current, petgraph::Direction::Incoming) {
                let source = edge.source();
                if !visited.contains(&source) {
                    visited.insert(source);
                    queue.push_back(source);
                }
            }
        }
        result
    }
}

impl Default for CodePropertyGraph {
    fn default() -> Self {
        Self::new()
    }
}
