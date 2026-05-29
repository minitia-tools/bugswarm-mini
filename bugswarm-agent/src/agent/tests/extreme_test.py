"""Phase 4 Aggressive Test — DeepSeek V3 on 12 extreme hidden bugs.

export DEEPSEEK_API_KEY="sk-..."
python src/agent/tests/extreme_test.py
"""

import asyncio, json, os, sys, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent))

from agent.core import BugSwarmAgent, AgentConfig
from gateway.client import LLMClient
from gateway.types import GatewayConfig, ProviderType

G = "\033[0;32m"
R = "\033[0;31m"
N = "\033[0m"
TARGET_BUGS = [
    "HMAC timing oracle",
    "Unicode normalization bypass",
    "Off-by-one buffer overflow",
    "Float comparison in finance",
    "SSL verification disabled",
    "Thread-safety violation",
    "Type confusion eval()",
    "Integer truncation overflow",
    "TOCTOU file permission",
    "JWT none algorithm",
    "ReDoS backtracking",
    "Use-after-free finalizer",
]


async def main():
    api_key = os.getenv("DEEPSEEK_API_KEY", "")
    if not api_key:
        print("Set DEEPSEEK_API_KEY first")
        return

    print(f"{'=' * 70}")
    print(f"  Phase 4 AGGRESSIVE TEST — DeepSeek V3 on 12 Extreme Hidden Bugs")
    print(f"{'=' * 70}\n")
    print(f"Target: /tmp/extreme-bugs (12 bugs, 430 lines)")
    print(f"Model: deepseek-chat (DeepSeek V3)")
    print(f"Persona: adversarial")
    print(f"Max rounds: 4, turns per round: 8\n")

    config = GatewayConfig.from_env()
    config.default_provider = ProviderType.DEEPSEEK
    gw = LLMClient(config)
    gw.register_default_adapters()

    agent_config = AgentConfig(
        repo_path=Path("/tmp/extreme-bugs"),
        max_rounds=4,
        max_turns_per_round=8,
        persona="adversarial",
        model="deepseek-chat",
        provider=ProviderType.DEEPSEEK,
    )

    agent = BugSwarmAgent(agent_config, gw)
    start = time.time()
    result = await agent.run()
    elapsed = time.time() - start

    findings = result.get("findings", [])
    verified = result.get("verified_findings", 0)
    tokens = gw.registry.total_tokens.total_tokens
    cost = gw.registry.total_cost

    print(f"\n{'=' * 70}")
    print(f"  RESULTS")
    print(f"{'=' * 70}")
    print(f"  Duration:  {elapsed:.0f}s")
    print(f"  Findings:  {len(findings)}")
    print(f"  Verified:  {verified}")
    print(f"  Tokens:    {tokens}")
    print(f"  Cost:      ${cost:.4f}")
    print(f"  Per bug:   ${cost / max(len(findings), 1):.4f}")
    print(f"\n  Target bugs: {len(TARGET_BUGS)}")
    print(f"  Found:       {len(findings)}/{len(TARGET_BUGS)}")

    # Show each finding
    for i, f in enumerate(findings[:15]):
        claim = f.get("claim", "?")[:100]
        v = "✓ VERIFIED" if f.get("verified") else ""
        sev = f.get("severity_estimate", "?")
        print(f"\n  [{i + 1}] {v} (sev={sev}) {claim}")

    print(f"\n{'=' * 70}")
    if len(findings) >= 3:
        print(f"  {G}✓ PHASE 4 AGGRESSIVE TEST PASSED{N}")
        print(f"  DeepSeek V3 found {len(findings)}/{len(TARGET_BUGS)} extreme bugs")
    else:
        print(f"  {R}✗ Found only {len(findings)} bugs — investigate{N}")
    print(f"{'=' * 70}")

    with open("/tmp/extreme_test_report.json", "w") as f:
        json.dump(result, f, indent=2, default=str)


if __name__ == "__main__":
    asyncio.run(main())
