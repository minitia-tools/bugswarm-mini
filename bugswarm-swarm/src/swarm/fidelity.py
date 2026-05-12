"""Fidelity Metrics — ROUGE-L, BERTScore approximation, compression ratio.

Rule: Word similarity uses trigram bag-of-words, NOT SHA-256 hashes.
SHA-256 is cryptographic — similar words produce maximally different hashes.
"""

from __future__ import annotations

import math
from collections import Counter


# ═══════════════════════════════════════════════════════════════
# Text Similarity (trigram BoW — NOT SHA-256)
# ═══════════════════════════════════════════════════════════════

def text_to_trigrams(text: str) -> Counter:
    """Convert text to character trigram frequencies."""
    clean = text.lower()
    return Counter(clean[i:i+3] for i in range(len(clean) - 2))


def trigram_similarity(a: str, b: str) -> float:
    """Cosine similarity between trigram frequency vectors."""
    ta = text_to_trigrams(a)
    tb = text_to_trigrams(b)
    if not ta or not tb:
        return 0.0
    all_keys = set(ta.keys()) | set(tb.keys())
    dot = sum(ta.get(k, 0) * tb.get(k, 0) for k in all_keys)
    na = math.sqrt(sum(v * v for v in ta.values()))
    nb = math.sqrt(sum(v * v for v in tb.values()))
    return dot / (na * nb) if na > 0 and nb > 0 else 0.0


# ═══════════════════════════════════════════════════════════════
# ROUGE-L — Longest Common Subsequence F1
# ═══════════════════════════════════════════════════════════════

def rouge_l(reference: str, candidate: str) -> float:
    """ROUGE-L: longest common subsequence based F1 score."""
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


# ═══════════════════════════════════════════════════════════════
# BERTScore Approximation (trigram BoW, not SHA-256)
# ═══════════════════════════════════════════════════════════════

def bertscore_approx(reference: str, candidate: str) -> float:
    """Approximate BERTScore using trigram overlap, not SHA-256 word embeddings."""
    ref_words = reference.lower().split()
    cand_words = candidate.lower().split()
    if not ref_words or not cand_words:
        return 0.0

    # Convert each word to its trigram vector
    ref_vecs = [text_to_trigrams(w) for w in ref_words]
    cand_vecs = [text_to_trigrams(w) for w in cand_words]

    def vec_similarity(va: Counter, vb: Counter) -> float:
        keys = set(va.keys()) | set(vb.keys())
        dot = sum(va.get(k, 0) * vb.get(k, 0) for k in keys)
        na = math.sqrt(sum(v * v for v in va.values()))
        nb = math.sqrt(sum(v * v for v in vb.values()))
        return dot / (na * nb) if na > 0 and nb > 0 else 0.0

    # Precision: avg max similarity for each candidate word
    prec_sum = sum(max((vec_similarity(ce, re) for re in ref_vecs), default=0.0) for ce in cand_vecs)
    precision = prec_sum / len(cand_vecs) if cand_vecs else 0.0

    # Recall: avg max similarity for each reference word
    rec_sum = sum(max((vec_similarity(re, ce) for ce in cand_vecs), default=0.0) for re in ref_vecs)
    recall = rec_sum / len(ref_vecs) if ref_vecs else 0.0

    if precision + recall == 0:
        return 0.0
    return 2 * precision * recall / (precision + recall)


# ═══════════════════════════════════════════════════════════════
# Fidelity Computation
# ═══════════════════════════════════════════════════════════════

def compute_fidelity(original: str, summary: str) -> dict[str, float]:
    """Compute fidelity scores for a summary against its original."""
    return {
        "rouge_l": rouge_l(original, summary),
        "bertscore": bertscore_approx(original, summary),
        "compression_ratio": len(summary) / max(len(original), 1),
    }
