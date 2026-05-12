"""Structured Progress Output — JSON lines for Minitia consumption."""

from __future__ import annotations

import json
import sys
from typing import Any


def emit_json(data: dict[str, Any]) -> None:
    """Write one JSON line to stdout. Flushed immediately."""
    sys.stdout.write(json.dumps(data) + "\n")
    sys.stdout.flush()


def emit_finding(finding: dict) -> None:
    emit_json({"type": "finding", **finding})


def emit_progress(round_num: int, turn: int, tokens: int, cost: float,
                  findings: int = 0, verified: int = 0) -> None:
    emit_json({
        "type": "progress", "round": round_num, "turn": turn,
        "tokens": tokens, "cost": round(cost, 6),
        "findings": findings, "verified": verified,
    })


def emit_complete(report: dict) -> None:
    emit_json({"type": "complete", **report})


def emit_error(message: str, detail: str = "") -> None:
    emit_json({"type": "error", "message": message, "detail": detail})


def emit_warning(message: str) -> None:
    emit_json({"type": "warning", "message": message})
