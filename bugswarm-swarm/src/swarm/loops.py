"""Enhanced Loop Detector — semantic loops, cross-agent loops, triadic echo chambers.

Rule: Similarity uses trigram bag-of-words, NOT SHA-256. Tarjan's SCC for
echo chamber detection. Pattern-break prompts disrupt detected loops.
"""

from __future__ import annotations

from collections import defaultdict

import structlog

from .fidelity import trigram_similarity

logger = structlog.get_logger(__name__)


class LoopDetector:
    """Detects semantic loops, cross-agent loops, and triadic echo chambers."""

    def __init__(self, similarity_threshold: float = 0.92, window_size: int = 5):
        self.threshold = similarity_threshold
        self.window_size = window_size
        self.agent_history: dict[str, list[str]] = {}
        self.pair_interactions: dict[tuple[str, str], int] = {}
        self.loop_count: dict[str, int] = {}
        self.pattern_breaks: dict[str, int] = {}

    def record(self, agent_id: str, content: str) -> None:
        if agent_id not in self.agent_history:
            self.agent_history[agent_id] = []
        self.agent_history[agent_id].append(content)
        if len(self.agent_history[agent_id]) > self.window_size * 3:
            self.agent_history[agent_id] = self.agent_history[agent_id][-self.window_size * 2:]

    def detect_semantic_loop(self, agent_id: str) -> tuple[bool, str]:
        """Check if agent's recent messages form a semantic loop.

        Uses trigram BoW similarity, not SHA-256 hashes.
        """
        history = self.agent_history.get(agent_id, [])
        if len(history) < self.window_size:
            return False, ""

        recent = history[-self.window_size:]
        similarities = []
        for i in range(len(recent)):
            for j in range(i + 1, len(recent)):
                similarities.append(trigram_similarity(recent[i], recent[j]))

        if not similarities:
            return False, ""

        avg_sim = sum(similarities) / len(similarities)
        if avg_sim > self.threshold:
            self.loop_count[agent_id] = self.loop_count.get(agent_id, 0) + 1
            return True, self._generate_pattern_break(agent_id)
        return False, ""

    def detect_cross_loop(self, agent_a: str, agent_b: str) -> bool:
        """Check if two agents are trading rephrased arguments."""
        ha = self.agent_history.get(agent_a, [])
        hb = self.agent_history.get(agent_b, [])
        if len(ha) < 3 or len(hb) < 3:
            return False

        for i in range(1, min(4, len(ha), len(hb))):
            sim = trigram_similarity(ha[-i], hb[-i])
            if sim < self.threshold:
                return False
        self.pair_interactions[(agent_a, agent_b)] = \
            self.pair_interactions.get((agent_a, agent_b), 0) + 1
        return True

    def detect_echo_chamber(self, agents: list[str]) -> list[set[str]]:
        """Detect groups of 3+ agents forming circular reinforcement chains.

        Uses Tarjan's Strongly Connected Components algorithm.
        """
        if len(agents) < 3:
            return []

        # Build agreement graph
        graph: dict[str, set[str]] = {a: set() for a in agents}
        for i, a1 in enumerate(agents):
            h1 = self.agent_history.get(a1, [])
            if not h1:
                continue
            for a2 in agents[i + 1:]:
                h2 = self.agent_history.get(a2, [])
                if not h2:
                    continue
                sim = trigram_similarity(h1[-1], h2[-1])
                if sim > 0.85:
                    graph[a1].add(a2)
                    graph[a2].add(a1)

        # Tarjan's SCC
        index_counter = [0]
        indices: dict[str, int] = {}
        lowlink: dict[str, int] = {}
        onstack: set[str] = set()
        stack: list[str] = []
        sccs: list[set[str]] = []

        def strongconnect(v: str) -> None:
            indices[v] = index_counter[0]
            lowlink[v] = index_counter[0]
            index_counter[0] += 1
            stack.append(v)
            onstack.add(v)

            for w in graph.get(v, set()):
                if w not in indices:
                    strongconnect(w)
                    lowlink[v] = min(lowlink[v], lowlink[w])
                elif w in onstack:
                    lowlink[v] = min(lowlink[v], indices[w])

            if lowlink[v] == indices[v]:
                scc: set[str] = set()
                while True:
                    w = stack.pop()
                    onstack.discard(w)
                    scc.add(w)
                    if w == v:
                        break
                if len(scc) >= 3:
                    sccs.append(scc)

        for a in agents:
            if a not in indices and a in graph:
                strongconnect(a)

        return sccs

    def _generate_pattern_break(self, agent_id: str) -> str:
        """Generate a pattern-break prompt to disrupt the loop."""
        count = self.pattern_breaks.get(agent_id, 0) + 1
        self.pattern_breaks[agent_id] = count

        prompts = [
            "Your previous outputs show repetition. Consider an entirely different approach.",
            "The debate has entered a repetitive cycle. Reframe your argument in terms of a concrete PoC.",
            "You appear to be looping. Pivot to investigating a different code path entirely.",
            "Stop repeating yourself. If you cannot produce new evidence, state your uncertainty and propose a test.",
            "CRITICAL: Your last 5 messages are nearly identical. Either find new evidence or disengage.",
        ]
        return prompts[count % len(prompts)]
