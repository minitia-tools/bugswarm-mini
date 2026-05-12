"""Swarm Types — shared data structures for the multi-agent swarm."""

from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any

from gateway.types import ProviderType


class AgentStatus(str, Enum):
    ACTIVE = "active"
    IDLE = "idle"
    FROZEN = "frozen"
    EJECTED = "ejected"
    LOOPING = "looping"
    COMPLETED = "completed"


class Persona(str, Enum):
    CAUSAL = "causal"
    ADVERSARIAL = "adversarial"
    DEFENSIVE = "defensive"
    SEMANTIC = "semantic"


@dataclass
class AgentSlot:
    id: str
    persona: Persona
    status: AgentStatus = AgentStatus.IDLE
    trust_score: float = 1.0
    hallucination_rate: float = 0.0
    contribution_score: float = 0.0
    loop_count: int = 0
    tokens_consumed: int = 0
    findings: list[dict] = field(default_factory=list)
    round_hypotheses: dict[int, str] = field(default_factory=dict)
    ejection_reason: str = ""
    ejection_reviewed: bool = False


@dataclass
class SwarmConfig:
    repo_path: Path
    num_agents: int = 12
    batch_size: int = 6
    max_rounds: int = 5
    max_turns_per_round: int = 8
    model: str = "deepseek-v4-flash"
    provider: ProviderType = field(default_factory=lambda: ProviderType.DEEPSEEK)
    spare_pool_size: int = 4
    max_loops_before_eject: int = 5
    max_hallucination_rate: float = 0.4
    max_budget_dominance_pct: float = 0.30
    recall_rate_limit: int = 3
    recall_cooldown_turns: int = 5
    loop_similarity_threshold: float = 0.92
    diversity_warning_threshold: float = 0.4


import random


def assign_personas(num_agents: int) -> list[Persona]:
    """Stratified sampling — equal distribution across all 4 personas."""
    personas = list(Persona)
    assigned = []
    for i in range(num_agents):
        assigned.append(personas[i % len(personas)])
    random.shuffle(assigned)
    return assigned
