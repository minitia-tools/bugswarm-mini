use thiserror::Error;

#[derive(Error, Debug)]
pub enum SandboxError {
    #[error("Docker daemon unavailable: {0}")]
    DockerUnavailable(String),

    #[error("Container execution failed: {0}")]
    ContainerExecution(String),

    #[error("Seccomp profile generation failed: {0}")]
    SeccompError(String),

    #[error("Sandbox escape attempt detected: {pattern}")]
    EscapeAttempt { pattern: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Bollard error: {0}")]
    Bollard(#[from] bollard::errors::Error),

    #[error("{0}")]
    Other(String),
}

pub type SandboxResult<T> = Result<T, SandboxError>;
