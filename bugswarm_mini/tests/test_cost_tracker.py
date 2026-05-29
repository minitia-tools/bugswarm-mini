from __future__ import annotations

import pytest

from bugswarm_mini.gateway.cost_tracker import CostTracker, RunCostSnapshot, UsageDB
from bugswarm_mini.gateway.protocol import ModelInfo, ModelPricing, ModelCapabilities, ChatResponse


class TestRunCostSnapshot:
    def test_no_warning_when_fresh(self):
        snap = RunCostSnapshot(
            model="test",
            provider="Test",
            input_cost_per_mtok=1.0,
            output_cost_per_mtok=2.0,
            stale_days=0,
        )
        assert snap.warning is None
        assert snap.is_stale is False

    def test_warning_when_stale_week(self):
        snap = RunCostSnapshot(
            model="test",
            provider="Test",
            input_cost_per_mtok=1.0,
            output_cost_per_mtok=2.0,
            stale_days=10,
        )
        assert snap.warning is not None
        assert "10 days" in snap.warning
        assert snap.is_stale is True

    def test_warning_when_stale_month(self):
        snap = RunCostSnapshot(
            model="test",
            provider="Test",
            input_cost_per_mtok=1.0,
            output_cost_per_mtok=2.0,
            stale_days=35,
        )
        assert snap.warning is not None
        assert "inaccurate" in snap.warning
        assert snap.is_stale is True

    def test_per_token_conversion(self):
        snap = RunCostSnapshot(
            model="test",
            provider="Test",
            input_cost_per_mtok=1.0,
            output_cost_per_mtok=2.0,
        )
        assert snap.input_cost_per_token == 0.000001
        assert snap.output_cost_per_token == 0.000002


class TestCostTracker:
    def test_initial_state(self):
        tracker = CostTracker(budget_tokens=1_000_000)
        assert tracker.tokens_consumed == 0
        assert tracker.budget_tokens == 1_000_000
        assert tracker.budget_dollars is None
        assert tracker.run_cost_snapshot is None

    def test_record_usage(self):
        tracker = CostTracker()
        resp = ChatResponse(
            content="Hello",
            model="test",
            usage_input_tokens=100,
            usage_output_tokens=50,
            duration_ms=100.0,
        )
        tracker.record_usage(resp)
        assert tracker.tokens_consumed == 150
        assert tracker.total_input_tokens == 100
        assert tracker.total_output_tokens == 50

    def test_record_usage_multiple(self):
        tracker = CostTracker()
        for _ in range(3):
            tracker.record_usage_raw(input_tokens=100, output_tokens=50)
        assert tracker.tokens_consumed == 450

    def test_budget_exhausted_with_tokens(self):
        tracker = CostTracker(budget_tokens=100)
        tracker.record_usage_raw(input_tokens=80, output_tokens=30)
        assert tracker.is_budget_exhausted() is True

    def test_budget_not_exhausted(self):
        tracker = CostTracker(budget_tokens=1000)
        tracker.record_usage_raw(input_tokens=100, output_tokens=50)
        assert tracker.is_budget_exhausted() is False

    def test_budget_exhausted_with_dollars(self):
        tracker = CostTracker(budget_tokens=10_000_000, budget_dollars=0.01)
        model_info = ModelInfo(
            id="test-model",
            provider_label="Test",
            protocol="openai-compatible",
            pricing=ModelPricing(input_cost_per_mtok=100.0, output_cost_per_mtok=200.0),
        )
        tracker.set_run_snapshot(model_info)
        tracker.record_usage_raw(input_tokens=100, output_tokens=50)
        assert tracker.is_budget_exhausted() is True

    def test_estimated_cost(self):
        tracker = CostTracker()
        model_info = ModelInfo(
            id="test-model",
            provider_label="Test",
            protocol="openai-compatible",
            pricing=ModelPricing(input_cost_per_mtok=1.0, output_cost_per_mtok=2.0),
        )
        tracker.set_run_snapshot(model_info)
        tracker.record_usage_raw(input_tokens=1_000_000, output_tokens=500_000)
        cost = tracker.estimated_cost_dollars()
        assert cost is not None
        assert cost == pytest.approx(1.0 + 1.0, rel=0.01)  # 1M * $1/1M + 500K * $2/1M

    def test_estimated_cost_no_pricing(self):
        tracker = CostTracker()
        tracker.record_usage_raw(input_tokens=100, output_tokens=50)
        assert tracker.estimated_cost_dollars() is None

    def test_budget_summary(self):
        tracker = CostTracker(budget_tokens=1000)
        tracker.record_usage_raw(input_tokens=200, output_tokens=100)
        summary = tracker.budget_summary()
        assert summary["tokens_consumed"] == 300
        assert summary["budget_tokens"] == 1000
        assert summary["tokens_remaining"] == 700
        assert summary["budget_pct"] == 30.0

    def test_budget_bar(self):
        tracker = CostTracker(budget_tokens=100)
        tracker.record_usage_raw(input_tokens=50, output_tokens=0)
        bar = tracker.budget_bar(width=10)
        assert "█" in bar
        assert "░" in bar
        assert "50" in bar
        assert "100" in bar

    def test_budget_bar_exact_percentages(self):
        tracker = CostTracker(budget_tokens=100)
        tracker.record_usage_raw(input_tokens=100, output_tokens=0)
        bar = tracker.budget_bar(width=10)
        assert "█" * 10 in bar
        assert "100" in bar


class TestUsageDB:
    def test_init_creates_table(self):
        import tempfile
        db = UsageDB(db_path=Path(tempfile.mktemp(suffix=".db")))
        summary = db.get_summary()
        assert summary["total_runs"] == 0
        assert summary["total_tokens"] == 0

    def test_save_run_and_summary(self):
        import tempfile
        db_path = Path(tempfile.mktemp(suffix=".db"))
        db = UsageDB(db_path=db_path)

        snap = RunCostSnapshot(
            model="test-model",
            provider="Test",
            input_cost_per_mtok=1.0,
            output_cost_per_mtok=2.0,
        )

        db.save_run(
            run_id="run-001",
            model="test-model",
            provider="Test",
            tokens_consumed=1500,
            input_tokens=1000,
            output_tokens=500,
            budget_tokens=10000,
            price_snapshot=snap,
            estimated_cost=0.002,
            repo="/test/repo",
            findings_count=3,
            verified_findings=2,
        )

        summary = db.get_summary()
        assert summary["total_runs"] == 1
        assert summary["total_tokens"] == 1500
        assert summary["total_cost"] == 0.002
        assert summary["total_verified_findings"] == 2

        runs = db.get_recent_runs(limit=10)
        assert len(runs) == 1
        assert runs[0]["model"] == "test-model"

    def test_consolidates_pricing(self):
        import tempfile
        db_path = Path(tempfile.mktemp(suffix=".db"))
        db = UsageDB(db_path=db_path)

        for i in range(3):
            db.save_run(
                run_id=f"run-{i:03d}",
                model="test-model",
                provider="Test",
                tokens_consumed=1000,
                input_tokens=500,
                output_tokens=500,
                budget_tokens=10000,
                price_snapshot=None,
                estimated_cost=0.001,
            )

        summary = db.get_summary()
        assert summary["total_runs"] == 3
        assert summary["total_tokens"] == 3000

    def test_multiple_models(self):
        import tempfile
        db_path = Path(tempfile.mktemp(suffix=".db"))
        db = UsageDB(db_path=db_path)

        for i, model in enumerate(["model-a", "model-b", "model-a"]):
            db.save_run(
                run_id=f"run-{i:03d}",
                model=model,
                provider="Test",
                tokens_consumed=1000,
                input_tokens=500,
                output_tokens=500,
                budget_tokens=10000,
                price_snapshot=None,
                estimated_cost=0.001,
            )

        breakdown = db.get_model_breakdown()
        assert len(breakdown) == 2
        model_a = next(m for m in breakdown if m["model"] == "model-a")
        assert model_a["runs"] == 2
        assert model_a["tokens"] == 2000

    def test_empty_db_returns_empty_recent(self):
        import tempfile
        db_path = Path(tempfile.mktemp(suffix=".db"))
        db = UsageDB(db_path=db_path)
        runs = db.get_recent_runs(limit=10)
        assert runs == []

    def test_empty_model_breakdown(self):
        import tempfile
        db_path = Path(tempfile.mktemp(suffix=".db"))
        db = UsageDB(db_path=db_path)
        assert db.get_model_breakdown() == []


from pathlib import Path
