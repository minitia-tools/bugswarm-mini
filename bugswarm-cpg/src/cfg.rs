/// Control Flow Graph — simplified linear blocks for SSA construction.
///
/// C6.2.1: Groups sequential AST nodes into basic blocks.
/// Handles conditions/returns by splitting blocks, but keeps implementation simple.

use serde::Serialize;
use std::collections::HashMap;

use crate::graph::{GraphNode, NodeKind};

/// A basic block in the control flow graph.
#[derive(Debug, Clone, Serialize)]
pub struct BasicBlock {
    pub id: usize,
    pub nodes: Vec<usize>,       // Line numbers of nodes in this block
    pub predecessors: Vec<usize>,
    pub successors: Vec<usize>,
    pub is_entry: bool,
    pub is_exit: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlFlowGraph {
    pub blocks: Vec<BasicBlock>,
    pub entry_block: usize,
    pub exit_block: usize,
    pub node_to_block: HashMap<usize, usize>, // line_number → block_id
}

impl ControlFlowGraph {
    pub fn build(nodes: &[&GraphNode]) -> Self {
        let mut cfg = ControlFlowGraph {
            blocks: Vec::new(),
            entry_block: 0,
            exit_block: 0,
            node_to_block: HashMap::new(),
        };

        if nodes.is_empty() {
            cfg.blocks.push(BasicBlock {
                id: 0, nodes: vec![], predecessors: vec![],
                successors: vec![], is_entry: true, is_exit: true,
            });
            return cfg;
        }

        // Simple approach: one block per function, split at returns
        let mut current_id = 0;
        let mut current = BasicBlock {
            id: current_id, nodes: vec![], predecessors: vec![],
            successors: vec![], is_entry: true, is_exit: false,
        };

        for node in nodes {
            let kind = &node.kind;
            current.nodes.push(node.line_start);
            cfg.node_to_block.insert(node.line_start, current_id);

            if *kind == NodeKind::Return && !current.nodes.is_empty() {
                current.is_exit = true;
                cfg.blocks.push(current);
                cfg.exit_block = current_id;
                current_id += 1;
                current = BasicBlock {
                    id: current_id, nodes: vec![],
                    predecessors: vec![current_id - 1],
                    successors: vec![], is_entry: false, is_exit: false,
                };
            }
        }

        // Push final block
        if !current.nodes.is_empty() {
            if !cfg.blocks.iter().any(|b| b.is_exit) {
                current.is_exit = true;
                cfg.exit_block = current_id;
            }
            cfg.blocks.push(current);
        }

        // Wire successors
        for i in 0..cfg.blocks.len().saturating_sub(1) {
            cfg.blocks[i].successors.push(i + 1);
            cfg.blocks[i + 1].predecessors.push(i);
        }

        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{GraphNode, NodeKind};
    use std::collections::HashMap;

    fn node(name: &str, kind: NodeKind, line: usize) -> GraphNode {
        GraphNode {
            id: line.to_string(), kind, name: name.to_string(), file: "t.py".into(),
            line_start: line, line_end: line, col_start: 0, col_end: 0,
            language: "python".into(), metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_cfg_linear() {
        let n1 = node("x = 1", NodeKind::Assignment, 1);
        let n2 = node("return x", NodeKind::Return, 2);
        let nodes = [&n1, &n2];
        let cfg = ControlFlowGraph::build(&nodes);
        assert!(cfg.blocks.len() >= 1);
    }
}
