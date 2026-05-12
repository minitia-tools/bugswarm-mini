"""Budget Types & Controller — triple budget enforcement (token, cost, time).

Rule: Budget checks run before every round and every batch. State is tracked
per-agent and per-severity-tier. Override history preserved for audit.
"""

from __future__ import annotations

import math
import time
from collections import defaultdict
from dataclasses import dataclass, field
from enum import Enum

import structlog

logger = structlog.get_logger(__name__)


class BudgetStatus(str, Enum):
    HEALTHY = "healthy"
    WARNING = "warning"      # >80%
    CRITICAL = "critical"    # >95%
    EXHAUSTED = "exhausted"


class SeverityTier(str, Enum):
    LOW = "low"        # 1-3
    MEDIUM = "medium"  # 4-7
    HIGH = "high"      # 8-10


@dataclass
class TokenLedger:
    agent_id: str = ""
    batch_id: str = ""
    round_num: int = 0
    input_tokens: int = 0
    output_tokens: int = 0
    cost_usd: float = 0.0
    timestamp: float = field(default_factory=time.time)


@dataclass
class BudgetConfig:
    token_budget: int = 5_000_000
    cost_budget_usd: float = 50.0
    time_budget_secs: float = 144_000  # 40 hours
    low_severity_pct: float = 0.10
    medium_severity_pct: float = 0.40
    high_severity_pct: float = 0.50
    override_max_multiplier: float = 1.5
    override_severity_threshold: int = 8
    anomaly_zscore_threshold: float = 3.0
    max_agent_token_share: float = 0.30
    novelty_threshold: float = 0.05
    novelty_window: int = 10
    cost_per_mtok_input: float = 0.14    # $0.14/1M input tokens (DeepSeek V4)
    cost_per_mtok_output: float = 0.28   # $0.28/1M output tokens


@dataclass
class BudgetState:
    tokens_used: int = 0
    cost_used: float = 0.0
    start_time: float = field(default_factory=time.time)
    elapsed_secs: float = 0.0
    tier_tokens: dict[SeverityTier, int] = field(default_factory=lambda: defaultdict(int))
    tier_cost: dict[SeverityTier, float] = field(default_factory=lambda: defaultdict(float))
    agent_tokens: dict[str, int] = field(default_factory=lambda: defaultdict(int))
    agent_costs: dict[str, float] = field(default_factory=lambda: defaultdict(float))
    agent_turns: dict[str, int] = field(default_factory=lambda: defaultdict(int))
    overrides: list[dict] = field(default_factory=list)
    recent_findings: list[bool] = field(default_factory=list)

    def update_elapsed(self) -> None:
        self.elapsed_secs = time.time() - self.start_time

    def to_dict(self) -> dict:
        self.update_elapsed()
        return {
            "tokens_used": self.tokens_used, "cost_used": round(self.cost_used, 6),
            "elapsed_secs": round(self.elapsed_secs, 1), "overrides": len(self.overrides),
        }


class BudgetController:
    """Enforces triple budget constraints (token, cost, time)."""

    def __init__(self, config: BudgetConfig | None = None):
        self.config = config or BudgetConfig()
        self.state = BudgetState()

    # ─── Checks ───

    def check_all(self) -> dict[str, BudgetStatus]:
        self.state.update_elapsed()
        return {"token": self._check_token(), "cost": self._check_cost(), "time": self._check_time()}

    def is_exhausted(self) -> bool:
        return any(s == BudgetStatus.EXHAUSTED for s in self.check_all().values())

    def _check_token(self) -> BudgetStatus:
        pct = self.state.tokens_used / max(self.config.token_budget, 1)
        if pct >= 1.0: return BudgetStatus.EXHAUSTED
        if pct >= 0.95: return BudgetStatus.CRITICAL
        if pct >= 0.80: return BudgetStatus.WARNING
        return BudgetStatus.HEALTHY

    def _check_cost(self) -> BudgetStatus:
        if self.config.cost_budget_usd <= 0: return BudgetStatus.HEALTHY
        pct = self.state.cost_used / self.config.cost_budget_usd
        if pct >= 1.0: return BudgetStatus.EXHAUSTED
        if pct >= 0.95: return BudgetStatus.CRITICAL
        if pct >= 0.80: return BudgetStatus.WARNING
        return BudgetStatus.HEALTHY

    def _check_time(self) -> BudgetStatus:
        if self.config.time_budget_secs <= 0: return BudgetStatus.HEALTHY
        if self.state.elapsed_secs >= self.config.time_budget_secs: return BudgetStatus.EXHAUSTED
        pct = self.state.elapsed_secs / self.config.time_budget_secs
        if pct >= 0.95: return BudgetStatus.CRITICAL
        if pct >= 0.80: return BudgetStatus.WARNING
        return BudgetStatus.HEALTHY

    # ─── Recording ───

    def record_usage(self, agent_id: str, batch_id: str, round_num: int,
                     input_tokens: int, output_tokens: int,
                     severity: int = 0) -> TokenLedger:
        cost = ((input_tokens / 1_000_000) * self.config.cost_per_mtok_input +
                (output_tokens / 1_000_000) * self.config.cost_per_mtok_output)

        self.state.tokens_used += input_tokens + output_tokens
        self.state.cost_used += cost
        self.state.agent_tokens[agent_id] += input_tokens + output_tokens
        self.state.agent_costs[agent_id] += cost
        self.state.agent_turns[agent_id] += 1

        tier = self._severity_to_tier(severity)
        self.state.tier_tokens[tier] += input_tokens + output_tokens
        self.state.tier_cost[tier] += cost

        return TokenLedger(agent_id=agent_id, batch_id=batch_id, round_num=round_num,
                          input_tokens=input_tokens, output_tokens=output_tokens, cost_usd=cost)

    def _severity_to_tier(self, severity: int) -> SeverityTier:
        if severity <= 3: return SeverityTier.LOW
        if severity <= 7: return SeverityTier.MEDIUM
        return SeverityTier.HIGH

    # ─── Attribution ───

    def get_attribution(self) -> dict:
        return {
            "by_agent": {aid: {"tokens": self.state.agent_tokens[aid],
                               "cost": round(self.state.agent_costs[aid], 6),
                               "turns": self.state.agent_turns[aid]}
                         for aid in self.state.agent_tokens},
            "by_tier": {tier.value: {"tokens": self.state.tier_tokens[tier],
                                     "cost": round(self.state.tier_cost[tier], 6)}
                        for tier in SeverityTier},
            "total": {"tokens": self.state.tokens_used,
                      "cost": round(self.state.cost_used, 6),
                      "elapsed_secs": round(self.state.elapsed_secs, 1)},
        }

    def get_budget_bars(self) -> dict:
        self.state.update_elapsed()
        return {
            "token_pct": min(100.0, (self.state.tokens_used / max(self.config.token_budget, 1)) * 100),
            "cost_pct": min(100.0, (self.state.cost_used / max(self.config.cost_budget_usd, 1e-9)) * 100),
            "time_pct": min(100.0, (self.state.elapsed_secs / max(self.config.time_budget_secs, 1)) * 100),
        }
