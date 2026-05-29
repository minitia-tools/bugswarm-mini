"""Recall Rate Limiter — prevents agent griefing via recall_raw_context overuse.

Rule: Max 3 recalls per round, 5-turn cooldown. Overuse decrements trust score.
"""

from __future__ import annotations

import structlog

logger = structlog.get_logger(__name__)


class RecallRateLimiter:
    """Prevents agents from griefing by overusing recall_raw_context."""

    def __init__(self, max_per_round: int = 3, cooldown_turns: int = 5):
        self.max_per_round = max_per_round
        self.cooldown_turns = cooldown_turns
        self.agent_recalls: dict[str, list[int]] = {}
        self.agent_trust_penalties: dict[str, float] = {}

    def can_recall(self, agent_id: str, turn: int) -> bool:
        """Check if agent can recall context this turn."""
        turns = self.agent_recalls.get(agent_id, [])
        recent = [t for t in turns if turn - t <= self.cooldown_turns]
        return len(recent) < self.max_per_round

    def record_recall(self, agent_id: str, turn: int, reason: str = "", context_hash: str = "") -> None:
        """Record a recall invocation."""
        if agent_id not in self.agent_recalls:
            self.agent_recalls[agent_id] = []
        self.agent_recalls[agent_id].append(turn)

        # Penalize trust if overused within cooldown window
        recent_count = len([t for t in self.agent_recalls[agent_id] if turn - t <= self.cooldown_turns])
        if recent_count > self.max_per_round:
            self.agent_trust_penalties[agent_id] = self.agent_trust_penalties.get(agent_id, 0.0) + 0.1
            logger.warning(
                "recall_rate_limited", agent=agent_id, count=recent_count, penalty=self.agent_trust_penalties[agent_id]
            )

    def get_trust_penalty(self, agent_id: str) -> float:
        """Get accumulated trust penalty for an agent."""
        return self.agent_trust_penalties.get(agent_id, 0.0)

    def reset(self, agent_id: str) -> None:
        """Reset recalls for an agent (new round)."""
        self.agent_recalls.pop(agent_id, None)
