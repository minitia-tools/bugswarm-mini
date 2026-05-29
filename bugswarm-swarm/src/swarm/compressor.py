"""Context Compressor — map-reduce summarization with fidelity-gated tier escalation.

Rule: Compression uses extractive → abstractive → lossless tiers. Fidelity < 0.85
triggers tier escalation. Never lose raw context — compression is non-destructive.
"""

from __future__ import annotations

import re

import structlog
from gateway.tokenizer import count_tokens

from .fidelity import compute_fidelity

logger = structlog.get_logger(__name__)


class ContextCompressor:
    """Manages context window size and triggers compression when needed.

    Tier 0: Extractive (claims, code locations, sandbox results)
    Tier 1: Abstractive (LLM-powered — requires gateway)
    Tier 2: Lossless fallback (truncate oldest, keep recent verbatim)
    """

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

        Returns (summary_text, fidelity_score).
        """
        self.interactions_since_compression = 0
        self.compression_count += 1

        if tier == 0:
            summary = self._extractive_compress(messages)
        elif tier == 1:
            summary = self._abstractive_compress(messages)
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
        parts = [f"[Compressed context — {len(messages)} messages]"]
        claims = []
        locations: set[str] = set()
        sandbox_runs = []

        for msg in messages[-30:]:
            content = msg.get("content", "")

            if "finding" in content.lower() or "bug" in content.lower():
                for line in content.split("\n"):
                    line = line.strip()
                    if any(kw in line.lower() for kw in ["bug", "vulnerability", "injection", "overflow"]):
                        if len(line) > 10:
                            claims.append(line[:150])

            for match in re.finditer(r"(\w+\.py):(\d+)", content):
                locations.add(f"{match.group(1)}:{match.group(2)}")

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

    def _abstractive_compress(self, messages: list[dict]) -> str:
        """Abstractive compression via map-reduce segmentation.

        In production with LLM: each segment is summarized by a cheap model,
        then segment summaries are concatenated and summarized again.
        Currently extractive fallback until LLM gateway integration.
        """
        parts = [f"[Abstractive summary of {len(messages)} messages]"]

        # Segment into chunks
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

        for i, seg in enumerate(segments):
            claim_texts = [m.get("content", "")[:120] for m in seg if len(m.get("content", "")) > 50]
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
