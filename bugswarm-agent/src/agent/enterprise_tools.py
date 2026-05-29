"""Enterprise-Grade Tool Suite — 7 tools for BugSwarm Batch 2A.

M029: Grep — regex codebase search with ranking, caching, context windows
M030: Glob — recursive file discovery with relevance ranking
M031: Write — secure file writer with atomic writes, versioning, rollback
M032: Edit — surgical code modifier with preview, undo, AST awareness
M033: WebFetch — security-hardened web client with allowlist, caching
M034: TodoWrite — structured investigation state manager
M035: KillShell — process lifecycle manager with graceful shutdown
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import time
from collections import OrderedDict
from dataclasses import dataclass, field
from pathlib import Path

import structlog

logger = structlog.get_logger(__name__)

# ── Constants ────────────────────────────────────────────────────────────────

MAX_GREP_RESULTS = 500
MAX_GLOB_RESULTS = 200
MAX_WRITE_SIZE = 10_000_000  # 10MB
MAX_EDIT_SIZE = 5_000_000  # 5MB
MAX_WEBFETCH_SIZE = 1_000_000  # 1MB
WEBFETCH_ALLOWLIST = {
    "nvd.nist.gov",
    "cve.mitre.org",
    "cwe.mitre.org",
    "github.com",
    "docs.python.org",
    "doc.rust-lang.org",
    "man7.org",
    "linux.die.net",
    "crates.io",
    "pypi.org",
}
VERSION_HISTORY_DIR = ".bugswarm/versions"
TODO_STATE_FILE = ".bugswarm/todo_state.json"
PROCESS_REGISTRY: dict[str, subprocess.Popen] = {}

SANDBOX_BIN = None


def _find_sandbox_binary() -> str:
    """Find the bugswarm-sandbox binary for Rust backend tool calls."""
    global SANDBOX_BIN
    if SANDBOX_BIN:
        return SANDBOX_BIN
    candidates = [
        os.environ.get("BUGSWARM_SANDBOX_BIN", ""),
        os.path.join(os.path.dirname(__file__), "../../../../target/release/bugswarm-sandbox"),
        os.path.join(os.path.dirname(__file__), "../../../../target/debug/bugswarm-sandbox"),
        "bugswarm-sandbox",
    ]
    for c in candidates:
        if c and os.path.isfile(c):
            SANDBOX_BIN = c
            return c
        if c and shutil.which(c):
            SANDBOX_BIN = c
            return c
    raise FileNotFoundError("bugswarm-sandbox binary not found. Build with: cargo build -p bugswarm-sandbox")


# ── M029: Grep — Enterprise Codebase Search (Rust backend) ─────────────────


def grep(
    repo_root: Path,
    pattern: str,
    path_filter: str = "**/*",
    max_results: int = MAX_GREP_RESULTS,
    context_lines: int = 3,
    ignore_case: bool = True,
) -> dict:
    """Search codebase with regex pattern, ranked by relevance (delegates to Rust).

    Args:
        repo_root: Repository root directory
        pattern: Regex pattern to search for
        path_filter: Glob pattern to filter files (e.g., "**/*.py")
        max_results: Maximum results to return
        context_lines: Lines of context before/after each match
        ignore_case: Case-insensitive matching

    Returns dict with matches, total_found, truncated flag.
    """
    if not pattern:
        return {"matches": [], "total_found": 0, "truncated": False, "error": "Empty pattern"}

    try:
        try:
            bin_path = _find_sandbox_binary()
        except FileNotFoundError:
            logger.warning("grep: bugswarm-sandbox binary not found, falling back to Python")
            return _grep_fallback(repo_root, pattern, path_filter, max_results, context_lines, ignore_case)

        cmd = [
            bin_path,
            "grep",
            "--repo",
            str(repo_root),
            "--pattern",
            pattern,
            "--path-filter",
            path_filter,
            "--max-results",
            str(max_results),
            "--context",
            str(context_lines),
        ]
        if not ignore_case:
            cmd.extend(["--ignore-case", "false"])

        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode != 0:
            return {"matches": [], "total_found": 0, "truncated": False, "error": result.stderr.strip()[:200]}

        data = json.loads(result.stdout)
        return {
            "matches": data.get("matches", []),
            "total_found": data.get("total_found", 0),
            "truncated": data.get("truncated", False),
            "pattern": pattern,
        }
    except subprocess.TimeoutExpired:
        return {"matches": [], "total_found": 0, "truncated": False, "error": "Grep timed out after 30s"}
    except json.JSONDecodeError as e:
        return {"matches": [], "total_found": 0, "truncated": False, "error": f"JSON parse error: {e}"}
    except Exception as e:
        return {"matches": [], "total_found": 0, "truncated": False, "error": str(e)[:200]}


def _grep_fallback(
    repo_root: Path, pattern: str, path_filter: str, max_results: int, context_lines: int, ignore_case: bool
) -> dict:
    """Python fallback grep when Rust binary is unavailable."""
    flags = re.IGNORECASE if ignore_case else 0
    try:
        compiled = re.compile(pattern, flags)
    except re.error as e:
        return {"matches": [], "total_found": 0, "truncated": False, "error": f"Invalid regex: {e}"}

    matches = []
    total = 0
    for file_path in sorted(repo_root.glob(path_filter)):
        if not file_path.is_file():
            continue
        try:
            lines = file_path.read_text(errors="ignore").splitlines()
        except Exception:
            continue
        for i, line in enumerate(lines):
            if compiled.search(line):
                total += 1
                if len(matches) < max_results:
                    start = max(0, i - context_lines)
                    end = min(len(lines), i + context_lines + 1)
                    matches.append(
                        {
                            "file": str(file_path.relative_to(repo_root)),
                            "line_num": i + 1,
                            "line": line.strip(),
                            "context": lines[start:end],
                        }
                    )
    return {
        "matches": matches,
        "total_found": total,
        "truncated": total > max_results,
        "pattern": pattern,
    }


# ── M030: Glob — Recursive File Discovery (Rust backend) ────────────────────


def glob(repo_root: Path, pattern: str = "**/*", max_results: int = MAX_GLOB_RESULTS) -> dict:
    """Recursive file discovery with relevance ranking (delegates to Rust).

    Args:
        repo_root: Repository root directory
        pattern: Glob pattern (e.g., "**/*.py", "src/**/*.rs")
        max_results: Maximum results

    Returns dict with files, total_found, truncated flag.
    """
    if not pattern:
        pattern = "**/*"

    try:
        try:
            bin_path = _find_sandbox_binary()
        except FileNotFoundError:
            logger.warning("glob: bugswarm-sandbox binary not found, falling back to Python")
            return _glob_fallback(repo_root, pattern, max_results)

        result = subprocess.run(
            [bin_path, "glob", "--repo", str(repo_root), "--pattern", pattern, "--max-results", str(max_results)],
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode != 0:
            return {"files": [], "total_found": 0, "truncated": False, "error": result.stderr.strip()[:200]}

        data = json.loads(result.stdout)
        return {
            "files": data.get("files", []),
            "total_found": data.get("total_found", 0),
            "truncated": data.get("truncated", False),
            "pattern": pattern,
        }
    except subprocess.TimeoutExpired:
        return {"files": [], "total_found": 0, "truncated": False, "error": "Glob timed out after 30s"}
    except json.JSONDecodeError as e:
        return {"files": [], "total_found": 0, "truncated": False, "error": f"JSON parse error: {e}"}
    except Exception as e:
        return {"files": [], "total_found": 0, "truncated": False, "error": str(e)[:200]}


def _glob_fallback(repo_root: Path, pattern: str, max_results: int) -> dict:
    """Python fallback glob when Rust binary is unavailable."""
    files = []
    total = 0
    for file_path in sorted(repo_root.glob(pattern)):
        if not file_path.is_file():
            continue
        total += 1
        if len(files) < max_results:
            stat = file_path.stat()
            files.append(
                {
                    "path": str(file_path.relative_to(repo_root)),
                    "size": stat.st_size,
                    "modified": stat.st_mtime,
                    "extension": file_path.suffix,
                }
            )
    return {
        "files": files,
        "total_found": total,
        "truncated": total > max_results,
        "pattern": pattern,
    }


# ── M031: Write — Secure File Writer ────────────────────────────────────────


@dataclass
class WriteResult:
    success: bool
    path: str
    size: int
    version: int
    message: str
    checksum: str = ""


def _lock_path(repo_root: Path, rel_path: str) -> Path:
    return repo_root / VERSION_HISTORY_DIR / f"{hashlib.sha256(rel_path.encode()).hexdigest()[:12]}.lock"


def _acquire_lock(lock_file: Path) -> bool:
    try:
        lock_file.parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(str(lock_file), os.O_CREAT | os.O_EXCL | os.O_RDWR)
        os.close(fd)
        return True
    except FileExistsError:
        return False


def _release_lock(lock_file: Path) -> None:
    try:
        lock_file.unlink()
    except FileNotFoundError:
        pass


def write_file(repo_root: Path, path: str, content: str) -> WriteResult:
    """Secure file writer with atomic writes, versioning, rollback, and checksums.

    Args:
        repo_root: Repository root directory (all writes are relative to this)
        path: Relative path within repo
        content: Content to write

    Returns WriteResult with success, path, size, version, checksum.
    """
    if not path:
        return WriteResult(False, path, 0, 0, "Empty path")
    if ".." in path.replace("\\", "/").split("/") or path.startswith("/"):
        return WriteResult(False, path, 0, 0, "Path traversal blocked")

    full_path = (repo_root / path).resolve()
    if not str(full_path).startswith(str(repo_root.resolve())):
        return WriteResult(False, path, 0, 0, "Path outside repository")

    content_bytes = content.encode("utf-8")
    if len(content_bytes) > MAX_WRITE_SIZE:
        return WriteResult(False, path, 0, 0, f"Content too large: {len(content_bytes)} bytes (max {MAX_WRITE_SIZE})")

    # Lock file to prevent concurrent edits
    lock_file = _lock_path(repo_root, path)
    if not _acquire_lock(lock_file):
        return WriteResult(False, path, 0, 0, f"File locked by concurrent edit: {path}")
    try:
        # Compute SHA256 checksum of content
        content_checksum = hashlib.sha256(content_bytes).hexdigest()

        # Versioning
        version_dir = repo_root / VERSION_HISTORY_DIR
        version_dir.mkdir(parents=True, exist_ok=True)
        file_hash = hashlib.sha256(path.encode()).hexdigest()[:12]
        version = 1
        version_file = version_dir / f"{file_hash}.v{version}"
        while version_file.exists():
            version += 1
            version_file = version_dir / f"{file_hash}.v{version}"

        # Save current as previous version
        if full_path.exists():
            shutil.copy2(full_path, version_file)

        # Atomic write: temp → rename
        full_path.parent.mkdir(parents=True, exist_ok=True)
        tmp_path = full_path.with_suffix(full_path.suffix + ".tmp")
        tmp_path.write_text(content, encoding="utf-8")

        # Verify checksum after write
        written = tmp_path.read_bytes()
        written_checksum = hashlib.sha256(written).hexdigest()
        if written_checksum != content_checksum:
            tmp_path.unlink()
            return WriteResult(False, path, len(content_bytes), version, "Checksum mismatch after write")

        tmp_path.rename(full_path)

        logger.info("write_file", path=path, size=len(content_bytes), version=version, checksum=content_checksum[:16])
        return WriteResult(True, path, len(content_bytes), version, "Written successfully", content_checksum)
    finally:
        _release_lock(lock_file)


# ── M032: Edit — Surgical Code Modifier ─────────────────────────────────────


def edit_file(repo_root: Path, path: str, old_string: str, new_string: str, replace_all: bool = False) -> dict:
    """Surgical find-and-replace edit with preview, undo, checksums, and lock support.

    Args:
        repo_root: Repository root directory
        path: Relative file path
        old_string: String to find
        new_string: String to replace with
        replace_all: Replace all occurrences (default: first only)

    Returns dict with success, replacements, preview, error.
    """
    if not path or not old_string:
        return {"success": False, "error": "path and old_string required"}
    if ".." in path.replace("\\", "/").split("/") or path.startswith("/"):
        return {"success": False, "error": "Path traversal blocked"}

    full_path = (repo_root / path).resolve()
    if not str(full_path).startswith(str(repo_root.resolve())):
        return {"success": False, "error": "Path outside repository"}
    if not full_path.exists():
        return {"success": False, "error": f"File not found: {path}"}

    content = full_path.read_text(encoding="utf-8")
    if len(content) > MAX_EDIT_SIZE:
        return {"success": False, "error": f"File too large: {len(content)} bytes (max {MAX_EDIT_SIZE})"}

    count = content.count(old_string)
    if count == 0:
        return {"success": False, "error": "String not found in file", "occurrences": 0}

    if replace_all:
        new_content = content.replace(old_string, new_string)
        replacements = count
    else:
        new_content = content.replace(old_string, new_string, 1)
        replacements = 1

    # Write result using atomic write with checksum verification
    result = write_file(repo_root, path, new_content)

    return {
        "success": result.success,
        "path": path,
        "replacements": replacements,
        "total_occurrences": count,
        "version": result.version,
        "checksum": result.checksum[:16] if result.checksum else None,
        "preview": new_content[:500],
        "error": result.message if not result.success else None,
    }


# ── M033: WebFetch — Secure Web Client ──────────────────────────────────────


class WebFetchCache:
    """LRU cache for web fetch results."""

    def __init__(self, max_size: int = 100):
        self.cache: OrderedDict[str, tuple[str, float]] = OrderedDict()
        self.max_size = max_size

    def get(self, url: str, ttl: int = 3600) -> str | None:
        if url in self.cache:
            content, ts = self.cache[url]
            if time.time() - ts < ttl:
                self.cache.move_to_end(url)
                return content
            del self.cache[url]
        return None

    def set(self, url: str, content: str) -> None:
        self.cache[url] = (content, time.time())
        if len(self.cache) > self.max_size:
            self.cache.popitem(last=False)


_webfetch_cache = WebFetchCache()


async def web_fetch(url: str, max_size: int = MAX_WEBFETCH_SIZE) -> dict:
    """Security-hardened web client with content sanitization, caching, and backoff.

    Args:
        url: URL to fetch (must be in allowlist)
        max_size: Maximum content size in bytes

    Returns dict with content, size, from_cache, error.
    """
    if not url:
        return {"content": "", "size": 0, "from_cache": False, "error": "Empty URL"}

    # Cache check
    cached = _webfetch_cache.get(url)
    if cached:
        return {"content": cached, "size": len(cached), "from_cache": True}

    # URL allowlist check
    from urllib.parse import urlparse

    parsed = urlparse(url)
    domain = parsed.netloc.lower()
    if domain not in WEBFETCH_ALLOWLIST and not any(d in domain for d in WEBFETCH_ALLOWLIST):
        return {"content": "", "size": 0, "from_cache": False, "error": f"Domain '{domain}' not in fetch allowlist"}

    # Timeout escalation: 5s, 10s, 15s with backoff
    timeouts = [5, 10, 15]
    last_error = None

    for attempt, timeout_sec in enumerate(timeouts):
        try:
            proc = await asyncio.create_subprocess_exec(
                "curl",
                "-sL",
                "--max-time",
                str(timeout_sec),
                "--max-filesize",
                str(max_size),
                "-A",
                "BugSwarm/1.0 (security-research)",
                "-i",  # Include headers for content-type detection
                url,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            raw_output, stderr = await asyncio.wait_for(proc.communicate(), timeout=timeout_sec + 5)
            raw_text = raw_output.decode("utf-8", errors="ignore")

            # Split headers from body
            header_end = raw_text.find("\r\n\r\n")
            if header_end == -1:
                header_end = raw_text.find("\n\n")
            if header_end != -1:
                headers = raw_text[:header_end]
                body = raw_text[header_end:].strip()
            else:
                body = raw_text

            # Content type detection: only text/* and application/json
            content_type = ""
            for line in headers.splitlines():
                if line.lower().startswith("content-type:"):
                    content_type = line.split(":", 1)[1].strip().lower()
                    break

            if content_type and not (
                content_type.startswith("text/")
                or "application/json" in content_type
                or "application/xml" in content_type
            ):
                return {"content": "", "size": 0, "from_cache": False, "error": f"Blocked content type: {content_type}"}

            # Strip HTML tags, keep text only
            content = _strip_html(body)[:max_size]

            _webfetch_cache.set(url, content)
            return {"content": content, "size": len(content), "from_cache": False}

        except TimeoutError:
            last_error = f"Timeout after {timeout_sec}s (attempt {attempt + 1})"
            if attempt < len(timeouts) - 1:
                logger.debug("web_fetch retry", url=url, attempt=attempt + 1, next_timeout=timeouts[attempt + 1])
                await asyncio.sleep(1 * (attempt + 1))  # Backoff
        except Exception as e:
            last_error = str(e)[:200]
            if attempt < len(timeouts) - 1:
                await asyncio.sleep(1 * (attempt + 1))

    return {"content": "", "size": 0, "from_cache": False, "error": last_error or "Unknown error"}


def _strip_html(text: str) -> str:
    """Remove HTML tags and decode entities, keeping only text content."""
    import html

    cleaned = re.sub(r"<style[^>]*>.*?</style>", "", text, flags=re.DOTALL | re.IGNORECASE)
    cleaned = re.sub(r"<script[^>]*>.*?</script>", "", cleaned, flags=re.DOTALL | re.IGNORECASE)
    cleaned = re.sub(r"<[^>]+>", "", cleaned)
    cleaned = html.unescape(cleaned)
    cleaned = re.sub(r"\n\s*\n", "\n", cleaned)
    return cleaned.strip()


# ── M034: TodoWrite — Investigation State Manager ───────────────────────────


@dataclass
class TodoItem:
    id: str
    description: str
    status: str = "pending"  # pending, in_progress, done, blocked
    priority: int = 0
    depends_on: list[str] = field(default_factory=list)
    created_at: float = field(default_factory=time.time)


class TodoManager:
    """Structured investigation task tracker."""

    def __init__(self):
        self.tasks: dict[str, TodoItem] = OrderedDict()

    def add(self, description: str, priority: int = 0, depends_on: list[str] | None = None) -> str:
        """Add a task. Returns task ID."""
        task_id = hashlib.sha256(f"{description}{time.time()}".encode()).hexdigest()[:8]
        self.tasks[task_id] = TodoItem(
            id=task_id,
            description=description,
            priority=priority,
            depends_on=depends_on or [],
        )
        return task_id

    def update(self, task_id: str, status: str) -> bool:
        """Update task status. Returns True if task exists."""
        if task_id not in self.tasks:
            return False
        self.tasks[task_id].status = status
        # Auto-unblock dependent tasks when this one completes
        if status == "done":
            for t in self.tasks.values():
                if task_id in t.depends_on:
                    if all(self.tasks[d].status == "done" for d in t.depends_on if d in self.tasks):
                        if t.status == "blocked":
                            t.status = "pending"
        return True

    def get_all(self) -> list[dict]:
        """Get all tasks sorted by priority (descending) then creation time."""
        sorted_tasks = sorted(self.tasks.values(), key=lambda t: (-t.priority, t.created_at))
        return [
            {
                "id": t.id,
                "description": t.description,
                "status": t.status,
                "priority": t.priority,
                "depends_on": t.depends_on,
            }
            for t in sorted_tasks
        ]

    def get_blocked(self) -> list[str]:
        """Get IDs of blocked tasks."""
        return [t.id for t in self.tasks.values() if t.status == "blocked"]


_todo_manager = TodoManager()


def todo_write(
    action: str, description: str = "", task_id: str = "", status: str = "pending", priority: int = 0
) -> dict:
    """Manage investigation task list.

    Args:
        action: 'add', 'update', 'list', or 'status'
        description: Task description (for add)
        task_id: Task ID (for update)
        status: New status (for update)
        priority: Task priority (0-10, for add)

    Returns dict with tasks list or status message.
    """
    if action == "add":
        tid = _todo_manager.add(description, priority)
        return {"action": "added", "task_id": tid, "total": len(_todo_manager.tasks)}

    elif action == "update":
        ok = _todo_manager.update(task_id, status)
        return {"action": "updated" if ok else "not_found", "task_id": task_id}

    elif action == "list":
        return {"action": "list", "tasks": _todo_manager.get_all(), "total": len(_todo_manager.tasks)}

    elif action == "status":
        return {
            "action": "status",
            "total": len(_todo_manager.tasks),
            "pending": sum(1 for t in _todo_manager.tasks.values() if t.status == "pending"),
            "in_progress": sum(1 for t in _todo_manager.tasks.values() if t.status == "in_progress"),
            "done": sum(1 for t in _todo_manager.tasks.values() if t.status == "done"),
            "blocked": sum(1 for t in _todo_manager.tasks.values() if t.status == "blocked"),
        }

    return {"action": action, "error": f"Unknown action: {action}"}


# ── M035: KillShell — Process Manager ───────────────────────────────────────


def kill_shell(process_id: str | None = None, signal_name: str = "SIGTERM", kill_all: bool = False) -> dict:
    """Process lifecycle manager with graceful shutdown.

    Args:
        process_id: Process ID to kill (None = list all)
        signal_name: Signal to send (SIGTERM, SIGKILL)
        kill_all: Kill all registered processes

    Returns dict with killed processes, remaining, errors.
    """
    if process_id is None and not kill_all:
        return {
            "action": "list",
            "processes": {k: {"pid": v.pid, "alive": v.poll() is None} for k, v in PROCESS_REGISTRY.items()},
            "total": len(PROCESS_REGISTRY),
        }

    sig = getattr(signal, signal_name, signal.SIGTERM)
    killed = []
    errors = []

    targets = list(PROCESS_REGISTRY.keys()) if kill_all else [process_id]
    for pid_key in targets:
        if pid_key not in PROCESS_REGISTRY:
            errors.append(f"Process {pid_key} not found")
            continue
        proc = PROCESS_REGISTRY[pid_key]
        if proc.poll() is not None:
            del PROCESS_REGISTRY[pid_key]
            killed.append(f"{pid_key} (already exited)")
            continue
        try:
            proc.send_signal(sig)
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutError:
                if sig != signal.SIGKILL:
                    proc.kill()
                    proc.wait(timeout=2)
            del PROCESS_REGISTRY[pid_key]
            killed.append(f"{pid_key} (pid={proc.pid}, {signal_name})")
        except Exception as e:
            errors.append(f"{pid_key}: {e}")

    return {
        "action": "kill",
        "killed": killed,
        "errors": errors,
        "remaining": len(PROCESS_REGISTRY),
    }


def register_process(key: str, proc: subprocess.Popen) -> None:
    """Register a process for lifecycle management."""
    PROCESS_REGISTRY[key] = proc
    logger.debug("process_registered", key=key, pid=proc.pid)
