"""Phase 7: Context Compression Engine.

Map-reduce summarization with fidelity-gated tiered routing.
Prevents context bloat when 12+ agents accumulate debate history.
"""

from __future__ import annotations

import hashlib
import math
import re
from collections import OrderedDict

import structlog
from gateway.tokenizer import count_tokens, count_tool_output

logger = structlog.get_logger(__name__)


# ═══════════════════════════════════════════════════════════════
# Simple Embedding + Similarity (no external deps needed)
# ═══════════════════════════════════════════════════════════════


def simple_embed(text: str, dim: int = 64) -> list[float]:
    h = hashlib.sha256(text.encode()).digest()
    vec = [h[i] / 255.0 for i in range(min(len(h), dim))]
    while len(vec) < dim:
        vec.append(0.0)
    return vec


def cosine_sim(a: list[float], b: list[float]) -> float:
    if not a or not b:
        return 0.0
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(x * x for x in b))
    return dot / (na * nb) if na > 0 and nb > 0 else 0.0


# ═══════════════════════════════════════════════════════════════
# ROUGE-L Fidelity Check
# ═══════════════════════════════════════════════════════════════


def rouge_l(reference: str, candidate: str) -> float:
    """ROUGE-L: longest common subsequence based recall."""
    ref_words = reference.lower().split()
    cand_words = candidate.lower().split()
    if not ref_words or not cand_words:
        return 0.0

    m, n = len(ref_words), len(cand_words)
    dp = [[0] * (n + 1) for _ in range(m + 1)]
    for i in range(m):
        for j in range(n):
            if ref_words[i] == cand_words[j]:
                dp[i + 1][j + 1] = dp[i][j] + 1
            else:
                dp[i + 1][j + 1] = max(dp[i + 1][j], dp[i][j + 1])

    lcs = dp[m][n]
    recall = lcs / m if m > 0 else 0.0
    precision = lcs / n if n > 0 else 0.0
    if recall + precision == 0:
        return 0.0
    return 2 * recall * precision / (recall + precision)


def bertscore_approx(reference: str, candidate: str) -> float:
    """Approximate BERTScore using word overlap + embedding similarity."""
    ref_words = reference.lower().split()
    cand_words = candidate.lower().split()
    if not ref_words or not cand_words:
        return 0.0

    ref_embs = [simple_embed(w, 16) for w in ref_words]
    cand_embs = [simple_embed(w, 16) for w in cand_words]

    # Precision: average max similarity for each candidate word
    prec_sum = 0.0
    for ce in cand_embs:
        max_sim = max((cosine_sim(ce, re) for re in ref_embs), default=0.0)
        prec_sum += max_sim
    precision = prec_sum / len(cand_embs) if cand_embs else 0.0

    # Recall: average max similarity for each reference word
    rec_sum = 0.0
    for re in ref_embs:
        max_sim = max((cosine_sim(re, ce) for ce in cand_embs), default=0.0)
        rec_sum += max_sim
    recall = rec_sum / len(ref_embs) if ref_embs else 0.0

    if precision + recall == 0:
        return 0.0
    return 2 * precision * recall / (precision + recall)


def compute_fidelity(original: str, summary: str) -> dict[str, float]:
    """Compute fidelity scores for a summary against its original."""
    return {
        "rouge_l": rouge_l(original, summary),
        "bertscore": bertscore_approx(original, summary),
        "compression_ratio": len(summary) / max(len(original), 1),
    }


# ═══════════════════════════════════════════════════════════════
# Context Compression Engine
# ═══════════════════════════════════════════════════════════════


class ContextCompressor:
    """Manages context window size and triggers compression when needed."""

    def __init__(
        self,
        max_context_tokens: int = 128_000,
        compression_trigger_pct: float = 0.80,
        interaction_threshold: int = 12,
        min_fidelity: float = 0.85,
        summary_max_tokens: int = 2000,
        segment_size_tokens: int = 4000,
    ):
        self.max_context_tokens = max_context_tokens
        self.compression_trigger_pct = compression_trigger_pct
        self.interaction_threshold = interaction_threshold
        self.min_fidelity = min_fidelity
        self.summary_max_tokens = summary_max_tokens
        self.segment_size_tokens = segment_size_tokens
        self.interactions_since_compression = 0
        self.compression_count = 0
        self.fidelity_history: list[float] = []

    def should_compress(self, current_tokens: int, context_window: int) -> bool:
        """Determine if compression should trigger."""
        self.interactions_since_compression += 1
        pct = current_tokens / max(context_window, 1)
        if pct >= self.compression_trigger_pct:
            return True
        if self.interactions_since_compression >= self.interaction_threshold:
            return True
        return False

    def compress(self, messages: list[dict], tier: int = 0) -> tuple[str, float]:
        """Compress conversation history using map-reduce.

        Tier 0: Extractive (keep key claims, code references, sandbox results)
        Tier 1: Abstractive (LLM-powered, simulated here)
        Tier 2: Lossless fallback (truncate oldest, keep recent verbatim)
        """
        self.interactions_since_compression = 0
        self.compression_count += 1

        if tier == 0:
            summary = self._extractive_compress(messages)
        elif tier == 1:
            summary = self._simulated_abstractive_compress(messages)
        else:
            summary = self._lossless_compress(messages)

        # Fidelity check
        original_text = " ".join(m.get("content", "") for m in messages[-20:])
        fidelity = compute_fidelity(original_text, summary)
        self.fidelity_history.append(fidelity["bertscore"])

        if fidelity["bertscore"] < self.min_fidelity and tier < 2:
            logger.warning(
                "compression_low_fidelity", tier=tier, bertscore=fidelity["bertscore"], action="escalating_tier"
            )
            return self.compress(messages, tier + 1)

        logger.info(
            "compression_complete",
            tier=tier,
            bertscore=fidelity["bertscore"],
            ratio=fidelity["compression_ratio"],
            total_compressions=self.compression_count,
        )

        return summary, fidelity["bertscore"]

    def _extractive_compress(self, messages: list[dict]) -> str:
        """Extract key claims, code locations, and sandbox results."""
        parts = []
        parts.append(f"[Compressed context — {len(messages)} messages]")

        claims = []
        locations = set()
        sandbox_runs = []

        for msg in messages[-30:]:
            content = msg.get("content", "")
            role = msg.get("role", "")

            if "finding" in content.lower() or "bug" in content.lower():
                # Extract claim
                lines = content.split("\n")
                for line in lines:
                    line = line.strip()
                    if any(kw in line.lower() for kw in ["bug", "vulnerability", "injection", "overflow"]):
                        if len(line) > 10:
                            claims.append(line[:150])

            # Extract code locations
            for match in re.finditer(r"(\w+\.py):(\d+)", content):
                locations.add(f"{match.group(1)}:{match.group(2)}")

            # Extract sandbox results
            if "sandbox" in content.lower() and ("pass" in content.lower() or "fail" in content.lower()):
                sandbox_runs.append(content[:200])

        if claims:
            parts.append("\nClaims:")
            for c in claims[:5]:
                parts.append(f"  - {c}")
        if locations:
            parts.append(f"\nCode locations: {', '.join(sorted(locations)[:10])}")
        if sandbox_runs:
            parts.append(f"\nSandbox results: {len(sandbox_runs)} executions")
        parts.append("\n[End compressed context]")
        return "\n".join(parts)

    def _simulated_abstractive_compress(self, messages: list[dict]) -> str:
        """Simulated LLM summarization (in production, calls cheap LLM)."""
        parts = [f"[Abstractive summary of {len(messages)} messages]"]

        # Segment into 4000-token chunks
        segments = []
        current = []
        current_len = 0
        for msg in messages:
            content = msg.get("content", "")
            token_est = count_tokens(content)
            if current_len + token_est > self.segment_size_tokens and current:
                segments.append(current)
                current = []
                current_len = 0
            current.append(msg)
            current_len += token_est
        if current:
            segments.append(current)

        # Summarize each segment
        for i, seg in enumerate(segments):
            claim_texts = []
            for m in seg:
                c = m.get("content", "")
                if len(c) > 50:
                    claim_texts.append(c[:120])
            if claim_texts:
                parts.append(f"\nSegment {i + 1}: {len(seg)} messages, {len(claim_texts)} claims")
                parts.append(f"  Key point: {claim_texts[0]}")

        parts.append("\n[End abstractive summary]")
        return "\n".join(parts)

    def _lossless_compress(self, messages: list[dict]) -> str:
        """Keep recent messages verbatim, truncate oldest."""
        keep_recent = min(10, len(messages))
        parts = [f"[{len(messages) - keep_recent} older messages truncated]"]
        for msg in messages[-keep_recent:]:
            content = msg.get("content", "")[:200]
            parts.append(f"[{msg.get('role', '?')}] {content}")
        parts.append("[End lossless context]")
        return "\n".join(parts)


# ═══════════════════════════════════════════════════════════════
# Enhanced Loop Detector
# ═══════════════════════════════════════════════════════════════


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
            self.agent_history[agent_id] = self.agent_history[agent_id][-self.window_size * 2 :]

    def detect_semantic_loop(self, agent_id: str) -> tuple[bool, str]:
        """Check if agent's recent messages form a semantic loop."""
        history = self.agent_history.get(agent_id, [])
        if len(history) < self.window_size:
            return False, ""

        recent = history[-self.window_size :]
        embeddings = [simple_embed(m) for m in recent]
        similarities = []
        for i in range(len(embeddings)):
            for j in range(i + 1, len(embeddings)):
                similarities.append(cosine_sim(embeddings[i], embeddings[j]))

        avg_sim = sum(similarities) / max(len(similarities), 1) if similarities else 0.0
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

        # Check last 3 exchanges
        for i in range(1, min(4, len(ha), len(hb))):
            sim = cosine_sim(simple_embed(ha[-i]), simple_embed(hb[-i]))
            if sim < self.threshold:
                return False
        self.pair_interactions[(agent_a, agent_b)] = self.pair_interactions.get((agent_a, agent_b), 0) + 1
        return True

    def detect_echo_chamber(self, agents: list[str]) -> list[set[str]]:
        """Detect groups of 3+ agents forming circular reinforcement chains."""
        if len(agents) < 3:
            return []

        # Build agreement graph
        graph: dict[str, set[str]] = {}
        for a in agents:
            graph[a] = set()
        for i, a1 in enumerate(agents):
            h1 = self.agent_history.get(a1, [])
            if not h1:
                continue
            for a2 in agents[i + 1 :]:
                h2 = self.agent_history.get(a2, [])
                if not h2:
                    continue
                sim = cosine_sim(simple_embed(h1[-1]), simple_embed(h2[-1]))
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

        def strongconnect(v: str):
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
            "CRITICAL: Your last 5 messages are nearly identical. Either find new evidence or disengage from this line of investigation.",
        ]
        return prompts[count % len(prompts)]


# ═══════════════════════════════════════════════════════════════
# Rate Limiter for recall_raw_context
# ═══════════════════════════════════════════════════════════════


class RecallRateLimiter:
    """Prevents agents from griefing by overusing recall_raw_context."""

    def __init__(self, max_per_round: int = 3, cooldown_turns: int = 5):
        self.max_per_round = max_per_round
        self.cooldown_turns = cooldown_turns
        self.agent_recalls: dict[str, list[int]] = {}  # agent_id → turn numbers
        self.agent_trust_penalties: dict[str, float] = {}

    def can_recall(self, agent_id: str, turn: int) -> bool:
        turns = self.agent_recalls.get(agent_id, [])
        recent = [t for t in turns if turn - t <= self.cooldown_turns]
        round_recalls = sum(1 for t in turns if t == turn)  # Simplified
        return len(recent) < self.max_per_round

    def record_recall(self, agent_id: str, turn: int, reason: str, context_hash: str) -> None:
        if agent_id not in self.agent_recalls:
            self.agent_recalls[agent_id] = []
        self.agent_recalls[agent_id].append(turn)
        # Penalize trust if overused
        if len([t for t in self.agent_recalls[agent_id] if turn - t <= self.cooldown_turns]) > self.max_per_round:
            self.agent_trust_penalties[agent_id] = self.agent_trust_penalties.get(agent_id, 0.0) + 0.1

    def get_trust_penalty(self, agent_id: str) -> float:
        return self.agent_trust_penalties.get(agent_id, 0.0)


# ═══════════════════════════════════════════════════════════════
# Tool Output Relevance Scorer (Heuristic, Zero LLM Cost)
# ═══════════════════════════════════════════════════════════════


class RelevanceScorer:
    """Determines whether tool output should be kept in context or offloaded to disk."""

    def __init__(self):
        self.content_cache: dict[str, str] = {}

    def score(self, hypothesis: str, tool_output: str) -> tuple[float, str]:
        """Score relevance and decide disposition.

        > 0.7: Keep in context window
        0.3-0.7: Offload to disk, keep summary
        < 0.3: Discard entirely
        """
        # 1. Exact duplicate check (SHA-256)
        output_hash = hashlib.sha256(tool_output.encode()).hexdigest()
        if output_hash in self.content_cache:
            return 0.0, "discard: duplicate"

        self.content_cache[output_hash] = tool_output

        # 2. Size heuristic
        token_est = count_tool_output(tool_output)
        if token_est < 200:
            return 1.0, "keep: small output"

        # 3. Structural signals
        if re.search(r'File ".*", line \d+', tool_output):
            return 0.95, "keep: python traceback"
        if re.search(r"(?:error|Error|ERROR|exception|Exception)", tool_output):
            return 0.90, "keep: error message"
        if re.search(r"✓|✓|✅|❌|PASS|FAIL|exit_code", tool_output):
            return 0.85, "keep: sandbox result"

        # 4. Keyword overlap with hypothesis
        if hypothesis:
            hyp_words = set(hypothesis.lower().split())
            out_words = set(tool_output.lower().split())
            overlap = len(hyp_words & out_words) / max(len(hyp_words), 1)
            if overlap > 0.3:
                return 0.7, "keep: hypothesis overlap"
            elif overlap > 0.1:
                return 0.5, "offload: moderate overlap"

        # 5. Size-based offload
        if token_est > 5000:
            return 0.4, "offload: large output"
        if token_est > 2000:
            return 0.5, "offload: verbose"

        return 0.6, "keep: default"


# ═══════════════════════════════════════════════════════════════
# Vector DB Query Cache
# ═══════════════════════════════════════════════════════════════


class QueryCache:
    """Per-swarm cache for Vector DB queries with embedding-based dedup."""

    def __init__(self, ttl_seconds: float = 300.0):
        self.cache: OrderedDict[str, tuple[str, float]] = OrderedDict()  # hash → (result, timestamp)
        self.ttl = ttl_seconds
        self.hits = 0
        self.misses = 0
        import time

        self._time = time.time

    def get(self, embedding: list[float]) -> str | None:
        """Get cached result for an embedding vector."""
        key = hashlib.sha256(",".join(f"{x:.6f}" for x in embedding).encode()).hexdigest()

        if key in self.cache:
            result, ts = self.cache[key]
            if self._time() - ts < self.ttl:
                self.hits += 1
                # Move to end (LRU)
                self.cache.move_to_end(key)
                return result
            else:
                del self.cache[key]

        self.misses += 1
        return None

    def set(self, embedding: list[float], result: str) -> None:
        key = hashlib.sha256(",".join(f"{x:.6f}" for x in embedding).encode()).hexdigest()
        self.cache[key] = (result, self._time())
        if len(self.cache) > 1000:
            self.cache.popitem(last=False)  # Evict oldest

    @property
    def hit_rate(self) -> float:
        total = self.hits + self.misses
        return self.hits / total if total > 0 else 0.0

    def invalidate(self) -> None:
        self.cache.clear()
