"""Bug Swarm Agent CLI."""

import asyncio
import json
import sys
from pathlib import Path

from gateway.client import LLMClient
from gateway.types import GatewayConfig, ProviderType

from agent.core import AgentConfig, BugSwarmAgent


async def main_async():
    import argparse
    p = argparse.ArgumentParser(description="Bug Swarm Agent — Single-agent bug hunter")
    p.add_argument("repo", nargs="?", default=".", help="Path to repository to analyze")
    p.add_argument("--rounds", type=int, default=5, help="Max rounds")
    p.add_argument("--turns", type=int, default=10, help="Max turns per round")
    p.add_argument("--persona", choices=["causal","adversarial","defensive","semantic"], default="causal")
    p.add_argument("--model", default="gpt-4o-mini")
    p.add_argument("--provider", choices=["openai","anthropic","google","ollama","deepseek"], default="openai")
    p.add_argument("--output", "-o", help="Output JSON file")
    p.add_argument("--test", action="store_true", help="Run Phase 4 Gate tests")
    args = p.parse_args()

    if args.test:
        from agent.tests.phase4_gate import run_phase4_gate
        await run_phase4_gate()
        return

    config = GatewayConfig.from_env()
    gw = LLMClient(config)
    gw.register_default_adapters()

    agent_config = AgentConfig(
        repo_path=Path(args.repo),
        max_rounds=args.rounds,
        max_turns_per_round=args.turns,
        persona=args.persona,
        model=args.model,
        provider=ProviderType(args.provider),
    )

    agent = BugSwarmAgent(agent_config, gw)
    result = await agent.run()

    if args.output:
        with open(args.output, 'w') as f:
            json.dump(result, f, indent=2)
    else:
        print(json.dumps(result, indent=2))


def main():
    asyncio.run(main_async())


if __name__ == "__main__":
    main()
