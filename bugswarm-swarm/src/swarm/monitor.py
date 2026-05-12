"""Performance Monitor — loop detection, echo chamber detection, agent ejection.

Rule: Semantic similarity for loop detection MUST use trigram BoW, not SHA-256 hashes.
"""

from __future__ import annotations

from collections import defaultdict

from .types import AgentSlot, AgentStatus, SwarmConfig
from .routing import text_similarity


class PerformanceMonitor:
    """Tracks agent performance and triggers ejection/recovery."""

    def __init__(self, config: SwarmConfig):
        self.config = config
        self.agent_history: dict[str, list[str]] = defaultdict(list)
        self.loop_detections: dict[str, int] = defaultdict(int)
        self.pair_interactions: dict[tuple[str, str], int] = defaultdict(int)
        self.round_findings: dict[int, list[dict]] = defaultdict(list)

    def record_message(self, agent_id: str, content: str) -> None:
        self.agent_history[agent_id].append(content)
        if len(self.agent_history[agent_id]) > 20:
            self.agent_history[agent_id] = self.agent_history[agent_id][-20:]

    def detect_loop(self, agent_id: str) -> bool:
        """Check if agent's last 5 messages are too similar (semantic loop).

        Uses trigram bag-of-words similarity — not SHA-256 hashes.
        """
        history = self.agent_history.get(agent_id, [])
        if len(history) < 5:
            return False

        recent = history[-5:]
        similarities = []
        for i in range(len(recent)):
            for j in range(i + 1, len(recent)):
                similarities.append(text_similarity(recent[i], recent[j]))

        if not similarities:
            return False
        avg_sim = sum(similarities) / len(similarities)
        return avg_sim > self.config.loop_similarity_threshold

    def detect_echo_chamber(self, agents: list[AgentSlot]) -> list[set[str]]:
        """Detect groups of 3+ agents forming circular reinforcement chains."""
        if len(agents) < 3:
            return []

        agreement: dict[str, set[str]] = defaultdict(set)
        for a1 in agents:
            h1 = self.agent_history.get(a1.id, [])
            if not h1:
                continue
            for a2 in agents:
                if a1.id >= a2.id:
                    continue
                h2 = self.agent_history.get(a2.id, [])
                if not h2:
                    continue
                sim = text_similarity(h1[-1], h2[-1])
                if sim > 0.85:
                    agreement[a1.id].add(a2.id)
                    agreement[a2.id].add(a1.id)

        visited: set[str] = set()
        chambers: list[set[str]] = []

        def dfs(node: str, component: set[str]) -> None:
            visited.add(node)
            component.add(node)
            for neighbor in agreement.get(node, set()):
                if neighbor not in visited:
                    dfs(neighbor, component)

        for a in agents:
            if a.id not in visited:
                comp: set[str] = set()
                dfs(a.id, comp)
                if len(comp) >= 3:
                    chambers.append(comp)

        return chambers

    def should_eject(self, agent: AgentSlot) -> tuple[bool, str]:
        """Determine if an agent should be ejected."""
        if agent.loop_count >= self.config.max_loops_before_eject:
            return True, f"Loop count {agent.loop_count} >= {self.config.max_loops_before_eject}"
        if agent.hallucination_rate > self.config.max_hallucination_rate:
            return True, f"Hallucination rate {agent.hallucination_rate:.0%} > {self.config.max_hallucination_rate:.0%}"
        return False, ""

    def evaluate_ejection(self, agent: AgentSlot, last_findings: list[dict]) -> bool:
        """Post-ejection review: were the agent's contributions valuable?"""
        valuable = any(
            f.get("verified") or (f.get("severity_estimate", 0) >= 7)
            for f in last_findings
        )
        agent.ejection_reviewed = True
        return valuable
