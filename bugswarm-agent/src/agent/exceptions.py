"""Bug Swarm Exception Hierarchy.

Every exception is typed by origin (infrastructure vs security vs configuration)
so callers can log at the correct severity and handle each class appropriately.

Security exceptions are logged at WARN.
Infrastructure exceptions are logged at ERROR.
"""

from __future__ import annotations

import structlog

logger = structlog.get_logger(__name__)


class BugSwarmError(Exception):
    """Base for all Bug Swarm exceptions."""

    severity: str = "ERROR"

    def log(self, **context: object) -> None:
        match self.severity:
            case "CRITICAL":
                logger.critical(str(self), **context)
            case "WARN":
                logger.warning(str(self), **context)
            case _:
                logger.error(str(self), **context)


# ── Infrastructure Errors (logged at ERROR) ──


class InfrastructureError(BugSwarmError):
    """Base for infrastructure and service errors."""

    severity = "ERROR"


class CPGError(InfrastructureError):
    """CPG query or service error."""


class CPGTimeoutError(CPGError):
    """CPG query exceeded timeout."""


class CPGConnectionError(CPGError):
    """Unable to connect to CPG service."""


class CPGResponseError(CPGError):
    """CPG returned invalid or unparseable response."""


class SandboxError(InfrastructureError):
    """Sandbox execution error."""


class SandboxTimeoutError(SandboxError):
    """Sandbox execution exceeded timeout."""


class SandboxInfrastructureError(SandboxError):
    """Sandbox infrastructure failure (connection, OS)."""


class SandboxExecutionError(SandboxError):
    """Sandbox execution failed for another reason."""


class LLMError(InfrastructureError):
    """LLM API communication error."""


class LLMTimeoutError(LLMError):
    """LLM API call exceeded timeout."""


class LLMInfrastructureError(LLMError):
    """LLM infrastructure failure (connection, OS)."""


class LLMInvalidRequestError(LLMError):
    """LLM request rejected as invalid."""


class FilesystemError(InfrastructureError):
    """Filesystem I/O error."""


class FileAbsentError(FilesystemError):
    """File does not exist."""


class FileAccessDeniedError(FilesystemError):
    """Permission denied when accessing file."""


class FileReadFailedError(FilesystemError):
    """Failed to read file for another reason."""


class ToolDispatchError(InfrastructureError):
    """Tool dispatch error."""


class ToolFormatError(ToolDispatchError):
    """Tool call could not be parsed from LLM output."""


class ToolJSONParseError(ToolDispatchError):
    """Tool JSON payload could not be decoded."""


class ToolExecutionError(ToolDispatchError):
    """Tool execution failed for an unexpected reason."""


# ── Security Errors (logged at WARN) ──


class SecurityError(BugSwarmError):
    """Base for all security-detection errors.

    These indicate a security event (PII found, escape attempt, etc.)
    and should be logged at WARN level to avoid alarm fatigue
    while still being visible in structured logs.
    """

    severity = "WARN"


class PIIError(SecurityError):
    """PII or sensitive data detected in content."""


class EscapeAttemptError(SecurityError):
    """Sandbox escape pattern detected in PoC code."""


class PromptInjectionError(SecurityError):
    """Prompt injection pattern detected in code context."""


class PathTraversalError(SecurityError):
    """Path traversal attempt blocked."""


# ── Configuration Errors (logged at ERROR) ──


class ConfigError(BugSwarmError):
    """Configuration or setup error."""

    severity = "ERROR"


# ── Classifiers — raw → typed exception ──


def classify_cpg_error(exc: Exception) -> CPGError:
    """Map a raw exception from CPG operations to our typed hierarchy."""
    if isinstance(exc, TimeoutError):
        return CPGTimeoutError(str(exc))
    if isinstance(exc, (ConnectionError, OSError)):
        return CPGConnectionError(str(exc))
    return CPGResponseError(str(exc))


def classify_sandbox_error(exc: Exception) -> SandboxError:
    """Map a raw exception from sandbox operations to our typed hierarchy."""
    if isinstance(exc, TimeoutError):
        return SandboxTimeoutError(str(exc))
    if isinstance(exc, (ConnectionError, OSError)):
        return SandboxInfrastructureError(str(exc))
    return SandboxExecutionError(str(exc))


def classify_llm_error(exc: Exception) -> LLMError:
    """Map a raw exception from LLM API calls to our typed hierarchy."""
    if isinstance(exc, TimeoutError):
        return LLMTimeoutError(str(exc))
    if isinstance(exc, (ConnectionError, OSError)):
        return LLMInfrastructureError(str(exc))
    if isinstance(exc, ValueError):
        return LLMInvalidRequestError(str(exc))
    return LLMError(str(exc))


def classify_filesystem_error(exc: Exception) -> FilesystemError:
    """Map a raw exception from filesystem operations to our typed hierarchy."""
    if isinstance(exc, FileNotFoundError):
        return FileAbsentError(str(exc))
    if isinstance(exc, PermissionError):
        return FileAccessDeniedError(str(exc))
    return FileReadFailedError(str(exc))
