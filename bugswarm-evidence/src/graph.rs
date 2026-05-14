use std::collections::{HashMap, HashSet};
use parking_lot::RwLock;
use tracing::{info, warn};

use crate::trigger;
use crate::types::{
    AgentScore, EdgeKind, EvidenceEdge, EvidenceNode, EvidenceQuery,
    GraphStats, NodeId, NodeKind,
};

/// The Evidence Graph — a thread-safe directed hypergraph for agent evidence.
///
/// # Invariants
/// - SandboxRun nodes are immutable once sealed (content_hash is set).
/// - Edges are never deleted, only added.
/// - Every ConfirmedBug node must have at least one Confirms edge from a SandboxRun.
/// - Node IDs are monotonically increasing.
pub struct EvidenceGraph {
    nodes: RwLock<Vec<EvidenceNode>>,
    edges: RwLock<Vec<EvidenceEdge>>,
    /// Adjacency list: node_id → list of (edge_index, target_node_id)
    out_edges: RwLock<HashMap<NodeId, Vec<(usize, NodeId)>>>,
    /// Reverse adjacency: node_id → list of (edge_index, source_node_id)
    in_edges: RwLock<HashMap<NodeId, Vec<(usize, NodeId)>>>,
    pub trigger_manager: RwLock<crate::trigger::TriggerManager>,
}

impl EvidenceGraph {
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(Vec::new()),
            edges: RwLock::new(Vec::new()),
            out_edges: RwLock::new(HashMap::new()),
            in_edges: RwLock::new(HashMap::new()),
            trigger_manager: RwLock::new(crate::trigger::TriggerManager::new()),
        }
    }

    /// Add a node and return its ID.
    pub fn add_node(&self, node: EvidenceNode) -> NodeId {
        let mut nodes = self.nodes.write();
        let id = nodes.len();
        let mut n = node;
        n.id = id;
        nodes.push(n);
        id
    }

    /// Add a sandbox run node (immutable, sealed with hash).
    /// Holds a single write lock for the entire operation — no TOCTOU.
    pub fn add_sandbox_run(
        &self,
        label: &str,
        description: &str,
        author: &str,
        receipt_json: &str,
    ) -> NodeId {
        let mut nodes = self.nodes.write();
        let id = nodes.len();
        let mut node = EvidenceNode::new(id, NodeKind::SandboxRun, label, author);
        node.description = description.to_string();
        node.metadata.insert("receipt".into(), receipt_json.to_string());
        node.seal(); // Immutable after creation
        nodes.push(node);
        id
    }

    /// Add a claim node from an agent.
    /// Holds a single write lock — no TOCTOU.
    pub fn add_claim(
        &self,
        claim_text: &str,
        author: &str,
        location: &str,
        severity: u8,
    ) -> NodeId {
        let mut nodes = self.nodes.write();
        let id = nodes.len();
        let mut node = EvidenceNode::new(id, NodeKind::Claim, claim_text, author);
        node.description = format!("Claimed bug at {}", location);
        node.metadata.insert("location".into(), location.to_string());
        node.severity = Some(severity);
        nodes.push(node);
        id
    }

    /// Add a prediction node derived from a claim.
    /// Holds a single write lock — no TOCTOU.
    pub fn add_prediction(
        &self,
        prediction_text: &str,
        author: &str,
        parent_claim_id: NodeId,
    ) -> (NodeId, NodeId) {
        let pred_id;
        {
            let mut nodes = self.nodes.write();
            pred_id = nodes.len();
            let mut node = EvidenceNode::new(pred_id, NodeKind::Prediction, prediction_text, author);
            node.description = format!("Derived from claim {}", parent_claim_id);
            node.seal();
            nodes.push(node);
        }

        // Edge: parent claim → predicts → prediction
        let edge = EvidenceEdge::new(EdgeKind::Predicts, parent_claim_id, pred_id, 1.0);
        let edge_idx = self.add_edge(edge);

        (pred_id, edge_idx)
    }

    /// Add a trigger condition to the graph and link it to the bug's trigger matrix.
    /// Creates TriggerMatrix node if one doesn't exist for this bug.
    pub fn add_trigger_condition(&self, bug_id: &str, condition: trigger::TriggerCondition) -> bool {
        let matrix_node_id = self.get_or_create_trigger_matrix(bug_id);

        let cond_node = EvidenceNode::new(
            0,
            NodeKind::TriggerCondition,
            &format!("{}-{:?}", bug_id, condition.dimension),
            condition.layer.layer_name(),
        );
        let cond_node_id = self.add_node(cond_node);

        let edge = EvidenceEdge::new(EdgeKind::Triggers, cond_node_id, matrix_node_id, 1.0);
        self.add_edge(edge);

        true
    }

    /// Get or create the trigger matrix node for a bug.
    fn get_or_create_trigger_matrix(&self, bug_id: &str) -> NodeId {
        let nodes = self.nodes.read();
        for node in nodes.iter() {
            if node.kind == NodeKind::TriggerMatrix && node.label == bug_id {
                return node.id;
            }
        }
        drop(nodes);

        let matrix_node = EvidenceNode::new(0, NodeKind::TriggerMatrix, bug_id, "trigger-matrix");
        self.add_node(matrix_node)
    }

    /// Get all trigger conditions for a bug from the graph.
    pub fn get_trigger_conditions(&self, bug_id: &str) -> Vec<NodeId> {
        let nodes = self.nodes.read();
        let matrix_id = nodes.iter()
            .find(|n| n.kind == NodeKind::TriggerMatrix && n.label == bug_id)
            .map(|n| n.id);

        let matrix_id = match matrix_id {
            Some(id) => id,
            None => return vec![],
        };
        drop(nodes);

        let out = self.out_edges.read();
        let mut conditions = vec![];
        for (node_id, edges) in out.iter() {
            for (_, target_id) in edges {
                if *target_id == matrix_id {
                    conditions.push(*node_id);
                }
            }
        }
        conditions
    }

    /// Add an edge between two nodes.
    pub fn add_edge(&self, edge: EvidenceEdge) -> usize {
        let mut edges = self.edges.write();
        let idx = edges.len();
        edges.push(edge.clone());

        let mut out = self.out_edges.write();
        out.entry(edge.from).or_default().push((idx, edge.to));

        let mut inp = self.in_edges.write();
        inp.entry(edge.to).or_default().push((idx, edge.from));

        idx
    }

    /// Link a sandbox run to a prediction with confirmation or contradiction.
    pub fn link_sandbox_result(
        &self,
        sandbox_run_id: NodeId,
        prediction_id: NodeId,
        confirmed: bool,
        confidence: f64,
    ) {
        let kind = if confirmed {
            EdgeKind::Confirms
        } else {
            EdgeKind::Contradicts
        };

        // SandboxRun → Confirms/Contradicts → Prediction
        let edge = EvidenceEdge::new(kind, sandbox_run_id, prediction_id, confidence);
        self.add_edge(edge);

        // If confirmed, also create a Verified edge from prediction back to sandbox
        if confirmed {
            let edge2 = EvidenceEdge::new(EdgeKind::Verifies, prediction_id, sandbox_run_id, 1.0);
            self.add_edge(edge2);
        }
    }

    /// Create a confirmed bug node aggregating a claim and its verifying sandbox runs.
    pub fn confirm_bug(&self, claim_id: NodeId, sandbox_run_ids: &[NodeId]) -> Option<NodeId> {
        let claim = {
            let nodes = self.nodes.read();
            if claim_id >= nodes.len() {
                return None;
            }
            nodes[claim_id].clone()
        };

        if claim.kind != NodeKind::Claim {
            return None;
        }

        let bug_id = {
            let nodes = self.nodes.read();
            nodes.len()
        };
        let mut bug = EvidenceNode::new(
            bug_id,
            NodeKind::ConfirmedBug,
            &format!("BUG: {}", claim.label),
            &claim.author,
        );
        bug.description = claim.description.clone();
        bug.severity = claim.severity;
        bug.independently_verified = true;
        bug.seal();
        let bug_id = self.add_node(bug);

        // Edge: Claim → Aggregates → ConfirmedBug
        let edge1 = EvidenceEdge::new(EdgeKind::Aggregates, claim_id, bug_id, 1.0);
        self.add_edge(edge1);

        // Edge: ConfirmedBug → Refines (derived from) → Claim
        let edge2 = EvidenceEdge::new(EdgeKind::Refines, bug_id, claim_id, 1.0);
        self.add_edge(edge2);

        // Edge each sandbox run to the confirmed bug
        for &run_id in sandbox_run_ids {
            let edge = EvidenceEdge::new(EdgeKind::Confirms, run_id, bug_id, 1.0);
            self.add_edge(edge);
        }

        Some(bug_id)
    }

    /// Get a node by ID.
    pub fn get_node(&self, id: NodeId) -> Option<EvidenceNode> {
        self.nodes.read().get(id).cloned()
    }

    /// Get all edges from a node.
    pub fn edges_from(&self, node_id: NodeId) -> Vec<(EvidenceEdge, NodeId)> {
        let out = self.out_edges.read();
        let edges = self.edges.read();
        out.get(&node_id)
            .map(|v| {
                v.iter()
                    .filter_map(|&(edge_idx, target)| {
                        edges.get(edge_idx).cloned().map(|e| (e, target))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all edges to a node.
    pub fn edges_to(&self, node_id: NodeId) -> Vec<(EvidenceEdge, NodeId)> {
        let inp = self.in_edges.read();
        let edges = self.edges.read();
        inp.get(&node_id)
            .map(|v| {
                v.iter()
                    .filter_map(|&(edge_idx, source)| {
                        edges.get(edge_idx).cloned().map(|e| (e, source))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find the weakest claim — the one with the least supporting evidence.
    pub fn weakest_claim(&self) -> Option<(NodeId, String, f64)> {
        let nodes = self.nodes.read();
        let mut weakest: Option<(NodeId, String, f64, usize)> = None;

        for (i, node) in nodes.iter().enumerate() {
            if node.kind != NodeKind::Claim {
                continue;
            }
            let incoming = self.edges_to(i);
            let support_count = incoming
                .iter()
                .filter(|(e, _)| matches!(e.kind, EdgeKind::Supports | EdgeKind::Confirms))
                .count();
            let contradict_count = incoming
                .iter()
                .filter(|(e, _)| matches!(e.kind, EdgeKind::Contradicts))
                .count();

            let strength = support_count as f64 - contradict_count as f64 * 2.0;
            match &weakest {
                None => weakest = Some((i, node.label.clone(), strength, support_count)),
                Some((_, _, s, _)) if strength < *s => {
                    weakest = Some((i, node.label.clone(), strength, support_count));
                }
                _ => {}
            }
        }
        weakest.map(|(id, label, strength, _)| (id, label, strength))
    }

    /// Find the strongest claim — the one with the most confirmed evidence.
    pub fn strongest_claim(&self) -> Option<(NodeId, String, f64, usize)> {
        let nodes = self.nodes.read();
        let mut best: Option<(NodeId, String, f64, usize)> = None;

        for (i, node) in nodes.iter().enumerate() {
            if node.kind != NodeKind::Claim {
                continue;
            }
            let incoming = self.edges_to(i);
            let confirm_count = incoming
                .iter()
                .filter(|(e, _)| e.kind == EdgeKind::Confirms)
                .count();
            let support_count = incoming
                .iter()
                .filter(|(e, _)| matches!(e.kind, EdgeKind::Supports | EdgeKind::Confirms))
                .count();

            let strength = support_count as f64 * (1.0 + confirm_count as f64 * 0.5);
            match &best {
                None => best = Some((i, node.label.clone(), strength, support_count)),
                Some((_, _, s, _)) if strength > *s => {
                    best = Some((i, node.label.clone(), strength, support_count));
                }
                _ => {}
            }
        }
        best.map(|(id, label, strength, count)| (id, label, strength, count))
    }

    /// Find contradictory evidence — pairs of edges that disagree.
    pub fn find_contradictions(&self) -> Vec<(NodeId, NodeId, String)> {
        let nodes = self.nodes.read();
        let mut contradictions = Vec::new();

        for (i, node) in nodes.iter().enumerate() {
            if node.kind != NodeKind::Prediction {
                continue;
            }
            let incoming = self.edges_to(i);
            let has_confirm = incoming.iter().any(|(e, _)| matches!(e.kind, EdgeKind::Confirms | EdgeKind::Verifies));
            let has_contradict = incoming.iter().any(|(e, _)| matches!(e.kind, EdgeKind::Contradicts));

            if has_confirm && has_contradict {
                contradictions.push((i, i, format!("Prediction '{}' has both confirming and contradicting evidence", node.label)));
            }
        }
        contradictions
    }

    /// Find consensus echo chambers — groups of agents that only agree with each other.
    pub fn find_echo_chambers(&self, similarity_threshold: f64) -> Vec<Vec<String>> {
        let nodes = self.nodes.read();
        let mut agent_claims: HashMap<String, Vec<NodeId>> = HashMap::new();

        // Group claims by agent
        for (i, node) in nodes.iter().enumerate() {
            if node.kind == NodeKind::Claim {
                agent_claims.entry(node.author.clone()).or_default().push(i);
            }
        }

        // Build agreement graph between agents
        let agents: Vec<String> = agent_claims.keys().cloned().collect();
        let mut agreement_count: HashMap<(usize, usize), usize> = HashMap::new();

        for (i, a1) in agents.iter().enumerate() {
            for (j, a2) in agents.iter().enumerate() {
                if i >= j { continue; }
                // Count edges between their claims
                let mut count = 0;
                for &c1 in agent_claims.get(a1).unwrap_or(&vec![]) {
                    for edge in self.edges_from(c1) {
                        let target_author = self.get_node(edge.1).map(|n| n.author).unwrap_or_default();
                        if target_author == *a2 {
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    agreement_count.insert((i, j), count);
                }
            }
        }

        // Find groups with high agreement density
        let mut chambers = Vec::new();
        let total_pairs = agents.len() * (agents.len() - 1) / 2;
        if total_pairs > 0 {
            let density = agreement_count.len() as f64 / total_pairs as f64;
            if density > similarity_threshold {
                chambers.push(agents.clone());
            }
        }

        chambers
    }

    /// Find orphaned claims — claims with no supporting or contradicting evidence.
    pub fn orphaned_claims(&self) -> Vec<(NodeId, String)> {
        let nodes = self.nodes.read();
        let mut orphans = Vec::new();

        for (i, node) in nodes.iter().enumerate() {
            if node.kind != NodeKind::Claim {
                continue;
            }
            let incoming = self.edges_to(i);
            let has_evidence = incoming.iter().any(|(e, _)| {
                matches!(e.kind, EdgeKind::Supports | EdgeKind::Confirms | EdgeKind::Contradicts)
            });
            if !has_evidence {
                orphans.push((i, node.label.clone()));
            }
        }
        orphans
    }

    /// Query the graph with filters.
    pub fn query(&self, query: &EvidenceQuery) -> Vec<EvidenceNode> {
        let nodes = self.nodes.read();
        let mut results: Vec<EvidenceNode> = nodes
            .iter()
            .filter(|n| {
                if let Some(ref kind) = query.kind {
                    if n.kind != *kind { return false; }
                }
                if let Some(ref author) = query.author {
                    if n.author != *author { return false; }
                }
                if let Some(min_sev) = query.min_severity {
                    if n.severity.unwrap_or(0) < min_sev { return false; }
                }
                if query.verified_only && !n.independently_verified {
                    return false;
                }
                if query.immutable_only && !n.immutable {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        results.truncate(query.limit);
        results
    }

    /// Score an agent based on their contributions to the graph.
    pub fn score_agent(&self, agent_id: &str) -> AgentScore {
        let nodes = self.nodes.read();
        let mut total_claims = 0usize;
        let mut verified = 0usize;
        let mut contradicted = 0usize;
        let mut novel_nodes = 0usize;

        for (i, node) in nodes.iter().enumerate() {
            if node.author != agent_id {
                continue;
            }
            match node.kind {
                    NodeKind::Claim => {
                        total_claims += 1;
                        let incoming = self.edges_to(i);
                        let outgoing = self.edges_from(i);
                        let has_confirm = incoming.iter().any(|(e, _)| e.kind == EdgeKind::Confirms)
                            || outgoing.iter().any(|(e, _)| e.kind == EdgeKind::Aggregates);
                        let has_contradict = incoming.iter().any(|(e, _)| e.kind == EdgeKind::Contradicts);
                    if has_confirm { verified += 1; }
                    if has_contradict { contradicted += 1; }
                }
                NodeKind::Prediction | NodeKind::CodeLocation => {
                    novel_nodes += 1;
                }
                _ => {}
            }
        }

        let novelty = if total_claims > 0 {
            novel_nodes as f64 / total_claims as f64
        } else { 0.0 };

        let verification = if total_claims > 0 {
            verified as f64 / total_claims as f64
        } else { 0.0 };

        let efficiency = if total_claims > 0 {
            (verified as f64 - contradicted as f64 * 0.5) / total_claims as f64
        } else { 0.0 };

        let composite = novelty * 0.3 + verification * 0.5 + efficiency.max(0.0) * 0.2;

        AgentScore {
            agent_id: agent_id.to_string(),
            novelty_score: novelty,
            verification_score: verification,
            efficiency_score: efficiency,
            total_claims,
            verified_claims: verified,
            contradicted_claims: contradicted,
            composite_score: composite,
        }
    }

    /// Compute statistics about the graph.
    pub fn stats(&self) -> GraphStats {
        let nodes = self.nodes.read();
        let edges = self.edges.read();

        let mut claims = 0;
        let mut predictions = 0;
        let mut sandbox_runs = 0;
        let mut code_locations = 0;
        let mut confirmed_bugs = 0;
        let mut trigger_matrices = 0;
        let mut trigger_conditions = 0;
        let mut agents_set = HashSet::new();
        let mut supports = 0;
        let mut contradicts = 0;
        let mut confirms = 0;
        let mut total_confidence = 0.0;

        for node in nodes.iter() {
            match node.kind {
                NodeKind::Claim => claims += 1,
                NodeKind::Prediction => predictions += 1,
                NodeKind::SandboxRun => sandbox_runs += 1,
                NodeKind::CodeLocation => code_locations += 1,
                NodeKind::ConfirmedBug => confirmed_bugs += 1,
                NodeKind::TriggerMatrix => trigger_matrices += 1,
                NodeKind::TriggerCondition => trigger_conditions += 1,
                _ => {}
            }
            agents_set.insert(node.author.clone());
        }

        for edge in edges.iter() {
            match edge.kind {
                EdgeKind::Supports => supports += 1,
                EdgeKind::Contradicts => contradicts += 1,
                EdgeKind::Confirms => confirms += 1,
                _ => {}
            }
            total_confidence += edge.confidence;
        }

        let avg_conf = if !edges.is_empty() {
            total_confidence / edges.len() as f64
        } else { 0.0 };

        let weakest = self.weakest_claim().map(|(_, label, _)| label);
        let strongest = self.strongest_claim().map(|(_, label, _, _)| label);
        let echo = self.find_echo_chambers(0.7).len();
        let orphaned = self.orphaned_claims().len();

        GraphStats {
            total_nodes: nodes.len(),
            total_edges: edges.len(),
            claims, predictions, sandbox_runs, code_locations, confirmed_bugs,
            agents: agents_set.len(),
            supports_count: supports,
            contradicts_count: contradicts,
            confirms_count: confirms,
            average_confidence: avg_conf,
            strongest_claim: strongest,
            weakest_claim: weakest,
            consensus_echo_chambers: echo,
            orphaned_claims: orphaned,
            trigger_matrices,
            trigger_conditions,
        }
    }

    /// Verify all immutable nodes have valid hashes.
    pub fn verify_integrity(&self) -> Vec<NodeId> {
        let nodes = self.nodes.read();
        nodes
            .iter()
            .filter(|n| n.immutable && !n.verify())
            .map(|n| n.id)
            .collect()
    }

    /// Node count.
    pub fn node_count(&self) -> usize {
        self.nodes.read().len()
    }

    /// Edge count.
    pub fn edge_count(&self) -> usize {
        self.edges.read().len()
    }
}

impl Default for EvidenceGraph {
    fn default() -> Self {
        Self::new()
    }
}
