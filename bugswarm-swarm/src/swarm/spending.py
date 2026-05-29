"""Tiered Spending, Override, and Anomaly Detection.

Rule: Severity 1-3 gets 10% of budget. Severity 8+ triggers override up to 150%.
Agents consuming >30% share get token-capped at 500 tokens/turn.
"""

from __future__ import annotations

import math
import time

import structlog

from .budget import BudgetConfig, BudgetState, SeverityTier

logger = structlog.get_logger(__name__)


class TieredSpending:
    """Enforces severity-gated budget allocation."""

    def __init__(self, config: BudgetConfig, state: BudgetState):
        self.config = config
        self.state = state

    def can_spend(self, severity: int, estimated_tokens: int) -> bool:
        tier = self._tier(severity)
        limits = {
            SeverityTier.LOW: self.config.low_severity_pct,
            SeverityTier.MEDIUM: self.config.medium_severity_pct,
            SeverityTier.HIGH: self.config.high_severity_pct,
        }
        max_tier = int(self.config.token_budget * limits[tier])
        projected = self.state.tier_tokens[tier] + estimated_tokens
        if tier == SeverityTier.HIGH:
            return self.state.tokens_used + estimated_tokens <= self.config.token_budget
        return projected <= max_tier

    @staticmethod
    def _tier(severity: int) -> SeverityTier:
        if severity <= 3:
            return SeverityTier.LOW
        if severity <= 7:
            return SeverityTier.MEDIUM
        return SeverityTier.HIGH


class BudgetOverride:
    """Severity-gated budget override for critical findings."""

    def __init__(self, config: BudgetConfig, state: BudgetState):
        self.config = config
        self.state = state

    def request(self, finding: dict, estimated_tokens: int) -> tuple[bool, str]:
        severity = finding.get("severity_estimate", 0)
        if severity < self.config.override_severity_threshold:
            return False, f"Severity {severity} below threshold {self.config.override_severity_threshold}"

        max_allowed = int(self.config.token_budget * self.config.override_max_multiplier)
        projected = self.state.tokens_used + estimated_tokens
        if projected > max_allowed:
            return False, f"Override would exceed {self.config.override_max_multiplier}x cap"

        self.state.overrides.append(
            {
                "timestamp": time.time(),
                "severity": severity,
                "finding": finding.get("claim", "")[:100],
                "estimated_tokens": estimated_tokens,
                "tokens_used_at_override": self.state.tokens_used,
                "approved": True,
            }
        )
        logger.warning(
            "budget_override_approved", severity=severity, tokens=estimated_tokens, total=len(self.state.overrides)
        )
        return True, "Override approved"


class AnomalyDetector:
    """Detects agents consuming disproportionate share of budget via z-score."""

    def __init__(self, config: BudgetConfig, state: BudgetState):
        self.config = config
        self.state = state

    def detect(self) -> list[dict]:
        total_agents = len(self.state.agent_tokens)
        if total_agents < 2 or self.state.tokens_used == 0:
            return []

        anomalies = []
        total = self.state.tokens_used
        for agent_id, tokens in self.state.agent_tokens.items():
            share = tokens / total
            if share > self.config.max_agent_token_share:
                mean = total / total_agents
                variance = sum((t - mean) ** 2 for t in self.state.agent_tokens.values()) / total_agents
                std = math.sqrt(variance) if variance > 0 else 1.0
                zscore = (tokens - mean) / std
                anomalies.append(
                    {
                        "agent_id": agent_id,
                        "token_share": round(share, 3),
                        "zscore": round(zscore, 2),
                        "tokens": tokens,
                        "action": "cap_tokens" if zscore > self.config.anomaly_zscore_threshold else "warn",
                    }
                )

        anomalies.sort(key=lambda a: a["token_share"], reverse=True)
        return anomalies

    def should_cap(self, agent_id: str) -> tuple[bool, int]:
        for a in self.detect():
            if a["agent_id"] == agent_id and a["action"] == "cap_tokens":
                return True, 500  # Hard cap 500 tokens/turn
        return False, 0
