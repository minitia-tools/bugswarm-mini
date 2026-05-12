"""CPG Service Client — persistent abstraction over the Code Property Graph.

Currently uses subprocess (daemon mode deferred to Rust R2).
When daemon mode is implemented in Rust, swap subprocess for Unix socket/gRPC.
"""

from __future__ import annotations

import asyncio
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import structlog

logger = structlog.get_logger(__name__)

# Path to CPG binary — configurable, not hardcoded.
DEFAULT_CPG_BINARY = "bugswarm-cpg"


@dataclass
class CPGStats:
    total_nodes: int = 0
    total_edges: int = 0
    total_files: int = 0
    total_functions: int = 0
    sources: int = 0
    sinks: int = 0
    taint_paths: int = 0
    by_language: dict[str, dict] = field(default_factory=dict)

    @classmethod
    def from_json(cls, data: dict) -> CPGStats:
        return cls(
            total_nodes=data.get("total_nodes", 0),
            total_edges=data.get("total_edges", 0),
            total_files=data.get("total_files", 0),
            total_functions=data.get("total_functions", 0),
            sources=data.get("sources", 0),
            sinks=data.get("sinks", 0),
            taint_paths=data.get("taint_paths", 0),
            by_language=data.get("by_language", {}),
        )


@dataclass
class TaintPathInfo:
    source: str
    sink: str
    length: int
    sanitized: bool
    confidence: float
    path_nodes: list[str] = field(default_factory=list)


@dataclass
class CallPathInfo:
    caller: str
    callee: str
    path: list[str]
    length: int


class CPGClient:
    """Client for the CPG service.

    Abstracts subprocess (current) or socket/gRPC (future).
    """

    def __init__(self, binary: str = DEFAULT_CPG_BINARY, timeout_secs: float = 30.0):
        self.binary = binary
        self.timeout = timeout_secs
        self._healthy: bool | None = None

    async def _run(self, *args: str) -> tuple[str, str, int]:
        """Run CPG binary and return (stdout, stderr, returncode)."""
        cmd = [self.binary] + list(args)
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        try:
            stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=self.timeout)
            return stdout.decode(), stderr.decode(), proc.returncode or 0
        except asyncio.TimeoutError:
            proc.kill()
            await proc.wait()
            raise

    # ─── Queries ───

    async def stats(self, repo_path: Path) -> CPGStats:
        """Get CPG statistics for a repository."""
        stdout, stderr, rc = await self._run("stats", "--repo", str(repo_path))
        if rc != 0:
            raise RuntimeError(f"CPG stats failed: {stderr[:200]}")
        return CPGStats.from_json(json.loads(stdout))

    async def taint_paths(self, repo_path: Path,
                           source: str | None = None,
                           sink: str | None = None) -> list[TaintPathInfo]:
        """Query taint paths from sources to sinks."""
        args = ["taint", "--repo", str(repo_path)]
        if source:
            args.extend(["--source", source])
        if sink:
            args.extend(["--sink", sink])
        stdout, stderr, rc = await self._run(*args)
        if rc != 0:
            logger.warning("cpg_taint_warning", stderr=stderr[:200])
        return self._parse_taint_paths(stdout)

    async def call_paths(self, repo_path: Path,
                         from_func: str, to_func: str) -> list[CallPathInfo]:
        """Find call paths between two functions."""
        stdout, stderr, rc = await self._run(
            "call-path", "--repo", str(repo_path),
            "--from", from_func, "--to", to_func,
        )
        if rc != 0:
            return []
        return self._parse_call_paths(stdout)

    async def index(self, repo_path: Path, prune: bool = False) -> CPGStats:
        """Index a repository and return stats."""
        args = ["index", "--repo", str(repo_path)]
        if prune:
            args.append("--prune")
        stdout, stderr, rc = await self._run(*args)
        if rc != 0:
            raise RuntimeError(f"CPG index failed: {stderr[:200]}")
        return CPGStats.from_json(json.loads(stdout))

    async def test_file(self, file_path: Path) -> CPGStats:
        """Test-parse a single file."""
        stdout, stderr, rc = await self._run("test", "--file", str(file_path))
        return CPGStats.from_json(json.loads(stdout)) if stdout.strip() else CPGStats()

    # ─── Health Check ───

    async def health_check(self) -> bool:
        """Check if CPG binary is responsive."""
        try:
            await self._run("--help")
            self._healthy = True
            return True
        except Exception:
            self._healthy = False
            return False

    # ─── Output Parsers ───

    @staticmethod
    def _parse_taint_paths(output: str) -> list[TaintPathInfo]:
        paths = []
        current = None
        for line in output.split('\n'):
            line = line.strip()
            if line.startswith("Path "):
                if current:
                    paths.append(current)
                current = TaintPathInfo(source="", sink="", length=0, sanitized=False, confidence=0.0)
                # "Path 1: source_name → sink_name (length=N, sanitized=..., confidence=...)"
                rest = line.split(": ", 1)[-1] if ": " in line else line
                if "→" in rest:
                    parts = rest.split("→")
                    current.source = parts[0].strip()
                    sink_part = parts[1].split("(")[0].strip() if "(" in parts[1] else parts[1].strip()
                    current.sink = sink_part
                if "length=" in line:
                    try:
                        current.length = int(line.split("length=")[1].split(",")[0].split(")")[0])
                    except (ValueError, IndexError):
                        pass
                current.sanitized = "sanitized=true" in line
                if "confidence=" in line:
                    try:
                        current.confidence = float(line.split("confidence=")[1].split(")")[0].split(",")[0])
                    except (ValueError, IndexError):
                        pass
            elif line.startswith("  ") and current:
                current.path_nodes.append(line.strip())
        if current:
            paths.append(current)
        return paths

    @staticmethod
    def _parse_call_paths(output: str) -> list[CallPathInfo]:
        paths = []
        for line in output.split('\n'):
            if "Path " in line and "length=" in line:
                try:
                    length = int(line.split("length=")[1].split(")")[0])
                    paths.append(CallPathInfo(caller="", callee="", path=[], length=length))
                except (ValueError, IndexError):
                    pass
        return paths
