#[cfg(test)]
mod tests {
    use bugswarm_cpg::graph::{CodePropertyGraph, EdgeKind, GraphEdge, GraphNode, NodeKind};
    use std::collections::HashMap;

    fn make_node(name: &str, kind: NodeKind, file: &str, line: usize) -> GraphNode {
        GraphNode {
            id: format!("node:{}", name),
            kind,
            name: name.to_string(),
            file: file.to_string(),
            line_start: line, line_end: line,
            col_start: 0, col_end: 0,
            language: "python".into(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_taint_path_direct() {
        let mut cpg = CodePropertyGraph::new();

        let src = cpg.add_node(make_node("request.body", NodeKind::Source, "app.py", 5));
        let sink = cpg.add_node(make_node("os.system", NodeKind::Sink, "app.py", 10));

        cpg.add_edge(src, sink, GraphEdge {
            kind: EdgeKind::DataFlow,
            label: Some("data".into()),
            confidence: 0.9,
            metadata: HashMap::new(),
        });

        let paths = cpg.find_taint_paths();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].length, 2);
        assert!(!paths[0].sanitized);
    }

    #[test]
    fn test_taint_path_sanitized() {
        let mut cpg = CodePropertyGraph::new();

        let src = cpg.add_node(make_node("request.body", NodeKind::Source, "app.py", 5));
        let sanitizer = cpg.add_node(make_node("html.escape", NodeKind::Sanitizer, "app.py", 6));
        let sink = cpg.add_node(make_node("db.execute", NodeKind::Sink, "app.py", 7));

        cpg.add_edge(src, sanitizer, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.9, metadata: HashMap::new(),
        });
        cpg.add_edge(sanitizer, sink, GraphEdge {
            kind: EdgeKind::Sanitized, label: None, confidence: 0.8, metadata: HashMap::new(),
        });

        let paths = cpg.find_taint_paths();
        assert!(paths.len() >= 1);
        assert!(paths[0].sanitized);
    }

    #[test]
    fn test_call_path() {
        let mut cpg = CodePropertyGraph::new();

        let a = cpg.add_node(make_node("app.py:main", NodeKind::Function, "app.py", 1));
        let b = cpg.add_node(make_node("app.py:helper", NodeKind::Function, "app.py", 10));
        let c = cpg.add_node(make_node("app.py:sink", NodeKind::Function, "app.py", 20));

        cpg.function_index.insert("app.py:main".into(), a);
        cpg.function_index.insert("app.py:helper".into(), b);
        cpg.function_index.insert("app.py:sink".into(), c);

        cpg.add_edge(a, b, GraphEdge {
            kind: EdgeKind::Calls, label: None, confidence: 1.0, metadata: HashMap::new(),
        });
        cpg.add_edge(b, c, GraphEdge {
            kind: EdgeKind::Calls, label: None, confidence: 1.0, metadata: HashMap::new(),
        });

        let paths = cpg.find_call_paths("app.py:main", "app.py:sink");
        assert!(!paths.is_empty());
        assert_eq!(paths[0].length, 3); // main → helper → sink
    }

    #[test]
    fn test_prune_unreachable() {
        let mut cpg = CodePropertyGraph::new();

        let source = cpg.add_node(make_node("request", NodeKind::Source, "a.py", 1));
        let _dangling = cpg.add_node(make_node("unused", NodeKind::Function, "a.py", 5));
        let sink = cpg.add_node(make_node("exec", NodeKind::Sink, "a.py", 10));

        cpg.add_edge(source, sink, GraphEdge {
            kind: EdgeKind::DataFlow, label: None, confidence: 0.9, metadata: HashMap::new(),
        });

        let before = cpg.graph.node_count();
        cpg.prune_unreachable();
        let after = cpg.graph.node_count();

        assert!(after < before, "Pruning should remove unreachable nodes");
        assert!(cpg.graph.node_count() >= 2, "Source and sink should be preserved");
    }

    #[test]
    fn test_stats() {
        let mut cpg = CodePropertyGraph::new();
        cpg.add_node(make_node("a", NodeKind::Source, "f.py", 1));
        cpg.add_node(make_node("b", NodeKind::Sink, "f.py", 2));
        cpg.files.insert("f.py".into(), bugswarm_cpg::graph::FileInfo {
            path: "f.py".into(), language: "python".into(),
            lines: 10, functions: vec![], imports: vec![],
        });

        let stats = cpg.stats();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.sources, 1);
        assert_eq!(stats.sinks, 1);
        assert_eq!(stats.total_files, 1);
    }
}
