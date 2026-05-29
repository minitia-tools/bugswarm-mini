from __future__ import annotations

import json
import sqlite3
import time
from dataclasses import asdict, dataclass
from pathlib import Path

from .protocol import ChatResponse, ModelInfo

USAGE_DB_PATH = Path.home() / ".config" / "bugswarm" / "usage.db"


@dataclass
class RunCostSnapshot:
    model: str
    provider: str
    input_cost_per_mtok: float
    output_cost_per_mtok: float
    registry_version: int = 1
    registry_refreshed_at: float = 0.0
    stale_days: int = 0

    @property
    def is_stale(self) -> bool:
        return self.stale_days > 7

    @property
    def warning(self) -> str | None:
        if self.stale_days > 30:
            return f"Price estimate may be inaccurate — model prices last refreshed {self.stale_days} days ago"
        if self.stale_days > 7:
            return f"Price estimate last refreshed {self.stale_days} days ago"
        return None

    @property
    def input_cost_per_token(self) -> float:
        return self.input_cost_per_mtok / 1_000_000

    @property
    def output_cost_per_token(self) -> float:
        return self.output_cost_per_mtok / 1_000_000


class CostTracker:
    def __init__(self, budget_tokens: int = 10_000_000, budget_dollars: float | None = None):
        self.budget_tokens = budget_tokens
        self.budget_dollars = budget_dollars
        self.tokens_consumed: int = 0
        self.total_input_tokens: int = 0
        self.total_output_tokens: int = 0
        self.run_cost_snapshot: RunCostSnapshot | None = None

    def record_usage(self, response: ChatResponse) -> None:
        self.total_input_tokens += response.usage_input_tokens
        self.total_output_tokens += response.usage_output_tokens
        self.tokens_consumed += response.total_tokens

    def record_usage_raw(self, input_tokens: int, output_tokens: int) -> None:
        self.total_input_tokens += input_tokens
        self.total_output_tokens += output_tokens
        self.tokens_consumed += input_tokens + output_tokens

    def set_run_snapshot(self, model_info: ModelInfo, registry_refreshed_at: float = 0.0) -> None:
        if model_info.pricing:
            stale_days = int((time.time() - registry_refreshed_at) / 86400) if registry_refreshed_at > 0 else 0
            self.run_cost_snapshot = RunCostSnapshot(
                model=model_info.id,
                provider=model_info.provider_label,
                input_cost_per_mtok=model_info.pricing.input_cost_per_mtok,
                output_cost_per_mtok=model_info.pricing.output_cost_per_mtok,
                registry_refreshed_at=registry_refreshed_at,
                stale_days=stale_days,
            )
        else:
            self.run_cost_snapshot = None

    def estimated_cost_dollars(self) -> float | None:
        if not self.run_cost_snapshot or self.tokens_consumed == 0:
            return None
        s = self.run_cost_snapshot
        in_cost = self.total_input_tokens * s.input_cost_per_token
        out_cost = self.total_output_tokens * s.output_cost_per_token
        return round(in_cost + out_cost, 6)

    def is_budget_exhausted(self) -> bool:
        if self.tokens_consumed >= self.budget_tokens:
            return True
        if self.budget_dollars is not None and self.run_cost_snapshot:
            est = self.estimated_cost_dollars()
            if est is not None and est >= self.budget_dollars:
                return True
        return False

    def budget_summary(self) -> dict:
        est_cost = self.estimated_cost_dollars()
        return {
            "tokens_consumed": self.tokens_consumed,
            "budget_tokens": self.budget_tokens,
            "tokens_remaining": max(0, self.budget_tokens - self.tokens_consumed),
            "budget_pct": round(self.tokens_consumed / self.budget_tokens * 100, 1) if self.budget_tokens else 0,
            "estimated_cost_dollars": est_cost,
            "budget_dollars": self.budget_dollars,
        }

    def budget_bar(self, width: int = 30) -> str:
        pct = self.budget_summary()["budget_pct"]
        filled = int(pct / 100 * width)
        bar = "█" * filled + "░" * (width - filled)
        color = "green"
        if pct > 80:
            color = "red"
        elif pct > 50:
            color = "yellow"
        ct = self.tokens_consumed
        bt = self.budget_tokens
        return f"[{color}]{bar}[/{color}] {ct:,} / {bt:,} tokens ({pct}%)"


class UsageDB:
    def __init__(self, db_path: Path = USAGE_DB_PATH):
        self.db_path = db_path
        self._init_db()

    def _get_conn(self) -> sqlite3.Connection:
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        conn = sqlite3.connect(str(self.db_path))
        conn.row_factory = sqlite3.Row
        return conn

    def _init_db(self) -> None:
        with self._get_conn() as conn:
            conn.execute("""
                CREATE TABLE IF NOT EXISTS runs (
                    id TEXT PRIMARY KEY,
                    repo TEXT NOT NULL DEFAULT '',
                    model TEXT NOT NULL,
                    provider TEXT NOT NULL DEFAULT '',
                    started_at TEXT NOT NULL,
                    duration_secs REAL DEFAULT 0,
                    tokens_consumed INTEGER DEFAULT 0,
                    input_tokens INTEGER DEFAULT 0,
                    output_tokens INTEGER DEFAULT 0,
                    budget_tokens INTEGER DEFAULT 10000000,
                    price_snapshot_json TEXT DEFAULT '{}',
                    estimated_cost_dollars REAL,
                    findings_count INTEGER DEFAULT 0,
                    verified_findings INTEGER DEFAULT 0,
                    stale_warning TEXT
                )
            """)
            conn.execute("""
                CREATE INDEX IF NOT EXISTS idx_runs_started
                ON runs(started_at DESC)
            """)

    def save_run(
        self,
        run_id: str,
        model: str,
        provider: str,
        tokens_consumed: int,
        input_tokens: int,
        output_tokens: int,
        budget_tokens: int,
        price_snapshot: RunCostSnapshot | None,
        estimated_cost: float | None,
        duration_secs: float = 0,
        repo: str = "",
        findings_count: int = 0,
        verified_findings: int = 0,
    ) -> None:
        import datetime

        with self._get_conn() as conn:
            conn.execute(
                """
                INSERT OR REPLACE INTO runs
                (id, repo, model, provider, started_at, duration_secs,
                 tokens_consumed, input_tokens, output_tokens, budget_tokens,
                 price_snapshot_json, estimated_cost_dollars,
                 findings_count, verified_findings, stale_warning)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
                (
                    run_id,
                    repo,
                    model,
                    provider,
                    datetime.datetime.now(datetime.UTC).isoformat(),
                    duration_secs,
                    tokens_consumed,
                    input_tokens,
                    output_tokens,
                    budget_tokens,
                    json.dumps(asdict(price_snapshot) if price_snapshot else {}),
                    estimated_cost,
                    findings_count,
                    verified_findings,
                    price_snapshot.warning if price_snapshot else None,
                ),
            )

    def get_recent_runs(self, limit: int = 20) -> list[dict]:
        with self._get_conn() as conn:
            rows = conn.execute("SELECT * FROM runs ORDER BY started_at DESC LIMIT ?", (limit,)).fetchall()
            return [dict(r) for r in rows]

    def get_summary(self) -> dict:
        with self._get_conn() as conn:
            total = conn.execute("SELECT COUNT(*) FROM runs").fetchone()[0]
            tokens = conn.execute("SELECT COALESCE(SUM(tokens_consumed), 0) FROM runs").fetchone()[0]
            cost = conn.execute(
                "SELECT COALESCE(SUM(estimated_cost_dollars), 0) FROM runs WHERE estimated_cost_dollars IS NOT NULL"
            ).fetchone()[0]
            models = conn.execute("SELECT COUNT(DISTINCT model) FROM runs").fetchone()[0]
            verified = conn.execute("SELECT COALESCE(SUM(verified_findings), 0) FROM runs").fetchone()[0]
            return {
                "total_runs": total,
                "total_tokens": tokens,
                "total_cost": round(cost, 4),
                "models_used": models,
                "total_verified_findings": verified,
            }

    def get_model_breakdown(self) -> list[dict]:
        with self._get_conn() as conn:
            rows = conn.execute("""
                SELECT model,
                       COUNT(*) as runs,
                       COALESCE(SUM(tokens_consumed), 0) as tokens,
                       COALESCE(SUM(estimated_cost_dollars), 0) as cost,
                       COALESCE(SUM(verified_findings), 0) as findings
                FROM runs
                GROUP BY model
                ORDER BY cost DESC
            """).fetchall()
            return [dict(r) for r in rows]
