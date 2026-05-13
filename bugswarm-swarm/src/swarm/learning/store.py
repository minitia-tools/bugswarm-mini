"""ChromaDB Store — pattern persistence and ANN similarity search.

C6.2 PEAK: Uses ChromaDB HNSW index for O(log N) approximate nearest neighbor.
Falls back to in-memory cosine similarity when ChromaDB is unavailable.
"""

from __future__ import annotations

import hashlib
import time
from typing import Any

import structlog

logger = structlog.get_logger(__name__)


class PatternStore:
    """Stores and queries bug patterns via ChromaDB with in-memory fallback."""

    def __init__(self, persist_path: str = "~/.bugswarm/chroma"):
        self.persist_path = persist_path
        self._client = None
        self._collection = None
        self._available = False
        self._fallback_patterns: list[dict] = []
        self._fallback_embeddings: list[list[float]] = []
        self._init_store()

    def _init_store(self):
        try:
            import chromadb
            import os
            path = os.path.expanduser(self.persist_path)
            os.makedirs(path, exist_ok=True)

            self._client = chromadb.PersistentClient(path=path)
            self._collection = self._client.get_or_create_collection(
                name="bug_patterns",
                metadata={"hnsw:space": "cosine"},
            )
            self._available = True
            logger.info("chromadb_initialized", path=path)
        except Exception as e:
            logger.warning("chromadb_unavailable", error=str(e)[:200], fallback="in_memory")
            self._available = False

    def store(self, pattern_id: str, embedding: list[float],
              metadata: dict[str, Any], document: str) -> None:
        if self._available and self._collection:
            try:
                self._collection.add(
                    ids=[pattern_id],
                    embeddings=[embedding],
                    metadatas=[metadata],
                    documents=[document[:1000]],
                )
                return
            except Exception as e:
                logger.warning("chromadb_store_failed", error=str(e)[:200])

        self._fallback_patterns.append({
            "id": pattern_id, "metadata": metadata, "document": document,
            "embedding": embedding, "stored_at": time.time(),
        })

    def query(self, embedding: list[float], top_k: int = 10,
              language_filter: str | None = None) -> list[dict]:
        if self._available and self._collection:
            try:
                where = {"language": language_filter} if language_filter else None
                results = self._collection.query(
                    query_embeddings=[embedding],
                    n_results=top_k,
                    where=where,
                    include=["metadatas", "documents", "distances"],
                )
                return self._format_results(results)
            except Exception as e:
                logger.warning("chromadb_query_failed", error=str(e)[:200])

        return self._fallback_query(embedding, top_k, language_filter)

    def _fallback_query(self, embedding: list[float], top_k: int,
                        language_filter: str | None) -> list[dict]:
        from .embed import CodeEmbedder
        emb = CodeEmbedder()

        scored = []
        for p in self._fallback_patterns:
            if language_filter and p["metadata"].get("language") != language_filter:
                continue
            sim = emb._cosine(embedding, p["embedding"])
            scored.append((1.0 - sim, p))

        scored.sort(key=lambda x: x[0])
        return [
            {
                "similarity": 1.0 - d,
                "cwe": p["metadata"].get("cwe", ""),
                "severity": p["metadata"].get("severity", 5),
                "language": p["metadata"].get("language", ""),
                "snippet": p["document"][:200],
            }
            for d, p in scored[:top_k]
        ]

    def _format_results(self, results: dict) -> list[dict]:
        formatted = []
        ids = results.get("ids", [[]])[0]
        distances = results.get("distances", [[]])[0]
        metadatas = results.get("metadatas", [[]])[0]
        documents = results.get("documents", [[]])[0]

        for i in range(len(ids)):
            formatted.append({
                "similarity": round(1.0 - distances[i], 4) if i < len(distances) else 0.0,
                "cwe": metadatas[i].get("cwe", "") if i < len(metadatas) else "",
                "severity": metadatas[i].get("severity", 5) if i < len(metadatas) else 5,
                "language": metadatas[i].get("language", "") if i < len(metadatas) else "",
                "snippet": documents[i][:200] if i < len(documents) else "",
            })
        return formatted

    def count(self) -> int:
        if self._available and self._collection:
            try:
                return self._collection.count()
            except Exception:
                pass
        return len(self._fallback_patterns)

    @property
    def is_available(self) -> bool:
        return self._available

    def clear(self) -> None:
        if self._available and self._collection:
            try:
                ids = self._collection.get()["ids"]
                if ids:
                    self._collection.delete(ids=ids)
            except Exception:
                pass
        self._fallback_patterns.clear()
