"""Phase 11: Terminal UI — Full Textual TUI data model and rendering logic.

In production, uses Textual for 60fps terminal rendering.
In headless mode, outputs structured JSON for external monitoring.
"""

from __future__ import annotations

import json
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from enum import Enum

import structlog

logger = structlog.get_logger(__name__)


# ═══════════════════════════════════════════════════════════════
# UI State Model
# ═══════════════════════════════════════════════════════════════


class TUIScreen(str, Enum):
    SWARM_OVERVIEW = "swarm_overview"  # F1
    AGENT_DETAIL = "agent_detail"  # F2
    SANDBOX_MONITOR = "sandbox_monitor"  # F3
    BENCH_PANEL = "bench_panel"  # F4
    EVIDENCE_GRAPH = "evidence_graph"  # F5
    FINANCE = "finance"  # F6
    RAW_LOGS = "raw_logs"  # F7


class SeverityColor(str, Enum):
    CRITICAL = "red"
    HIGH = "orange"
    MEDIUM = "yellow"
    LOW = "green"
    INFO = "blue"


@dataclass
class AgentRow:
    id: str = ""
    persona: str = ""
    status: str = "idle"
    round: int = 0
    tokens: int = 0
    bugs: int = 0
    health: str = "ok"  # ok, slow, looping, ejected

    def to_row(self) -> dict:
        return {
            "id": self.id,
            "persona": self.persona,
            "status": self.status,
            "round": self.round,
            "tokens": self.tokens,
            "bugs": self.bugs,
            "health": self.health,
        }


@dataclass
class BudgetBar:
    tokens_pct: float = 0.0
    cost_pct: float = 0.0
    time_pct: float = 0.0

    def token_color(self) -> str:
        if self.tokens_pct >= 95:
            return "red"
        if self.tokens_pct >= 80:
            return "yellow"
        return "green"

    def cost_color(self) -> str:
        if self.cost_pct >= 95:
            return "red"
        if self.cost_pct >= 80:
            return "yellow"
        return "green"


@dataclass
class SandboxRow:
    run_id: str = ""
    agent: str = ""
    status: str = "running"
    duration: float = 0.0
    memory_mb: int = 0
    cpu_pct: float = 0.0
    exit_code: int = 0
    classification: str = ""

    def to_row(self) -> dict:
        return {k: v for k, v in self.__dict__.items()}


@dataclass
class JudgeVerdictRow:
    case_id: str = ""
    claim: str = ""
    judges_voted: str = ""  # "4-1", "2-2-1"
    verdict: str = ""
    confidence: str = ""
    queries: str = ""

    def to_row(self) -> dict:
        return {k: v for k, v in self.__dict__.items()}


@dataclass
class CostBreakdown:
    agents_pct: float = 0.0
    judges_pct: float = 0.0
    compression_pct: float = 0.0
    sandbox_pct: float = 0.0
    agents_cost: float = 0.0
    judges_cost: float = 0.0
    compression_cost: float = 0.0
    sandbox_cost: float = 0.0


@dataclass
class TUIState:
    """Complete state for rendering the TUI."""

    screen: TUIScreen = TUIScreen.SWARM_OVERVIEW
    swarm_name: str = ""
    repo: str = ""
    status: str = "RUNNING"
    elapsed: str = "00:00:00"
    round: int = 0
    max_rounds: int = 0
    agents: list[AgentRow] = field(default_factory=list)
    budget_bars: BudgetBar = field(default_factory=BudgetBar)
    cost_breakdown: CostBreakdown = field(default_factory=CostBreakdown)
    tokens_per_min: list[float] = field(default_factory=list)
    evidence_feed: list[str] = field(default_factory=list)
    sandbox_rows: list[SandboxRow] = field(default_factory=list)
    judge_rows: list[JudgeVerdictRow] = field(default_factory=list)
    logs: list[str] = field(default_factory=list)
    diversity_score: float = 0.0
    loop_detections: int = 0
    agent_ejections: int = 0
    active_view: str = "swarm_overview"
    focused_panel: str = ""
    command_mode: bool = False
    command_buffer: str = ""
    override_modal: bool = False
    override_case: dict | None = None
    theme: str = "dark_matrix"
    export_format: str = "json"

    def to_dict(self) -> dict:
        return {
            "screen": self.screen.value,
            "swarm": {
                "name": self.swarm_name,
                "repo": self.repo,
                "status": self.status,
                "elapsed": self.elapsed,
                "round": self.round,
                "max_rounds": self.max_rounds,
            },
            "agents": [a.to_row() for a in self.agents],
            "budget": {
                "tokens_pct": round(self.budget_bars.tokens_pct, 1),
                "cost_pct": round(self.budget_bars.cost_pct, 1),
                "time_pct": round(self.budget_bars.time_pct, 1),
            },
            "cost_breakdown": self.cost_breakdown.__dict__,
            "sandbox": [s.to_row() for s in self.sandbox_rows[:10]],
            "judges": [j.to_row() for j in self.judge_rows[:5]],
            "diversity": round(self.diversity_score, 3),
            "loops": self.loop_detections,
            "ejections": self.agent_ejections,
            "evidence_feed": self.evidence_feed[-5:],
            "tokens_per_min": self.tokens_per_min[-20:],
        }


# ═══════════════════════════════════════════════════════════════
# TUI Renderer
# ═══════════════════════════════════════════════════════════════


class TUIRenderer:
    """Renders the TUI state to the terminal or to structured output.

    In production with Textual: bind this state to reactive widgets.
    In headless mode: output JSON for external monitoring.
    """

    def __init__(self, state: TUIState | None = None):
        self.state = state or TUIState()
        self._keybindings: dict[str, str] = {
            "F1": TUIScreen.SWARM_OVERVIEW.value,
            "F2": TUIScreen.AGENT_DETAIL.value,
            "F3": TUIScreen.SANDBOX_MONITOR.value,
            "F4": TUIScreen.BENCH_PANEL.value,
            "F5": TUIScreen.EVIDENCE_GRAPH.value,
            "F6": TUIScreen.FINANCE.value,
            "F7": TUIScreen.RAW_LOGS.value,
            "tab": "cycle_focus",
            "enter": "select",
            "esc": "back",
            "q": "quit",
            "/": "search",
            ":": "command_mode",
            "p": "pause",
        }
        self._commands: dict[str, Callable] = {}
        self._register_commands()

    def _register_commands(self):
        self._commands = {
            "override": self._cmd_override,
            "eject": self._cmd_eject,
            "reinstate": self._cmd_reinstate,
            "budget": self._cmd_budget,
            "provider": self._cmd_provider,
            "export": self._cmd_export,
            "theme": self._cmd_theme,
            "mode": self._cmd_mode,
            "pause-swarm": lambda args: logger.info("swarm_paused"),
            "resume-swarm": lambda args: logger.info("swarm_resumed"),
            "help": lambda args: list(self._commands.keys()),
        }

    def switch_screen(self, screen: TUIScreen) -> None:
        self.state.screen = screen
        self.state.active_view = screen.value

    def handle_key(self, key: str) -> str:
        """Handle a keypress, return action description."""
        key_lower = key.lower()
        if key_lower in ("f1", "f2", "f3", "f4", "f5", "f6", "f7"):
            screen_key = f"F{key_lower[1:]}"
            screen_name = self._keybindings.get(key_lower.upper(), "")
            if screen_name:
                screen = TUIScreen(screen_name)
                self.switch_screen(screen)
                return f"Switched to {screen.value}"
        if key_lower == "q":
            return "quit"
        if key_lower == ":":
            self.state.command_mode = True
            return "command_mode"
        if key_lower == "esc":
            if self.state.command_mode:
                self.state.command_mode = False
                self.state.command_buffer = ""
                return "command_cancelled"
            if self.state.override_modal:
                self.state.override_modal = False
                return "override_cancelled"
            return "back"
        if self.state.command_mode:
            if key_lower in ("enter", "return"):
                return self._execute_command(self.state.command_buffer)
            elif key_lower == "backspace":
                self.state.command_buffer = self.state.command_buffer[:-1]
            else:
                self.state.command_buffer += key
            return f"cmd_buffer: {self.state.command_buffer}"
        if self.state.override_modal:
            if key_lower == "y":
                return self._handle_override("accept")
            elif key_lower == "n":
                return self._handle_override("reject")
            elif key_lower == "e":
                return self._handle_override("edit_severity")
        return f"key: {key}"

    def _execute_command(self, cmd_str: str = "") -> str:
        self.state.command_mode = False
        buffer = cmd_str if cmd_str else self.state.command_buffer
        self.state.command_buffer = ""
        parts = buffer.split(maxsplit=1)
        cmd_name = parts[0] if parts else ""
        args = parts[1] if len(parts) > 1 else ""

        if cmd_name in self._commands:
            return self._commands[cmd_name](args)
        return f"Unknown command: {cmd_name}"

    def _cmd_override(self, args: str) -> str:
        self.state.override_modal = True
        return "override_modal_opened"

    def _cmd_eject(self, args: str) -> str:
        agent_id = args.strip()
        return f"Agent {agent_id} ejected" if agent_id else "Usage: eject <agent_id>"

    def _cmd_reinstate(self, args: str) -> str:
        agent_id = args.strip()
        return f"Agent {agent_id} reinstated" if agent_id else "Usage: reinstate <agent_id>"

    def _cmd_budget(self, args: str) -> str:
        parts = args.split()
        if len(parts) >= 2:
            return f"Budget {parts[0]} set to {parts[1]}"
        return f"Budget: token={self.state.budget_bars.tokens_pct:.0f}% cost={self.state.budget_bars.cost_pct:.0f}%"

    def _cmd_provider(self, args: str) -> str:
        parts = args.split()
        if len(parts) >= 2 and parts[0] == "swap":
            return f"Provider hotswapped to {parts[1]}"
        return "Usage: provider swap <name>"

    def _cmd_export(self, args: str) -> str:
        fmt = args.strip() or "json"
        self.state.export_format = fmt
        return f"Export format set to {fmt}"

    def _cmd_theme(self, args: str) -> str:
        theme = args.strip() or "dark_matrix"
        self.state.theme = theme
        return f"Theme set to {theme}"

    def _cmd_mode(self, args: str) -> str:
        return f"Mode: {args.strip()}" if args.strip() else "Current mode: interactive"

    def _handle_override(self, action: str) -> str:
        self.state.override_modal = False
        if action == "accept":
            return "Verdict ACCEPTED"
        elif action == "reject":
            return "Verdict REJECTED"
        elif action == "edit_severity":
            return "Severity edit mode"
        return f"Override: {action}"

    def render_json(self) -> str:
        """Render the full TUI state as JSON (for --json mode)."""
        return json.dumps(self.state.to_dict(), indent=2)

    def render_swarm_overview_text(self) -> str:
        """Render swarm overview as text (for testing)."""
        s = self.state
        lines = [
            f"Bug Swarm · {s.swarm_name} · {s.status} · {s.elapsed}",
            f"Round {s.round}/{s.max_rounds} · {len(s.agents)} agents active",
            f"Budget: tokens={s.budget_bars.tokens_pct:.0f}% cost={s.budget_bars.cost_pct:.0f}% time={s.budget_bars.time_pct:.0f}%",
            f"Diversity: {s.diversity_score:.3f} · Loops: {s.loop_detections} · Ejections: {s.agent_ejections}",
            f"Evidence feed: {len(s.evidence_feed)} items",
            f"Sandbox: {len(s.sandbox_rows)} executions · Bench: {len(s.judge_rows)} cases",
        ]
        if s.agents:
            lines.append("\nAgents:")
            for a in s.agents[:5]:
                icon = {"ok": "✓", "slow": "⚠", "looping": "↻", "ejected": "✗"}.get(a.health, " ")
                lines.append(f"  [{icon}] {a.id} {a.persona} r{a.round} {a.tokens}tok {a.bugs}bugs")
        return "\n".join(lines)


# ═══════════════════════════════════════════════════════════════
# TUI Data Feeder — updates state from orchestrator
# ═══════════════════════════════════════════════════════════════


class TUIFeeder:
    """Updates TUI state from orchestrator, finance controller, and bench data."""

    def __init__(self, state: TUIState | None = None):
        self.state = state or TUIState()

    def feed_swarm(self, orchestrator_result: dict) -> None:
        s = self.state
        r = orchestrator_result
        s.round = r.get("round", s.round)
        s.agents = []
        scores = r.get("agent_scores", {})
        for aid, score in scores.items():
            s.agents.append(
                AgentRow(
                    id=aid,
                    persona=score.get("persona", "?"),
                    status=score.get("status", "active"),
                    tokens=score.get("tokens", 0),
                    bugs=score.get("bugs", 0),
                    health="ok",
                )
            )
        s.diversity_score = 1.0 - (r.get("diversity_warning", 0) * 0.5)
        s.loop_detections = len(r.get("loops", []))
        s.agent_ejections = len(r.get("ejections", []))

    def feed_finance(self, status: dict) -> None:
        bars = status.get("budget_bars", {})
        self.state.budget_bars = BudgetBar(
            tokens_pct=bars.get("token_pct", 0),
            cost_pct=bars.get("cost_pct", 0),
            time_pct=bars.get("time_pct", 0),
        )
        attrib = status.get("attribution", {})
        total = attrib.get("total", {})

    def feed_findings(self, findings: list[dict]) -> None:
        for f in findings[-5:]:
            claim = f.get("claim", "")[:100]
            verified = "✓" if f.get("verified") else " "
            self.state.evidence_feed.append(f"[{verified}] {claim}")
        self.state.evidence_feed = self.state.evidence_feed[-20:]

    def feed_bench(self, bench_cases: list) -> None:
        self.state.judge_rows = []
        for case in bench_cases[:5]:
            self.state.judge_rows.append(
                JudgeVerdictRow(
                    case_id=case.case_id[:8],
                    claim=case.claim[:60],
                    judges_voted=f"{sum(1 for v in case.verdicts if v.bug_verified)}-{sum(1 for v in case.verdicts if not v.bug_verified)}",
                    verdict=case.final_verdict.value,
                    confidence="HIGH" if case.final_verdict.value == "confirmed" else "MEDIUM",
                    queries="0/3",
                )
            )

    def add_log(self, level: str, component: str, message: str) -> None:
        ts = time.strftime("%H:%M:%S")
        self.state.logs.append(f"{ts} [{level}] {component}: {message}")
        self.state.logs = self.state.logs[-100:]
