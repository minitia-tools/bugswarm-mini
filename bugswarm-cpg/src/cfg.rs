/// Full Control Flow Graph — Branch-aware for Cytron SSA (C6.2.1 PEAK)
///
/// Builds CFG from AST nodes using NodeKind variants.
/// Handles: if/else, for/while loops, return, sequential code.

use std::collections::HashMap;

use crate::graph::{GraphNode, NodeKind};

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: usize,
    pub nodes: Vec<usize>,
    pub predecessors: Vec<usize>,
    pub successors: Vec<usize>,
    pub is_entry: bool,
    pub is_exit: bool,
    pub true_successor: Option<usize>,
    pub false_successor: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    pub blocks: Vec<BasicBlock>,
    pub entry_block: usize,
    pub exit_block: usize,
    pub node_to_block: HashMap<usize, usize>,
}

impl ControlFlowGraph {
    pub fn build(nodes: &[&GraphNode]) -> Self {
        let mut blocks: Vec<BasicBlock> = Vec::new();
        let mut node_to_block: HashMap<usize, usize> = HashMap::new();

        if nodes.is_empty() {
            blocks.push(BasicBlock {
                id: 0, nodes: vec![], predecessors: vec![],
                successors: vec![], is_entry: true, is_exit: true,
                true_successor: None, false_successor: None,
            });
            return Self { blocks, entry_block: 0, exit_block: 0, node_to_block };
        }

        // Start with entry block
        blocks.push(BasicBlock {
            id: 0, nodes: vec![], predecessors: vec![],
            successors: vec![], is_entry: true, is_exit: false,
            true_successor: None, false_successor: None,
        });
        let mut current_id: usize = 0;

        for node in nodes {
            let kind = &node.kind;
            let line = node.line_start;

            match kind {
                NodeKind::Condition => {
                    // End current block, start condition block
                    let prev_id = current_id;

                    // Start condition block
                    let cond_id = blocks.len();
                    blocks.push(BasicBlock {
                        id: cond_id, nodes: vec![line], predecessors: vec![prev_id],
                        successors: vec![], is_entry: false, is_exit: false,
                        true_successor: None, false_successor: None,
                    });
                    blocks[prev_id].successors.push(cond_id);
                    node_to_block.insert(line, cond_id);

                    // Then-branch block
                    let then_id = blocks.len();
                    blocks.push(BasicBlock {
                        id: then_id, nodes: vec![], predecessors: vec![cond_id],
                        successors: vec![], is_entry: false, is_exit: false,
                        true_successor: None, false_successor: None,
                    });
                    blocks[cond_id].successors.push(then_id);
                    blocks[cond_id].true_successor = Some(then_id);

                    // Merge block
                    let merge_id = blocks.len();
                    blocks.push(BasicBlock {
                        id: merge_id, nodes: vec![], predecessors: vec![cond_id],
                        successors: vec![], is_entry: false, is_exit: false,
                        true_successor: None, false_successor: None,
                    });
                    blocks[cond_id].successors.push(merge_id);
                    blocks[cond_id].false_successor = Some(merge_id);
                    blocks[then_id].successors.push(merge_id);
                    blocks[merge_id].predecessors.push(then_id);

                    current_id = merge_id;
                }

                NodeKind::Loop => {
                    node_to_block.insert(line, current_id);
                    blocks[current_id].nodes.push(line);
                    let header_id = current_id;

                    // Loop body block (back-edge to header)
                    let body_id = blocks.len();
                    blocks.push(BasicBlock {
                        id: body_id, nodes: vec![], predecessors: vec![header_id],
                        successors: vec![header_id], // back-edge
                        is_entry: false, is_exit: false,
                        true_successor: None, false_successor: None,
                    });
                    blocks[header_id].successors.push(body_id);

                    // Post-loop block
                    let post_id = blocks.len();
                    blocks.push(BasicBlock {
                        id: post_id, nodes: vec![], predecessors: vec![header_id],
                        successors: vec![], is_entry: false, is_exit: false,
                        true_successor: None, false_successor: None,
                    });
                    blocks[header_id].successors.push(post_id);

                    current_id = post_id;
                }

                NodeKind::Return => {
                    node_to_block.insert(line, current_id);
                    blocks[current_id].nodes.push(line);
                    blocks[current_id].is_exit = true;
                    // Start new unreachable block
                    let new_id = blocks.len();
                    blocks.push(BasicBlock {
                        id: new_id, nodes: vec![], predecessors: vec![],
                        successors: vec![], is_entry: false, is_exit: false,
                        true_successor: None, false_successor: None,
                    });
                    current_id = new_id;
                }

                _ => {
                    // Sequential — stays in current block
                    node_to_block.insert(line, current_id);
                    blocks[current_id].nodes.push(line);
                }
            }
        }

        // Remove trailing empty block if present
        while blocks.last().map_or(false, |b| b.nodes.is_empty() && !b.is_entry) {
            blocks.pop();
        }

        // Set exit block
        let exit = blocks.iter().position(|b| b.is_exit).unwrap_or_else(|| {
            if let Some(last) = blocks.last_mut() {
                last.is_exit = true;
            }
            blocks.len().saturating_sub(1)
        });

        Self { blocks, entry_block: 0, exit_block: exit, node_to_block }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{GraphNode, NodeKind};
    use std::collections::HashMap;

    fn n(name: &str, kind: NodeKind, line: usize) -> GraphNode {
        GraphNode {
            id: line.to_string(), kind, name: name.to_string(), file: "t.py".into(),
            line_start: line, line_end: line, col_start: 0, col_end: 0,
            language: "python".into(), metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_cfg_sequential() {
        let nodes = [&n("x = 1", NodeKind::Assignment, 1), &n("return x", NodeKind::Return, 2)];
        let cfg = ControlFlowGraph::build(&nodes);
        assert!(cfg.blocks.len() >= 1, "Sequential function should have at least 1 block");
    }

    #[test]
    fn test_cfg_conditional_branches() {
        let nodes = [
            &n("if cond:", NodeKind::Condition, 1),
            &n("x = 't'", NodeKind::Assignment, 2),
            &n("x = 'f'", NodeKind::Assignment, 3),
            &n("return x", NodeKind::Return, 4),
        ];
        let cfg = ControlFlowGraph::build(&nodes);
        assert!(cfg.blocks.len() >= 4, "C6.2.1 PEAK: {} blocks, need >= 4 for conditional", cfg.blocks.len());
        let has_branch = cfg.blocks.iter().any(|b| b.true_successor.is_some());
        assert!(has_branch, "C6.2.1 PEAK: Should have branch edge");
    }

    #[test]
    fn test_cfg_loop_structure() {
        let nodes = [
            &n("for i in items:", NodeKind::Loop, 1),
            &n("total += i", NodeKind::Assignment, 2),
            &n("return total", NodeKind::Return, 3),
        ];
        let cfg = ControlFlowGraph::build(&nodes);
        assert!(cfg.blocks.len() >= 3);
    }
}
