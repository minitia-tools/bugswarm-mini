"""Phase 13: Observability & CI/CD.

Prometheus metrics, structured logging, alerting engine,
CI/CD integration command, SARIF export.
"""

from __future__ import annotations

import json, os, time, threading
from collections import defaultdict
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Callable

import structlog

logger = structlog.get_logger(__name__)


# ═══════════════════════════════════════════════════════════════
# Prometheus Metrics Registry
# ═══════════════════════════════════════════════════════════════

class MetricType(str, Enum):
    COUNTER = "counter"
    GAUGE = "gauge"
    HISTOGRAM = "histogram"


@dataclass
class Metric:
    name: str
    kind: MetricType
    help: str
    labels: dict[str, str] = field(default_factory=dict)
    value: float = 0.0
    _lock: threading.Lock = field(default_factory=threading.Lock)

    def set(self, value: float) -> None:
        with self._lock:
            self.value = value

    def inc(self, delta: float = 1.0) -> None:
        with self._lock:
            self.value += delta

    def observe(self, value: float) -> None:
        """For histograms: track value."""
        with self._lock:
            self.value = value


class MetricsRegistry:
    """Prometheus-compatible metrics registry."""

    def __init__(self):
        self._metrics: dict[str, Metric] = {}
        self._histogram_buckets: dict[str, list[float]] = {}

    def counter(self, name: str, help: str, labels: dict[str, str] | None = None) -> Metric:
        return self._register(name, MetricType.COUNTER, help, labels or {})

    def gauge(self, name: str, help: str, labels: dict[str, str] | None = None) -> Metric:
        return self._register(name, MetricType.GAUGE, help, labels or {})

    def histogram(self, name: str, help: str, buckets: list[float] | None = None,
                  labels: dict[str, str] | None = None) -> Metric:
        m = self._register(name, MetricType.HISTOGRAM, help, labels or {})
        if buckets:
            self._histogram_buckets[name] = buckets
        return m

    def _register(self, name: str, kind: MetricType, help: str, labels: dict[str, str]) -> Metric:
        if name not in self._metrics:
            self._metrics[name] = Metric(name=name, kind=kind, help=help, labels=labels)
        return self._metrics[name]

    def get(self, name: str) -> Metric | None:
        return self._metrics.get(name)

    def render_prometheus(self) -> str:
        """Render all metrics in Prometheus text format."""
        lines = []
        for name, m in sorted(self._metrics.items()):
            lines.append(f"# HELP {name} {m.help}")
            lines.append(f"# TYPE {name} {m.kind.value}")
            label_str = ",".join(f'{k}="{v}"' for k, v in m.labels.items())
            label_part = f"{{{label_str}}}" if label_str else ""
            lines.append(f"{name}{label_part} {m.value}")
        return "\n".join(lines) + "\n"

    def render_json(self) -> list[dict]:
        return [
            {"name": m.name, "type": m.kind.value, "help": m.help,
             "labels": m.labels, "value": m.value}
            for m in self._metrics.values()
        ]


# ═══════════════════════════════════════════════════════════════
# Alerting Engine
# ═══════════════════════════════════════════════════════════════

class AlertSeverity(str, Enum):
    WARN = "warn"
    CRITICAL = "critical"


@dataclass
class AlertRule:
    name: str
    description: str
    severity: AlertSeverity
    condition: Callable[[], bool]
    channel: str  # slack, pagerduty, email
    cooldown_secs: float = 300.0
    last_fired: float = 0.0
    firing: bool = False

    def evaluate(self) -> bool:
        if self.condition():
            now = time.time()
            if now - self.last_fired > self.cooldown_secs:
                self.last_fired = now
                self.firing = True
                return True
        else:
            self.firing = False
        return False


class AlertEngine:
    """Evaluates alert rules and routes to notification channels."""

    def __init__(self):
        self.rules: list[AlertRule] = []
        self._fired_history: list[dict] = []
        self._notifier: Callable[[AlertRule], None] | None = None

    def add_rule(self, rule: AlertRule) -> None:
        self.rules.append(rule)

    def set_notifier(self, notifier: Callable[[AlertRule], None]) -> None:
        self._notifier = notifier

    def evaluate_all(self) -> list[AlertRule]:
        triggered = []
        for rule in self.rules:
            if rule.evaluate():
                triggered.append(rule)
                self._fired_history.append({
                    "rule": rule.name, "severity": rule.severity.value,
                    "channel": rule.channel, "timestamp": time.time(),
                })
                if self._notifier:
                    self._notifier(rule)
        return triggered

    def active_alerts(self) -> list[dict]:
        return [
            {"name": r.name, "severity": r.severity.value, "description": r.description}
            for r in self.rules if r.firing
        ]

    def alert_history(self, limit: int = 50) -> list[dict]:
        return self._fired_history[-limit:]


# ═══════════════════════════════════════════════════════════════
# Structured Logger
# ═══════════════════════════════════════════════════════════════

class StructuredLogger:
    """Configures structured JSON logging for all components."""

    LEVELS = {"TRACE": 0, "DEBUG": 1, "INFO": 2, "WARN": 3, "ERROR": 4, "CRITICAL": 5}

    def __init__(self, level: str = "INFO"):
        self.level = level
        self.level_value = self.LEVELS.get(level.upper(), 2)
        self._handlers: list[Callable[[dict], None]] = []

    def add_handler(self, handler: Callable[[dict], None]) -> None:
        self._handlers.append(handler)

    def log(self, level: str, message: str, **context) -> None:
        lv = self.LEVELS.get(level.upper(), 2)
        if lv < self.level_value:
            return
        entry = {
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S.", time.gmtime()) + f"{time.time() % 1:.6f}"[2:8] + "Z",
            "level": level.upper(),
            "message": message,
            **context,
        }
        for handler in self._handlers:
            handler(entry)

    def trace(self, msg: str, **ctx): self.log("TRACE", msg, **ctx)
    def debug(self, msg: str, **ctx): self.log("DEBUG", msg, **ctx)
    def info(self, msg: str, **ctx): self.log("INFO", msg, **ctx)
    def warn(self, msg: str, **ctx): self.log("WARN", msg, **ctx)
    def error(self, msg: str, **ctx): self.log("ERROR", msg, **ctx)
    def critical(self, msg: str, **ctx): self.log("CRITICAL", msg, **ctx)


# ═══════════════════════════════════════════════════════════════
# SARIF Export
# ═══════════════════════════════════════════════════════════════

def export_sarif(findings: list[dict], repo_uri: str = "file:///src") -> dict:
    """Export findings as SARIF (Static Analysis Results Interchange Format).

    Compatible with GitHub Code Scanning, GitLab SAST, and Azure DevOps.
    """
    results = []
    for i, f in enumerate(findings):
        location = f.get("location", "unknown:0")
        parts = location.split(":") if ":" in location else [location, "0"]
        file_path = parts[0]
        line = int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else 1

        results.append({
            "ruleId": f"bugswarm/{f.get('claim', 'unknown')[:50].replace(' ', '-')}",
            "ruleIndex": i,
            "level": "error" if f.get("severity_estimate", 0) >= 7 else "warning",
            "message": {
                "text": f.get("claim", "Bug detected")[:200],
            },
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": {"uri": file_path, "uriBaseId": "%SRCROOT%"},
                    "region": {"startLine": line, "startColumn": 1},
                },
            }],
            "properties": {
                "severity": f.get("severity_estimate", 0),
                "verified": f.get("verified", False),
                "mechanism": f.get("mechanism", "")[:200],
            },
        })

    return {
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "Bug Swarm",
                    "version": "2.1.0",
                    "informationUri": "https://bugswarm.ai",
                    "rules": [
                        {"id": f"bugswarm/{f.get('claim','')[:50].replace(' ','-')}",
                         "shortDescription": {"text": f.get("claim", "")[:100]},
                         "helpUri": "https://bugswarm.ai/rules"}
                        for f in findings
                    ],
                },
            },
            "results": results,
            "originalUriBaseIds": {
                "SRCROOT": {"uri": repo_uri},
            },
        }],
    }


# ═══════════════════════════════════════════════════════════════
# CI/CD Command
# ═══════════════════════════════════════════════════════════════

@dataclass
class CIConfig:
    repo_path: str = "."
    diff_target: str = "origin/main...HEAD"
    severity_min: int = 7
    timeout_secs: int = 1800
    format: str = "sarif"  # json, sarif, text
    fail_on: str = "any"   # any, critical, none


def run_ci(config: CIConfig, findings_provider: Callable[[], list[dict]] | None = None) -> tuple[int, str]:
    """Run CI check against a diff or full repo.

    Returns (exit_code, output).
    0 = clean, 1 = issues found.
    """
    logger.info("ci_run_started", repo=config.repo_path, diff=config.diff_target)

    if findings_provider:
        findings = findings_provider()
    else:
        # Simulated findings (in production, runs agent/swarm)
        findings = []

    # Filter by severity
    filtered = [f for f in findings if f.get("severity_estimate", 0) >= config.severity_min]

    if config.format == "sarif":
        output = json.dumps(export_sarif(filtered, f"file://{config.repo_path}"), indent=2)
    elif config.format == "json":
        output = json.dumps(filtered, indent=2)
    else:
        lines = []
        for f in filtered:
            lines.append(f"[{f.get('severity_estimate','?')}] {f.get('location','?')}: {f.get('claim','?')}")
        output = "\n".join(lines)

    # Determine exit code
    if config.fail_on == "none":
        exit_code = 0
    elif config.fail_on == "critical":
        critical = [f for f in filtered if f.get("severity_estimate", 0) >= 9]
        exit_code = 1 if critical else 0
    else:
        exit_code = 1 if filtered else 0

    logger.info("ci_run_completed",
        findings=len(filtered), severity_min=config.severity_min, exit_code=exit_code)

    return exit_code, output


# ═══════════════════════════════════════════════════════════════
# Observability Manager
# ═══════════════════════════════════════════════════════════════

class ObservabilityManager:
    """Unified observability: metrics + logging + alerting."""

    def __init__(self):
        self.metrics = MetricsRegistry()
        self.alerts = AlertEngine()
        self.logs = StructuredLogger()
        self._setup_default_metrics()
        self._setup_default_alerts()

    def _setup_default_metrics(self) -> None:
        m = self.metrics
        m.gauge("bugswarm_agents_active", "Number of active agents")
        m.counter("bugswarm_tokens_consumed", "Total tokens consumed")
        m.counter("bugswarm_cost_usd", "Total cost in USD")
        m.counter("bugswarm_bugs_found", "Total bugs found")
        m.counter("bugswarm_sandbox_executions", "Total sandbox executions")
        m.gauge("bugswarm_sandbox_execution_seconds", "Sandbox execution duration")
        m.counter("bugswarm_judge_verdicts", "Total judge verdicts")
        m.gauge("bugswarm_debate_rounds", "Current debate round")
        m.counter("bugswarm_semantic_loops_detected", "Semantic loops detected")
        m.gauge("bugswarm_vector_db_cache_hit_ratio", "Vector DB cache hit rate")
        m.gauge("bugswarm_compression_fidelity_score", "Compression fidelity score")
        m.gauge("bugswarm_queue_depth", "DB writer queue depth")
        m.gauge("bugswarm_host_resource_usage", "Host resource usage pct")

    def _setup_default_alerts(self) -> None:
        a = self.alerts
        a.add_rule(AlertRule("cost_warning", "Cost > 80% of budget", AlertSeverity.WARN,
            lambda: self.metrics.get("bugswarm_cost_usd").value > 40.0, "slack"))
        a.add_rule(AlertRule("cost_critical", "Cost budget exhausted", AlertSeverity.CRITICAL,
            lambda: self.metrics.get("bugswarm_cost_usd").value > 50.0, "pagerduty"))
        a.add_rule(AlertRule("sandbox_tainted", "Sandbox returned tainted execution", AlertSeverity.WARN,
            lambda: False, "slack"))
        a.add_rule(AlertRule("sandbox_escape", "Sandbox escape attempt detected", AlertSeverity.CRITICAL,
            lambda: False, "pagerduty"))
        a.add_rule(AlertRule("host_memory", "Host memory > 80%", AlertSeverity.WARN,
            lambda: False, "slack"))
        a.add_rule(AlertRule("judge_degradation", "Judge dismiss rate > 90% for 3 swarms", AlertSeverity.WARN,
            lambda: False, "slack"))
        a.add_rule(AlertRule("compression_low_fidelity", "Compression fidelity < 0.7", AlertSeverity.WARN,
            lambda: False, "slack"))
        a.add_rule(AlertRule("db_replication_lag", "PostgreSQL replication lag > 5s", AlertSeverity.WARN,
            lambda: False, "pagerduty"))
        a.add_rule(AlertRule("orchestrator_heartbeat", "Orchestrator heartbeat missing > 30s", AlertSeverity.CRITICAL,
            lambda: False, "pagerduty"))

    def notify(self, rule: AlertRule) -> None:
        """Simulated notification. In production: Slack, PagerDuty, email."""
        logger.warning("alert_fired", rule=rule.name, severity=rule.severity.value, channel=rule.channel)
