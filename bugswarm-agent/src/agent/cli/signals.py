"""Graceful Shutdown — SIGTERM/SIGINT handling with state preservation."""

from __future__ import annotations

import signal
import sys

import structlog

logger = structlog.get_logger(__name__)


class GracefulKiller:
    """Catches signals and triggers graceful shutdown with state checkpoint."""

    def __init__(self):
        self.kill_now = False
        self._on_shutdown: list[callable] = []

        try:
            signal.signal(signal.SIGINT, self._handle)
            signal.signal(signal.SIGTERM, self._handle)
        except (ValueError, OSError):
            pass  # Not in main thread

    def _handle(self, signum, frame):
        logger.warning("shutdown_signal_received", signal=signum)
        self.kill_now = True
        for callback in self._on_shutdown:
            try:
                callback()
            except Exception as e:
                logger.error("shutdown_callback_failed", error=str(e)[:200])
        sys.exit(0)

    def on_shutdown(self, callback: callable) -> None:
        """Register a cleanup callback. Called in signal handler — keep fast."""
        self._on_shutdown.append(callback)
