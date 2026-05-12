"""Phase 8: Financial Control Plane.

Triple budget enforcement, severity-gated overrides, anomaly detection,
tiered spending allocation, information-theoretic stopping, cost attribution.
"""

from __future__ import annotations

import math, time
from collections import defaultdict
from dataclasses import dataclass, field
from enum import Enum
from typing import Any

import structlog

logger = structlog.get_logger(__name__)


# ═══════════════════════════════════════════════════════════════
# Types
# ═══════════════════════════════════════════════════════════════

class BudgetStatus(str, Enum):
    HEALTHY = "healthy"
    WARNING = "warning"    # >80%
    CRITICAL = "critical"  # >95%
    EXHAUSTED = "exhausted"


class SeverityTier(str, Enum):
    LOW = "low"        # 1-3
    MEDIUM = "medium"  # 4-7
    HIGH = "high"      # 8-10


@dataclass
class TokenLedger:
    """Per-agent, per-batch token accounting."""
    agent_id: str = ""
    batch_id: str = ""
    round_num: int = 0
    input_tokens: int = 0
    output_tokens: int = 0
    cost_usd: float = 0.0
    timestamp: float = field(default_factory=time.time)


@dataclass
class BudgetConfig:
    token_budget: int = 5_000_000       # 5M tokens
    cost_budget_usd: float = 50.0       # $50
    time_budget_secs: float = 144_000   # 40 hours

    # Tiered allocation (fraction of total budget)
    low_severity_pct: float = 0.10     # 1-3: 10%
    medium_severity_pct: float = 0.40  # 4-7: 40%
    high_severity_pct: float = 0.50    # 8-10: 50% (effectively uncapped rem)

    # Override
    override_max_multiplier: float = 1.5  # Max 150% of original budget
    override_severity_threshold: int = 8  # Severity >= 8 triggers override

    # Anomaly detection
    anomaly_zscore_threshold: float = 3.0
    anomaly_ema_alpha: float = 0.3
    max_agent_token_share: float = 0.30   # Max 30% of total tokens per agent

    # Information-theoretic stopping
    novelty_threshold: float = 0.05       # <5% novel findings → stop
    novelty_window: int = 10              # Check last N sandbox runs

    # Cost per million tokens (for cost estimation)
    cost_per_mtok_input: float = 0.14
    cost_per_mtok_output: float = 0.28


@dataclass
class BudgetState:
    tokens_used: int = 0
    cost_used: float = 0.0
    start_time: float = field(default_factory=time.time)
    elapsed_secs: float = 0.0

    # Per-tier tracking
    tier_tokens: dict[SeverityTier, int] = field(default_factory=lambda: defaultdict(int))
    tier_cost: dict[SeverityTier, float] = field(default_factory=lambda: defaultdict(float))

    # Per-agent tracking
    agent_tokens: dict[str, int] = field(default_factory=lambda: defaultdict(int))
    agent_costs: dict[str, float] = field(default_factory=lambda: defaultdict(float))
    agent_turns: dict[str, int] = field(default_factory=lambda: defaultdict(int))

    # Override history
    overrides: list[dict] = field(default_factory=list)

    # Novelty tracking
    recent_findings: list[bool] = field(default_factory=list)  # True = novel

    def update_elapsed(self):
        self.elapsed_secs = time.time() - self.start_time

    def to_dict(self) -> dict:
        self.update_elapsed()
        return {
            "tokens_used": self.tokens_used,
            "cost_used": round(self.cost_used, 6),
            "elapsed_secs": round(self.elapsed_secs, 1),
            "overrides": len(self.overrides),
        }


# ═══════════════════════════════════════════════════════════════
# Financial Controller
# ═══════════════════════════════════════════════════════════════

class FinancialController:
    """Enforces budget constraints and manages spending across the swarm."""

    def __init__(self, config: BudgetConfig | None = None):
        self.config = config or BudgetConfig()
        self.state = BudgetState()
        self._novelty_window: list[bool] = []

    # ─── Budget Checks ───

    def check_all(self) -> dict[str, BudgetStatus]:
        """Check all three budget constraints."""
        self.state.update_elapsed()
        return {
            "token": self._check_token(),
            "cost": self._check_cost(),
            "time": self._check_time(),
        }

    def is_exhausted(self) -> bool:
        statuses = self.check_all()
        return any(s == BudgetStatus.EXHAUSTED for s in statuses.values())

    def _check_token(self) -> BudgetStatus:
        pct = self.state.tokens_used / self.config.token_budget
        if pct >= 1.0: return BudgetStatus.EXHAUSTED
        if pct >= 0.95: return BudgetStatus.CRITICAL
        if pct >= 0.80: return BudgetStatus.WARNING
        return BudgetStatus.HEALTHY

    def _check_cost(self) -> BudgetStatus:
        if self.config.cost_budget_usd <= 0:
            return BudgetStatus.HEALTHY
        pct = self.state.cost_used / self.config.cost_budget_usd
        if pct >= 1.0: return BudgetStatus.EXHAUSTED
        if pct >= 0.95: return BudgetStatus.CRITICAL
        if pct >= 0.80: return BudgetStatus.WARNING
        return BudgetStatus.HEALTHY

    def _check_time(self) -> BudgetStatus:
        if self.config.time_budget_secs <= 0:
            return BudgetStatus.HEALTHY
        if self.state.elapsed_secs >= self.config.time_budget_secs:
            return BudgetStatus.EXHAUSTED
        pct = self.state.elapsed_secs / self.config.time_budget_secs
        if pct >= 0.95: return BudgetStatus.CRITICAL
        if pct >= 0.80: return BudgetStatus.WARNING
        return BudgetStatus.HEALTHY

    # ─── Token/Cost Recording ───

    def record_usage(
        self,
        agent_id: str,
        batch_id: str,
        round_num: int,
        input_tokens: int,
        output_tokens: int,
        severity: int = 0,
    ) -> TokenLedger:
        """Record token usage and update budgets."""
        cost_input = (input_tokens / 1_000_000) * self.config.cost_per_mtok_input
        cost_output = (output_tokens / 1_000_000) * self.config.cost_per_mtok_output
        cost = cost_input + cost_output

        self.state.tokens_used += input_tokens + output_tokens
        self.state.cost_used += cost

        # Per-agent
        self.state.agent_tokens[agent_id] += input_tokens + output_tokens
        self.state.agent_costs[agent_id] += cost
        self.state.agent_turns[agent_id] += 1

        # Per-tier
        tier = self._severity_to_tier(severity)
        self.state.tier_tokens[tier] += input_tokens + output_tokens
        self.state.tier_cost[tier] += cost

        return TokenLedger(
            agent_id=agent_id, batch_id=batch_id, round_num=round_num,
            input_tokens=input_tokens, output_tokens=output_tokens,
            cost_usd=cost,
        )

    # ─── Tiered Spending ───

    def _severity_to_tier(self, severity: int) -> SeverityTier:
        if severity <= 3: return SeverityTier.LOW
        if severity <= 7: return SeverityTier.MEDIUM
        return SeverityTier.HIGH

    def can_spend_on_severity(self, severity: int, estimated_tokens: int) -> bool:
        """Check if there's budget remaining for this severity tier."""
        tier = self._severity_to_tier(severity)
        tier_limit = {
            SeverityTier.LOW: self.config.low_severity_pct,
            SeverityTier.MEDIUM: self.config.medium_severity_pct,
            SeverityTier.HIGH: self.config.high_severity_pct,
        }[tier]

        max_tier_tokens = int(self.config.token_budget * tier_limit)
        projected = self.state.tier_tokens[tier] + estimated_tokens
        if tier == SeverityTier.HIGH:
            # High severity effectively uncapped (consumes remaining)
            return self.state.tokens_used + estimated_tokens <= self.config.token_budget
        return projected <= max_tier_tokens

    # ─── Severity-Gated Override ───

    def request_override(
        self,
        finding: dict,
        estimated_additional_tokens: int,
    ) -> tuple[bool, str]:
        """Request a budget override for a critical finding."""
        severity = finding.get("severity_estimate", 0)
        if severity < self.config.override_severity_threshold:
            return False, f"Severity {severity} below override threshold {self.config.override_severity_threshold}"

        max_allowed = self.config.token_budget * self.config.override_max_multiplier
        projected = self.state.tokens_used + estimated_additional_tokens
        if projected > max_allowed:
            return False, f"Override would exceed {self.config.override_max_multiplier}x budget cap"

        self.state.overrides.append({
            "timestamp": time.time(),
            "severity": severity,
            "finding": finding.get("claim", "")[:100],
            "estimated_tokens": estimated_additional_tokens,
            "tokens_used_at_override": self.state.tokens_used,
            "approved": True,
        })

        logger.warning("budget_override_approved",
            severity=severity,
            tokens=estimated_additional_tokens,
            total_overrides=len(self.state.overrides))

        return True, "Override approved"

    # ─── Anomaly Detection ───

    def detect_anomaly(self) -> list[dict]:
        """Detect agents consuming disproportionate share of budget."""
        total_agents = len(self.state.agent_tokens)
        if total_agents < 2:
            return []

        total_tokens = self.state.tokens_used
        if total_tokens == 0:
            return []

        anomalies = []
        for agent_id, tokens in self.state.agent_tokens.items():
            share = tokens / total_tokens
            if share > self.config.max_agent_token_share:
                # Compute z-score
                mean = total_tokens / total_agents
                variance = sum((t - mean) ** 2 for t in self.state.agent_tokens.values()) / total_agents
                std = math.sqrt(variance) if variance > 0 else 1.0
                zscore = (tokens - mean) / std

                anomalies.append({
                    "agent_id": agent_id,
                    "token_share": round(share, 3),
                    "zscore": round(zscore, 2),
                    "tokens": tokens,
                    "action": "cap_tokens" if zscore > self.config.anomaly_zscore_threshold else "warn",
                })

        anomalies.sort(key=lambda a: a["token_share"], reverse=True)
        return anomalies

    def should_cap_agent(self, agent_id: str) -> tuple[bool, int]:
        """Determine if an agent should be token-capped. Returns (should_cap, max_tokens_per_turn)."""
        anomalies = self.detect_anomaly()
        for a in anomalies:
            if a["agent_id"] == agent_id and a["action"] == "cap_tokens":
                return True, 500  # Hard cap at 500 tokens per turn
        return False, 0

    # ─── Information-Theoretic Stopping ───

    def record_finding(self, is_novel: bool) -> None:
        """Record whether a finding was novel (new code location, new failure mode)."""
        self._novelty_window.append(is_novel)
        if len(self._novelty_window) > self.config.novelty_window * 3:
            self._novelty_window = self._novelty_window[-self.config.novelty_window * 2:]

    def should_stop_early(self) -> tuple[bool, str]:
        """Check if marginal information gain has dropped below threshold."""
        if len(self._novelty_window) < self.config.novelty_window:
            return False, ""

        recent = self._novelty_window[-self.config.novelty_window:]
        novel_count = sum(1 for n in recent if n)
        novel_rate = novel_count / len(recent)

        if novel_rate < self.config.novelty_threshold:
            return True, f"Novelty rate {novel_rate:.1%} below threshold {self.config.novelty_threshold:.1%}"
        return False, ""

    # ─── Cost Attribution ───

    def get_attribution(self) -> dict:
        """Get cost breakdown by agent and tier."""
        return {
            "by_agent": {
                aid: {
                    "tokens": self.state.agent_tokens[aid],
                    "cost": round(self.state.agent_costs[aid], 6),
                    "turns": self.state.agent_turns[aid],
                    "cost_per_turn": round(self.state.agent_costs[aid] / max(self.state.agent_turns[aid], 1), 6),
                }
                for aid in self.state.agent_tokens
            },
            "by_tier": {
                tier.value: {
                    "tokens": self.state.tier_tokens[tier],
                    "cost": round(self.state.tier_cost[tier], 6),
                }
                for tier in SeverityTier
            },
            "overrides": len(self.state.overrides),
            "total": {
                "tokens": self.state.tokens_used,
                "cost": round(self.state.cost_used, 6),
                "elapsed_secs": round(self.state.elapsed_secs, 1),
            },
        }

    def get_budget_bars(self) -> dict:
        """Get budget burn percentages for visualization."""
        self.state.update_elapsed()
        return {
            "token_pct": min(100.0, (self.state.tokens_used / self.config.token_budget) * 100) if self.config.token_budget else 0,
            "cost_pct": min(100.0, (self.state.cost_used / self.config.cost_budget_usd) * 100) if self.config.cost_budget_usd else 0,
            "time_pct": min(100.0, (self.state.elapsed_secs / self.config.time_budget_secs) * 100) if self.config.time_budget_secs else 0,
        }

    def status_report(self) -> dict:
        """Full status report for CLI/TUI display."""
        statuses = self.check_all()
        bars = self.get_budget_bars()
        anomalies = self.detect_anomaly()
        early_stop, stop_reason = self.should_stop_early()

        return {
            "status": {k: v.value for k, v in statuses.items()},
            "budget_bars": bars,
            "anomalies": anomalies,
            "early_stop": early_stop,
            "early_stop_reason": stop_reason,
            "is_exhausted": self.is_exhausted(),
            "attribution": self.get_attribution(),
        }
