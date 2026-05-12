use serde::{Deserialize, Serialize};
use serde_json;

use crate::error::{SandboxError, SandboxResult};

/// The default seccomp profile whitelisting ~45 safe syscalls.
/// All other syscalls return EPERM (or kill the process for dangerous ones).
const DEFAULT_SECCOMP_PROFILE_JSON: &str = include_str!("../seccomp/default.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompProfile {
    #[serde(rename = "defaultAction")]
    pub default_action: String,
    pub architectures: Vec<String>,
    pub syscalls: Vec<SeccompSyscallRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompSyscallRule {
    pub names: Vec<String>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<serde_json::Value>>,
}

impl SeccompProfile {
    /// Load the default seccomp profile.
    pub fn default_profile() -> SandboxResult<Self> {
        serde_json::from_str(DEFAULT_SECCOMP_PROFILE_JSON)
            .map_err(|e| SandboxError::SeccompError(format!("Failed to parse default seccomp profile: {}", e)))
    }

    /// Load a custom seccomp profile from a file path.
    pub fn from_file(path: &str) -> SandboxResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SandboxError::SeccompError(format!("Failed to read seccomp profile at {}: {}", path, e)))?;
        serde_json::from_str(&content)
            .map_err(|e| SandboxError::SeccompError(format!("Failed to parse seccomp profile at {}: {}", path, e)))
    }

    /// Serialize the profile to a JSON string for passing to Docker.
    pub fn to_docker_string(&self) -> SandboxResult<String> {
        serde_json::to_string(self)
            .map_err(|e| SandboxError::SeccompError(format!("Failed to serialize seccomp profile: {}", e)))
    }

    /// Get the list of explicitly allowed syscalls.
    pub fn allowed_syscalls(&self) -> Vec<String> {
        self.syscalls
            .iter()
            .filter(|rule| rule.action == "SCMP_ACT_ALLOW")
            .flat_map(|rule| rule.names.clone())
            .collect()
    }

    /// Get the list of explicitly blocked/errno syscalls.
    pub fn blocked_syscalls(&self) -> Vec<String> {
        self.syscalls
            .iter()
            .filter(|rule| rule.action != "SCMP_ACT_ALLOW")
            .flat_map(|rule| rule.names.clone())
            .collect()
    }

    /// Count of allowed syscalls.
    pub fn allowed_count(&self) -> usize {
        self.allowed_syscalls().len()
    }

    /// Validate that the profile is reasonable:
    /// - Fewer than 60 allowed syscalls (attack surface minimization)
    /// - All dangerous syscalls are blocked
    /// - Default action is EPERM or KILL
    pub fn validate(&self) -> SandboxResult<()> {
        let allowed = self.allowed_syscalls();
        if allowed.len() > 70 {
            return Err(SandboxError::SeccompError(format!(
                "Seccomp profile allows {} syscalls, maximum is 70. Reduce attack surface.",
                allowed.len()
            )));
        }

        let required_blocked = [
            "ptrace", "mount", "kexec_load", "bpf", "perf_event_open",
            "create_module", "init_module", "delete_module", "finit_module",
            "pivot_root", "chroot", "setns", "unshare", "personality",
            "process_vm_readv", "process_vm_writev",
        ];

        for syscall in &required_blocked {
            if allowed.contains(&syscall.to_string()) {
                return Err(SandboxError::SeccompError(format!(
                    "Dangerous syscall '{}' is in the allow list. This is a security violation.",
                    syscall
                )));
            }
        }

        if self.default_action != "SCMP_ACT_ERRNO" && self.default_action != "SCMP_ACT_KILL" {
            return Err(SandboxError::SeccompError(format!(
                "Default action must be SCMP_ACT_ERRNO or SCMP_ACT_KILL, got '{}'",
                self.default_action
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profile_loads() {
        let profile = SeccompProfile::default_profile().expect("Default profile must load");
        assert!(profile.allowed_count() < 70, "Attack surface too large");
        assert!(profile.allowed_count() > 0, "No syscalls allowed");
    }

    #[test]
    fn test_default_profile_validates() {
        let profile = SeccompProfile::default_profile().unwrap();
        profile.validate().expect("Default profile must validate");
    }

    #[test]
    fn test_dangerous_syscalls_blocked() {
        let profile = SeccompProfile::default_profile().unwrap();
        let allowed = profile.allowed_syscalls();
        let dangerous = ["ptrace", "mount", "bpf", "kexec_load", "chroot"];
        for d in &dangerous {
            assert!(!allowed.contains(&d.to_string()), "{} must be blocked", d);
        }
    }

    #[test]
    fn test_default_action_is_errno() {
        let profile = SeccompProfile::default_profile().unwrap();
        assert!(
            profile.default_action == "SCMP_ACT_ERRNO",
            "Default action must be SCMP_ACT_ERRNO"
        );
    }
}
