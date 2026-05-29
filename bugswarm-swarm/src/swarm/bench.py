"""Phase 10: The Bench — 5-Judge Arbitration Panel.

Structured verdicts, bias calibration, tiebreaker protocol,
judge queries, inter-judge synthesis debate.
"""

from __future__ import annotations

import json
import random
from collections import defaultdict
from dataclasses import dataclass, field
from enum import Enum

import structlog

logger = structlog.get_logger(__name__)


# ═══════════════════════════════════════════════════════════════
# Types
# ═══════════════════════════════════════════════════════════════


class JudgeSpecialty(str, Enum):
    SECURITY = "security"
    LOGIC = "logic"
    CONCURRENCY = "concurrency"
    ARCHITECTURE = "architecture"
    DATA_INTEGRITY = "data_integrity"


class VerdictStatus(str, Enum):
    CONFIRMED = "confirmed"
    DISMISSED = "dismissed"
    NEEDS_INVESTIGATION = "needs_investigation"


@dataclass
class Verdict:
    """Structured verdict from a judge."""

    judge_id: str
    specialty: JudgeSpecialty
    bug_verified: bool
    severity: int  # 1-10
    evidence_quality: float  # 0.0-1.0
    causal_chain_valid: bool
    false_positive_risk: float  # 0.0-1.0
    exploitability: float  # 0.0-1.0
    trace_citation: str  # Must cite sandbox run ID or evidence node
    confidence: float  # 0.0-1.0
    reasoning: str
    queries_used: int = 0
    raw_verdict: dict | None = None

    def to_dict(self) -> dict:
        return {
            "judge_id": self.judge_id,
            "specialty": self.specialty.value,
            "bug_verified": self.bug_verified,
            "severity": self.severity,
            "evidence_quality": self.evidence_quality,
            "causal_chain_valid": self.causal_chain_valid,
            "false_positive_risk": self.false_positive_risk,
            "exploitability": self.exploitability,
            "trace_citation": self.trace_citation,
            "confidence": self.confidence,
            "reasoning": self.reasoning[:200],
            "queries_used": self.queries_used,
        }


@dataclass
class JudgeCalibration:
    """Bias parameters for a judge model."""

    judge_id: str
    severity_bias: float = 0.0  # Mean deviation from ground truth
    false_positive_rate: float = 0.0
    false_negative_rate: float = 0.0
    calibration_samples: int = 0
    stable: bool = False

    def normalize_severity(self, raw_severity: int) -> int:
        return max(1, min(10, round(raw_severity - self.severity_bias)))

    def voting_weight(self, for_confirmation: bool) -> float:
        if for_confirmation:
            return 1.0 - self.false_positive_rate
        return 1.0 - self.false_negative_rate


@dataclass
class Case:
    """A bug case to be adjudicated."""

    case_id: str
    claim: str
    location: str
    mechanism: str
    evidence: list[dict]  # Sandbox receipts, CPG paths
    agent_id: str
    severity_estimate: int
    bug_type: str = "unknown"  # security, logic, concurrency, etc.
    verdicts: list[Verdict] = field(default_factory=list)
    final_verdict: VerdictStatus = field(default_factory=lambda: VerdictStatus.NEEDS_INVESTIGATION)
    final_severity: int = 0
    tiebroken: bool = False
    human_overridden: bool = False

    def to_brief(self) -> str:
        """Generate structured brief for judges — not raw debate transcript."""
        parts = [
            f"CASE {self.case_id}",
            f"Claim: {self.claim}",
            f"Location: {self.location}",
            f"Mechanism: {self.mechanism}",
            f"Agent: {self.agent_id}",
            f"Estimated Severity: {self.severity_estimate}/10",
            f"Bug Type: {self.bug_type}",
        ]
        if self.evidence:
            parts.append(f"\nEVIDENCE ({len(self.evidence)} items):")
            for i, e in enumerate(self.evidence[:5]):
                parts.append(f"  [{i + 1}] {json.dumps(e)[:300]}")
        return "\n".join(parts)


# ═══════════════════════════════════════════════════════════════
# Judge Prompts
# ═══════════════════════════════════════════════════════════════

JUDGE_PROMPTS: dict[JudgeSpecialty, str] = {
    JudgeSpecialty.SECURITY: """You are the SECURITY JUDGE. Evaluate findings for security impact.
Focus on: exploitability, attack surface, data exposure, privilege escalation.
A 'bug' must be triggerable by realistic user input and cause a security breach (data leak, auth bypass, RCE, injection).
Downgrade severity if the vulnerable code path is not reachable from external input.""",
    JudgeSpecialty.LOGIC: """You are the LOGIC JUDGE. Evaluate findings for logical correctness.
Focus on: whether the claimed mechanism logically produces the observed behavior.
A 'bug' must have a clear causal chain from input to incorrect output.
Flag findings where the mechanism doesn't match the evidence.""",
    JudgeSpecialty.CONCURRENCY: """You are the CONCURRENCY JUDGE. Evaluate findings for concurrency issues.
Focus on: race conditions, deadlocks, data races, atomicity violations.
For flaky bugs: check statistical significance (failure rate >5%, confidence interval >0).
Single-run results for race conditions are insufficient evidence.""",
    JudgeSpecialty.ARCHITECTURE: """You are the ARCHITECTURE JUDGE. Evaluate findings in system context.
Focus on: whether the bug is in dead code, behind a WAF, in a deprecated path, or requires unrealistic preconditions.
Contextualize blast radius: is this a single-user bug or a system-wide vulnerability?
Evaluate fix complexity: does fixing this require cascading refactors?""",
    JudgeSpecialty.DATA_INTEGRITY: """You are the DATA INTEGRITY JUDGE. Evaluate findings for data corruption risk.
Focus on: data loss, corruption, inconsistency, privacy violations.
Check whether the evidence shows actual data impact or just theoretical risk.
Verify that the PoC demonstrates data modification, not just a crash.""",
}


# ═══════════════════════════════════════════════════════════════
# The Bench
# ═══════════════════════════════════════════════════════════════


class Bench:
    """5-judge arbitration panel."""

    SPECIALTIES = list(JudgeSpecialty)

    def __init__(self):
        self.judges: dict[str, dict] = {
            "J1": {"specialty": JudgeSpecialty.SECURITY, "model": "gpt-4o"},
            "J2": {"specialty": JudgeSpecialty.LOGIC, "model": "gpt-4o"},
            "J3": {"specialty": JudgeSpecialty.CONCURRENCY, "model": "gpt-4o"},
            "J4": {"specialty": JudgeSpecialty.ARCHITECTURE, "model": "claude-3.5-sonnet"},
            "J5": {"specialty": JudgeSpecialty.DATA_INTEGRITY, "model": "claude-3.5-sonnet"},
        }
        self.calibrations: dict[str, JudgeCalibration] = {jid: JudgeCalibration(judge_id=jid) for jid in self.judges}
        self.cases: dict[str, Case] = {}
        self.query_budgets: dict[str, int] = defaultdict(lambda: 3)
        self.total_judge_queries: int = 0
        self.max_total_queries: int = 20

    def calibrate(self, golden_dataset: list[dict]) -> None:
        """Calibrate judges against a golden dataset of 200 pre-adjudicated bugs."""
        for jid in self.judges:
            cal = self.calibrations[jid]
            severity_diffs = []
            fp = 0
            fn = 0
            total = 0

            for entry in golden_dataset:
                # Simulated: in production, each judge evaluates and we compare
                raw_sev = entry.get("severity", 5)
                gt_sev = entry.get("ground_truth_severity", 5)
                severity_diffs.append(raw_sev - gt_sev)

                if entry.get("is_bug") and not entry.get("judge_confirmed", True):
                    fn += 1
                if not entry.get("is_bug") and entry.get("judge_confirmed", False):
                    fp += 1
                total += 1

            cal.severity_bias = sum(severity_diffs) / max(len(severity_diffs), 1)
            cal.false_positive_rate = fp / max(total, 1)
            cal.false_negative_rate = fn / max(total, 1)
            cal.calibration_samples = total
            cal.stable = total >= 50

            logger.info(
                "judge_calibrated",
                judge=jid,
                bias=cal.severity_bias,
                fpr=cal.false_positive_rate,
                fnr=cal.false_negative_rate,
            )

    def submit_case(self, case: Case) -> None:
        self.cases[case.case_id] = case

    def adjudicate(self, case: Case) -> list[Verdict]:
        """All 5 judges produce verdicts for a case."""
        brief = case.to_brief()
        verdicts = []

        for jid, jinfo in self.judges.items():
            cal = self.calibrations[jid]
            prompt = JUDGE_PROMPTS[jinfo["specialty"]]

            # Simulated judge deliberation (in production, calls LLM via Gateway)
            verdict = self._simulate_verdict(jid, jinfo["specialty"], brief, case, cal)
            verdicts.append(verdict)

        case.verdicts = verdicts
        return verdicts

    def _simulate_verdict(
        self,
        jid: str,
        specialty: JudgeSpecialty,
        brief: str,
        case: Case,
        cal: JudgeCalibration,
    ) -> Verdict:
        """Simulate a judge's verdict. In production, this calls the LLM."""
        has_evidence = len(case.evidence) > 0
        has_sandbox = any("sandbox" in str(e).lower() or "exit_code" in str(e) for e in case.evidence)

        # Base assessment
        if has_sandbox and case.severity_estimate >= 7:
            confirmed = True
            severity = case.severity_estimate
            evidence_quality = 0.85 + random.uniform(-0.1, 0.1)
            fp_risk = 0.05 + random.uniform(0, 0.1)
        elif has_evidence:
            confirmed = random.random() < 0.6
            severity = max(1, case.severity_estimate + random.randint(-2, 0))
            evidence_quality = 0.5 + random.uniform(0, 0.3)
            fp_risk = 0.15 + random.uniform(0, 0.15)
        else:
            confirmed = False
            severity = max(1, case.severity_estimate - random.randint(2, 4))
            evidence_quality = random.uniform(0.1, 0.4)
            fp_risk = 0.5 + random.uniform(0, 0.3)

        # Apply calibration
        raw_sev = severity
        severity = cal.normalize_severity(raw_sev)

        # Specialty-specific adjustments
        if specialty == JudgeSpecialty.SECURITY and case.bug_type == "security":
            severity = min(10, severity + 1)
            confirmed = confirmed or case.severity_estimate >= 6
        elif specialty == JudgeSpecialty.CONCURRENCY and "race" in case.claim.lower():
            evidence_quality *= 0.8  # Concurrency judge is stricter
        elif specialty == JudgeSpecialty.ARCHITECTURE:
            severity = max(1, severity - 1)  # Architecture downgrades for context

        trace = f"sandbox://{case.case_id}" if has_sandbox else f"evidence://{case.case_id}"

        return Verdict(
            judge_id=jid,
            specialty=specialty,
            bug_verified=confirmed,
            severity=severity,
            evidence_quality=round(evidence_quality, 3),
            causal_chain_valid=has_evidence,
            false_positive_risk=round(fp_risk, 3),
            exploitability=round(0.5 + random.uniform(0, 0.5), 3) if confirmed else 0.0,
            trace_citation=trace,
            confidence=round(0.6 + random.uniform(0, 0.3), 3) if confirmed else round(0.3 + random.uniform(0, 0.3), 3),
            reasoning=f"[{specialty.value}] {'Confirmed' if confirmed else 'Dismissed'} based on {'sandbox evidence' if has_sandbox else 'code analysis'}",
        )

    def reach_consensus(self, case: Case) -> dict:
        """Aggregate verdicts and resolve ties."""
        verdicts = case.verdicts
        if not verdicts:
            return {"status": "error", "reason": "no verdicts"}

        confirmed = sum(1 for v in verdicts if v.bug_verified)
        dismissed = len(verdicts) - confirmed
        cal = self.calibrations

        # 1. Weighted vote
        weighted_confirmed = sum(v.bug_verified * cal[v.judge_id].voting_weight(True) for v in verdicts)
        weighted_dismissed = sum((not v.bug_verified) * cal[v.judge_id].voting_weight(False) for v in verdicts)

        # 2. If split, try evidence-only re-vote
        if confirmed == dismissed or abs(weighted_confirmed - weighted_dismissed) < 0.5:
            case.tiebroken = True
            return self._tiebreaker(case)

        # Majority reached
        if weighted_confirmed > weighted_dismissed:
            case.final_verdict = VerdictStatus.CONFIRMED
            # Severity: weighted average
            total_weight = sum(cal[v.judge_id].voting_weight(True) for v in verdicts if v.bug_verified)
            if total_weight > 0:
                weighted_sev = (
                    sum(v.severity * cal[v.judge_id].voting_weight(True) for v in verdicts if v.bug_verified)
                    / total_weight
                )
            else:
                weighted_sev = sum(v.severity for v in verdicts) / len(verdicts)
            case.final_severity = max(1, min(10, round(weighted_sev)))
        else:
            case.final_verdict = VerdictStatus.DISMISSED
            case.final_severity = 0

        return {
            "status": case.final_verdict.value,
            "severity": case.final_severity,
            "confirmed_votes": confirmed,
            "dismissed_votes": dismissed,
            "weighted_confirmed": round(weighted_confirmed, 3),
            "weighted_dismissed": round(weighted_dismissed, 3),
            "tiebroken": case.tiebroken,
        }

    def _tiebreaker(self, case: Case) -> dict:
        """Three-level tiebreaker protocol."""
        verdicts = case.verdicts

        # Level 1: Evidence-only re-vote
        case.bug_type = self._classify_bug_type(case)
        logger.info("tiebreaker_level1", case=case.case_id, bug_type=case.bug_type)

        # Level 2: Specialist escalation
        specialist_map = {
            "security": JudgeSpecialty.SECURITY,
            "concurrency": JudgeSpecialty.CONCURRENCY,
            "logic": JudgeSpecialty.LOGIC,
        }
        specialist = specialist_map.get(case.bug_type)

        if specialist:
            # Find the specialist judge
            for v in verdicts:
                if v.specialty == specialist:
                    # Check calibration — revoke authority if inaccurate
                    cal = self.calibrations[v.judge_id]
                    if cal.false_positive_rate > 0.20 or cal.false_negative_rate > 0.20:
                        logger.warning(
                            "tiebreaker_authority_revoked",
                            judge=v.judge_id,
                            fpr=cal.false_positive_rate,
                            fnr=cal.false_negative_rate,
                        )
                        continue

                    # Specialist breaks tie
                    case.final_verdict = VerdictStatus.CONFIRMED if v.bug_verified else VerdictStatus.DISMISSED
                    case.final_severity = v.severity
                    case.tiebroken = True
                    return {
                        "status": case.final_verdict.value,
                        "severity": case.final_severity,
                        "tiebreaker": f"specialist:{v.judge_id}",
                    }

        # Level 3: Conservative principle — escalate severity
        highest_sev = max(v.severity for v in verdicts)
        confirming = [v for v in verdicts if v.bug_verified]
        case.final_verdict = VerdictStatus.CONFIRMED if confirming else VerdictStatus.DISMISSED
        case.final_severity = highest_sev
        case.tiebroken = True

        logger.warning(
            "tiebreaker_conservative",
            case=case.case_id,
            severity=highest_sev,
            reason="When in doubt, escalate severity",
        )

        return {
            "status": case.final_verdict.value,
            "severity": case.final_severity,
            "tiebreaker": "conservative_principle",
            "annotation": "SEVERITY_DISPUTED: unresolved disagreement — conservative escalation applied",
        }

    def request_judge_query(self, judge_id: str, case: Case, query: str) -> tuple[bool, str]:
        """Judge requests more information from the swarm."""
        if self.total_judge_queries >= self.max_total_queries:
            return False, "Global query budget exhausted"

        budget = self.query_budgets[case.case_id]
        if budget <= 0:
            return False, f"Case query budget exhausted ({3} max)"

        self.query_budgets[case.case_id] -= 1
        self.total_judge_queries += 1

        logger.info(
            "judge_query",
            judge=judge_id,
            case=case.case_id,
            query=query[:100],
            remaining_budget=self.query_budgets[case.case_id],
        )

        return True, f"Query accepted. {self.query_budgets[case.case_id]} remaining for this case."

    def synthesis_debate(self, case: Case, judge_a: str, judge_b: str) -> dict:
        """Inter-Judge Synthesis Debate for unresolvable disagreement."""
        logger.warning("synthesis_debate_started", case=case.case_id, judge_a=judge_a, judge_b=judge_b)

        # Simulated 2-round debate (in production, calls LLMs)
        va = next((v for v in case.verdicts if v.judge_id == judge_a), None)
        vb = next((v for v in case.verdicts if v.judge_id == judge_b), None)

        if not va or not vb:
            return {"status": "error", "reason": "judge not found"}

        # Round 1: each states case
        # Round 2: other judges vote

        # Find 2 neutral judges
        neutral = [v for v in case.verdicts if v.judge_id not in (judge_a, judge_b)][:2]
        neutral_confirm = sum(1 for v in neutral if v.bug_verified)

        if neutral_confirm >= len(neutral) / 2:
            resolution = VerdictStatus.CONFIRMED
            severity = max(va.severity, vb.severity)
        else:
            # Conservative principle
            resolution = VerdictStatus.CONFIRMED
            severity = max(va.severity, vb.severity)

        case.final_verdict = resolution
        case.final_severity = severity
        case.tiebroken = True

        return {
            "status": resolution.value,
            "severity": severity,
            "debate_participants": [judge_a, judge_b],
            "neutral_votes": neutral_confirm,
            "resolution": "synthesis_debate",
        }

    def _classify_bug_type(self, case: Case) -> str:
        claim = case.claim.lower()
        if any(k in claim for k in ("sql", "injection", "xss", "rce", "auth bypass", "overflow")):
            return "security"
        if any(k in claim for k in ("race", "deadlock", "concurrent", "atomic")):
            return "concurrency"
        if any(k in claim for k in ("null", "type", "logic", "incorrect")):
            return "logic"
        return "unknown"

    def calibrate_against_golden(self, dataset: list[dict]) -> dict:
        """Run calibration against the golden dataset."""
        results = {"total": len(dataset), "correct": 0, "wrong": 0, "by_judge": {}}
        for entry in dataset:
            case = Case(
                case_id=f"cal_{entry.get('id', '?')}",
                claim=entry.get("claim", ""),
                location=entry.get("location", ""),
                mechanism=entry.get("mechanism", ""),
                evidence=entry.get("evidence", []),
                agent_id="calibration",
                severity_estimate=entry.get("severity", 5),
                bug_type=entry.get("type", "unknown"),
            )
            self.submit_case(case)
            self.adjudicate(case)
            consensus = self.reach_consensus(case)
            is_correct = (consensus["status"] == "confirmed") == entry.get("is_bug", False)
            if is_correct:
                results["correct"] += 1
            else:
                results["wrong"] += 1
        results["accuracy"] = results["correct"] / max(results["total"], 1)
        return results
