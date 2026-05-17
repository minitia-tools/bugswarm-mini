//! Cross-crate integration tests — verify sandbox ↔ evidence ↔ CPG integration.

/// Test that evidence types can be imported from the evidence crate.
#[test]
fn test_evidence_types_accessible() {
    // Verify the evidence crate's types are reachable
    use bugswarm_evidence::types::{NodeKind, EdgeKind};
    assert_ne!(NodeKind::Claim, NodeKind::ConfirmedBug);
    assert_ne!(EdgeKind::Supports, EdgeKind::Contradicts);
}

/// Test that sandbox → evidence dependency works.
#[test]
fn test_sandbox_evidence_dep_compiles() {
    // This test just verifies the dependency compiles
    use bugswarm_evidence::trigger::TriggerDimension;
    let dim = TriggerDimension::Input;
    assert!(dim.weight() > 0.0);
}
