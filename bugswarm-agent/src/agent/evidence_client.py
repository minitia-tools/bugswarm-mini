"""Evidence Daemon Client — communicates with the evidence graph daemon."""

from __future__ import annotations

import asyncio
import json

import structlog

logger = structlog.get_logger(__name__)

DEFAULT_EVIDENCE_BINARY = "bugswarm-evidence"
DEFAULT_EVIDENCE_SOCKET = "/var/run/bugswarm/evidence.sock"


class EvidenceClient:
    """Client for the evidence graph daemon."""

    def __init__(self, binary: str = DEFAULT_EVIDENCE_BINARY, timeout_secs: float = 30.0):
        self.binary = binary
        self.timeout = timeout_secs
        self._request_id: str | None = None

    def set_request_id(self, request_id: str) -> None:
        self._request_id = request_id

    async def _run(self, *args: str) -> tuple[str, str, int]:
        cmd = [self.binary] + list(args)
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        try:
            stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=self.timeout)
            return stdout.decode(), stderr.decode(), proc.returncode or 0
        except TimeoutError:
            proc.kill()
            await proc.wait()
            raise

    async def _send_json(self, request: dict) -> dict:
        """Send a JSON request to the daemon and receive a JSON response."""
        if self._request_id:
            request["request_id"] = self._request_id
        request_json = json.dumps(request)
        stdout, stderr, rc = await self._run("--request", request_json)
        if rc != 0:
            raise RuntimeError(f"Evidence daemon returned code {rc}: {stderr[:200]}")
        try:
            return json.loads(stdout)
        except json.JSONDecodeError:
            return {"success": False, "error": f"Invalid JSON response: {stdout[:200]}"}

    async def add_trigger_condition(self, bug_id: str, dimension: str, description: str, layer: str = "agent") -> dict:
        """Add a trigger condition to the evidence graph."""
        return await self._send_json(
            {
                "method": "add_trigger_condition",
                "bug_id": bug_id,
                "dimension": dimension,
                "description": description,
                "layer": layer,
            }
        )

    async def get_trigger_matrix(self, bug_id: str) -> dict:
        """Get the complete trigger matrix for a bug."""
        return await self._send_json(
            {
                "method": "get_trigger_matrix",
                "bug_id": bug_id,
            }
        )

    async def stats(self) -> dict:
        """Get evidence graph statistics."""
        return await self._send_json({"method": "stats"})

    async def health_check(self) -> bool:
        """Check if the evidence daemon is responsive."""
        try:
            result = await self._send_json({"method": "health"})
            return result.get("success", False)
        except Exception:
            return False

    async def suggest_chain(self, bug_ids: list[str], max_hops: int = 10) -> dict:
        """Analyze bugs and discover exploit chains.

        Args:
            bug_ids: List of confirmed bug IDs to analyze
            max_hops: Maximum chain length

        Returns dict with chains, severity escalations, etc.
        """
        return await self._send_json(
            {
                "method": "suggest_chain",
                "bug_ids": bug_ids,
                "max_hops": max_hops,
            }
        )

    async def predict_fix_impact(
        self,
        bug_id: str,
        function_name: str,
        file_path: str,
        original_line: str,
        replacement_line: str,
        line_number: int = 0,
        language: str = "python",
        description: str = "",
    ) -> dict:
        """Predict whether a proposed fix will introduce new bugs.

        Returns FixImpactReport with affected callers, confidence score,
        regression tests, and recommendation.
        """
        return await self._send_json(
            {
                "method": "predict_fix_impact",
                "bug_id": bug_id,
                "function": function_name,
                "file_path": file_path,
                "original_line": original_line,
                "replacement_line": replacement_line,
                "line_number": line_number,
                "language": language,
                "description": description,
            }
        )
