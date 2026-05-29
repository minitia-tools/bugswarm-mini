"""Persistent Learning Layer — Bug Pattern DB, Hotspot Tracker, CVE Corpus, Agent History.

C6.2 PEAK: Pattern storage via ChromaDB HNSW. Hotspot decay via exponential forgetting.
Agent history tracked per persona/model/repo-type for allocation optimization.
"""

from __future__ import annotations

import hashlib
import os
import time
from dataclasses import dataclass, field
from pathlib import Path

import structlog
import yaml

from .learning.embed import CodeEmbedder
from .learning.store import PatternStore

logger = structlog.get_logger(__name__)


@dataclass
class BugPattern:
    id: str
    cwe: str = ""
    language: str = "python"
    severity: int = 5
    code_snippet: str = ""
    function_signature: str = ""
    taint_metadata: dict = field(default_factory=dict)
    fix_diff: str | None = None
    embedding: list[float] | None = None
    discovered_at: float = field(default_factory=time.time)
    repo_hash: str = ""
    run_id: str = ""


@dataclass
class HotspotEntry:
    file_path: str
    score: float = 0.0
    bug_count: int = 0
    last_bug_at: float = 0.0
    last_scan_at: float = field(default_factory=time.time)
    recency_decay: float = 0.95

    def decay(self, now: float) -> None:
        days = (now - self.last_scan_at) / 86400.0
        if days > 0:
            self.score *= self.recency_decay**days
        self.last_scan_at = now

    def add_bug(self, severity: int, now: float) -> None:
        self.decay(now)
        self.score += severity
        self.bug_count += 1
        self.last_bug_at = now


@dataclass
class AgentPerformance:
    agent_id: str
    persona: str = ""
    model: str = ""
    runs: int = 0
    bugs_found: int = 0
    false_positives: int = 0
    tokens_consumed: int = 0
    avg_tokens_per_verified: float = 0.0
    last_run_at: float = 0.0

    @property
    def fp_rate(self) -> float:
        total = self.bugs_found + self.false_positives
        return self.false_positives / max(total, 1)

    def update(self, bugs: int, fps: int, tokens: int, verified: int) -> None:
        self.runs += 1
        self.bugs_found += bugs
        self.false_positives += fps
        self.tokens_consumed += tokens
        if verified > 0:
            total_verified = self.bugs_found - self.false_positives
            self.avg_tokens_per_verified = self.tokens_consumed / max(total_verified, 1)
        self.last_run_at = time.time()


class BugPatternDB:
    """Stores and queries bug patterns across runs."""

    def __init__(self, persist_path: str = "~/.bugswarm/chroma"):
        self.store = PatternStore(persist_path)
        self.embedder = CodeEmbedder()
        self._training_counter_path = Path(os.path.expanduser("~/.bugswarm/chroma/training_counter"))
        self._confirmed_cache: list[dict] = []
        self._train_counter_cache: int | None = None

    def _load_counter(self) -> int:
        if self._train_counter_cache is not None:
            return self._train_counter_cache
        try:
            self._training_counter_path.parent.mkdir(parents=True, exist_ok=True)
            if self._training_counter_path.exists():
                self._train_counter_cache = int(self._training_counter_path.read_text().strip())
            else:
                self._train_counter_cache = 0
        except Exception:
            self._train_counter_cache = 0
        return self._train_counter_cache

    def _save_counter(self, value: int) -> None:
        self._train_counter_cache = value
        try:
            self._training_counter_path.parent.mkdir(parents=True, exist_ok=True)
            tmp = self._training_counter_path.with_suffix(".tmp")
            tmp.write_text(str(value))
            tmp.rename(self._training_counter_path)
        except Exception as e:
            logger.warning("counter_save_failed", error=str(e)[:100])

    @property
    def count_since_last_train(self) -> int:
        return self._load_counter()

    def reset_training_counter(self) -> int:
        """Reset counter and return the count that was reset."""
        prev = self._load_counter()
        self._save_counter(0)
        return prev

    def get_all_confirmed(self) -> list[dict]:
        """Return all confirmed bug patterns stored.

        Uses cached list + ChromaDB query as fallback.
        """
        if self._confirmed_cache:
            return self._confirmed_cache
        results = self.store.query([0.0] * 384, top_k=1000)
        return [
            {
                "cwe": r.get("cwe", ""),
                "severity": r.get("severity", 5),
                "language": r.get("language", ""),
                "snippet": r.get("snippet", ""),
            }
            for r in results
        ]

    def store_finding(self, finding: dict, code_snippet: str = "", language: str = "python") -> str:
        pid = hashlib.sha256(code_snippet.encode()).hexdigest()[:16]

        # Check duplicate
        existing = self.store.query(self.embedder.encode(code_snippet), top_k=1)
        if existing and existing[0].get("similarity", 0) > 0.98:
            return pid  # Already stored

        embedding = self.embedder.encode(
            f"{finding.get('claim', '')[:200]} | {finding.get('mechanism', '')[:200]} | {code_snippet[:500]}"
        )

        metadata = {
            "cwe": self._guess_cwe(finding.get("claim", "")),
            "language": language,
            "severity": finding.get("severity_estimate", 5),
            "location": finding.get("location", ""),
            "discovered_at": time.time(),
        }

        self.store.store(pid, embedding, metadata, code_snippet[:1000])
        self._confirmed_cache.append(
            {
                "id": pid,
                "cwe": metadata["cwe"],
                "location": metadata.get("location", ""),
                "severity_estimate": metadata.get("severity", 5),
                "language": metadata.get("language", "python"),
            }
        )
        current = self._load_counter()
        self._save_counter(current + 1)
        logger.info("pattern_stored", id=pid, cwe=metadata["cwe"], severity=metadata["severity"])
        return pid

    def query_similar(self, code: str, top_k: int = 10, language: str | None = None) -> list[dict]:
        embedding = self.embedder.encode(code[:2000])
        return self.store.query(embedding, top_k, language)

    @staticmethod
    def _guess_cwe(claim: str) -> str:
        lower = claim.lower()
        if "sql" in lower or "injection" in lower:
            return "CWE-89"
        if "xss" in lower or "cross-site" in lower:
            return "CWE-79"
        if "overflow" in lower or "buffer" in lower:
            return "CWE-120"
        if "race" in lower or "concurrent" in lower:
            return "CWE-362"
        if "deserial" in lower or "pickle" in lower:
            return "CWE-502"
        if "null" in lower or "dereference" in lower:
            return "CWE-476"
        if "auth" in lower or "bypass" in lower:
            return "CWE-287"
        if "path traversal" in lower or "directory traversal" in lower:
            return "CWE-22"
        return "CWE-unknown"

    @property
    def count(self) -> int:
        return self.store.count()


class HotspotTracker:
    """Tracks file-level bug density with exponential decay."""

    def __init__(self, data_path: str = "~/.bugswarm/hotspots.yaml"):
        self.path = Path(os.path.expanduser(data_path))
        self.entries: dict[str, HotspotEntry] = {}
        self._load()

    def _load(self):
        if self.path.exists():
            try:
                data = yaml.safe_load(self.path.read_text())
                if data:
                    for file_path, entry_data in data.items():
                        self.entries[file_path] = HotspotEntry(
                            file_path=file_path,
                            score=entry_data.get("score", 0.0),
                            bug_count=entry_data.get("bug_count", 0),
                            last_bug_at=entry_data.get("last_bug_at", 0.0),
                            last_scan_at=entry_data.get("last_scan_at", time.time()),
                        )
            except Exception as e:
                logger.warning("hotspot_load_failed", error=str(e)[:200], action="reset")
                self.entries = {}

    def _save(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        data = {
            fp: {
                "score": e.score,
                "bug_count": e.bug_count,
                "last_bug_at": e.last_bug_at,
                "last_scan_at": e.last_scan_at,
            }
            for fp, e in self.entries.items()
        }
        # Atomic write
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(yaml.dump(data))
        tmp.rename(self.path)

    def record_bug(self, file_path: str, severity: int) -> None:
        now = time.time()
        entry = self.entries.get(file_path, HotspotEntry(file_path=file_path))
        entry.add_bug(severity, now)
        self.entries[file_path] = entry
        self._save()
        logger.info("hotspot_updated", file=file_path, score=round(entry.score, 1))

    def get_top(self, n: int = 20) -> list[tuple[str, float]]:
        now = time.time()
        for entry in self.entries.values():
            entry.decay(now)
        ranked = sorted(self.entries.items(), key=lambda x: x[1].score, reverse=True)
        return [(fp, e.score) for fp, e in ranked[:n]]

    def get_score(self, file_path: str) -> float:
        entry = self.entries.get(file_path)
        if entry:
            entry.decay(time.time())
            return entry.score
        return 0.0


class CveCorpus:
    """CVE pattern corpus for known vulnerability matching."""

    def __init__(self):
        self.embedder = CodeEmbedder()
        self.patterns: list[dict] = []

    def ingest(self, cve_id: str, description: str, code_pattern: str, language: str = "python") -> None:
        embedding = self.embedder.encode(f"{description} | {code_pattern[:500]}")
        self.patterns.append(
            {
                "cve_id": cve_id,
                "description": description,
                "code_pattern": code_pattern,
                "language": language,
                "embedding": embedding,
                "ingested_at": time.time(),
            }
        )
        logger.info("cve_ingested", cve=cve_id)

    def match(self, code: str, language: str | None = None, threshold: float = 0.3) -> list[dict]:
        embedding = self.embedder.encode(code[:2000])
        matches = []
        for p in self.patterns:
            if language and p["language"] != language:
                continue
            sim = self.embedder._cosine(embedding, p["embedding"])
            if sim > (1.0 - threshold):
                matches.append(
                    {
                        "cve_id": p["cve_id"],
                        "similarity": round(sim, 4),
                        "description": p["description"][:200],
                    }
                )
        matches.sort(key=lambda m: m["similarity"], reverse=True)
        return matches[:5]

    @property
    def count(self) -> int:
        return len(self.patterns)


class AgentHistory:
    """Tracks agent performance across runs for allocation optimization."""

    def __init__(self, data_path: str = "~/.bugswarm/agent_history.yaml"):
        self.path = Path(os.path.expanduser(data_path))
        self.records: dict[str, AgentPerformance] = {}
        self._load()

    def _load(self):
        if self.path.exists():
            try:
                data = yaml.safe_load(self.path.read_text())
                if data:
                    for aid, rec in data.items():
                        self.records[aid] = AgentPerformance(
                            agent_id=aid,
                            persona=rec.get("persona", ""),
                            model=rec.get("model", ""),
                            runs=rec.get("runs", 0),
                            bugs_found=rec.get("bugs_found", 0),
                            false_positives=rec.get("false_positives", 0),
                            tokens_consumed=rec.get("tokens_consumed", 0),
                            avg_tokens_per_verified=rec.get("avg_tokens_per_verified", 0.0),
                        )
            except Exception as e:
                logger.warning("agent_history_load_failed", error=str(e)[:200])
                self.records = {}

    def _save(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        data = {
            aid: {
                "persona": r.persona,
                "model": r.model,
                "runs": r.runs,
                "bugs_found": r.bugs_found,
                "false_positives": r.false_positives,
                "tokens_consumed": r.tokens_consumed,
                "avg_tokens_per_verified": round(r.avg_tokens_per_verified, 2),
            }
            for aid, r in self.records.items()
        }
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(yaml.dump(data))
        tmp.rename(self.path)

    def record_run(
        self, agent_id: str, persona: str, model: str, bugs_found: int, false_positives: int, tokens: int, verified: int
    ) -> None:
        rec = self.records.get(agent_id, AgentPerformance(agent_id=agent_id))
        rec.persona = persona
        rec.model = model
        rec.update(bugs_found, false_positives, tokens, verified)
        self.records[agent_id] = rec
        self._save()

    def get_best_agents(self, n: int = 5) -> list[AgentPerformance]:
        ranked = sorted(
            self.records.values(),
            key=lambda r: r.avg_tokens_per_verified if r.avg_tokens_per_verified > 0 else 999999,
        )
        return ranked[:n]

    def get_persona_effectiveness(self) -> dict[str, float]:
        """Returns avg tokens/bug per persona."""
        by_persona: dict[str, list[float]] = {}
        for r in self.records.values():
            if r.persona and r.avg_tokens_per_verified > 0:
                by_persona.setdefault(r.persona, []).append(r.avg_tokens_per_verified)
        return {p: sum(vals) / len(vals) for p, vals in by_persona.items()}
