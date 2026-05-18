use std::collections::HashMap;
use std::path::Path;
use tree_sitter::{Language, Node, Parser};
use tracing::{debug, info, warn};
use walkdir::WalkDir;
use once_cell::sync::Lazy;

use crate::graph::{
    CodePropertyGraph, EdgeKind, FunctionInfo, GraphEdge, GraphNode, NodeId, NodeKind,
};
use petgraph::visit::EdgeRef;

static PYTHON_IMPORT_RE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?:import|from)\s+(\S+)").expect("valid regex"));
static JS_IMPORT_RE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r#"(?:import|require)\s*\(?["']([^"']+)["']"#).expect("valid regex"));

/// Detect the language of a file based on its extension.
pub fn detect_language(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "py" => Some("python"),
        "js" => Some("javascript"),
        "mjs" => Some("javascript"),
        "cjs" => Some("javascript"),
        "ts" => Some("typescript"),
        "tsx" => Some("typescript"),
        "jsx" => Some("javascript"),
        "go" => Some("go"),
        "java" => Some("java"),
        "rb" => Some("ruby"),
        "rs" => Some("rust"),
        "c" => Some("c"),
        "h" => Some("c"),
        "cpp" => Some("cpp"),
        "cc" => Some("cpp"),
        "cxx" => Some("cpp"),
        "hpp" => Some("cpp"),
        _ => None,
    }
}

/// Get the tree-sitter language for a given language name.
pub fn get_language(name: &str) -> Option<Language> {
    match name {
        "python" => Some(tree_sitter_python::LANGUAGE.into()),
        "javascript" | "typescript" => Some(tree_sitter_javascript::LANGUAGE.into()),
        _ => None,
    }
}

/// Parse a single file and add its contents to the CPG.
pub fn parse_file(cpg: &mut CodePropertyGraph, path: &Path) -> anyhow::Result<()> {
    let language_name = match detect_language(path) {
        Some(l) => l,
        None => {
            debug!("Skipping file with unknown extension: {:?}", path);
            return Ok(());
        }
    };

    let language = match get_language(language_name) {
        Some(l) => l,
        None => {
            debug!("No parser for language: {}", language_name);
            return Ok(());
        }
    };

    let content = std::fs::read_to_string(path)?;
    let file_path = path.to_string_lossy().to_string();

    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser.parse(&content, None).ok_or_else(|| {
        anyhow::anyhow!("Failed to parse: {}", file_path)
    })?;

    let root = tree.root_node();

    // Add file node
    let _file_node = if language_name == "python" {
        parse_python_file(cpg, &file_path, &content, root, language_name)
    } else {
        parse_javascript_file(cpg, &file_path, &content, root, language_name)
    };

    // Track file info
    let lines = content.lines().count();
    let funcs: Vec<String> = cpg.functions
        .iter()
        .filter(|(_, f)| f.file == file_path)
        .map(|(_, f)| f.name.clone())
        .collect();

    let imports: Vec<String> = extract_imports(&content, language_name);

    cpg.files.insert(file_path.clone(), crate::graph::FileInfo {
        path: file_path.clone(),
        language: language_name.to_string(),
        lines,
        functions: funcs,
        imports,
    });

    info!("Parsed {} ({} lines, {} nodes)", cpg.files[&file_path].path, lines, cpg.graph.node_count());

    Ok(())
}

/// C6.2.5: Two-pass deferred cross-file resolution.
///
/// Pass 1: Parse all files, collect function signatures.
/// Pass 2: Resolve cross-file call edges using collected signatures.
pub fn index_directory(cpg: &mut CodePropertyGraph, root: &Path) -> anyhow::Result<()> {
    // Pass 1: Parse all files, collect function names
    info!("Pass 1: Parsing files...");
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Err(e) = parse_file(cpg, path) {
                warn!("Failed to parse {}: {}", path.display(), e);
            }
        }
    }

    // Pass 2: Resolve cross-file call edges
    // Collect all function names across all files
    let all_functions: Vec<(String, NodeId)> = cpg.function_index.iter()
        .map(|(name, id)| (name.clone(), *id))
        .collect();

    let resolved_count = resolve_cross_file_calls(cpg, &all_functions);
    info!("Pass 2: Resolved {} cross-file call edges", resolved_count);

    Ok(())
}

/// C6.2.5: Resolve cross-file calls using the global function registry.
fn resolve_cross_file_calls(
    cpg: &mut CodePropertyGraph,
    all_functions: &[(String, NodeId)],
) -> usize {
    let mut resolved = 0;

    // Collect all external-stub call edges (confidence < 0.9)
    let stub_targets: Vec<(NodeId, String)> = {
        let mut targets = Vec::new();
        for edge_ref in cpg.graph.edge_references() {
            let ew = edge_ref.weight();
            if ew.kind == EdgeKind::Calls && ew.confidence < 0.9 {
                let target_id = edge_ref.target();
                if let Some(node) = cpg.get_node(target_id) {
                    targets.push((target_id, node.name.clone()));
                }
            }
        }
        targets
    };

    // For each stub, try to resolve against global function registry
    for (stub_id, name) in &stub_targets {
        let found = all_functions.iter().find(|(fname, _)| {
            fname == name || fname.ends_with(&format!(":{}", name))
        });

        if let Some((_, _real_id)) = found {
            // Mark the stub node as resolved via metadata
            if let Some(node) = cpg.graph.node_weight_mut(*stub_id) {
                node.metadata.insert("resolved".into(), "true".into());
                resolved += 1;
            }
        }
    }

    info!("Cross-file resolution: {}/{} stubs resolved", resolved, stub_targets.len());
    resolved
}

// ─── Python Parser ───

fn parse_python_file(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    root: Node,
    lang: &str,
) {
    let file_id = cpg.add_node(GraphNode {
        id: format!("file:{}", file),
        kind: NodeKind::File,
        name: file.to_string(),
        file: file.to_string(),
        line_start: 0, line_end: 0,
        col_start: 0, col_end: 0,
        language: lang.to_string(),
        metadata: HashMap::new(),
    });

    let mut cursor = root.walk();
    let _stack = vec![(root, file_id)];

    // Walk the AST
    for child in root.children(&mut cursor) {
        walk_python_node(cpg, file, content, child, file_id, lang);
    }
}

fn walk_python_node(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let kind = node.kind();

    match kind {
        "function_definition" => handle_python_function(cpg, file, content, node, parent, lang),
        "class_definition" => handle_python_class(cpg, file, content, node, parent, lang),
        "call" => handle_python_call(cpg, file, content, node, parent, lang),
        "import_statement" | "import_from_statement" => handle_python_import(cpg, file, content, node, parent, lang),
        "assignment" => handle_python_assignment(cpg, file, content, node, parent, lang),
        "return_statement" => handle_python_return(cpg, file, content, node, parent, lang),
        "decorated_definition" => {
            for child in node.children(&mut node.walk()) {
                walk_python_node(cpg, file, content, child, parent, lang);
            }
        }
        "expression_statement" => {
            // Unwrap expression statements to find calls
            for child in node.children(&mut node.walk()) {
                walk_python_node(cpg, file, content, child, parent, lang);
            }
        }
        _ => {
            // Recurse into children
            for child in node.children(&mut node.walk()) {
                walk_python_node(cpg, file, content, child, parent, lang);
            }
        }
    }
}

fn handle_python_function(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let name = get_child_text(node, "name", content).unwrap_or_else(|| "anonymous".into());
    let start = node.start_position();
    let end = node.end_position();

    // Collect parameters
    let params = get_python_params(node, content);

    // Collect function calls within the body
    let mut calls = Vec::new();
    collect_python_calls(node, content, &mut calls);

    // Detect async
    let is_async = node.child(0).map_or(false, |c| c.kind() == "async");

    let func_id = cpg.add_node(GraphNode {
        id: format!("func:{}:{}", file, name),
        kind: NodeKind::Function,
        name: format!("{}:{}", file, name),
        file: file.to_string(),
        line_start: start.row + 1,
        line_end: end.row + 1,
        col_start: start.column,
        col_end: end.column,
        language: lang.to_string(),
        metadata: HashMap::new(),
    });

    cpg.add_edge(parent, func_id, GraphEdge {
        kind: EdgeKind::Contains,
        label: None,
        confidence: 1.0,
        metadata: HashMap::new(),
    });

    cpg.functions.insert(name.clone(), FunctionInfo {
        name: name.clone(),
        file: file.to_string(),
        line_start: start.row + 1,
        line_end: end.row + 1,
        params,
        return_type: None,
        calls: calls.clone(),
        called_by: Vec::new(),
        is_async,
        is_exported: !name.starts_with('_'),
    });

    // Add call edges
    for call in &calls {
        let call_id = cpg.find_function(call);
        if let Some(target) = call_id {
            cpg.add_edge(func_id, target, GraphEdge {
                kind: EdgeKind::Calls,
                label: Some(call.clone()),
                confidence: 0.95,
                metadata: HashMap::new(),
            });
        } else {
            // External call — create a stub
            let stub = cpg.add_node(GraphNode {
                id: format!("ext:{}", call),
                kind: NodeKind::Function,
                name: call.clone(),
                file: "external".into(),
                line_start: 0, line_end: 0,
                col_start: 0, col_end: 0,
                language: lang.to_string(),
                metadata: HashMap::new(),
            });
            cpg.add_edge(func_id, stub, GraphEdge {
                kind: EdgeKind::Calls,
                label: Some(call.clone()),
                confidence: 0.5,  // Lower confidence for external
                metadata: HashMap::new(),
            });
        }
    }

    // Walk body for more patterns
    if let Some(body) = node.child_by_field_name("body") {
        for child in body.children(&mut body.walk()) {
            walk_python_node(cpg, file, content, child, func_id, lang);
        }
    }
}

fn handle_python_class(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let name = get_child_text(node, "name", content).unwrap_or_else(|| "Anonymous".into());
    let start = node.start_position();
    let end = node.end_position();

    let class_id = cpg.add_node(GraphNode {
        id: format!("class:{}:{}", file, name),
        kind: NodeKind::Class,
        name: format!("{}:{}", file, name),
        file: file.to_string(),
        line_start: start.row + 1,
        line_end: end.row + 1,
        col_start: start.column,
        col_end: end.column,
        language: lang.to_string(),
        metadata: HashMap::new(),
    });

    cpg.add_edge(parent, class_id, GraphEdge {
        kind: EdgeKind::Contains,
        label: None,
        confidence: 1.0,
        metadata: HashMap::new(),
    });

    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for child in body.children(&mut body.walk()) {
            if child.kind() == "function_definition" {
                let mname = get_child_text(child, "name", content).unwrap_or_default();
                methods.push(format!("{}.{}", name, mname));
            }
            walk_python_node(cpg, file, content, child, class_id, lang);
        }
    }

    // Superclass
    let parent_name = get_python_superclass(node, content);

    cpg.classes.insert(name.clone(), crate::graph::ClassInfo {
        name: name.clone(),
        file: file.to_string(),
        line_start: start.row + 1,
        methods,
        parent: parent_name.clone(),
        interfaces: Vec::new(),
    });

    if let Some(sup) = parent_name {
        if let Some(sup_id) = cpg.find_function(&sup) {
            cpg.add_edge(class_id, sup_id, GraphEdge {
                kind: EdgeKind::Inherits,
                label: Some(sup),
                confidence: 0.9,
                metadata: HashMap::new(),
            });
        }
    }
}

fn handle_python_call(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let start = node.start_position();

    // Get function name
    let func_name = if let Some(attr) = node.child_by_field_name("function") {
        get_node_text(attr, content)
    } else {
        get_node_text(node, content)
    };

    let call_id = cpg.add_node(GraphNode {
        id: format!("call:{}:{}:{}", file, func_name, start.row),
        kind: NodeKind::CallSite,
        name: func_name.clone(),
        file: file.to_string(),
        line_start: start.row + 1,
        line_end: start.row + 1,
        col_start: start.column,
        col_end: start.column + func_name.len(),
        language: lang.to_string(),
        metadata: HashMap::new(),
    });

    cpg.add_edge(parent, call_id, GraphEdge {
        kind: EdgeKind::Contains,
        label: None,
        confidence: 1.0,
        metadata: HashMap::new(),
    });

    // Detect taint sinks
    let lower = func_name.to_lowercase();
    if is_known_sink(&lower) {
        if let Some(n) = cpg.graph.node_weight_mut(call_id) {
            n.kind = NodeKind::Sink;
        }
        cpg.sinks.push(call_id);
        cpg.add_edge(call_id, parent, GraphEdge {
            kind: EdgeKind::TaintFlow,
            label: Some(format!("sink: {}", func_name)),
            confidence: 0.9,
            metadata: HashMap::new(),
        });
    }

    // Arguments contain data flow
    if let Some(args) = node.child_by_field_name("arguments") {
        for arg in args.children(&mut args.walk()) {
            let arg_text = get_node_text(arg, content);
            if is_input_source(&arg_text) {
                let src_id = cpg.add_node(GraphNode {
                    id: format!("src:{}:{}:{}", file, arg_text, start.row),
                    kind: NodeKind::Source,
                    name: arg_text.clone(),
                    file: file.to_string(),
                    line_start: start.row + 1,
                    line_end: start.row + 1,
                    col_start: start.column,
                    col_end: start.column + arg_text.len().min(40),
                    language: lang.to_string(),
                    metadata: HashMap::new(),
                });
                cpg.sources.push(src_id);
                cpg.add_edge(src_id, call_id, GraphEdge {
                    kind: EdgeKind::DataFlow,
                    label: Some(arg_text),
                    confidence: 0.85,
                    metadata: HashMap::new(),
                });
            }
        }
    }
}

fn handle_python_import(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let start = node.start_position();
    let text = get_node_text(node, content);

    let import_id = cpg.add_node(GraphNode {
        id: format!("import:{}:{}", file, start.row),
        kind: NodeKind::Import,
        name: text.clone(),
        file: file.to_string(),
        line_start: start.row + 1,
        line_end: start.row + 1,
        col_start: start.column,
        col_end: start.column + text.len(),
        language: lang.to_string(),
        metadata: HashMap::new(),
    });

    cpg.add_edge(parent, import_id, GraphEdge {
        kind: EdgeKind::Imports,
        label: Some(text),
        confidence: 1.0,
        metadata: HashMap::new(),
    });
}

fn handle_python_assignment(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let start = node.start_position();
    let text = get_node_text(node, content);
    let tlen = text.len().min(60);
    let text_clone = text.clone();

    let assign_id = cpg.add_node(GraphNode {
        id: format!("assign:{}:{}", file, start.row),
        kind: NodeKind::Assignment,
        name: text,
        file: file.to_string(),
        line_start: start.row + 1,
        line_end: start.row + 1,
        col_start: start.column,
        col_end: start.column + tlen,
        language: lang.to_string(),
        metadata: HashMap::new(),
    });

    cpg.add_edge(parent, assign_id, GraphEdge {
        kind: EdgeKind::Assigns,
        label: Some(text_clone),
        confidence: 1.0,
        metadata: HashMap::new(),
    });

    // Check if right-hand side is an input source
    if let Some(rhs) = node.child_by_field_name("right") {
        let rhs_text = get_node_text(rhs, content);
        if is_input_source(&rhs_text) {
            let src_id = cpg.add_node(GraphNode {
                id: format!("src:{}:{}:{}", file, rhs_text, start.row),
                kind: NodeKind::Source,
                name: rhs_text.clone(),
                file: file.to_string(),
                line_start: start.row + 1,
                line_end: start.row + 1,
                col_start: start.column,
                col_end: start.column + rhs_text.len().min(40),
                language: lang.to_string(),
                metadata: HashMap::new(),
            });
            cpg.sources.push(src_id);
            cpg.add_edge(src_id, assign_id, GraphEdge {
                kind: EdgeKind::DataFlow,
                label: Some(rhs_text),
                confidence: 0.85,
                metadata: HashMap::new(),
            });
        }
    }
}

fn handle_python_return(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let start = node.start_position();
    let text = get_node_text(node, content);
    let tlen = text.len().min(40);

    let ret_id = cpg.add_node(GraphNode {
        id: format!("return:{}:{}", file, start.row),
        kind: NodeKind::Return,
        name: text,
        file: file.to_string(),
        line_start: start.row + 1,
        line_end: start.row + 1,
        col_start: start.column,
        col_end: start.column + tlen,
        language: lang.to_string(),
        metadata: HashMap::new(),
    });

    cpg.add_edge(parent, ret_id, GraphEdge {
        kind: EdgeKind::Returns,
        label: None,
        confidence: 1.0,
        metadata: HashMap::new(),
    });
}

// ─── JavaScript Parser ───

fn parse_javascript_file(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    root: Node,
    lang: &str,
) {
    let file_id = cpg.add_node(GraphNode {
        id: format!("file:{}", file),
        kind: NodeKind::File,
        name: file.to_string(),
        file: file.to_string(),
        line_start: 0, line_end: 0,
        col_start: 0, col_end: 0,
        language: lang.to_string(),
        metadata: HashMap::new(),
    });

    for child in root.children(&mut root.walk()) {
        walk_js_node(cpg, file, content, child, file_id, lang);
    }
}

fn walk_js_node(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let kind = node.kind();

    match kind {
        "function_declaration" | "method_definition" | "arrow_function" => {
            handle_js_function(cpg, file, content, node, parent, lang);
        }
        "class_declaration" => {
            handle_js_class(cpg, file, content, node, parent, lang);
        }
        "call_expression" => {
            handle_js_call(cpg, file, content, node, parent, lang);
        }
        "import_statement" | "import" => {
            handle_js_import(cpg, file, content, node, parent, lang);
        }
        "variable_declaration" | "lexical_declaration" => {
            handle_js_variable(cpg, file, content, node, parent, lang);
        }
        _ => {
            for child in node.children(&mut node.walk()) {
                walk_js_node(cpg, file, content, child, parent, lang);
            }
        }
    }
}

fn handle_js_function(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let name = get_child_text(node, "name", content).unwrap_or_else(|| "anonymous".into());
    let start = node.start_position();
    let end = node.end_position();

    let mut calls = Vec::new();
    collect_js_calls(node, content, &mut calls);

    let is_async = node.child(0).map_or(false, |c| c.kind() == "async");

    let func_id = cpg.add_node(GraphNode {
        id: format!("func:{}:{}", file, name),
        kind: NodeKind::Function,
        name: format!("{}:{}", file, name),
        file: file.to_string(),
        line_start: start.row + 1,
        line_end: end.row + 1,
        col_start: start.column,
        col_end: end.column,
        language: lang.to_string(),
        metadata: HashMap::new(),
    });

    cpg.add_edge(parent, func_id, GraphEdge {
        kind: EdgeKind::Contains, label: None, confidence: 1.0, metadata: HashMap::new(),
    });

    let params = get_js_params(node, content);
    cpg.functions.insert(name.clone(), FunctionInfo {
        name: name.clone(), file: file.to_string(),
        line_start: start.row + 1, line_end: end.row + 1,
        params, return_type: None,
        calls: calls.clone(), called_by: Vec::new(),
        is_async, is_exported: !name.starts_with('_'),
    });

    for call in &calls {
        // Check if called function exists in the same project first
        let existing = cpg.find_function(call);
        if let Some(target) = existing {
            cpg.add_edge(func_id, target, GraphEdge {
                kind: EdgeKind::Calls, label: Some(call.clone()),
                confidence: 0.9, metadata: HashMap::new(),
            });
        } else {
            // Create external stub with lower confidence
            let stub = cpg.add_node(GraphNode {
                id: format!("ext:{}", call),
                kind: NodeKind::Function, name: call.clone(),
                file: "external".into(),
                line_start: 0, line_end: 0, col_start: 0, col_end: 0,
                language: lang.to_string(), metadata: HashMap::new(),
            });
            cpg.add_edge(func_id, stub, GraphEdge {
                kind: EdgeKind::Calls, label: Some(call.clone()), confidence: 0.5, metadata: HashMap::new(),
            });
        }
    }

    if let Some(body) = node.child_by_field_name("body") {
        for child in body.children(&mut body.walk()) {
            walk_js_node(cpg, file, content, child, func_id, lang);
        }
    }
}

fn handle_js_class(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let name = get_child_text(node, "name", content).unwrap_or_else(|| "Anonymous".into());
    let start = node.start_position();
    let end = node.end_position();
    let mut methods = Vec::new();

    let class_id = cpg.add_node(GraphNode {
        id: format!("class:{}:{}", file, name),
        kind: NodeKind::Class,
        name: format!("{}:{}", file, name),
        file: file.to_string(),
        line_start: start.row + 1, line_end: end.row + 1,
        col_start: start.column, col_end: end.column,
        language: lang.to_string(), metadata: HashMap::new(),
    });

    cpg.add_edge(parent, class_id, GraphEdge {
        kind: EdgeKind::Contains, label: None, confidence: 1.0, metadata: HashMap::new(),
    });

    if let Some(body) = node.child_by_field_name("body") {
        for child in body.children(&mut body.walk()) {
            if child.kind() == "method_definition" {
                let mname = get_child_text(child, "name", content).unwrap_or_default();
                methods.push(format!("{}.{}", name, mname));
            }
            walk_js_node(cpg, file, content, child, class_id, lang);
        }
    }

    cpg.classes.insert(name.clone(), crate::graph::ClassInfo {
        name, file: file.to_string(),
        line_start: start.row + 1, methods,
        parent: None, interfaces: Vec::new(),
    });
}

fn handle_js_call(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let start = node.start_position();
    let func_name = if let Some(fn_node) = node.child_by_field_name("function") {
        get_node_text(fn_node, content)
    } else {
        get_node_text(node, content).chars().take(50).collect()
    };

    let call_id = cpg.add_node(GraphNode {
        id: format!("call:{}:{}:{}", file, func_name, start.row),
        kind: NodeKind::CallSite,
        name: func_name.clone(),
        file: file.to_string(),
        line_start: start.row + 1, line_end: start.row + 1,
        col_start: start.column, col_end: start.column + func_name.len(),
        language: lang.to_string(), metadata: HashMap::new(),
    });

    cpg.add_edge(parent, call_id, GraphEdge {
        kind: EdgeKind::Contains, label: None, confidence: 1.0, metadata: HashMap::new(),
    });

    let lower = func_name.to_lowercase();
    if is_known_sink(&lower) {
        if let Some(n) = cpg.graph.node_weight_mut(call_id) {
            n.kind = NodeKind::Sink;
        }
        cpg.sinks.push(call_id);
        cpg.add_edge(call_id, parent, GraphEdge {
            kind: EdgeKind::TaintFlow,
            label: Some(format!("sink: {}", func_name)),
            confidence: 0.9, metadata: HashMap::new(),
        });
    }
}

fn handle_js_import(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let start = node.start_position();
    let text = get_node_text(node, content);
    let import_id = cpg.add_node(GraphNode {
        id: format!("import:{}:{}", file, start.row),
        kind: NodeKind::Import, name: text.clone(),
        file: file.to_string(),
        line_start: start.row + 1, line_end: start.row + 1,
        col_start: start.column, col_end: start.column + text.len().min(60),
        language: lang.to_string(), metadata: HashMap::new(),
    });
    cpg.add_edge(parent, import_id, GraphEdge {
        kind: EdgeKind::Imports, label: Some(text), confidence: 1.0, metadata: HashMap::new(),
    });
}

fn handle_js_variable(
    cpg: &mut CodePropertyGraph,
    file: &str,
    content: &str,
    node: Node,
    parent: crate::graph::NodeId,
    lang: &str,
) {
    let start = node.start_position();
    let text = get_node_text(node, content);
    let tlen = text.len().min(60);
    let assign_id = cpg.add_node(GraphNode {
        id: format!("assign:{}:{}", file, start.row),
        kind: NodeKind::Assignment, name: text.clone(),
        file: file.to_string(),
        line_start: start.row + 1, line_end: start.row + 1,
        col_start: start.column, col_end: start.column + tlen,
        language: lang.to_string(), metadata: HashMap::new(),
    });
    cpg.add_edge(parent, assign_id, GraphEdge {
        kind: EdgeKind::Assigns, label: Some(text), confidence: 1.0, metadata: HashMap::new(),
    });
}

// ─── Helpers ───

fn get_child_text(node: Node, field: &str, content: &str) -> Option<String> {
    node.child_by_field_name(field)
        .map(|c| get_node_text(c, content))
}

fn get_node_text(node: Node, content: &str) -> String {
    node.utf8_text(content.as_bytes())
        .unwrap_or("")
        .to_string()
}

fn get_python_params(node: Node, content: &str) -> Vec<String> {
    if let Some(params) = node.child_by_field_name("parameters") {
        params.children(&mut params.walk())
            .filter(|c| c.kind() == "identifier")
            .map(|c| get_node_text(c, content))
            .collect()
    } else {
        Vec::new()
    }
}

fn get_python_superclass(node: Node, content: &str) -> Option<String> {
    node.children(&mut node.walk())
        .find(|c| c.kind() == "argument_list")
        .and_then(|args| {
            args.children(&mut args.walk())
                .find(|c| c.kind() == "identifier")
                .map(|c| get_node_text(c, content))
        })
}

fn get_js_params(node: Node, content: &str) -> Vec<String> {
    if let Some(params) = node.child_by_field_name("parameters") {
        params.children(&mut params.walk())
            .filter(|c| c.kind() == "identifier" || c.kind() == "property_identifier")
            .map(|c| get_node_text(c, content))
            .collect()
    } else {
        Vec::new()
    }
}

fn collect_python_calls(node: Node, content: &str, calls: &mut Vec<String>) {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "call" {
            if let Some(func) = child.child_by_field_name("function") {
                let name = get_node_text(func, content);
                if !calls.contains(&name) {
                    calls.push(name);
                }
            }
        }
        collect_python_calls(child, content, calls);
    }
}

fn collect_js_calls(node: Node, content: &str, calls: &mut Vec<String>) {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "call_expression" {
            if let Some(func) = child.child_by_field_name("function") {
                let name = get_node_text(func, content);
                if !calls.contains(&name) {
                    calls.push(name);
                }
            }
        }
        collect_js_calls(child, content, calls);
    }
}

fn extract_imports(content: &str, lang: &str) -> Vec<String> {
    match lang {
        "python" => {
            PYTHON_IMPORT_RE.captures_iter(content)
                .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
                .collect()
        }
        "javascript" | "typescript" => {
            JS_IMPORT_RE.captures_iter(content)
                .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Known taint sinks: functions that are dangerous if fed untrusted input.
pub fn is_known_sink(name: &str) -> bool {
    let lower = name.to_lowercase();
    let sinks: &[&str] = &[
        "exec", "eval", "compile", "__import__", "open",
        "system", "popen", "call", "run", "popen",
        "execute", "executemany", "executescript",
        "raw_input", "input",
        "loads", "load", "unsafe_load",
        "write", "writeln",
        "send", "sendfile",
        "render", "innerhtml", "outerhtml", "document.write",
        "settimeout", "setinterval",
        "query", "raw",
    ];
    sinks.iter().any(|s| lower.contains(s))
}

/// Known taint sources: functions/variables that introduce untrusted data.
pub fn is_input_source(text: &str) -> bool {
    let sources = [
        "request", "req", "params", "query", "body", "form",
        "input", "argv", "environ", "getenv", "cookie",
        "header", "headers", "args", "data", "payload",
        "stdin", "read", "recv", "socket",
        "localStorage", "sessionStorage", "document.cookie",
        "window.location", "location", "URL", "searchParams",
    ];
    sources.iter().any(|s| text.contains(s))
}
