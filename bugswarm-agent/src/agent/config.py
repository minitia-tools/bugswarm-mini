"""Unified BugSwarm Configuration — enterprise config management."""

from __future__ import annotations

import os
from dataclasses import dataclass


@dataclass
class UnifiedConfig:
    schema_version: int = 1
    log_level: str = "info"
    sandbox_socket: str = "/var/run/bugswarm/sandbox.sock"
    sandbox_pid_file: str = "/var/run/bugswarm/sandbox.pid"
    sandbox_http_port: int = 8080
    sandbox_docker_image: str = "bugswarm/sandbox-base:1.0.0"
    sandbox_memory_limit_mb: int = 256
    sandbox_timeout_secs: int = 30
    cpg_socket: str = "/var/run/bugswarm/cpg.sock"
    cpg_pid_file: str = "/var/run/bugswarm/cpg.pid"
    cpg_http_port: int = 8081
    evidence_socket: str = "/var/run/bugswarm/evidence.sock"
    evidence_pid_file: str = "/var/run/bugswarm/evidence.pid"
    evidence_port: int = 8082
    evidence_state_dir: str = "/var/lib/bugswarm"
    storage_state_dir: str = "/var/lib/bugswarm"
    storage_chroma_persist_dir: str = "/var/lib/bugswarm/chroma"

    @classmethod
    def load(cls, path: str = "/etc/bugswarm/config.yaml") -> UnifiedConfig:
        if not os.path.exists(path):
            return cls()

        try:
            import yaml
        except ImportError:
            return cls()

        with open(path) as f:
            raw = yaml.safe_load(f) or {}

        schema_version = raw.get("schema_version", 1)
        if schema_version != 1:
            raise ValueError(
                f"Unsupported schema_version {schema_version} in {path}. "
                f"Expected 1. Please migrate or update your config."
            )

        logging_cfg = raw.get("logging", {})
        log_level = os.getenv("BGSWARM_LOG_LEVEL", logging_cfg.get("level", "info"))

        daemons = raw.get("daemons", {})
        sandbox = daemons.get("sandbox", {})
        cpg = daemons.get("cpg", {})
        evidence = daemons.get("evidence", {})

        storage = raw.get("storage", {})

        return cls(
            schema_version=schema_version,
            log_level=log_level,
            sandbox_socket=os.getenv("BGSWARM_SANDBOX_SOCKET", sandbox.get("socket", cls.sandbox_socket)),
            sandbox_pid_file=os.getenv("BGSWARM_SANDBOX_PID_FILE", sandbox.get("pid_file", cls.sandbox_pid_file)),
            sandbox_http_port=int(
                os.getenv("BGSWARM_SANDBOX_HTTP_PORT", sandbox.get("http_port", cls.sandbox_http_port))
            ),
            sandbox_docker_image=os.getenv(
                "BGSWARM_SANDBOX_DOCKER_IMAGE", sandbox.get("docker_image", cls.sandbox_docker_image)
            ),
            sandbox_memory_limit_mb=int(
                os.getenv(
                    "BGSWARM_SANDBOX_MEMORY_LIMIT_MB", sandbox.get("memory_limit_mb", cls.sandbox_memory_limit_mb)
                )
            ),
            sandbox_timeout_secs=int(
                os.getenv("BGSWARM_SANDBOX_TIMEOUT_SECS", sandbox.get("timeout_secs", cls.sandbox_timeout_secs))
            ),
            cpg_socket=os.getenv("BGSWARM_CPG_SOCKET", cpg.get("socket", cls.cpg_socket)),
            cpg_pid_file=os.getenv("BGSWARM_CPG_PID_FILE", cpg.get("pid_file", cls.cpg_pid_file)),
            cpg_http_port=int(os.getenv("BGSWARM_CPG_HTTP_PORT", cpg.get("http_port", cls.cpg_http_port))),
            evidence_socket=os.getenv("BGSWARM_EVIDENCE_SOCKET", evidence.get("socket", cls.evidence_socket)),
            evidence_pid_file=os.getenv("BGSWARM_EVIDENCE_PID_FILE", evidence.get("pid_file", cls.evidence_pid_file)),
            evidence_port=int(os.getenv("BGSWARM_EVIDENCE_HTTP_PORT", evidence.get("http_port", cls.evidence_port))),
            evidence_state_dir=os.getenv(
                "BGSWARM_EVIDENCE_STATE_DIR", evidence.get("state_dir", cls.evidence_state_dir)
            ),
            storage_state_dir=os.getenv("BGSWARM_STORAGE_STATE_DIR", storage.get("state_dir", cls.storage_state_dir)),
            storage_chroma_persist_dir=os.getenv(
                "BGSWARM_STORAGE_CHROMA_PERSIST_DIR", storage.get("chroma_persist_dir", cls.storage_chroma_persist_dir)
            ),
        )
