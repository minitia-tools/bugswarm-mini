//! Dominator Tree — Lengauer-Tarjan algorithm.
//!
//! C6.2.1: Computes dominators and dominance frontiers for φ-node insertion.
//! Used by Cytron et al. SSA construction.

use std::collections::{HashMap, HashSet};

use crate::cfg::ControlFlowGraph;

/// Dominator tree for a CFG.
#[derive(Debug, Clone)]
pub struct DominatorTree {
    /// Map: block_id → immediate dominator (idom)
    pub idom: HashMap<usize, usize>,
    /// Map: block_id → set of blocks it dominates
    pub dominated: HashMap<usize, HashSet<usize>>,
    /// Map: block_id → dominance frontier (DF)
    pub dominance_frontier: HashMap<usize, HashSet<usize>>,
}

impl DominatorTree {
    /// Build dominator tree from a CFG using iterative data-flow.
    /// Simplified version for the number of blocks we handle (typically < 100 per function).
    pub fn build(cfg: &ControlFlowGraph) -> Self {
        let n = cfg.blocks.len();
        if n == 0 {
            return Self {
                idom: HashMap::new(),
                dominated: HashMap::new(),
                dominance_frontier: HashMap::new(),
            };
        }

        // Initialize: entry dominates itself, everything else is dominated by everything
        let entry = cfg.entry_block;
        let mut dom: Vec<HashSet<usize>> = vec![HashSet::new(); n];

        // Entry block dominates only itself
        dom[entry].insert(entry);

        // All other blocks: initially dominated by ALL blocks
        for (i, dom_i) in dom.iter_mut().enumerate() {
            if i != entry {
                *dom_i = (0..n).collect();
            }
        }

        // Iterate to fixed point
        let mut changed = true;
        while changed {
            changed = false;

            for b in 0..n {
                if b == entry {
                    continue;
                }

                // dom[b] = {b} ∪ ∩_{p ∈ preds[b]} dom[p]
                let preds = &cfg.blocks[b].predecessors;
                let mut new_dom: HashSet<usize> = if preds.is_empty() {
                    // Unreachable block
                    HashSet::new()
                } else {
                    // Start with first predecessor's dominators
                    let mut intersection = dom[preds[0]].clone();
                    for p in &preds[1..] {
                        intersection = intersection.intersection(&dom[*p]).copied().collect();
                    }
                    intersection
                };

                new_dom.insert(b);

                if new_dom != dom[b] {
                    dom[b] = new_dom;
                    changed = true;
                }
            }
        }

        // Compute immediate dominators
        let mut idom: HashMap<usize, usize> = HashMap::new();
        let mut dominated: HashMap<usize, HashSet<usize>> = HashMap::new();

        for b in 0..n {
            // idom[b] = the strict dominator closest to b
            let strict_doms: Vec<usize> = dom[b].iter().filter(|&&d| d != b).copied().collect();

            if !strict_doms.is_empty() {
                // idom is the one that dominates all other strict dominators
                let mut best = strict_doms[0];
                for &d in &strict_doms[1..] {
                    if dom[best].contains(&d) {
                        // d dominates best → d is "closer" to b
                        best = d;
                    }
                }
                idom.insert(b, best);
            }

            dominated.entry(b).or_default();
        }

        // Build dominated sets from idom
        for b in 0..n {
            if let Some(&parent) = idom.get(&b) {
                dominated.entry(parent).or_default().insert(b);
            }
        }

        // Compute dominance frontiers
        let mut df: HashMap<usize, HashSet<usize>> = HashMap::new();
        for b in 0..n {
            df.entry(b).or_default();
        }

        for b in 0..n {
            let preds = &cfg.blocks[b].predecessors;
            if preds.len() >= 2 {
                // b has multiple predecessors → it's a join point
                for &p in preds {
                    let mut runner = p;
                    // Walk up the dominator tree from p until we hit idom[b]
                    while runner != entry {
                        if let Some(&idom_b) = idom.get(&b) {
                            if runner == idom_b {
                                break;
                            }
                        }
                        df.entry(runner).or_default().insert(b);
                        if let Some(&next) = idom.get(&runner) {
                            runner = next;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        Self {
            idom,
            dominated,
            dominance_frontier: df,
        }
    }

    /// Get the dominance frontier for a block.
    pub fn get_df(&self, block: usize) -> HashSet<usize> {
        self.dominance_frontier.get(&block).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{BasicBlock, ControlFlowGraph};

    #[test]
    fn test_dominators_simple() {
        // Build a simple CFG: entry → a → b → exit
        let cfg = ControlFlowGraph {
            blocks: vec![
                BasicBlock { id: 0, nodes: vec![], predecessors: vec![], successors: vec![1], is_entry: true, is_exit: false, true_successor: None, false_successor: None },
                BasicBlock { id: 1, nodes: vec![], predecessors: vec![0], successors: vec![2], is_entry: false, is_exit: false, true_successor: None, false_successor: None },
                BasicBlock { id: 2, nodes: vec![], predecessors: vec![1], successors: vec![], is_entry: false, is_exit: true, true_successor: None, false_successor: None },
            ],
            entry_block: 0, exit_block: 2,
            node_to_block: HashMap::new(),
        };

        let dt = DominatorTree::build(&cfg);
        // Block 0 dominates 0,1,2
        // Block 1's idom should be 0
        assert!(dt.idom.contains_key(&1));
        assert_eq!(dt.idom[&1], 0);
    }

    #[test]
    fn test_dominators_branch() {
        // With simplified CFG, branches are sequential — dominator tree should still be valid
        let cfg = ControlFlowGraph { blocks: vec![
            BasicBlock { id: 0, nodes: vec![], predecessors: vec![], successors: vec![1], is_entry: true, is_exit: false, true_successor: None, false_successor: None },
            BasicBlock { id: 1, nodes: vec![], predecessors: vec![0], successors: vec![], is_entry: false, is_exit: true, true_successor: None, false_successor: None },
        ], entry_block: 0, exit_block: 1, node_to_block: HashMap::new() };
        let dt = DominatorTree::build(&cfg);
        assert!(dt.idom.contains_key(&1));
        assert_eq!(dt.idom[&1], 0);
    }
}
