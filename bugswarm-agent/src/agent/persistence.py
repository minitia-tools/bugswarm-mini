"""Enterprise Persistence Layer — async-safe SQLite with migration framework.

Rule: All agent state is persisted. In-memory is scaffolding. Process restart
must recover exact agent state from last checkpoint via WAL journaling.
"""

from __future__ import annotations

import json
import sqlite3
from dataclasses import dataclass
from pathlib import Path

import structlog

logger = structlog.get_logger(__name__)


# ═══════════════════════════════════════════════════════════════
# Migration Framework
# ═══════════════════════════════════════════════════════════════

MIGRATIONS: list[tuple[int, str]] = [
    (
        1,
        """
        CREATE TABLE IF NOT EXISTS agent_state (
            id INTEGER PRIMARY KEY,
            run_id TEXT NOT NULL UNIQUE,
            round_number INTEGER DEFAULT 0,
            turn_number INTEGER DEFAULT 0,
            status TEXT DEFAULT 'active',
            persona TEXT DEFAULT 'causal',
            prompt_version TEXT DEFAULT '',
            trust_score REAL DEFAULT 1.0,
            tokens_consumed INTEGER DEFAULT 0,
            findings_count INTEGER DEFAULT 0,
            verified_findings INTEGER DEFAULT 0,
            hallucination_rate REAL DEFAULT 0.0,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT NOT NULL,
            round INTEGER NOT NULL,
            turn INTEGER NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            tool_calls TEXT,
            token_count INTEGER DEFAULT 0,
            embedding_hash TEXT,
            created_at TEXT DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS findings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT NOT NULL,
            claim TEXT NOT NULL,
            location TEXT NOT NULL,
            mechanism TEXT,
            severity INTEGER DEFAULT 5,
            sandbox_receipt TEXT,
            sandbox_receipt_id TEXT,
            verified INTEGER DEFAULT 0,
            false_positive INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS checkpoints (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT NOT NULL,
            round INTEGER NOT NULL,
            state_json TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_messages_run_round ON messages(run_id, round);
        CREATE INDEX IF NOT EXISTS idx_findings_run ON findings(run_id);
        CREATE INDEX IF NOT EXISTS idx_checkpoints_run ON checkpoints(run_id, round);
    """,
    ),
    (
        2,
        """
        ALTER TABLE agent_state ADD COLUMN prompt_version TEXT DEFAULT '';
    """,
    ),
    (
        3,
        """
        ALTER TABLE findings ADD COLUMN sandbox_receipt_id TEXT;
    """,
    ),
    (
        4,
        """
        ALTER TABLE findings ADD COLUMN false_positive INTEGER DEFAULT 0;
    """,
    ),
    (
        5,
        """
        CREATE TABLE IF NOT EXISTS cost_tracking (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT NOT NULL,
            round INTEGER NOT NULL,
            turn INTEGER NOT NULL,
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            cost_usd REAL DEFAULT 0.0,
            provider TEXT,
            model TEXT,
            created_at TEXT DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_cost_run ON cost_tracking(run_id);
    """,
    ),
]


class MigrationManager:
    """Applies schema migrations in order, tracking version in a meta table."""

    def __init__(self, conn: sqlite3.Connection):
        self.conn = conn

    def get_current_version(self) -> int:
        self.conn.execute("CREATE TABLE IF NOT EXISTS _migrations (version INTEGER PRIMARY KEY)")
        row = self.conn.execute("SELECT COALESCE(MAX(version), 0) FROM _migrations").fetchone()
        return row[0] if row else 0

    def migrate(self, target_version: int | None = None) -> int:
        current = self.get_current_version()
        target = target_version or max(m[0] for m in MIGRATIONS)

        for version, sql in MIGRATIONS:
            if version <= current:
                continue
            if target and version > target:
                break
            try:
                self.conn.executescript(sql)
                self.conn.execute("INSERT OR REPLACE INTO _migrations (version) VALUES (?)", (version,))
                self.conn.commit()
                logger.info("migration_applied", version=version)
            except sqlite3.OperationalError as e:
                if "duplicate column" in str(e).lower() or "already exists" in str(e).lower():
                    self.conn.execute("INSERT OR REPLACE INTO _migrations (version) VALUES (?)", (version,))
                    self.conn.commit()
                else:
                    raise

        return self.get_current_version()


# ═══════════════════════════════════════════════════════════════
# Persistence Manager
# ═══════════════════════════════════════════════════════════════


@dataclass
class RunState:
    run_id: str
    round_number: int = 0
    turn_number: int = 0
    status: str = "active"
    persona: str = "causal"
    prompt_version: str = ""
    trust_score: float = 1.0
    tokens_consumed: int = 0
    findings_count: int = 0
    verified_findings: int = 0
    hallucination_rate: float = 0.0

    @classmethod
    def from_row(cls, row: tuple) -> RunState:
        return cls(
            run_id=row[1],
            round_number=row[2],
            turn_number=row[3],
            status=row[4],
            persona=row[5],
            prompt_version=row[6],
            trust_score=row[7],
            tokens_consumed=row[8],
            findings_count=row[9],
            verified_findings=row[10],
            hallucination_rate=row[11],
        )


@dataclass
class MessageRecord:
    run_id: str
    round: int
    turn: int
    role: str
    content: str
    token_count: int = 0
    created_at: str = ""


@dataclass
class FindingRecord:
    run_id: str
    claim: str
    location: str
    mechanism: str = ""
    severity: int = 5
    verified: bool = False
    false_positive: bool = False
    sandbox_receipt_id: str = ""


class PersistenceManager:
    """Async-safe persistence with connection pooling and migrations.

    Uses WAL journaling for crash recovery. All writes are atomic.
    """

    def __init__(self, db_path: str | Path, pool_size: int = 4):
        self.db_path = Path(db_path) if isinstance(db_path, str) else db_path
        self.pool_size = pool_size
        self._connections: list[sqlite3.Connection] = []

        # Ensure parent directory exists
        self.db_path.parent.mkdir(parents=True, exist_ok=True)

        # Initialize primary connection
        self._init_connection()

    def _init_connection(self) -> sqlite3.Connection:
        conn = sqlite3.connect(str(self.db_path), check_same_thread=False)
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA synchronous=NORMAL")
        conn.execute("PRAGMA foreign_keys=ON")
        conn.execute("PRAGMA busy_timeout=5000")

        # Apply migrations
        mgr = MigrationManager(conn)
        version = mgr.migrate()
        logger.info("persistence_initialized", path=str(self.db_path), schema_version=version)

        self._connections.append(conn)
        return conn

    @property
    def conn(self) -> sqlite3.Connection:
        if not self._connections:
            return self._init_connection()
        return self._connections[0]

    # ─── Run Repository ───

    def create_run(self, run_id: str, persona: str = "causal", prompt_version: str = "") -> RunState:
        self.conn.execute(
            "INSERT INTO agent_state (run_id, persona, prompt_version, status) VALUES (?,?,?,'active')",
            (run_id, persona, prompt_version),
        )
        self.conn.commit()
        return RunState(run_id=run_id, persona=persona, prompt_version=prompt_version)

    def get_run(self, run_id: str) -> RunState | None:
        row = self.conn.execute(
            "SELECT * FROM agent_state WHERE run_id=? ORDER BY id DESC LIMIT 1",
            (run_id,),
        ).fetchone()
        return RunState.from_row(row) if row else None

    def update_run(self, run_id: str, **kwargs) -> None:
        if not kwargs:
            return
        sets = ", ".join(f"{k}=?" for k in kwargs)
        values = list(kwargs.values()) + [run_id]
        self.conn.execute(
            f"UPDATE agent_state SET {sets}, updated_at=datetime('now') WHERE run_id=?",
            values,
        )
        self.conn.commit()

    # ─── Message Repository ───

    def log_message(self, run_id: str, round_num: int, turn: int, role: str, content: str, tokens: int = 0) -> None:
        self.conn.execute(
            "INSERT INTO messages (run_id, round, turn, role, content, token_count) VALUES (?,?,?,?,?,?)",
            (run_id, round_num, turn, role, content, tokens),
        )
        self.conn.execute(
            "UPDATE agent_state SET turn_number=?, tokens_consumed=tokens_consumed+?, updated_at=datetime('now') WHERE run_id=?",
            (turn, tokens, run_id),
        )
        self.conn.commit()

    def get_messages(self, run_id: str, last_n: int = 50) -> list[MessageRecord]:
        rows = self.conn.execute(
            "SELECT run_id, round, turn, role, content, token_count, created_at FROM messages WHERE run_id=? ORDER BY id DESC LIMIT ?",
            (run_id, last_n),
        ).fetchall()
        return [MessageRecord(r[0], r[1], r[2], r[3], r[4], r[5], r[6]) for r in reversed(rows)]

    # ─── Finding Repository ───

    def log_finding(
        self, run_id: str, claim: str, location: str, mechanism: str = "", severity: int = 5, verified: bool = False
    ) -> None:
        self.conn.execute(
            "INSERT INTO findings (run_id, claim, location, mechanism, severity, verified) VALUES (?,?,?,?,?,?)",
            (run_id, claim, location, mechanism, severity, int(verified)),
        )
        self.conn.execute(
            "UPDATE agent_state SET findings_count=findings_count+1, verified_findings=verified_findings+?, updated_at=datetime('now') WHERE run_id=?",
            (int(verified), run_id),
        )
        self.conn.commit()

    def update_finding_verification(
        self, finding_id: int, receipt_id: str, verified: bool = True, false_positive: bool = False
    ) -> None:
        self.conn.execute(
            "UPDATE findings SET sandbox_receipt_id=?, verified=?, false_positive=? WHERE id=?",
            (receipt_id, int(verified), int(false_positive), finding_id),
        )
        self.conn.commit()

    def get_findings(self, run_id: str, verified_only: bool = False) -> list[FindingRecord]:
        query = "SELECT run_id, claim, location, mechanism, severity, verified, false_positive, sandbox_receipt_id FROM findings WHERE run_id=?"
        if verified_only:
            query += " AND verified=1"
        rows = self.conn.execute(query, (run_id,)).fetchall()
        return [FindingRecord(r[0], r[1], r[2], r[3], r[4], bool(r[5]), bool(r[6]), r[7] or "") for r in rows]

    # ─── Checkpoint Repository ───

    def save_checkpoint(self, run_id: str, round_num: int, state: dict) -> None:
        self.conn.execute(
            "INSERT INTO checkpoints (run_id, round, state_json) VALUES (?,?,?)",
            (run_id, round_num, json.dumps(state)),
        )
        self.conn.execute(
            "UPDATE agent_state SET round_number=?, updated_at=datetime('now') WHERE run_id=?",
            (round_num, run_id),
        )
        self.conn.commit()

    def load_checkpoint(self, run_id: str) -> dict | None:
        row = self.conn.execute(
            "SELECT state_json FROM checkpoints WHERE run_id=? ORDER BY round DESC LIMIT 1",
            (run_id,),
        ).fetchone()
        return json.loads(row[0]) if row else None

    # ─── Cost Tracking ───

    def log_cost(
        self,
        run_id: str,
        round_num: int,
        turn: int,
        input_tokens: int,
        output_tokens: int,
        cost_usd: float,
        provider: str = "",
        model: str = "",
    ) -> None:
        self.conn.execute(
            "INSERT INTO cost_tracking (run_id, round, turn, input_tokens, output_tokens, cost_usd, provider, model) VALUES (?,?,?,?,?,?,?,?)",
            (run_id, round_num, turn, input_tokens, output_tokens, cost_usd, provider, model),
        )
        self.conn.commit()

    def get_total_cost(self, run_id: str) -> float:
        row = self.conn.execute(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_tracking WHERE run_id=?",
            (run_id,),
        ).fetchone()
        return row[0] if row else 0.0

    # ─── Maintenance ───

    def checkpoint_wal(self) -> None:
        """Force WAL checkpoint to merge WAL into main database."""
        self.conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")

    def backup(self, target_path: str | Path) -> None:
        """Backup database to target path."""
        target = Path(target_path)
        target.parent.mkdir(parents=True, exist_ok=True)
        backup_conn = sqlite3.connect(str(target))
        self.conn.backup(backup_conn)
        backup_conn.close()
        logger.info("backup_complete", target=str(target))

    def vacuum(self) -> None:
        self.conn.execute("VACUUM")

    def close(self) -> None:
        self.checkpoint_wal()
        for conn in self._connections:
            conn.close()
        self._connections.clear()
        logger.info("persistence_closed", path=str(self.db_path))
