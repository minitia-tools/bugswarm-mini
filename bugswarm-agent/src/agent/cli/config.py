"""CLI Configuration — hierarchical merge: defaults → file → env → flags."""

from __future__ import annotations

import argparse
import os
from dataclasses import dataclass, field
from pathlib import Path

from agent.prompts import Persona

DEFAULT_CONFIG_PATHS = [
    Path.home() / ".bugswarm" / "config.yaml",
    Path(".bugswarm.yaml"),
]


@dataclass
class CLIConfig:
    repo: Path = field(default_factory=Path.cwd)
    persona: Persona = Persona.ADVERSARIAL
    model: str = "deepseek-v4-flash"
    provider: str = "deepseek"
    rounds: int = 5
    turns: int = 10
    token_budget: int = 5_000_000
    cost_budget: float = 50.0
    time_budget_minutes: int = 2400
    cpg_binary: str = "bugswarm-cpg"
    sandbox_binary: str = "bugswarm-sandbox"
    output: Path | None = None
    format: str = "json"
    json_mode: bool = False
    verbose: bool = False
    dry_run: bool = False
    db_path: str = ":memory:"
    scanner_config: str = ""
    # Phase 19: ML probability prediction
    probability_enabled: bool = True
    probability_model_path: str = "~/.bugswarm/probability_model.json"
    probability_top_k: int = 20
    probability_confidence_threshold: float = 0.5

    @classmethod
    def from_args(cls, args: argparse.Namespace) -> CLIConfig:
        """Build from CLI args with env var overrides and unified config loading."""
        from agent.config import UnifiedConfig

        config_path = args.config or os.getenv("BGSWARM_CONFIG", "/etc/bugswarm/config.yaml")
        unified = UnifiedConfig.load(config_path)

        persona = Persona(args.persona) if args.persona else Persona.ADVERSARIAL
        model = args.model or os.getenv("BGSWARM_MODEL", "deepseek-v4-flash")
        provider = args.provider or os.getenv("BGSWARM_PROVIDER", "deepseek")
        rounds = args.rounds or int(os.getenv("BGSWARM_ROUNDS", "5"))
        turns = args.turns or int(os.getenv("BGSWARM_TURNS", "10"))
        token_budget = args.token_budget or int(os.getenv("BGSWARM_TOKEN_BUDGET", "5000000"))
        cost_budget = args.cost_budget or float(os.getenv("BGSWARM_COST_BUDGET", "50.0"))
        time_budget = args.time_budget or int(os.getenv("BGSWARM_TIME_BUDGET", "2400"))

        cpg_binary = args.cpg_binary or os.getenv("BGSWARM_CPG_BIN", "bugswarm-cpg")
        sandbox_binary = args.sandbox_binary or os.getenv("BGSWARM_SANDBOX_BIN", "bugswarm-sandbox")

        # Set daemon socket/env from unified config
        os.environ.setdefault("BGSWARM_SANDBOX_SOCKET", unified.sandbox_socket)
        os.environ.setdefault("BGSWARM_CPG_SOCKET", unified.cpg_socket)
        os.environ.setdefault("BGSWARM_EVIDENCE_SOCKET", unified.evidence_socket)
        os.environ.setdefault("BGSWARM_LOG_LEVEL", unified.log_level)

        return cls(
            repo=Path(args.repo).resolve(),
            persona=persona, model=model, provider=provider,
            rounds=rounds, turns=turns,
            token_budget=token_budget, cost_budget=cost_budget,
            time_budget_minutes=time_budget,
            cpg_binary=cpg_binary,
            sandbox_binary=sandbox_binary,
            output=Path(args.output) if args.output else None,
            format=args.format or "json",
            json_mode=args.json or False,
            verbose=args.verbose or False,
            dry_run=args.dry_run or False,
            db_path=args.db or os.getenv("BGSWARM_DB", ":memory:"),
            scanner_config=args.scanner_config or "",
            probability_enabled=not getattr(args, 'no_probability', False),
            probability_model_path=args.probability_model or os.getenv(
                "BGSWARM_PROBABILITY_MODEL", "~/.bugswarm/probability_model.json"
            ),
        )

    @classmethod
    def build_parser(cls) -> argparse.ArgumentParser:
        p = argparse.ArgumentParser(
            prog="bugswarm",
            description="Bug Swarm Agent — AI-powered vulnerability detection",
            epilog="Set DEEPSEEK_API_KEY or OPENAI_API_KEY to use a real LLM.",
        )
        p.add_argument("repo", nargs="?", default=".", help="Path to repository")
        p.add_argument("--persona", choices=["causal","adversarial","defensive","semantic"])
        p.add_argument("--model", help="LLM model (default: deepseek-v4-flash)")
        p.add_argument("--provider", choices=["openai","anthropic","deepseek","google","ollama"])
        p.add_argument("--rounds", type=int, help="Max IEP rounds (default: 5)")
        p.add_argument("--turns", type=int, help="Max turns per round (default: 10)")
        p.add_argument("--token-budget", type=int, help="Token budget limit")
        p.add_argument("--cost-budget", type=float, help="Cost budget in USD")
        p.add_argument("--time-budget", type=int, help="Time budget in minutes")
        p.add_argument("--cpg-binary", help="Path to CPG binary")
        p.add_argument("--sandbox-binary", help="Path to sandbox binary")
        p.add_argument("--output", "-o", help="Output file for JSON/SARIF report")
        p.add_argument("--format", choices=["json","sarif","text"], default="json")
        p.add_argument("--json", action="store_true", help="Stream JSON lines to stdout")
        p.add_argument("--verbose", "-v", action="store_true", help="Verbose output")
        p.add_argument("--dry-run", action="store_true", help="Validate without running")
        p.add_argument("--db", help="SQLite database path for persistence")
        p.add_argument("--scanner-config", help="Path to scanner YAML config")
        p.add_argument("--probability-model", help="Path to ML probability model file")
        p.add_argument("--probability", action="store_true", default=True, help="Enable ML probability prediction (default: on)")
        p.add_argument("--no-probability", action="store_true", help="Disable ML probability prediction")
        p.add_argument("--config", "-c", help="Path to unified BugSwarm config YAML")
        p.add_argument("--test", action="store_true", help=argparse.SUPPRESS)
        return p
