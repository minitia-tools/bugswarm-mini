"""Centralized token counting — single source of truth for all token estimation.

Provides tiktoken-backed counting with graceful fallback heuristics.
All code in the monorepo that needs token counts imports from here.
"""

from __future__ import annotations

import functools
from typing import Any

_ENCODING_CACHE: dict[str, Any] = {}


def _get_encoding(model_or_encoding: str = "cl100k_base") -> Any | None:
    """Get a tiktoken encoding, cached."""
    if model_or_encoding in _ENCODING_CACHE:
        return _ENCODING_CACHE[model_or_encoding]
    try:
        import tiktoken

        try:
            enc = tiktoken.encoding_for_model(model_or_encoding)
        except KeyError:
            enc = tiktoken.get_encoding(model_or_encoding)
        _ENCODING_CACHE[model_or_encoding] = enc
        return enc
    except ImportError:
        return None


# ── Model → encoding mapping for known models ──

MODEL_TO_ENCODING: dict[str, str] = {
    # OpenAI
    "gpt-4o": "o200k_base",
    "gpt-4o-mini": "o200k_base",
    "gpt-4-turbo": "cl100k_base",
    "gpt-4": "cl100k_base",
    "gpt-3.5-turbo": "cl100k_base",
    # DeepSeek (uses OpenAI-compatible tokenizer)
    "deepseek-chat": "cl100k_base",
    "deepseek-v4-flash": "cl100k_base",
    "deepseek-v4": "cl100k_base",
    "deepseek-r1": "cl100k_base",
    # Anthropic (estimated via cl100k_base — Claude uses different tokenization)
    "claude-3-opus": "cl100k_base",
    "claude-3-sonnet": "cl100k_base",
    "claude-3-haiku": "cl100k_base",
    "claude-3.5-sonnet": "cl100k_base",
    "claude-3.5-haiku": "cl100k_base",
    # Google
    "gemini-1.5-pro": "cl100k_base",
    "gemini-1.5-flash": "cl100k_base",
    "gemini-2.0-flash": "cl100k_base",
}

# ── Public API ──


@functools.lru_cache(maxsize=4096)
def count_tokens(text: str, model_or_encoding: str = "cl100k_base") -> int:
    """Count tokens in text. Uses tiktoken; falls back to character heuristic.

    The result is cached (LRU, 4096 entries) to avoid re-encoding identical strings.
    """
    if not text:
        return 0
    enc = _get_encoding(model_or_encoding)
    if enc is not None:
        return len(enc.encode(text))
    return _heuristic(text)


def count_message_tokens(messages: list[dict], model: str = "gpt-4o-mini") -> int:
    """Count tokens for a list of chat messages, including framing overhead.

    Each message dict must have a 'content' key (string).
    Optionally: 'role' (used for framing estimation).

    Follows the same per-message overhead conventions as openai.py:
      4 tokens per message + 2 tokens for reply priming.
    """
    enc_name = MODEL_TO_ENCODING.get(model, "cl100k_base")
    total = 0
    for msg in messages:
        content = msg.get("content", "")
        total += count_tokens(content, enc_name) + 4
    total += 2
    return total


def count_tool_output(text: str, model: str = "gpt-4o-mini") -> int:
    """Count tokens for tool output. Same as count_tokens but semantically explicit."""
    return count_tokens(text, MODEL_TO_ENCODING.get(model, "cl100k_base"))


def segment_size(text: str, segment_tokens: int, model: str = "gpt-4o-mini") -> int:
    """Estimate how many characters approximate *segment_tokens* for this model.

    Useful for pre-segmentation before counting: a rough character-level split
    avoids calling encode() on every message during initial chunking.
    """
    chars_per_token = _chars_per_token_hint(model)
    return int(segment_tokens * chars_per_token)


# ── Heuristic fallback ──

_CHARS_PER_TOKEN_HINTS: dict[str, float] = {
    "o200k_base": 4.0,
    "cl100k_base": 3.5,
}


def _chars_per_token_hint(model_or_encoding: str = "cl100k_base") -> float:
    return _CHARS_PER_TOKEN_HINTS.get(model_or_encoding, 3.5)


def _heuristic(text: str) -> int:
    """Character-based heuristic when tiktoken is unavailable."""
    return max(1, round(len(text) / 3.5))
