"""Pre-flight Validation — check everything before spending a single API token."""

from __future__ import annotations

import os
import shutil

import structlog

from agent.cpg_client import CPGClient
from agent.sandbox_client import SandboxClient

from .config import CLIConfig

logger = structlog.get_logger(__name__)

REQUIRED_BINARIES = ["docker"]
REQUIRED_ENV_VARS = ["DEEPSEEK_API_KEY"]  # plus OPENAI_API_KEY, ANTHROPIC_API_KEY, etc.


class PreFlight:
    """Fail-fast validation. Run before any LLM call."""

    @classmethod
    async def check_all(cls, config: CLIConfig) -> list[str]:
        """Returns list of warnings. Empty list = all checks passed."""
        warnings = []

        # 1. Required binaries on PATH
        for binary in REQUIRED_BINARIES:
            if not shutil.which(binary):
                warnings.append(f"Binary not found on PATH: {binary}")

        # 2. CPG binary check
        try:
            cpg = CPGClient(binary=config.cpg_binary)
            if not await cpg.health_check():
                warnings.append(f"CPG binary not responsive: {config.cpg_binary}")
        except Exception as e:
            warnings.append(f"CPG binary check failed: {e}")

        # 3. Sandbox binary check
        try:
            sandbox = SandboxClient(binary=config.sandbox_binary)
            if not await sandbox.health_check():
                warnings.append(f"Sandbox binary not responsive: {config.sandbox_binary}")
        except Exception as e:
            warnings.append(f"Sandbox binary check failed: {e}")

        # 4. API key check
        provider_key = f"{config.provider.upper()}_API_KEY"
        if not os.getenv(provider_key) and not os.getenv("DEEPSEEK_API_KEY"):
            warnings.append(f"No API key found. Set {provider_key} or DEEPSEEK_API_KEY")

        # 5. Repo exists
        if not config.repo.exists():
            warnings.append(f"Repository path does not exist: {config.repo}")

        # 6. Repo is not empty
        if config.repo.exists() and not any(config.repo.iterdir()):
            warnings.append(f"Repository is empty: {config.repo}")

        return warnings

    @classmethod
    def print_warnings(cls, warnings: list[str]) -> None:
        """Print warnings in a user-friendly format."""
        if not warnings:
            return
        print("\n⚠  Pre-flight warnings:")
        for w in warnings:
            print(f"   • {w}")
        print()
