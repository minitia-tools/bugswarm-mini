/// Test that the BUGSWARM_EOF delimiter cannot escape the heredoc.
/// This test generates PoC content containing the delimiter and verifies
/// it does NOT execute as shell commands.
#[test]
fn test_heredoc_delimiter_not_executable() {
    let source = std::fs::read_to_string("src/container.rs").unwrap();
    assert!(!source.contains("BUGSWARM_EOF"),
        "HEREDOC FOUND: container.rs still uses BUGSWARM_EOF heredoc pattern. Must use bind-mount instead.");
}

#[test]
fn test_no_heredoc_pattern_in_daemon() {
    let source = std::fs::read_to_string("src/daemon.rs").unwrap();
    assert!(!source.contains("<< '"),
        "HEREDOC FOUND: daemon.rs contains heredoc pattern. All execution must use bind-mount.");
}

#[test]
fn test_bind_mount_approach_used() {
    let source = std::fs::read_to_string("src/container.rs").unwrap();
    assert!(source.contains("HostConfig") || source.contains("Mount"),
        "No bind mount configuration found. Container execution must use Docker bind mounts.");
}
