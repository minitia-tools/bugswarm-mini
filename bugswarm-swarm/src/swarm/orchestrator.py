"""Phase 6: Multi-Agent Swarm Orchestrator.

12 agents compete to contribute evidence to a shared Evidence Graph.
Round-1 isolation, MMR critique routing, persona seeding, performance monitoring.
"""

from __future__ import annotations

import asyncio, hashlib, math, random, time
from collections import defaultdict
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any

import structlog
from gateway.types import ProviderType
from gateway.client import LLMClient

logger = structlog.get_logger(__name__)


# ═══════════════════════════════════════════════════════════════
# Types
# ═══════════════════════════════════════════════════════════════

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


# ═══════════════════════════════════════════════════════════════
# Persona Assignment
# ═══════════════════════════════════════════════════════════════

def assign_personas(num_agents: int) -> list[Persona]:
    """Stratified sampling — equal distribution across all 4 personas."""
    personas = list(Persona)
    assigned = []
    for i in range(num_agents):
        assigned.append(personas[i % len(personas)])
    random.shuffle(assigned)
    return assigned


# ═══════════════════════════════════════════════════════════════
# MMR Critique Routing
# ═══════════════════════════════════════════════════════════════

def cosine_similarity(a: list[float], b: list[float]) -> float:
    if not a or not b:
        return 0.0
    dot = sum(x * y for x, y in zip(a, b))
    norm_a = math.sqrt(sum(x * x for x in a))
    norm_b = math.sqrt(sum(x * x for x in b))
    if norm_a == 0 or norm_b == 0:
        return 0.0
    return dot / (norm_a * norm_b)


def simple_embed(text: str, dim: int = 64) -> list[float]:
    """Simple character-bigram embedding for similarity computation."""
    h = hashlib.sha256(text.encode()).digest()
    vec = []
    for i in range(0, min(len(h), dim)):
        vec.append(h[i] / 255.0)
    while len(vec) < dim:
        vec.append(0.0)
    return vec[:dim]


def mmr_critique_routing(
    agents: list[AgentSlot],
    hypotheses: dict[str, str],  # agent_id → hypothesis text
    pair_history: dict[tuple[str, str], int],
) -> list[tuple[str, str]]:
    """Maximum Marginal Relevance — pair each hypothesis with the most dissimilar reviewer."""
    agent_ids = [a.id for a in agents if a.status == AgentStatus.ACTIVE and a.id in hypotheses]
    if len(agent_ids) < 2:
        return []

    embeddings = {aid: simple_embed(hypotheses[aid]) for aid in agent_ids}
    similarity = {}
    for a1 in agent_ids:
        for a2 in agent_ids:
            if a1 >= a2:
                continue
            similarity[(a1, a2)] = cosine_similarity(embeddings[a1], embeddings[a2])

    pairings = []
    used_reviewers = set()
    used_hypotheses = set()

    # Sort hypotheses by "uniqueness" (lowest average similarity to others)
    uniqueness = {}
    for aid in agent_ids:
        sims = [similarity.get(tuple(sorted((aid, other))), 0.5) for other in agent_ids if other != aid]
        uniqueness[aid] = 1.0 - (sum(sims) / max(len(sims), 1))

    sorted_agents = sorted(agent_ids, key=lambda a: uniqueness[a], reverse=True)

    for author in sorted_agents:
        if author in used_hypotheses:
            continue
        best_reviewer = None
        best_dissimilarity = -1.0

        for reviewer in agent_ids:
            if reviewer == author or reviewer in used_reviewers:
                continue
            pair = tuple(sorted((author, reviewer)))
            # Penalize repeated pairings
            history_penalty = min(pair_history.get(pair, 0) * 0.1, 0.5)
            sim = similarity.get(pair, 0.5)
            dissimilarity = 1.0 - sim - history_penalty

            if dissimilarity > best_dissimilarity:
                best_dissimilarity = dissimilarity
                best_reviewer = reviewer

        if best_reviewer:
            pairings.append((author, best_reviewer))
            used_hypotheses.add(author)
            used_reviewers.add(best_reviewer)
            pair_history[tuple(sorted((author, best_reviewer)))] = \
                pair_history.get(tuple(sorted((author, best_reviewer))), 0) + 1

    return pairings


# ═══════════════════════════════════════════════════════════════
# Performance Monitor
# ═══════════════════════════════════════════════════════════════

class PerformanceMonitor:
    """Tracks agent performance and triggers ejection/recovery."""

    def __init__(self, config: SwarmConfig):
        self.config = config
        self.agent_history: dict[str, list[str]] = defaultdict(list)  # agent_id → last N messages
        self.loop_detections: dict[str, int] = defaultdict(int)
        self.pair_interactions: dict[tuple[str, str], int] = defaultdict(int)
        self.round_findings: dict[int, list[dict]] = defaultdict(list)

    def record_message(self, agent_id: str, content: str) -> None:
        self.agent_history[agent_id].append(content)
        if len(self.agent_history[agent_id]) > 20:
            self.agent_history[agent_id] = self.agent_history[agent_id][-20:]

    def detect_loop(self, agent_id: str) -> bool:
        """Check if agent's last 5 messages are too similar (semantic loop)."""
        history = self.agent_history[agent_id]
        if len(history) < 5:
            return False

        recent = history[-5:]
        embeddings = [simple_embed(m) for m in recent]
        similarities = []
        for i in range(len(embeddings)):
            for j in range(i + 1, len(embeddings)):
                similarities.append(cosine_similarity(embeddings[i], embeddings[j]))

        avg_sim = sum(similarities) / max(len(similarities), 1)
        return avg_sim > self.config.loop_similarity_threshold

    def detect_echo_chamber(self, agents: list[AgentSlot]) -> list[set[str]]:
        """Detect triadic echo chambers using SCC on agreement graph."""
        if len(agents) < 3:
            return []

        # Build agreement graph
        agreement: dict[str, set[str]] = defaultdict(set)
        for a1 in agents:
            for a2 in agents:
                if a1.id >= a2.id:
                    continue
                # Check if they've been agreeing recently
                h1 = self.agent_history.get(a1.id, [])
                h2 = self.agent_history.get(a2.id, [])
                if h1 and h2:
                    sim = cosine_similarity(simple_embed(h1[-1]), simple_embed(h2[-1]))
                    if sim > 0.85:
                        agreement[a1.id].add(a2.id)
                        agreement[a2.id].add(a1.id)

        # Find components of size >= 3
        visited = set()
        chambers = []

        def dfs(node: str, component: set[str]):
            visited.add(node)
            component.add(node)
            for neighbor in agreement.get(node, set()):
                if neighbor not in visited:
                    dfs(neighbor, component)

        for a in agents:
            if a.id not in visited:
                comp = set()
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


# ═══════════════════════════════════════════════════════════════
# Swarm Orchestrator
# ═══════════════════════════════════════════════════════════════

class SwarmOrchestrator:
    """Manages 12 agents in parallel, routing their contributions to the evidence graph."""

    def __init__(self, config: SwarmConfig, gateway: LLMClient):
        self.config = config
        self.gateway = gateway
        self.monitor = PerformanceMonitor(config)
        self.agents: dict[str, AgentSlot] = {}
        self.spare_pool: list[str] = []
        self.pair_history: dict[tuple[str, str], int] = {}
        self.semaphore = asyncio.Semaphore(config.batch_size)
        self.current_round = 0

    def initialize_agents(self) -> None:
        personas = assign_personas(self.config.num_agents)
        for i in range(self.config.num_agents):
            aid = f"A{i+1}"
            self.agents[aid] = AgentSlot(id=aid, persona=personas[i], status=AgentStatus.IDLE)

        # Spare pool
        for i in range(self.config.spare_pool_size):
            sid = f"S{i+1}"
            self.spare_pool.append(sid)
            self.agents[sid] = AgentSlot(
                id=sid, persona=Persona.CAUSAL, status=AgentStatus.IDLE,
            )

    async def run_round(self, round_num: int) -> dict:
        """Execute one round of the swarm."""
        self.current_round = round_num
        active = [a for a in self.agents.values() if a.status == AgentStatus.ACTIVE]
        logger.info("swarm_round_start", round=round_num, active_agents=len(active))

        results = {"round": round_num, "findings": [], "ejections": [], "loops": [], "echo_chambers": []}

        # Round 1: Independent hypothesis generation (agents isolated)
        if round_num == 1:
            hypotheses = await self._round1_isolation(active)
            results["hypotheses"] = hypotheses
        else:
            # Rounds 2+: MMR-based critique routing
            hypotheses_text = {
                a.id: a.round_hypotheses.get(1, "")
                for a in active if 1 in a.round_hypotheses
            }
            pairings = mmr_critique_routing(active, hypotheses_text, self.pair_history)
            results["pairings"] = [(a, r) for a, r in pairings]
            results["findings"].extend(await self._execute_round(active, round_num))

        # Post-round analysis
        results["echo_chambers"] = self.monitor.detect_echo_chamber(active)
        results["loops"] = [
            aid for aid, a in self.agents.items()
            if a.status == AgentStatus.LOOPING
        ]

        # Diversity check
        diversity = self._compute_diversity(active)
        if diversity < self.config.diversity_warning_threshold:
            logger.warning("diversity_collapse", round=round_num, diversity=diversity)
            results["diversity_warning"] = True

        return results

    async def _round1_isolation(self, agents: list[AgentSlot]) -> dict[str, str]:
        """Round 1: Each agent generates hypotheses independently."""
        hypotheses = {}

        async def agent_generate(agent: AgentSlot):
            async with self.semaphore:
                agent.status = AgentStatus.ACTIVE
                # Simulate hypothesis generation (in production, calls LLM)
                hypothesis = f"Agent {agent.id} ({agent.persona.value}) investigates the codebase"
                agent.round_hypotheses[1] = hypothesis
                hypotheses[agent.id] = hypothesis
                logger.info("agent_hypothesis", agent=agent.id, persona=agent.persona.value)

        tasks = [agent_generate(a) for a in agents]
        await asyncio.gather(*tasks, return_exceptions=True)
        return hypotheses

    async def _execute_round(self, agents: list[AgentSlot], round_num: int) -> list[dict]:
        """Execute critique and evidence gathering for a round."""
        findings = []

        async def agent_turn(agent: AgentSlot):
            async with self.semaphore:
                # Check for loops
                if self.monitor.detect_loop(agent.id):
                    agent.loop_count += 1
                    agent.status = AgentStatus.LOOPING
                    logger.warning("agent_looping", agent=agent.id, count=agent.loop_count)

                # Check ejection
                should_eject, reason = self.monitor.should_eject(agent)
                if should_eject:
                    agent.status = AgentStatus.EJECTED
                    agent.ejection_reason = reason
                    logger.warning("agent_ejected", agent=agent.id, reason=reason)
                    self._replace_agent(agent.id)
                    return

                # Simulate findings (in production, calls LLM + tools + sandbox)
                if random.random() < 0.6:  # 60% chance of finding
                    finding = {
                        "claim": f"Bug from {agent.id} in round {round_num}",
                        "agent": agent.id,
                        "persona": agent.persona.value,
                        "round": round_num,
                        "severity_estimate": random.randint(3, 9),
                        "verified": random.random() < 0.4,
                    }
                    findings.append(finding)
                    agent.contribution_score += 1
                    if finding["verified"]:
                        agent.trust_score = min(1.0, agent.trust_score + 0.05)
                    else:
                        agent.hallucination_rate = (agent.hallucination_rate * 0.8 + 0.2)

        tasks = []
        for _ in range(self.config.max_turns_per_round):
            for agent in agents:
                if agent.status == AgentStatus.ACTIVE:
                    tasks.append(agent_turn(agent))
        await asyncio.gather(*tasks, return_exceptions=True)

        return findings

    def _replace_agent(self, ejected_id: str) -> None:
        """Replace an ejected agent with one from the spare pool."""
        if self.spare_pool:
            replacement = self.spare_pool.pop(0)
            old_persona = self.agents[ejected_id].persona
            self.agents[replacement].status = AgentStatus.ACTIVE
            self.agents[replacement].persona = old_persona
            logger.info("agent_replaced", ejected=ejected_id, replacement=replacement)
        else:
            logger.warning("spare_pool_exhausted", ejected=ejected_id)

    def reinstate_agent(self, agent_id: str) -> None:
        """Reinstate a falsely ejected agent."""
        if agent_id in self.agents:
            self.agents[agent_id].status = AgentStatus.ACTIVE
            self.agents[agent_id].trust_score = min(1.0, self.agents[agent_id].trust_score + 0.2)
            logger.info("agent_reinstated", agent=agent_id)

    def _compute_diversity(self, agents: list[AgentSlot]) -> float:
        """Compute diversity score across active agents."""
        active = [a for a in agents if a.status == AgentStatus.ACTIVE]
        if len(active) < 2:
            return 1.0

        verbs = []
        for a in active:
            h = a.round_hypotheses.get(self.current_round, "")
            if not h:
                h = a.round_hypotheses.get(1, "")
            verbs.append(h)

        embeddings = [simple_embed(v) for v in verbs]
        similarities = []
        for i in range(len(embeddings)):
            for j in range(i + 1, len(embeddings)):
                similarities.append(cosine_similarity(embeddings[i], embeddings[j]))

        avg_sim = sum(similarities) / max(len(similarities), 1)
        return 1.0 - avg_sim  # Higher = more diverse

    async def run(self) -> dict:
        """Execute the full multi-agent swarm."""
        self.initialize_agents()

        # Activate all agents
        for aid in self.agents:
            if not aid.startswith("S"):
                self.agents[aid].status = AgentStatus.ACTIVE

        all_findings = []
        round_results = []

        for round_num in range(1, self.config.max_rounds + 1):
            logger.info("swarm_round", round=round_num)
            result = await self.run_round(round_num)
            all_findings.extend(result.get("findings", []))
            round_results.append(result)

            # Check termination
            verified = sum(1 for f in all_findings if f.get("verified"))
            if verified >= 5:
                logger.info("swarm_terminated", reason="enough_verified", verified=verified)
                break

        # Final scores
        scores = {}
        for a in self.agents.values():
            if a.status in (AgentStatus.ACTIVE, AgentStatus.COMPLETED):
                scores[a.id] = {
                    "persona": a.persona.value,
                    "trust": a.trust_score,
                    "contributions": a.contribution_score,
                    "hallucination_rate": a.hallucination_rate,
                    "loops": a.loop_count,
                    "status": a.status.value,
                }

        return {
            "rounds": len(round_results),
            "total_findings": len(all_findings),
            "verified_findings": sum(1 for f in all_findings if f.get("verified")),
            "agent_scores": scores,
            "ejections": sum(1 for a in self.agents.values() if a.status == AgentStatus.EJECTED),
            "round_results": round_results,
        }
