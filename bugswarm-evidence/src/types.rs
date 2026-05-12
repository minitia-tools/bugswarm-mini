use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Unique identifier for evidence nodes.
pub type NodeId = usize;

/// Types of evidence nodes in the graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// An agent's claim about a potential bug.
    Claim,
    /// A falsifiable prediction derived from a claim.
    Prediction,
    /// A sandbox execution receipt — immutable once written.
    SandboxRun,
    /// A code location (file:line) referenced by a claim.
    CodeLocation,
    /// An agent entity in the swarm.
    Agent,
    /// A confirmed vulnerability (aggregate of Claim + SandboxRun).
    ConfirmedBug,
}

/// An evidence node in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: String,
    pub description: String,
    /// Agent that created this node.
    pub author: String,
    /// When the node was created.
    pub created_at: DateTime<Utc>,
    /// Whether this node is immutable (sandbox runs, confirmed bugs).
    pub immutable: bool,
    /// Cryptographic hash of the content for immutability verification.
    pub content_hash: Option<String>,
    /// Arbitrary metadata.
    pub metadata: HashMap<String, String>,
    /// Severity estimate (for claims).
    pub severity: Option<u8>,
    /// Whether this node has been independently verified.
    pub independently_verified: bool,
}

/// Types of edges between evidence nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// Evidence supports the claim.
    Supports,
    /// Evidence contradicts the claim.
    Contradicts,
    /// Evidence confirms the claim (stronger than supports).
    Confirms,
    /// One claim refines another (adds detail, fixes scope).
    Refines,
    /// Agent authored this node.
    Authored,
    /// A prediction is derived from a claim.
    Predicts,
    /// A claim references a code location.
    References,
    /// A sandbox run is linked to a prediction.
    Verifies,
    /// A confirmed bug aggregates multiple nodes.
    Aggregates,
}

/// An edge in the evidence graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEdge {
    pub kind: EdgeKind,
    pub from: NodeId,
    pub to: NodeId,
    /// Confidence in this edge (0.0–1.0).
    pub confidence: f64,
    /// Human-readable label.
    pub label: Option<String>,
    /// When the edge was created.
    pub created_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

/// Agent performance metrics computed from the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentScore {
    pub agent_id: String,
    /// How many new nodes did this agent create?
    pub novelty_score: f64,
    /// What fraction of predictions were confirmed by sandbox?
    pub verification_score: f64,
    /// Predictions confirmed per token spent (placeholder for now).
    pub efficiency_score: f64,
    /// Total claims made.
    pub total_claims: usize,
    /// Claims verified by sandbox.
    pub verified_claims: usize,
    /// Claims contradicted by sandbox.
    pub contradicted_claims: usize,
    /// Composite score.
    pub composite_score: f64,
}

/// Statistical summary of the evidence graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub claims: usize,
    pub predictions: usize,
    pub sandbox_runs: usize,
    pub code_locations: usize,
    pub confirmed_bugs: usize,
    pub agents: usize,
    pub supports_count: usize,
    pub contradicts_count: usize,
    pub confirms_count: usize,
    pub average_confidence: f64,
    pub strongest_claim: Option<String>,
    pub weakest_claim: Option<String>,
    pub consensus_echo_chambers: usize,
    pub orphaned_claims: usize,
}

/// Query filters for searching the graph.
#[derive(Debug, Clone)]
pub struct EvidenceQuery {
    /// Filter by node kind.
    pub kind: Option<NodeKind>,
    /// Filter by author agent.
    pub author: Option<String>,
    /// Filter by minimum severity.
    pub min_severity: Option<u8>,
    /// Filter by verification status.
    pub verified_only: bool,
    /// Filter by immutability.
    pub immutable_only: bool,
    /// Sort by (field name).
    pub sort_by: Option<String>,
    /// Maximum results.
    pub limit: usize,
}

impl Default for EvidenceQuery {
    fn default() -> Self {
        Self {
            kind: None, author: None, min_severity: None,
            verified_only: false, immutable_only: false,
            sort_by: None, limit: 100,
        }
    }
}

impl EvidenceNode {
    pub fn new(id: NodeId, kind: NodeKind, label: &str, author: &str) -> Self {
        Self {
            id, kind,
            label: label.to_string(),
            description: String::new(),
            author: author.to_string(),
            created_at: Utc::now(),
            immutable: false,
            content_hash: None,
            metadata: HashMap::new(),
            severity: None,
            independently_verified: false,
        }
    }

    /// Compute and set the content hash, making this node immutable.
    /// Hashes ALL mutable fields: id, label, description, author, kind,
    /// severity, independently_verified, metadata, created_at.
    pub fn seal(&mut self) -> String {
        let content = format!(
            "{}:{}:{}:{}:{:?}:{}:{}:{}:{}",
            self.id,
            self.label,
            self.description,
            self.author,
            self.kind,
            self.severity.unwrap_or(0),
            self.independently_verified,
            serde_json::to_string(&self.metadata).unwrap_or_default(),
            self.created_at
        );
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        self.content_hash = Some(hash.clone());
        self.immutable = true;
        hash
    }

    /// Verify the node's content hash matches its current state.
    /// Returns false if any field was modified after sealing.
    pub fn verify(&self) -> bool {
        if let Some(ref stored_hash) = self.content_hash {
            let content = format!(
                "{}:{}:{}:{}:{:?}:{}:{}:{}:{}",
                self.id,
                self.label,
                self.description,
                self.author,
                self.kind,
                self.severity.unwrap_or(0),
                self.independently_verified,
                serde_json::to_string(&self.metadata).unwrap_or_default(),
                self.created_at
            );
            let computed = hex::encode(Sha256::digest(content.as_bytes()));
            stored_hash == &computed
        } else {
            false // No hash = node was never sealed, cannot verify
        }
    }
}

impl EvidenceEdge {
    pub fn new(kind: EdgeKind, from: NodeId, to: NodeId, confidence: f64) -> Self {
        Self {
            kind, from, to, confidence,
            label: None,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }
}
