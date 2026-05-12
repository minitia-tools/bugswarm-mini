"""Phase 4 Live Test — Real LLM (DeepSeek) on enterprise bugs.

Usage:
    export DEEPSEEK_API_KEY="sk-..."
    python src/agent/tests/live_deepseek_test.py

This runs the full IEP loop with a real DeepSeek model against the enterprise bug repo.
"""

import asyncio, json, os, sys, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent))

from agent.core import BugSwarmAgent, AgentConfig
from gateway.client import LLMClient
from gateway.types import GatewayConfig, ProviderType


async def main():
    api_key = os.getenv("DEEPSEEK_API_KEY", "")
    if not api_key or api_key == "sk-...":
        print("Set DEEPSEEK_API_KEY environment variable first:")
        print("  export DEEPSEEK_API_KEY='sk-...'")
        print("  python src/agent/tests/live_deepseek_test.py")
        sys.exit(1)

    print("=== Phase 4 Live Test: DeepSeek on Enterprise Bugs ===\n")
    print(f"Model: deepseek-chat (DeepSeek V3)")
    print(f"Repo: /tmp/enterprise-bugs")
    print(f"Persona: adversarial\n")

    config = GatewayConfig.from_env()
    config.default_provider = ProviderType.DEEPSEEK
    gw = LLMClient(config)
    gw.register_default_adapters()

    agent_config = AgentConfig(
        repo_path=Path("/tmp/enterprise-bugs"),
        max_rounds=3,
        max_turns_per_round=6,
        persona="adversarial",
        model="deepseek-chat",
        provider=ProviderType.DEEPSEEK,
    )

    agent = BugSwarmAgent(agent_config, gw)
    start = time.time()
    result = await agent.run()
    elapsed = time.time() - start

    print(f"\n{'='*60}")
    print(f"RESULTS")
    print(f"{'='*60}")
    print(f"Duration: {elapsed:.0f}s")
    print(f"Findings: {len(result.get('findings', []))}")
    print(f"Verified: {result.get('verified_findings', 0)}")
    print(f"Tokens:   {gw.registry.total_tokens.total_tokens}")
    print(f"Cost:     ${gw.registry.total_cost:.4f}")

    findings = result.get("findings", [])
    for i, f in enumerate(findings[:5]):
        print(f"\n  Finding {i+1}: {f.get('claim','?')[:100]}")
        if f.get("verified"):
            print(f"    ✓ VERIFIED | severity={f.get('severity_estimate','?')}")

    # Save report
    with open("/tmp/phase4_live_report.json", "w") as f:
        json.dump(result, f, indent=2, default=str)
    print(f"\nFull report: /tmp/phase4_live_report.json")


if __name__ == "__main__":
    asyncio.run(main())
