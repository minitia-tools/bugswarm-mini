"""Bug Swarm Agent CLI — thin entry point.

Wires everything together: config → validation → wiring → run.
"""

from __future__ import annotations

import asyncio
import sys

from agent.cli.config import CLIConfig
from agent.cli.commands import cmd_run, cmd_status, cmd_report, cmd_learn
from agent.cli.signals import GracefulKiller
from agent.cli.validation import PreFlight


async def main_async() -> int:
    parser = CLIConfig.build_parser()
    args = parser.parse_args()

    # Handle gate tests (hidden flag)
    if args.test:
        from agent.tests.phase4_gate import run_phase4_gate
        receipt = await run_phase4_gate()
        return 0 if receipt.get("status") == "PASSED" else 1

    # Build config
    config = CLIConfig.from_args(args)

    # Setup graceful shutdown
    killer = GracefulKiller()

    # Dispatch
    if args.dry_run:
        return await _dry_run(config)

    return await cmd_run(config)


async def _dry_run(config: CLIConfig) -> int:
    """Validate everything without running."""
    print(f"Bug Swarm Agent — Dry Run")
    print(f"  Repository: {config.repo}")
    print(f"  Persona:    {config.persona.value}")
    print(f"  Model:      {config.model} ({config.provider})")
    print(f"  Rounds:     {config.rounds}, Turns: {config.turns}")
    print(f"  Budget:     {config.token_budget:,} tokens, ${config.cost_budget:.2f}, {config.time_budget_minutes}min")
    print()

    warnings = await PreFlight.check_all(config)
    if warnings:
        PreFlight.print_warnings(warnings)
        return 2

    print("✓ All pre-flight checks passed")
    return 0


def main() -> None:
    try:
        exit_code = asyncio.run(main_async())
    except KeyboardInterrupt:
        print("\nInterrupted")
        exit_code = 130
    except Exception as e:
        print(f"Fatal error: {e}", file=sys.stderr)
        exit_code = 2
    sys.exit(exit_code)


if __name__ == "__main__":
    main()
