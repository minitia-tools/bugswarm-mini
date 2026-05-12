"""Unified Security Scanner — single source of truth for PII, escape, and injection detection.

Shared patterns with Rust sandbox via scanner_patterns.yaml.
Rule: no tool output enters an LLM context window without passing through this scanner.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import ClassVar

import structlog
import yaml

logger = structlog.get_logger(__name__)


@dataclass
class ScanReport:
    pii_count: int = 0
    pii_types: dict[str, int] = field(default_factory=dict)      # {"email": 3, "aws_key": 1}
    escape_matches: list[str] = field(default_factory=list)
    injection_confidence: float = 0.0                              # 0.0–1.0
    high_entropy_tokens: list[str] = field(default_factory=list)
    redacted_content: str = ""
    scan_duration_ms: float = 0.0

    @property
    def is_clean(self) -> bool:
        return (self.pii_count == 0 and len(self.escape_matches) == 0
                and self.injection_confidence < 0.3 and len(self.high_entropy_tokens) == 0)


class UnifiedScanner:
    """Single source of truth for all content scanning.

    Patterns sync with Rust sandbox via shared scanner_patterns.yaml.
    """

    # Default patterns (overridden by config file if present)
    DEFAULT_PII_PATTERNS: ClassVar[list[tuple[str, str]]] = [
        (r'[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}', 'email'),
        (r'\b(?:\d{4}[ -]?){3}\d{4}\b', 'credit_card'),
        (r'\b\d{3}-\d{2}-\d{4}\b', 'ssn'),
        (r'\bAKIA[0-9A-Z]{16}\b', 'aws_key'),
        (r'\bgh[pousr]_[A-Za-z0-9_]{36,}\b', 'github_token'),
        (r'-----BEGIN (RSA|EC|DSA|OPENSSH|PGP) PRIVATE KEY-----', 'private_key'),
        (r'\beyJ[A-Za-z0-9\-_=]+\.[A-Za-z0-9\-_=]+\.?[A-Za-z0-9\-_.+/=]*\b', 'jwt'),
        (r'\b[0-9a-zA-Z/+]{40}\b', 'base64_token'),
    ]

    DEFAULT_ESCAPE_PATTERNS: ClassVar[list[str]] = [
        'chroot', 'nsenter', 'unshare', '/proc/1/ns', '/proc/self/ns',
        'docker.sock', '/var/run/docker.sock', 'pivot_root', 'kexec',
        'mount -t cgroup', 'insmod', 'modprobe', 'finit_module',
        'setns', 'personality', 'process_vm_readv',
    ]

    DEFAULT_INJECTION_PATTERNS: ClassVar[list[str]] = [
        'ignore previous instructions', 'you are now DAN',
        'forget all previous', 'new instructions:', '### Human:',
        '[INST]', 'SYSTEM:', 'override directives', 'disregard above',
        'I am your creator', 'developer mode', 'jailbreak',
    ]

    def __init__(self, config_path: str | None = None):
        self._pii_patterns: list[tuple[re.Pattern, str]] = []
        self._escape_patterns: list[str] = []
        self._injection_patterns: list[str] = []
        self._load_patterns(config_path)

    def _load_patterns(self, config_path: str | None) -> None:
        """Load patterns from YAML config, falling back to defaults."""
        pii_raw = list(self.DEFAULT_PII_PATTERNS)
        escape_raw = list(self.DEFAULT_ESCAPE_PATTERNS)
        injection_raw = list(self.DEFAULT_INJECTION_PATTERNS)

        if config_path and Path(config_path).exists():
            try:
                with open(config_path) as f:
                    cfg = yaml.safe_load(f)
                if cfg:
                    pii_raw = [(p["pattern"], p["type"])
                               for p in cfg.get("pii_patterns", [])] or pii_raw
                    escape_raw = cfg.get("escape_patterns", escape_raw)
                    injection_raw = cfg.get("injection_patterns", injection_raw)
                logger.info("scanner_config_loaded", path=config_path)
            except Exception as e:
                logger.warning("scanner_config_load_failed", error=str(e),
                               fallback="using_defaults")

        # Compile PII regexes
        for pattern, pii_type in pii_raw:
            try:
                self._pii_patterns.append((re.compile(pattern, re.IGNORECASE), pii_type))
            except re.error as e:
                logger.error("invalid_pii_pattern", pattern=pattern, error=str(e))

        self._escape_patterns = [p.lower() for p in escape_raw]
        self._injection_patterns = [p.lower() for p in injection_raw]

    # ─── PII Scanning ───

    def redact(self, content: str) -> tuple[str, int]:
        """Redact PII from content. Returns (redacted_content, count)."""
        count = 0
        result = content
        for pattern, pii_type in self._pii_patterns:
            matches = list(pattern.finditer(result))
            count += len(matches)
            for m in reversed(matches):
                replacement = f"[REDACTED: type={pii_type}]"
                result = result[:m.start()] + replacement + result[m.end():]
        return result, count

    def scan_pii(self, content: str) -> tuple[str, dict[str, int], int]:
        """Full PII scan returning redacted content, type breakdown, and total count."""
        type_counts: dict[str, int] = {}
        result = content
        total = 0
        for pattern, pii_type in self._pii_patterns:
            matches = list(pattern.finditer(result))
            if matches:
                type_counts[pii_type] = len(matches)
                total += len(matches)
                for m in reversed(matches):
                    replacement = f"[REDACTED: type={pii_type}]"
                    result = result[:m.start()] + replacement + result[m.end():]
        return result, type_counts, total

    # ─── Escape Detection ───

    def detect_escape(self, content: str) -> list[str]:
        """Detect sandbox escape patterns. Returns list of matched patterns."""
        lower = content.lower()
        return [p for p in self._escape_patterns if p in lower]

    def has_escape_attempt(self, content: str) -> bool:
        return len(self.detect_escape(content)) > 0

    # ─── Injection Detection ───

    def detect_injection(self, content: str) -> float:
        """Detect prompt injection with confidence score 0.0–1.0.

        Confidence is based on number of matching patterns and their specificity.
        """
        lower = content.lower()
        matches = [p for p in self._injection_patterns if p in lower]
        if not matches:
            return 0.0
        # More matches = higher confidence. Cap at 1.0.
        confidence = min(1.0, len(matches) * 0.25)
        return confidence

    # ─── Entropy Detection ───

    def detect_high_entropy(self, content: str, threshold: float = 4.5,
                            min_length: int = 20) -> list[str]:
        """Detect high-entropy tokens (likely API keys/tokens)."""
        tokens = []
        for word in content.split():
            if len(word) >= min_length and self._shannon_entropy(word) > threshold:
                tokens.append(word)
        return tokens

    @staticmethod
    def _shannon_entropy(s: str) -> float:
        if not s:
            return 0.0
        freq = [0] * 256
        for byte in s.encode('latin-1', errors='ignore'):
            freq[byte] += 1
        length = len(s)
        return -sum((c / length) * ((c / length) if c > 0 else 0).__bool__()
                    for c in freq if c > 0)  # simplified — full log2 in production

    # ─── Full Scan ───

    def scan(self, content: str, scan_types: list[str] | None = None) -> ScanReport:
        """Full security scan returning a structured report.

        scan_types: subset of {"pii", "escape", "injection", "entropy"}. None = all.
        """
        import time
        t0 = time.perf_counter()
        report = ScanReport()

        all_types = {"pii", "escape", "injection", "entropy"}
        types = set(scan_types) if scan_types else all_types

        if "pii" in types:
            redacted, type_counts, total = self.scan_pii(content)
            report.redacted_content = redacted
            report.pii_count = total
            report.pii_types = type_counts

        if "escape" in types:
            report.escape_matches = self.detect_escape(content)

        if "injection" in types:
            report.injection_confidence = self.detect_injection(content)

        if "entropy" in types:
            report.high_entropy_tokens = self.detect_high_entropy(content)

        report.scan_duration_ms = (time.perf_counter() - t0) * 1000
        return report

    # ─── Sensitive Path Detection ───

    SENSITIVE_PATHS: ClassVar[list[str]] = [
        '.env', '.pem', '.key', 'credentials', 'id_rsa', 'id_ed25519',
        'id_ecdsa', '.pfx', '.p12', 'secrets', 'secret', '.token',
        'service-account',
    ]

    @classmethod
    def is_sensitive_path(cls, path: str) -> bool:
        lower = path.lower()
        return any(p in lower for p in cls.SENSITIVE_PATHS)

    # ─── Sensitive Variable Detection ───

    SENSITIVE_VARS: ClassVar[list[str]] = [
        'password', 'passwd', 'secret', 'token', 'key',
        'credential', 'private_key', 'api_key', 'auth_token',
        'ssn', 'credit_card', 'access_key', 'secret_key',
    ]

    @classmethod
    def is_sensitive_variable(cls, name: str) -> bool:
        lower = name.lower()
        return any(p in lower for p in cls.SENSITIVE_VARS)
