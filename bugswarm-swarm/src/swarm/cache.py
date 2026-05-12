"""Query Cache — LRU cache with TTL and embedding-based deduplication.

Rule: Cache keys use text content hash, not embedding vector exact match.
Exact embedding match is too fragile — an embedding shifted by 0.0001 would miss.
"""

from __future__ import annotations

import hashlib
import time
from collections import OrderedDict

import structlog

logger = structlog.get_logger(__name__)


class QueryCache:
    """Per-swarm cache with TTL-based LRU eviction.

    Key = SHA-256 of (query_text + tool_name), not embedding vector.
    This ensures semantically identical queries hit the cache regardless of
    minor embedding variation.
    """

    def __init__(self, ttl_seconds: float = 300.0, max_size: int = 1000):
        self.cache: OrderedDict[str, tuple[str, float]] = OrderedDict()
        self.ttl = ttl_seconds
        self.max_size = max_size
        self.hits = 0
        self.misses = 0

    def _key(self, query_text: str, tool_name: str = "") -> str:
        return hashlib.sha256(f"{tool_name}:{query_text}".encode()).hexdigest()

    def get(self, query_text: str, tool_name: str = "") -> str | None:
        """Get cached result for a query. Returns None on miss or expiry."""
        key = self._key(query_text, tool_name)
        if key in self.cache:
            result, ts = self.cache[key]
            if time.time() - ts < self.ttl:
                self.hits += 1
                self.cache.move_to_end(key)
                return result
            del self.cache[key]
        self.misses += 1
        return None

    def set(self, query_text: str, result: str, tool_name: str = "") -> None:
        """Cache a query result."""
        key = self._key(query_text, tool_name)
        self.cache[key] = (result, time.time())
        if len(self.cache) > self.max_size:
            self.cache.popitem(last=False)

    @property
    def hit_rate(self) -> float:
        total = self.hits + self.misses
        return self.hits / total if total > 0 else 0.0

    def invalidate(self) -> None:
        """Clear all cached entries."""
        self.cache.clear()
        self.hits = 0
        self.misses = 0

    def __len__(self) -> int:
        return len(self.cache)
