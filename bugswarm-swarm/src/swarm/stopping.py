"""Information-Theoretic Stopping — terminate when marginal information gain drops.

Rule: <5% novel findings in last 10 → early termination. Saves budget.
"""

from __future__ import annotations

import structlog

from .budget import BudgetConfig, BudgetState, BudgetController

logger = structlog.get_logger(__name__)


class InfoTheoreticStopping:
    """Stops the swarm when marginal information gain drops below threshold."""

    def __init__(self, config: BudgetConfig, state: BudgetState):
        self.config = config
        self.state = state
        self._novelty_window: list[bool] = []

    def record(self, is_novel: bool) -> None:
        self._novelty_window.append(is_novel)
        max_size = self.config.novelty_window * 3
        if len(self._novelty_window) > max_size:
            self._novelty_window = self._novelty_window[-max_size:]

    def should_stop(self) -> tuple[bool, str]:
        if len(self._novelty_window) < self.config.novelty_window:
            return False, ""
        recent = self._novelty_window[-self.config.novelty_window:]
        novel_count = sum(1 for n in recent if n)
        rate = novel_count / len(recent)
        if rate < self.config.novelty_threshold:
            return True, f"Novelty rate {rate:.1%} below threshold {self.config.novelty_threshold:.1%}"
        return False, ""


class FinancialController:
    """Unified financial controller composing BudgetController + TieredSpending + BudgetOverride + AnomalyDetector + InfoTheoreticStopping."""

    def __init__(self, config: BudgetConfig | None = None):
        cfg = config or BudgetConfig()
        self.budget = BudgetController(cfg)
        from .spending import TieredSpending, BudgetOverride, AnomalyDetector
        self.spending = TieredSpending(cfg, self.budget.state)
        self.override = BudgetOverride(cfg, self.budget.state)
        self.anomaly = AnomalyDetector(cfg, self.budget.state)
        self.stopping = InfoTheoreticStopping(cfg, self.budget.state)

    # Passthrough for backward compat
    def check_all(self): return self.budget.check_all()
    def is_exhausted(self): return self.budget.is_exhausted()
    def record_usage(self, *a, **kw): return self.budget.record_usage(*a, **kw)
    def can_spend_on_severity(self, s, t): return self.spending.can_spend(s, t)
    def request_override(self, f, t): return self.override.request(f, t)
    def detect_anomaly(self): return self.anomaly.detect()
    def should_cap_agent(self, a): return self.anomaly.should_cap(a)
    def record_finding(self, n): self.stopping.record(n)
    def should_stop_early(self): return self.stopping.should_stop()
    def get_attribution(self): return self.budget.get_attribution()
    def get_budget_bars(self): return self.budget.get_budget_bars()

    def status_report(self) -> dict:
        statuses = self.check_all()
        bars = self.get_budget_bars()
        anomalies = self.detect_anomaly()
        early_stop, reason = self.should_stop_early()
        return {
            "status": {k: v.value for k, v in statuses.items()},
            "budget_bars": bars, "anomalies": anomalies,
            "early_stop": early_stop, "early_stop_reason": reason,
            "is_exhausted": self.is_exhausted(),
            "attribution": self.get_attribution(),
        }
