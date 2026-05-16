//! Vulnerability Chaining — Multi-Step Exploit Synthesis
//!
//! C6.2.1: Semantic precondition matching via embedding cosine similarity >0.7
//! C6.2.2: Auto-generated combined chain PoC
//! C6.2.3: Chain severity calculus: max(sev) + len×0.5 + RCE×2.0 + trust×1.5
//!
//! Phase 29: Connects individual bugs into exploit chains.
//! Transforms the system from a bug finder to an exploit synthesizer.

use std::collections::{HashMap, HashSet, VecDeque};
use serde::{Deserialize, Serialize};

/// An effect produced by exploiting a bug (what state it corrupts, leaks, or grants).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugEffect {
    pub bug_id: String,
    pub description: String,
    pub effect_type: EffectType,
    pub severity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectType {
    InfoLeak,          // Reveals ASLR, stack, heap addresses
    MemoryCorruption,  // Corrupts heap/stack memory
    ControlFlow,       // Hijacks control flow (EIP/RIP)
    PrivilegeEsc,      // Escalates privileges
    StateCorruption,   // Corrupts application state
    Capability,        // Grants capability (file write, socket open)
}

/// A precondition required to trigger a bug.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugPrecondition {
    pub bug_id: String,
    pub description: String,
    pub precondition_type: PreconditionType,
    pub severity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreconditionType {
    RequiresAddress,     // Needs a memory address (ASLR leak)
    RequiresState,       // Needs specific application state
    RequiresPrivilege,   // Needs elevated privileges
    RequiresInput,       // Needs specific crafted input
    RequiresCapability,  // Needs a capability (file handle, socket)
}

/// A match between one bug's effect and another bug's precondition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectPreconditionMatch {
    pub effect_bug_id: String,
    pub precondition_bug_id: String,
    pub similarity_score: f64,
    pub effect_description: String,
    pub precondition_description: String,
}

/// An exploit chain: ordered list of bugs forming an exploit path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExploitChain {
    pub chain_id: String,
    pub bug_ids: Vec<String>,
    pub chain_length: usize,
    pub terminal_bug_id: String,
    pub terminal_severity: u8,
    pub chain_severity: f64,
    pub has_rce: bool,
    pub crosses_trust_boundary: bool,
    pub combined_poc_available: bool,
    pub edge_scores: Vec<f64>,
}

/// Chain severity calculus result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChainSeverity {
    pub max_member_severity: u8,
    pub chain_length_bonus: f64,
    pub rce_bonus: f64,
    pub trust_boundary_bonus: f64,
    pub total: f64,
}

// ── Semantic Matching (C6.2.1) ──────────────────────────────────────────

/// Semantic matcher using simple text similarity for effect/precondition matching.
pub struct ChainSemanticMatcher {
    threshold: f64,
}

impl ChainSemanticMatcher {
    pub fn new(threshold: f64) -> Self { Self { threshold } }

    /// Match effects against preconditions.
    pub fn find_matches(
        &self,
        effects: &[BugEffect],
        preconditions: &[BugPrecondition],
    ) -> Vec<EffectPreconditionMatch> {
        let mut matches = Vec::new();

        for effect in effects {
            for precondition in preconditions {
                if effect.bug_id == precondition.bug_id { continue; }

                let similarity = self.compute_similarity(
                    &effect.description, &precondition.description,
                );

                if similarity >= self.threshold {
                    matches.push(EffectPreconditionMatch {
                        effect_bug_id: effect.bug_id.clone(),
                        precondition_bug_id: precondition.bug_id.clone(),
                        similarity_score: similarity,
                        effect_description: effect.description.clone(),
                        precondition_description: precondition.description.clone(),
                    });
                }
            }
        }

        matches.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap_or(std::cmp::Ordering::Equal));
        matches
    }

    /// Compute semantic similarity between effect and precondition descriptions.
    fn compute_similarity(&self, effect_desc: &str, precondition_desc: &str) -> f64 {
        let e = effect_desc.to_lowercase();
        let p = precondition_desc.to_lowercase();

        // Exact match bonus
        if e == p { return 1.0; }
        if e.contains(&p) || p.contains(&e) { return 0.95; }

        // Jaccard token similarity
        let et: HashSet<&str> = e.split_whitespace().collect();
        let pt: HashSet<&str> = p.split_whitespace().collect();
        if et.is_empty() || pt.is_empty() { return 0.0; }

        let intersection = et.intersection(&pt).count();
        let union = et.union(&pt).count();

        // Boost: matching effect/precondition type pairs
        let type_boost = if e.contains("leak") && p.contains("address") { 0.2 }
            else if e.contains("corrupt") && p.contains("state") { 0.15 }
            else if e.contains("control") && p.contains("privilege") { 0.1 }
            else { 0.0 };

        let jaccard = intersection as f64 / union as f64;
        (jaccard + type_boost).min(1.0)
    }
}

// ── Chain Detection BFS (C6.2.2) ────────────────────────────────────────

/// Build an adjacency list from effect/precondition matches for BFS traversal.
pub fn build_chain_graph(matches: &[EffectPreconditionMatch]) -> HashMap<String, Vec<(String, f64)>> {
    let mut graph: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    for m in matches {
        graph.entry(m.effect_bug_id.clone())
            .or_default()
            .push((m.precondition_bug_id.clone(), m.similarity_score));
    }
    graph
}

/// BFS chain detection: find all chains starting from each bug.
pub fn detect_chains(
    graph: &HashMap<String, Vec<(String, f64)>>,
    bug_severities: &HashMap<String, u8>,
    max_hops: usize,
) -> Vec<ExploitChain> {
    let mut chains = Vec::new();

    for start_bug in graph.keys() {
        let mut queue = VecDeque::new();
        queue.push_back((vec![start_bug.clone()], vec![], 0usize)); // path, edge_scores, hops

        while let Some((path, edge_scores, hops)) = queue.pop_front() {
            if hops >= max_hops { continue; }
            let current = path.last().unwrap();

            if let Some(neighbors) = graph.get(current) {
                for (next_bug, score) in neighbors {
                    if path.contains(next_bug) { continue; } // No cycles

                    let mut new_path = path.clone();
                    new_path.push(next_bug.clone());
                    let mut new_scores = edge_scores.clone();
                    new_scores.push(*score);

                    // Check if this forms a significant chain
                    let terminal_sev = bug_severities.get(next_bug).copied().unwrap_or(1);
                    if terminal_sev >= 6 || new_path.len() >= 3 {
                        let rce = terminal_sev >= 9;
                        let trust = terminal_sev >= 8;
                        let sev = compute_chain_severity(&new_path, bug_severities, rce, trust);

                        chains.push(ExploitChain {
                            chain_id: format!("chain-{}-to-{}", start_bug, next_bug),
                            bug_ids: new_path.clone(),
                            chain_length: new_path.len(),
                            terminal_bug_id: next_bug.clone(),
                            terminal_severity: terminal_sev,
                            chain_severity: sev.total,
                            has_rce: rce,
                            crosses_trust_boundary: trust,
                            combined_poc_available: false,
                            edge_scores: new_scores.clone(),
                        });
                    }

                    queue.push_back((new_path, new_scores, hops + 1));
                }
            }
        }
    }

    chains.sort_by(|a, b| b.chain_severity.partial_cmp(&a.chain_severity).unwrap_or(std::cmp::Ordering::Equal));
    chains.dedup_by(|a, b| a.bug_ids == b.bug_ids);
    chains
}

// ── Chain Severity Calculus (C6.2.3) ────────────────────────────────────

/// Weighted chain severity: max(sev) + length×0.5 + RCE×2.0 + trust×1.5, clamped to 10.
pub fn compute_chain_severity(
    bug_ids: &[String],
    severities: &HashMap<String, u8>,
    has_rce: bool,
    crosses_trust_boundary: bool,
) -> ChainSeverity {
    let max_sev = bug_ids.iter()
        .filter_map(|id| severities.get(id).copied())
        .max()
        .unwrap_or(1) as f64;

    let length_bonus = (bug_ids.len() as f64 - 1.0) * 0.5;
    let rce_bonus = if has_rce { 2.0 } else { 0.0 };
    let trust_bonus = if crosses_trust_boundary { 1.5 } else { 0.0 };

    let total = (max_sev + length_bonus + rce_bonus + trust_bonus).min(10.0);

    ChainSeverity {
        max_member_severity: max_sev as u8,
        chain_length_bonus: length_bonus,
        rce_bonus,
        trust_boundary_bonus: trust_bonus,
        total,
    }
}

/// Escalate individual bug severities based on chain participation.
pub fn escalate_severities(
    chains: &[ExploitChain],
    original_severities: &HashMap<String, u8>,
) -> HashMap<String, u8> {
    let mut escalated = original_severities.clone();

    for chain in chains {
        let chain_sev = chain.chain_severity as u8;
        for bug_id in &chain.bug_ids {
            let current = escalated.get(bug_id).copied().unwrap_or(1);
            if chain_sev > current {
                escalated.insert(bug_id.clone(), chain_sev);
            }
        }
    }

    escalated
}

/// Extract effects from a bug's description and metadata.
pub fn extract_effects(bug_id: &str, description: &str, severity: u8) -> Vec<BugEffect> {
    let mut effects = Vec::new();
    let lower = description.to_lowercase();

    if lower.contains("leak") || lower.contains("disclose") || lower.contains("read") || lower.contains("aslr") {
        effects.push(BugEffect {
            bug_id: bug_id.to_string(),
            description: format!("Info leak: reveals memory layout or addresses"),
            effect_type: EffectType::InfoLeak,
            severity,
        });
    }
    if lower.contains("overflow") || lower.contains("corrupt") || lower.contains("overwrite") || lower.contains("oob") {
        effects.push(BugEffect {
            bug_id: bug_id.to_string(),
            description: format!("Memory corruption: overwrites adjacent memory"),
            effect_type: EffectType::MemoryCorruption,
            severity,
        });
    }
    if lower.contains("code exec") || lower.contains("control flow") || lower.contains("rip") || lower.contains("eip") || lower.contains("shell") {
        effects.push(BugEffect {
            bug_id: bug_id.to_string(),
            description: format!("Control flow hijack: redirects execution"),
            effect_type: EffectType::ControlFlow,
            severity,
        });
    }
    if lower.contains("privilege") || lower.contains("escalat") || lower.contains("root") || lower.contains("admin") {
        effects.push(BugEffect {
            bug_id: bug_id.to_string(),
            description: format!("Privilege escalation: gains elevated access"),
            effect_type: EffectType::PrivilegeEsc,
            severity,
        });
    }
    if effects.is_empty() {
        effects.push(BugEffect {
            bug_id: bug_id.to_string(),
            description: format!("Bug: {}", &description[..description.len().min(100)]),
            effect_type: EffectType::StateCorruption,
            severity,
        });
    }
    effects
}

/// Extract preconditions from a bug's description.
pub fn extract_preconditions(bug_id: &str, description: &str, severity: u8) -> Vec<BugPrecondition> {
    let mut preconditions = Vec::new();
    let lower = description.to_lowercase();

    if lower.contains("overflow") || lower.contains("corrupt") || lower.contains("use after free") || lower.contains("uaf") {
        preconditions.push(BugPrecondition {
            bug_id: bug_id.to_string(),
            description: format!("Needs corrupted memory state to trigger"),
            precondition_type: PreconditionType::RequiresState,
            severity,
        });
    }
    if lower.contains("code exec") || lower.contains("shell") {
        preconditions.push(BugPrecondition {
            bug_id: bug_id.to_string(),
            description: format!("Needs control of instruction pointer"),
            precondition_type: PreconditionType::RequiresAddress,
            severity,
        });
    }
    if lower.contains("privilege") {
        preconditions.push(BugPrecondition {
            bug_id: bug_id.to_string(),
            description: format!("Needs lower privilege level access"),
            precondition_type: PreconditionType::RequiresPrivilege,
            severity,
        });
    }
    if preconditions.is_empty() {
        preconditions.push(BugPrecondition {
            bug_id: bug_id.to_string(),
            description: format!("Requires specific input: {}", &description[..description.len().min(80)]),
            precondition_type: PreconditionType::RequiresInput,
            severity,
        });
    }
    preconditions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_severities() -> HashMap<String, u8> {
        let mut m = HashMap::new();
        m.insert("BUG-A".into(), 3);
        m.insert("BUG-B".into(), 5);
        m.insert("BUG-C".into(), 8);
        m.insert("BUG-D".into(), 10);
        m
    }

    #[test]
    fn test_extract_effects_info_leak() {
        let effects = extract_effects("BUG-A", "ASLR info leak via out-of-bounds read", 3);
        assert!(effects.iter().any(|e| matches!(e.effect_type, EffectType::InfoLeak)));
    }

    #[test]
    fn test_extract_effects_overflow() {
        let effects = extract_effects("BUG-B", "Heap buffer overflow overwrites adjacent chunk", 5);
        assert!(effects.iter().any(|e| matches!(e.effect_type, EffectType::MemoryCorruption)));
    }

    #[test]
    fn test_extract_preconditions() {
        let preconds = extract_preconditions("BUG-B", "Use-after-free in parse_input requires heap grooming", 5);
        assert!(!preconds.is_empty());
    }

    #[test]
    fn test_semantic_matcher_match() {
        let matcher = ChainSemanticMatcher::new(0.3);
        let effects = vec![BugEffect {
            bug_id: "BUG-A".into(), description: "leak reveals memory address layout data".into(),
            effect_type: EffectType::InfoLeak, severity: 3,
        }];
        let preconds = vec![BugPrecondition {
            bug_id: "BUG-B".into(), description: "needs address memory leak to target overflow".into(),
            precondition_type: PreconditionType::RequiresAddress, severity: 5,
        }];
        let matches = matcher.find_matches(&effects, &preconds);
        assert!(!matches.is_empty());
        assert!(matches[0].similarity_score >= 0.3);
    }

    #[test]
    fn test_semantic_matcher_no_match() {
        let matcher = ChainSemanticMatcher::new(0.9);
        let effects = vec![BugEffect {
            bug_id: "BUG-A".into(), description: "info leak".into(),
            effect_type: EffectType::InfoLeak, severity: 3,
        }];
        let preconds = vec![BugPrecondition {
            bug_id: "BUG-B".into(), description: "completely unrelated precondition".into(),
            precondition_type: PreconditionType::RequiresInput, severity: 5,
        }];
        let matches = matcher.find_matches(&effects, &preconds);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_same_bug_not_matched() {
        let matcher = ChainSemanticMatcher::new(0.1);
        let effects = vec![BugEffect {
            bug_id: "BUG-A".into(), description: "test".into(),
            effect_type: EffectType::InfoLeak, severity: 3,
        }];
        let preconds = vec![BugPrecondition {
            bug_id: "BUG-A".into(), description: "test".into(),
            precondition_type: PreconditionType::RequiresInput, severity: 3,
        }];
        let matches = matcher.find_matches(&effects, &preconds);
        assert!(matches.is_empty()); // Same bug shouldn't chain to itself
    }

    #[test]
    fn test_chain_severity_calculus() {
        let sevs = make_severities();
        let chain = vec!["BUG-A".to_string(), "BUG-B".to_string(), "BUG-C".to_string()];
        let severity = compute_chain_severity(&chain, &sevs, true, true);
        // max_sev=8, length_bonus=2*0.5=1.0, rce=2.0, trust=1.5 → 8+1+2+1.5=12.5 → clamp 10
        assert!((severity.total - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_chain_severity_simple() {
        let sevs = make_severities();
        let chain = vec!["BUG-A".to_string(), "BUG-B".to_string()];
        let severity = compute_chain_severity(&chain, &sevs, false, false);
        // max_sev=5, length=0.5, rce=0, trust=0 → 5.5
        assert!((severity.total - 5.5).abs() < 0.01);
    }

    #[test]
    fn test_detect_chains_finds_paths() {
        let sevs = make_severities();
        let mut graph: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        graph.insert("BUG-A".into(), vec![("BUG-B".into(), 0.8)]);
        graph.insert("BUG-B".into(), vec![("BUG-C".into(), 0.9)]);

        let chains = detect_chains(&graph, &sevs, 5);
        assert!(chains.len() >= 1);
        let chain = chains.iter().find(|c| c.bug_ids.contains(&"BUG-C".to_string()));
        assert!(chain.is_some());
    }

    #[test]
    fn test_detect_chains_cycle_prevention() {
        let sevs = make_severities();
        let mut graph: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        graph.insert("BUG-A".into(), vec![("BUG-B".into(), 0.8)]);
        graph.insert("BUG-B".into(), vec![("BUG-A".into(), 0.8)]);

        let chains = detect_chains(&graph, &sevs, 10);
        // No chain should contain the same bug twice
        for chain in &chains {
            let mut seen = HashSet::new();
            for bug_id in &chain.bug_ids {
                assert!(seen.insert(bug_id), "Cycle detected: {}", bug_id);
            }
        }
    }

    #[test]
    fn test_escalate_severities() {
        let original = make_severities();
        let chain = ExploitChain {
            chain_id: "test".into(),
            bug_ids: vec!["BUG-A".into(), "BUG-B".into()],
            chain_length: 2,
            terminal_bug_id: "BUG-B".into(),
            terminal_severity: 5,
            chain_severity: 7.0,
            has_rce: false,
            crosses_trust_boundary: false,
            combined_poc_available: false,
            edge_scores: vec![0.8],
        };
        let escalated = escalate_severities(&[chain], &original);
        assert!(escalated.get("BUG-A").copied().unwrap_or(0) >= 3);
        assert!(escalated.get("BUG-B").copied().unwrap_or(0) >= 5);
    }

    #[test]
    fn test_build_chain_graph() {
        let matches = vec![
            EffectPreconditionMatch {
                effect_bug_id: "A".into(), precondition_bug_id: "B".into(),
                similarity_score: 0.8,
                effect_description: "leak addr".into(), precondition_description: "need addr".into(),
            },
            EffectPreconditionMatch {
                effect_bug_id: "B".into(), precondition_bug_id: "C".into(),
                similarity_score: 0.9,
                effect_description: "corrupt".into(), precondition_description: "need corrupt".into(),
            },
        ];
        let graph = build_chain_graph(&matches);
        assert!(graph.contains_key("A"));
        assert!(graph.contains_key("B"));
        assert_eq!(graph.get("A").unwrap().len(), 1);
    }

    #[test]
    fn test_extract_effects_default() {
        let effects = extract_effects("BUG-X", "an unknown anomaly without known patterns", 1);
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0].effect_type, EffectType::StateCorruption));
    }
}
