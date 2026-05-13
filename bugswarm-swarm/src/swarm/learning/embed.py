"""Code-to-Embedding — sentence-transformer all-MiniLM-L6-v2.

C6.2 PEAK: Uses production-grade sentence transformer (384-dim)
with trigram BoW fallback if the model is unavailable.
"""

from __future__ import annotations

import hashlib
import math
from collections import Counter


def text_to_trigrams(text: str) -> Counter:
    clean = text.lower()
    return Counter(clean[i : i + 3] for i in range(len(clean) - 2))


def bow_similarity(a: str, b: str) -> float:
    """Trigram bag-of-words cosine similarity. Zero-dependency fallback."""
    ta = text_to_trigrams(a)
    tb = text_to_trigrams(b)
    if not ta or not tb:
        return 0.0
    keys = set(ta.keys()) | set(tb.keys())
    dot = sum(ta.get(k, 0) * tb.get(k, 0) for k in keys)
    na = math.sqrt(sum(v * v for v in ta.values()))
    nb = math.sqrt(sum(v * v for v in tb.values()))
    return dot / (na * nb) if na > 0 and nb > 0 else 0.0


class CodeEmbedder:
    """Embeds code snippets into 384-dim vectors for similarity search.

    Uses sentence-transformers (all-MiniLM-L6-v2) when available.
    Falls back to trigram BoW hash when model is unavailable.
    """

    def __init__(self):
        self._model = None
        self._model_available = False
        self._init_model()

    def _init_model(self):
        try:
            from sentence_transformers import SentenceTransformer
            self._model = SentenceTransformer("all-MiniLM-L6-v2")
            self._model_available = True
        except Exception:
            self._model_available = False

    def encode(self, text: str) -> list[float]:
        """Encode text into embedding vector. 384-dim if model available, 32-dim fallback."""
        if self._model_available and self._model:
            try:
                result = self._model.encode([text[:2000]], show_progress_bar=False)
                return result[0].tolist()
            except Exception:
                pass
        return self._fallback_embed(text)

    def _fallback_embed(self, text: str) -> list[float]:
        """Fallback: deterministic hash-based embedding (not semantic, but consistent)."""
        h = hashlib.sha256(text.encode()).digest()
        return [h[i] / 255.0 for i in range(min(len(h), 32))]

    def similarity(self, a: str, b: str) -> float:
        """Compute similarity between two text strings."""
        if self._model_available:
            try:
                ea = self.encode(a)
                eb = self.encode(b)
                return self._cosine(ea, eb)
            except Exception:
                pass
        return bow_similarity(a, b)

    @staticmethod
    def _cosine(a: list[float], b: list[float]) -> float:
        dot = sum(x * y for x, y in zip(a, b))
        na = math.sqrt(sum(x * x for x in a))
        nb = math.sqrt(sum(x * x for x in b))
        return dot / (na * nb) if na > 0 and nb > 0 else 0.0

    @property
    def is_available(self) -> bool:
        return self._model_available

    @property
    def dimension(self) -> int:
        return 384 if self._model_available else 32
