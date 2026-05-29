"""Tool Output Relevance Scorer — heuristic pipeline, zero LLM cost.

Rule: Determines whether tool output stays in context window, is offloaded to disk,
or discarded. Uses structural signals, keyword overlap, and size heuristics.
"""

from __future__ import annotations

import hashlib
import re

import structlog
from gateway.tokenizer import count_tool_output

logger = structlog.get_logger(__name__)


class RelevanceScorer:
    """Determines whether tool output should be kept in context or offloaded.

    Scoring:
    > 0.7: Keep in context window
    0.3–0.7: Offload to disk, keep summary
    < 0.3: Discard entirely (duplicate or irrelevant)
    """

    def __init__(self):
        self.content_cache: dict[str, str] = {}
        self.stats = {"kept": 0, "offloaded": 0, "discarded": 0}

    def score(self, hypothesis: str, tool_output: str) -> tuple[float, str]:
        """Score relevance and decide disposition."""
        # 1. Exact duplicate check (SHA-256)
        output_hash = hashlib.sha256(tool_output.encode()).hexdigest()
        if output_hash in self.content_cache:
            self.stats["discarded"] += 1
            return 0.0, "discard: duplicate"

        self.content_cache[output_hash] = tool_output

        # 2. Size heuristic — small outputs always kept
        token_est = count_tool_output(tool_output)
        if token_est < 200:
            self.stats["kept"] += 1
            return 1.0, "keep: small output"

        # 3. Structural signals (Python traceback, error messages, sandbox results)
        if re.search(r'File ".*", line \d+', tool_output):
            self.stats["kept"] += 1
            return 0.95, "keep: python traceback"
        if re.search(r"(?:error|Error|ERROR|exception|Exception)", tool_output):
            self.stats["kept"] += 1
            return 0.90, "keep: error message"
        if re.search(r"(?:PASS|FAIL|exit_code|oom_killed|status)", tool_output):
            self.stats["kept"] += 1
            return 0.85, "keep: sandbox result"

        # 4. Keyword overlap with current hypothesis
        if hypothesis:
            hyp_words = set(hypothesis.lower().split())
            out_words = set(tool_output.lower().split())
            if hyp_words:
                overlap = len(hyp_words & out_words) / len(hyp_words)
                if overlap > 0.3:
                    self.stats["kept"] += 1
                    return 0.7, "keep: hypothesis overlap"
                elif overlap > 0.1:
                    self.stats["offloaded"] += 1
                    return 0.5, "offload: moderate overlap"

        # 5. Size-based disposition
        if token_est > 5000:
            self.stats["offloaded"] += 1
            return 0.4, "offload: large output"
        if token_est > 2000:
            self.stats["offloaded"] += 1
            return 0.5, "offload: verbose"

        self.stats["kept"] += 1
        return 0.6, "keep: default"
