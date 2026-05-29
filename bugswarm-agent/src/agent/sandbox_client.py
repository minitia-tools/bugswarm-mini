"""Sandbox Service Client — persistent abstraction over the Execution Sandbox.

Currently uses subprocess (daemon mode deferred to Rust R2).
When daemon mode is implemented in Rust, swap subprocess for Unix socket/gRPC.
"""

from __future__ import annotations

import asyncio
import json
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

import structlog

from agent.cli.signals import register_temp_file

logger = structlog.get_logger(__name__)

DEFAULT_SANDBOX_BINARY = "bugswarm-sandbox"


@dataclass
class ExecutionReceipt:
    """Structured sandbox execution result."""

    execution_id: str = ""
    exit_code: int | None = None
    status: str = ""
    duration_secs: float = 0.0
    stdout_truncated: str = ""
    stderr_truncated: str = ""
    exception_type: str | None = None
    exception_message: str | None = None
    oom_killed: bool = False
    tainted: bool = False
    taint_reason: str | None = None
    pii_redactions: int = 0
    raw_receipt: dict | None = None

    @classmethod
    def from_json(cls, data: dict) -> ExecutionReceipt:
        return cls(
            execution_id=data.get("execution_id", ""),
            exit_code=data.get("exit_code"),
            status=data.get("status", ""),
            duration_secs=data.get("duration_secs", 0.0),
            stdout_truncated=data.get("stdout_truncated", ""),
            stderr_truncated=data.get("stderr_truncated", ""),
            exception_type=data.get("exception_type"),
            exception_message=data.get("exception_message"),
            oom_killed=data.get("memory_profile", {}).get("oom_killed", False),
            tainted=data.get("tainted", False),
            taint_reason=data.get("taint_reason"),
            pii_redactions=data.get("pii_redactions", 0),
            raw_receipt=data,
        )

    @property
    def is_success(self) -> bool:
        return self.exit_code == 0

    @property
    def is_bug_confirmed(self) -> bool:
        return self.exit_code is not None and self.exit_code != 0

    @property
    def is_timeout(self) -> bool:
        return self.status == "Timeout"

    def to_summary(self) -> dict:
        return {
            "execution_id": self.execution_id,
            "exit_code": self.exit_code,
            "status": self.status,
            "duration_secs": round(self.duration_secs, 3),
            "oom_killed": self.oom_killed,
            "exception": self.exception_type or "",
        }


@dataclass
class StatisticalResult:
    """Result of statistical re-execution for flaky bug detection."""

    total_runs: int = 0
    failures: int = 0
    passes: int = 0
    timeouts: int = 0
    oom_kills: int = 0
    failure_rate: float = 0.0
    confidence_interval_lower: float = 0.0
    confidence_interval_upper: float = 0.0
    is_significant: bool = False
    dominant_failure_mode: str | None = None
    top_exceptions: list[tuple[str, int]] = field(default_factory=list)
    top_crash_locations: list[tuple[str, int]] = field(default_factory=list)

    @classmethod
    def from_json(cls, data: dict) -> StatisticalResult:
        return cls(
            total_runs=data.get("total_runs", 0),
            failures=data.get("failures", 0),
            passes=data.get("passes", 0),
            timeouts=data.get("timeouts", 0),
            oom_kills=data.get("oom_kills", 0),
            failure_rate=data.get("failure_rate", 0.0),
            confidence_interval_lower=data.get("confidence_interval_lower", 0.0),
            confidence_interval_upper=data.get("confidence_interval_upper", 0.0),
            is_significant=data.get("is_significant", False),
            dominant_failure_mode=data.get("dominant_failure_mode"),
            top_exceptions=[(e[0], e[1]) for e in data.get("top_exceptions", [])],
            top_crash_locations=[(l[0], l[1]) for l in data.get("top_crash_locations", [])],
        )


class SandboxClient:
    """Client for the Sandbox daemon.

    Abstracts subprocess (current) or socket/gRPC (future).
    """

    def __init__(self, binary: str = DEFAULT_SANDBOX_BINARY, execution_timeout: float = 130.0):
        self.binary = binary
        self.execution_timeout = execution_timeout
        self._healthy: bool | None = None
        self._request_id: str | None = None

    def set_request_id(self, request_id: str) -> None:
        self._request_id = request_id

    async def _run(self, *args: str) -> tuple[str, str, int]:
        """Run sandbox binary and return (stdout, stderr, returncode)."""
        cmd = [self.binary] + list(args)
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        try:
            stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=self.execution_timeout)
            return stdout.decode(), stderr.decode(), proc.returncode or 0
        except TimeoutError:
            proc.kill()
            await proc.wait()
            raise

    def _extract_receipt(self, stdout: str) -> dict | None:
        """Extract the execution receipt from structured log output."""
        for line in stdout.split("\n"):
            line = line.strip()
            if line.startswith("{") and '"execution_id"' in line:
                try:
                    return json.loads(line)
                except json.JSONDecodeError:
                    continue
        try:
            return json.loads(stdout) if stdout.strip().startswith("{") else None
        except json.JSONDecodeError:
            return None

    # ─── Single Execution ───

    async def execute(self, poc_code: str, env: dict[str, str] | None = None, flaky: bool = False) -> ExecutionReceipt:
        """Execute a single PoC and return the receipt."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
            f.write(poc_code)
            poc_path = f.name
            register_temp_file(poc_path)

        try:
            args = ["execute", "--poc", poc_path]
            if flaky:
                args.append("--flaky")
            if env:
                for k, v in env.items():
                    args.extend(["--env", f"{k}={v}"])

            stdout, stderr, rc = await self._run(*args)
            receipt_data = self._extract_receipt(stdout)

            if receipt_data:
                return ExecutionReceipt.from_json(receipt_data)
            return ExecutionReceipt(
                exit_code=rc,
                status="executed",
                stdout_truncated=stdout[:500],
                stderr_truncated=stderr[:500],
            )
        except TimeoutError:
            return ExecutionReceipt(status="Timeout")
        except Exception as e:
            logger.error("sandbox_execution_failed", error=str(e)[:200])
            return ExecutionReceipt(status="Error", tainted=True, taint_reason=str(e)[:200])
        finally:
            try:
                Path(poc_path).unlink(missing_ok=True)
            except OSError:
                pass

    # ─── Statistical Execution ───

    async def execute_statistical(self, poc_code: str, count: int = 100) -> StatisticalResult:
        """Run statistical re-execution for flaky bug detection."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
            f.write(poc_code)
            poc_path = f.name
            register_temp_file(poc_path)

        try:
            stdout, stderr, rc = await self._run(
                "execute-statistical",
                "--poc",
                poc_path,
                "--count",
                str(count),
            )
            try:
                return StatisticalResult.from_json(json.loads(stdout))
            except json.JSONDecodeError:
                receipt = self._extract_receipt(stdout)
                if receipt:
                    return StatisticalResult(total_runs=count, failures=count)
                return StatisticalResult(total_runs=count, failures=0)
        finally:
            try:
                Path(poc_path).unlink(missing_ok=True)
            except OSError:
                pass

    # ─── Independent Re-execution ───

    async def independent_reexecute(self, poc_code: str, env: dict[str, str] | None = None) -> ExecutionReceipt:
        """Independently re-execute a PoC to verify a previous receipt."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
            f.write(poc_code)
            poc_path = f.name
            register_temp_file(poc_path)

        try:
            args = ["verify-receipt", "--poc", poc_path]
            if env:
                for k, v in env.items():
                    args.extend(["--env", f"{k}={v}"])
            stdout, stderr, rc = await self._run(*args)
            receipt_data = self._extract_receipt(stdout)
            if receipt_data:
                receipt = ExecutionReceipt.from_json(receipt_data)
                return receipt
            return ExecutionReceipt(exit_code=rc, status="verified")
        finally:
            try:
                Path(poc_path).unlink(missing_ok=True)
            except OSError:
                pass

    # ─── Delta Debugging ───

    async def delta_debug(self, input_bytes: bytes, max_iterations: int = 200, timeout_secs: int = 30) -> dict:
        """Run delta debugging to minimize a crashing input.

        Args:
            input_bytes: The crashing input bytes to minimize
            max_iterations: Maximum ddmin iterations
            timeout_secs: Timeout in seconds

        Returns dict with: minimized (base64), original_size, minimized_size,
                          reduction_ratio, iterations, is_1_minimal, elapsed_ms
        """
        import base64

        with tempfile.NamedTemporaryFile(mode="wb", suffix=".bin", delete=False) as f:
            f.write(input_bytes)
            input_path = f.name
            register_temp_file(input_path)

        try:
            stdout, stderr, rc = await self._run(
                "delta",
                "--input",
                input_path,
                "--max-iterations",
                str(max_iterations),
                "--timeout",
                str(timeout_secs),
            )
            result = json.loads(stdout)
            return result
        except TimeoutError:
            return {
                "minimized": base64.b64encode(input_bytes).decode(),
                "original_size": len(input_bytes),
                "minimized_size": len(input_bytes),
                "reduction_ratio": 0.0,
                "iterations": 0,
                "is_1_minimal": False,
                "elapsed_ms": 0,
                "error": "timeout",
            }
        except Exception as e:
            return {
                "minimized": base64.b64encode(input_bytes).decode(),
                "original_size": len(input_bytes),
                "minimized_size": len(input_bytes),
                "reduction_ratio": 0.0,
                "iterations": 0,
                "is_1_minimal": False,
                "elapsed_ms": 0,
                "error": str(e)[:200],
            }
        finally:
            try:
                Path(input_path).unlink(missing_ok=True)
            except OSError:
                pass

    # ─── Differential Analysis ───

    async def diff_execute(self, input_str: str, reference: str, normalizer: str = "Text") -> dict:
        """Compare two outputs using differential analysis.

        Args:
            input_str: First (baseline) output to compare
            reference: Second (changed) output to compare
            normalizer: Output normalizer (Json, Xml, Dict, Text, Binary)

        Returns DiffExecution dict with is_different, diff_magnitude, etc.
        """
        import json as _json

        payload = _json.dumps(
            {
                "method": "diff",
                "input": input_str,
                "reference": reference,
                "normalizer": normalizer,
                "request_id": self._request_id or "",
            }
        )
        try:
            stdout, stderr, rc = await self._run("--request", payload)
            return _json.loads(stdout)
        except Exception as e:
            return {"is_different": False, "error": str(e)[:200]}

    # ─── Invariant Mining ───

    async def mine_invariants(self, function_name: str, param_types: list[str], count: int = 100) -> dict:
        """Mine invariants from function execution traces.

        Args:
            function_name: Target function name
            param_types: List of parameter type hints
            count: Number of inputs to generate

        Returns dict with invariants_found, violations, etc.
        """
        import json as _json

        payload = _json.dumps(
            {
                "method": "mine_invariants",
                "function": function_name,
                "param_types": param_types,
                "count": count,
                "request_id": self._request_id or "",
            }
        )
        stdout, stderr, rc = await self._run("--request", payload)
        try:
            return _json.loads(stdout)
        except Exception:
            return {"invariants_found": 0, "violations_found": 0, "error": stderr[:200]}

    # ─── Mutation Testing ───

    async def run_mutations(
        self, source_code: str, file_path: str = "unknown", operators: list[str] | None = None
    ) -> dict:
        """Run mutation testing against source code.

        Args:
            source_code: Source code to mutate
            file_path: File path for identification
            operators: List of mutation operators to apply

        Returns MutationSessionResult with mutation_score, survivors, etc.
        """
        import json as _json

        payload = _json.dumps(
            {
                "method": "run_mutations",
                "source": source_code,
                "file": file_path,
                "operators": operators or [],
                "request_id": self._request_id or "",
            }
        )
        stdout, stderr, rc = await self._run("--request", payload)
        try:
            return _json.loads(stdout)
        except Exception:
            return {"mutation_score": 0.0, "error": stderr[:200]}

    # ─── Symbolic Execution ───

    async def solve_reachability(self, target_location: str, path_conditions: list[dict]) -> dict:
        """Solve for the exact input that reaches a target code location.

        Args:
            target_location: Target code location (e.g., "auth.py:42")
            path_conditions: List of {line: int, condition: str} dicts

        Returns dict with solutions, constraints_generated, elapsed_ms, etc.
        """
        import json as _json

        payload = _json.dumps(
            {
                "method": "solve_reachability",
                "target": target_location,
                "conditions": path_conditions,
                "request_id": self._request_id or "",
            }
        )
        stdout, stderr, rc = await self._run("--request", payload)
        try:
            return _json.loads(stdout)
        except Exception:
            return {"solutions": [], "error": stderr[:200]}

    async def explore_paths(self, target_location: str, path_conditions: list[dict], max_queries: int = 100) -> dict:
        """Systematically explore all code paths using concolic execution.

        Args:
            target_location: Target code location
            path_conditions: List of {line: int, condition: str} dicts
            max_queries: Maximum negation queries (default: 100)

        Returns dict with coverage_pct, branches_covered, solutions, etc.
        """
        import json as _json

        payload = _json.dumps(
            {
                "method": "explore_paths",
                "target": target_location,
                "conditions": path_conditions,
                "max_queries": max_queries,
                "request_id": self._request_id or "",
            }
        )
        stdout, stderr, rc = await self._run("--request", payload)
        try:
            return _json.loads(stdout)
        except Exception:
            return {"coverage_pct": 0.0, "error": stderr[:200]}

    # ─── Health Check ───

    async def health_check(self) -> bool:
        """Check if sandbox binary is responsive."""
        try:
            stdout, _, _ = await self._run("default-config")
            self._healthy = True
            return True
        except Exception:
            self._healthy = False
            return False
