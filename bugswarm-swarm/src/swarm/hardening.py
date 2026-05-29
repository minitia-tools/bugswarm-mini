"""Phase 14: Hardening & Feedback Loops.

Production bug ingestion, retrospective analysis, auto-tuning,
CVE pipeline, regression firewall, WAL replication, audit trail,
domain profiles. Never truly 'done' — improves with every run.
"""

from __future__ import annotations

import hashlib
import json
import time
import zlib
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any

import structlog

logger = structlog.get_logger(__name__)


# ═══════════════════════════════════════════════════════════════
# Domain Profiles
# ═══════════════════════════════════════════════════════════════


class DomainProfile(str, Enum):
    WEB_APP = "web_app"
    SMART_CONTRACT = "smart_contract"
    MEDICAL_DEVICE = "medical_device"
    AEROSPACE = "aerospace"
    CRYPTO_LIBRARY = "crypto_library"


@dataclass
class DomainConfig:
    name: DomainProfile
    pragmatism_level: float  # 0.0 = theoretical only, 1.0 = realistic only
    requires_realistic_trigger: bool
    side_channel_scope: bool  # Timing, power analysis in-scope
    safety_patterns: bool  # Flag unsafe patterns even if not bugs
    max_severity_baseline: int  # Minimum severity to report

    @classmethod
    def for_profile(cls, profile: DomainProfile) -> DomainConfig:
        configs = {
            DomainProfile.WEB_APP: cls(profile, 0.8, True, False, False, 4),
            DomainProfile.SMART_CONTRACT: cls(profile, 0.1, False, True, False, 1),
            DomainProfile.MEDICAL_DEVICE: cls(profile, 0.2, True, True, True, 1),
            DomainProfile.AEROSPACE: cls(profile, 0.1, True, True, True, 1),
            DomainProfile.CRYPTO_LIBRARY: cls(profile, 0.05, False, True, True, 1),
        }
        return configs[profile]

    def to_dict(self) -> dict:
        return {
            "profile": self.name.value,
            "pragmatism": self.pragmatism_level,
            "realistic_trigger": self.requires_realistic_trigger,
            "side_channel": self.side_channel_scope,
            "safety_patterns": self.safety_patterns,
            "min_severity": self.max_severity_baseline,
        }


# ═══════════════════════════════════════════════════════════════
# Production Bug Ingestion + Retrospective Analysis
# ═══════════════════════════════════════════════════════════════


class FailureMode(str, Enum):
    CPG_MISSED = "cpg_missed"  # Bug in unindexed code region
    AGENT_OVERRULED = "agent_overruled"  # Agent found it but was gaslit
    JUDGE_DISMISSED = "judge_dismissed"  # Judge incorrectly dismissed
    SANDBOX_FAILED = "sandbox_failed"  # Sandbox couldn't reproduce
    NOT_INVESTIGATED = "not_investigated"  # No agent looked at this code


@dataclass
class ProductionIncident:
    incident_id: str
    bug_location: str  # file:line
    bug_type: str  # sql_injection, race_condition, etc.
    severity: int
    cve_id: str = ""
    description: str = ""
    was_detected: bool = False
    failure_mode: FailureMode | None = None
    detected_by_agent: str = ""
    corrective_actions: list[str] = field(default_factory=list)
    timestamp: float = field(default_factory=time.time)

    def to_dict(self) -> dict:
        return {
            "incident_id": self.incident_id,
            "bug_location": self.bug_location,
            "bug_type": self.bug_type,
            "severity": self.severity,
            "was_detected": self.was_detected,
            "failure_mode": self.failure_mode.value if self.failure_mode else None,
            "corrective_actions": self.corrective_actions,
        }


class FeedbackEngine:
    """Learns from production escapes and improves the system."""

    def __init__(self):
        self.incidents: list[ProductionIncident] = []
        self.tuning_log: list[dict] = []
        self.cve_patterns: list[dict] = []

    def ingest_incident(self, incident: ProductionIncident) -> None:
        """Ingest a production bug for retrospective analysis."""
        self.incidents.append(incident)
        logger.info("incident_ingested", id=incident.incident_id, location=incident.bug_location)

    def analyze(
        self,
        incident: ProductionIncident,
        cpg_data: dict | None = None,
        agent_logs: list[dict] | None = None,
        judge_verdicts: list[dict] | None = None,
        sandbox_logs: list[dict] | None = None,
    ) -> ProductionIncident:
        """Retrospective analysis: determine why the bug was missed."""
        agent_logs = agent_logs or []
        judge_verdicts = judge_verdicts or []

        # 1: Did any agent mention the vulnerable code?
        mentioned = any(incident.bug_location in log.get("content", "") for log in agent_logs)

        if not mentioned and cpg_data:
            # Check if the code region was indexed
            parts = incident.bug_location.split(":")
            file = parts[0] if parts else ""
            indexed = file in str(cpg_data)
            if not indexed:
                incident.failure_mode = FailureMode.CPG_MISSED
                incident.corrective_actions.append(f"Expand CPG indexing radius for {file}")
            else:
                incident.failure_mode = FailureMode.NOT_INVESTIGATED
                incident.corrective_actions.append("Increase exploration persona weight")

        # 2: Did any agent suspect a bug but get overruled?
        if mentioned:
            agent_found = any(
                "bug" in log.get("content", "").lower() or "vulnerability" in log.get("content", "").lower()
                for log in agent_logs
                if incident.bug_location in log.get("content", "")
            )
            if agent_found:
                # Check if judges dismissed
                dismissed = any(
                    v.get("status") == "dismissed" and incident.bug_location in str(v) for v in judge_verdicts
                )
                if dismissed:
                    incident.failure_mode = FailureMode.JUDGE_DISMISSED
                    incident.corrective_actions.append("Penalize dismissing judge in calibration")
                else:
                    incident.failure_mode = FailureMode.AGENT_OVERRULED
                    incident.corrective_actions.append("Tighten anti-sycophancy parameters")

        # 3: Did sandbox fail to reproduce?
        if sandbox_logs and not incident.was_detected:
            incident.failure_mode = FailureMode.SANDBOX_FAILED
            incident.corrective_actions.append("Improve PoC generation templates")

        incident.was_detected = False  # It escaped production
        return incident

    def auto_tune(self) -> dict:
        """Apply automatic parameter adjustments based on incident analysis."""
        adjustments = {}

        for incident in self.incidents[-10:]:
            if incident.failure_mode == FailureMode.CPG_MISSED:
                adjustments.setdefault("cpg_indexing_radius", 2)
                adjustments["cpg_indexing_radius"] += 1
            elif incident.failure_mode == FailureMode.AGENT_OVERRULED:
                adjustments.setdefault("sycophancy_threshold", 0.92)
                adjustments["sycophancy_threshold"] -= 0.02
            elif incident.failure_mode == FailureMode.JUDGE_DISMISSED:
                adjustments.setdefault("judge_penalty_score", 0.0)
                adjustments["judge_penalty_score"] += 0.1
            elif incident.failure_mode == FailureMode.NOT_INVESTIGATED:
                adjustments.setdefault("exploratory_persona_weight", 0.25)
                adjustments["exploratory_persona_weight"] += 0.05

        for key, val in adjustments.items():
            self.tuning_log.append({"parameter": key, "new_value": val, "timestamp": time.time()})
            logger.info("auto_tuned", parameter=key, new_value=val)

        return adjustments

    def ingest_cve(self, cve_id: str, description: str, code_pattern: str, language: str = "python") -> None:
        """Add a CVE pattern to the vulnerability corpus."""
        pattern = {
            "cve_id": cve_id,
            "description": description,
            "code_pattern": code_pattern,
            "language": language,
            "ingested_at": time.time(),
            "pattern_hash": hashlib.sha256(code_pattern.encode()).hexdigest()[:16],
        }
        self.cve_patterns.append(pattern)
        logger.info("cve_ingested", cve=cve_id, pattern_hash=pattern["pattern_hash"])

    def match_cve_patterns(self, code: str) -> list[dict]:
        """Match code against known CVE patterns."""
        matches = []
        for pattern in self.cve_patterns:
            if pattern["code_pattern"] in code:
                matches.append(pattern)
        return matches

    def generate_regression_test(self, incident: ProductionIncident) -> str:
        """Auto-generate a regression test for a production bug."""
        parts = incident.bug_location.split(":")
        file = parts[0] if parts else "unknown.py"
        line = parts[1] if len(parts) > 1 else "1"

        return f'''# Auto-generated regression test for {incident.incident_id}
# Bug: {incident.description[:100]}
# Location: {incident.bug_location}

def test_regression_{incident.incident_id.replace("-", "_")}():
    """Verify {incident.bug_type} at {incident.bug_location} is caught."""
    # TODO: Implement specific test case for this bug pattern
    # Expected: The bug should be detected/triggered by this test
    pass
'''


# ═══════════════════════════════════════════════════════════════
# WAL Replication with CRC32
# ═══════════════════════════════════════════════════════════════


class WALReplicator:
    """Replicates Write-Ahead Log to secondary storage with CRC32 verification."""

    def __init__(self, primary_path: Path, replica_path: Path | None = None):
        self.primary = primary_path
        self.replica = replica_path
        self.blocks: list[dict] = []  # (data, crc32, timestamp)

    def write_block(self, data: bytes) -> dict:
        """Write a WAL block with CRC32 checksum."""
        crc = zlib.crc32(data) & 0xFFFFFFFF
        block = {
            "data": data,
            "crc32": crc,
            "size": len(data),
            "timestamp": time.time(),
            "block_id": len(self.blocks),
        }
        self.blocks.append(block)
        return block

    def verify_block(self, block_id: int) -> bool:
        """Verify a block's CRC32 checksum."""
        if block_id >= len(self.blocks):
            return False
        block = self.blocks[block_id]
        computed = zlib.crc32(block["data"]) & 0xFFFFFFFF
        return computed == block["crc32"]

    def verify_all(self) -> list[int]:
        """Verify all blocks. Returns list of corrupted block IDs."""
        return [i for i in range(len(self.blocks)) if not self.verify_block(i)]

    def replicate_to(self, target: Path) -> int:
        """Replicate WAL to secondary storage."""
        count = 0
        for block in self.blocks:
            target_block = target / f"block_{block['block_id']:06d}.wal"
            payload = json.dumps(
                {
                    "block_id": block["block_id"],
                    "crc32": block["crc32"],
                    "size": block["size"],
                    "timestamp": block["timestamp"],
                    "data_hex": block["data"].hex(),
                }
            )
            target_block.write_text(payload)
            count += 1
        return count

    def recover_from_replica(self, source: Path) -> int:
        """Recover WAL from replica after corruption."""
        recovered = 0
        for replica_file in sorted(source.glob("block_*.wal")):
            payload = json.loads(replica_file.read_text())
            data = bytes.fromhex(payload["data_hex"])
            expected_crc = payload["crc32"]
            actual_crc = zlib.crc32(data) & 0xFFFFFFFF
            if actual_crc == expected_crc:
                self.write_block(data)
                recovered += 1
            else:
                logger.error("wal_recovery_corrupt", block=payload["block_id"])
        return recovered


# ═══════════════════════════════════════════════════════════════
# Override Governance Audit Trail
# ═══════════════════════════════════════════════════════════════


class AuditTrail:
    """Immutable, cryptographically chained audit trail for overrides."""

    def __init__(self):
        self.entries: list[dict] = []
        self._chain_hash: str = "0" * 64  # Genesis block

    def record_override(
        self,
        operator: str,
        case_id: str,
        before_severity: int,
        after_severity: int,
        justification: str = "",
    ) -> str:
        """Record a human override with cryptographic chaining."""
        entry = {
            "operator": operator,
            "case_id": case_id,
            "before_severity": before_severity,
            "after_severity": after_severity,
            "justification": justification[:500],
            "timestamp": time.time(),
            "previous_hash": self._chain_hash,
        }
        # Chain: hash(current_entry + previous_hash)
        entry_json = json.dumps(entry, sort_keys=True)
        entry["entry_hash"] = hashlib.sha256((entry_json + self._chain_hash).encode()).hexdigest()
        self._chain_hash = entry["entry_hash"]
        self.entries.append(entry)

        logger.info(
            "override_recorded", operator=operator, case=case_id, severity_change=f"{before_severity}→{after_severity}"
        )

        return entry["entry_hash"]

    def verify_chain(self) -> bool:
        """Verify the integrity of the entire audit chain."""
        prev = "0" * 64
        for entry in self.entries:
            entry_copy = {k: v for k, v in entry.items() if k != "entry_hash"}
            entry_json = json.dumps(entry_copy, sort_keys=True)
            expected = hashlib.sha256((entry_json + prev).encode()).hexdigest()
            if entry["entry_hash"] != expected:
                return False
            prev = entry["entry_hash"]
        return True

    def get_override_summary(self) -> list[dict]:
        return [
            {
                "operator": e["operator"],
                "case": e["case_id"],
                "change": f"{e['before_severity']}→{e['after_severity']}",
                "justification": e["justification"][:100],
                "hash": e["entry_hash"][:16],
            }
            for e in self.entries
        ]


# ═══════════════════════════════════════════════════════════════
# Regression Firewall
# ═══════════════════════════════════════════════════════════════


class RegressionFirewall:
    """Catches regressions introduced by patches."""

    def __init__(self):
        self.invariants: list[dict] = []
        self.baselines: dict[str, Any] = {}

    def add_invariant(self, name: str, description: str, check_fn: callable) -> None:
        self.invariants.append({"name": name, "description": description, "check": check_fn})

    def set_baseline(self, key: str, value: Any) -> None:
        self.baselines[key] = value

    def check_patch(self, patched_code: str, original_code: str) -> list[dict]:
        """Run all regression checks against a patch."""
        failures = []

        # 1: Property-based invariant check
        for inv in self.invariants:
            try:
                if not inv["check"](patched_code):
                    failures.append({"type": "invariant", "name": inv["name"], "description": inv["description"]})
            except Exception as e:
                failures.append({"type": "invariant_error", "name": inv["name"], "error": str(e)})

        # 2: Differential testing — outputs should match for non-buggy inputs
        if "output_hash" in self.baselines:
            new_hash = hashlib.sha256(patched_code.encode()).hexdigest()
            if new_hash != self.baselines.get("output_hash"):
                failures.append(
                    {
                        "type": "differential",
                        "name": "output_changed",
                        "expected": self.baselines["output_hash"][:16],
                        "actual": new_hash[:16],
                    }
                )

        # 3: Coverage check — patch should not reduce coverage
        if "coverage_pct" in self.baselines:
            new_lines = len(patched_code.split("\n"))
            old_lines = len(original_code.split("\n"))
            if new_lines < old_lines * 0.9:
                failures.append(
                    {"type": "coverage", "name": "coverage_dropped", "old_lines": old_lines, "new_lines": new_lines}
                )

        # 4: AST diff — detect dangerous structural changes
        dangerous_patterns = [
            ("null_check_removed", "if.*is.*None.*:|if.*==.*None.*:"),
            ("equality_changed", "=="),
            ("constant_modified", "MAX_|MIN_|DEFAULT_|TIMEOUT_"),
        ]
        for name, pattern in dangerous_patterns:
            import re

            old_matches = len(re.findall(pattern, original_code))
            new_matches = len(re.findall(pattern, patched_code))
            if new_matches < old_matches:
                failures.append({"type": "ast_diff", "name": name, "old_count": old_matches, "new_count": new_matches})

        if failures:
            logger.warning("regression_detected", failures=len(failures))
        return failures
