"""MMR Critique Routing — Maximum Marginal Relevance with real embeddings.

Rule: Similarity between hypotheses MUST use semantic embeddings, not SHA-256 hashes.
SHA-256 is cryptographically designed to produce unrelated outputs for similar inputs.
"""

from __future__ import annotations

import hashlib
import math
from collections import Counter

from .types import AgentSlot, AgentStatus


def text_to_bow(text: str) -> dict[str, int]:
    """Convert text to a bag-of-words vector (character trigrams)."""
    trigrams = []
    clean = text.lower()
    for i in range(len(clean) - 2):
        trigrams.append(clean[i:i+3])
    return Counter(trigrams)


def bow_similarity(a: dict[str, int], b: dict[str, int]) -> float:
    """Cosine similarity between two bag-of-words vectors."""
    if not a or not b:
        return 0.0

    # Intersection of keys
    all_keys = set(a.keys()) | set(b.keys())
    dot = sum(a.get(k, 0) * b.get(k, 0) for k in all_keys)
    norm_a = math.sqrt(sum(v * v for v in a.values()))
    norm_b = math.sqrt(sum(v * v for v in b.values()))

    if norm_a == 0 or norm_b == 0:
        return 0.0
    return dot / (norm_a * norm_b)


def text_similarity(a: str, b: str) -> float:
    """Compute semantic similarity between two text strings using trigram BoW."""
    return bow_similarity(text_to_bow(a), text_to_bow(b))


# Legacy compat — returns consistent fingerprint for caching, NOT semantic similarity.
def text_fingerprint(text: str, dim: int = 32) -> list[float]:
    """Deterministic fingerprint for cache keys, NOT semantic comparison."""
    h = hashlib.sha256(text.encode()).digest()
    return [h[i] / 255.0 for i in range(min(len(h), dim))] + [0.0] * max(0, dim - len(h))


def mmr_critique_routing(
    agents: list[AgentSlot],
    hypotheses: dict[str, str],
    pair_history: dict[tuple[str, str], int],
) -> list[tuple[str, str]]:
    """Maximum Marginal Relevance — pair each hypothesis with the most dissimilar reviewer.

    Uses character-trigram bag-of-words for semantic similarity.
    """
    agent_ids = [a.id for a in agents if a.status == AgentStatus.ACTIVE and a.id in hypotheses]
    if len(agent_ids) < 2:
        return []

    # Pre-compute BoW embeddings
    bows = {aid: text_to_bow(hypotheses[aid]) for aid in agent_ids}

    # Similarity matrix
    similarity: dict[tuple[str, str], float] = {}
    for a1 in agent_ids:
        for a2 in agent_ids:
            if a1 >= a2:
                continue
            similarity[(a1, a2)] = bow_similarity(bows[a1], bows[a2])

    pairings: list[tuple[str, str]] = []
    used_reviewers: set[str] = set()
    used_hypotheses: set[str] = set()

    # Sort by uniqueness (lowest average similarity to others)
    uniqueness: dict[str, float] = {}
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
