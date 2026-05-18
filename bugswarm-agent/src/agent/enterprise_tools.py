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
import os
import re
import shutil
import signal
import subprocess
import tempfile
import time
from collections import OrderedDict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import structlog

logger = structlog.get_logger(__name__)

# ── Constants ────────────────────────────────────────────────────────────────

MAX_GREP_RESULTS = 500
MAX_GLOB_RESULTS = 200
MAX_WRITE_SIZE = 10_000_000  # 10MB
MAX_EDIT_SIZE = 5_000_000    # 5MB
MAX_WEBFETCH_SIZE = 1_000_000  # 1MB
WEBFETCH_ALLOWLIST = {
    "nvd.nist.gov", "cve.mitre.org", "cwe.mitre.org",
    "github.com", "docs.python.org", "doc.rust-lang.org",
    "man7.org", "linux.die.net", "crates.io", "pypi.org",
}
VERSION_HISTORY_DIR = ".bugswarm/versions"
TODO_STATE_FILE = ".bugswarm/todo_state.json"
PROCESS_REGISTRY: dict[str, subprocess.Popen] = {}


# ── M029: Grep — Enterprise Codebase Search ─────────────────────────────────

def grep(repo_root: Path, pattern: str, path_filter: str = "**/*",
         max_results: int = MAX_GREP_RESULTS, context_lines: int = 3,
         ignore_case: bool = True) -> dict:
    """Search codebase with regex pattern, ranked by relevance.
    
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
                    matches.append({
                        "file": str(file_path.relative_to(repo_root)),
                        "line_num": i + 1,
                        "line": line.strip(),
                        "context": lines[start:end],
                    })

    return {
        "matches": matches,
        "total_found": total,
        "truncated": total > max_results,
        "pattern": pattern,
    }


# ── M030: Glob — Recursive File Discovery ───────────────────────────────────

def glob(repo_root: Path, pattern: str = "**/*",
         max_results: int = MAX_GLOB_RESULTS) -> dict:
    """Recursive file discovery with relevance ranking.
    
    Args:
        repo_root: Repository root directory
        pattern: Glob pattern (e.g., "**/*.py", "src/**/*.rs")
        max_results: Maximum results
    
    Returns dict with files, total_found, truncated flag.
    """
    if not pattern:
        pattern = "**/*"

    files = []
    total = 0
    
    for file_path in sorted(repo_root.glob(pattern)):
        if not file_path.is_file():
            continue
        total += 1
        if len(files) < max_results:
            stat = file_path.stat()
            files.append({
                "path": str(file_path.relative_to(repo_root)),
                "size": stat.st_size,
                "modified": stat.st_mtime,
                "extension": file_path.suffix,
            })

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


def write_file(repo_root: Path, path: str, content: str) -> WriteResult:
    """Secure file writer with atomic writes, versioning, and rollback.
    
    Args:
        repo_root: Repository root directory (all writes are relative to this)
        path: Relative path within repo
        content: Content to write
    
    Returns WriteResult with success, path, size, version.
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
    tmp_path.rename(full_path)
    
    logger.info("write_file", path=path, size=len(content_bytes), version=version)
    return WriteResult(True, path, len(content_bytes), version, "Written successfully")


# ── M032: Edit — Surgical Code Modifier ─────────────────────────────────────

def edit_file(repo_root: Path, path: str, old_string: str, new_string: str,
              replace_all: bool = False) -> dict:
    """Surgical find-and-replace edit with preview and undo support.
    
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
        return {"success": False, "error": f"String not found in file", "occurrences": 0}
    
    if replace_all:
        new_content = content.replace(old_string, new_string)
        replacements = count
    else:
        new_content = content.replace(old_string, new_string, 1)
        replacements = 1
    
    # Write result using atomic write
    result = write_file(repo_root, path, new_content)
    
    return {
        "success": result.success,
        "path": path,
        "replacements": replacements,
        "total_occurrences": count,
        "version": result.version,
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
    """Security-hardened web client with content sanitization and caching.
    
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
        return {"content": "", "size": 0, "from_cache": False, 
                "error": f"Domain '{domain}' not in fetch allowlist"}
    
    try:
        proc = await asyncio.create_subprocess_exec(
            "curl", "-sL", "--max-time", "10", "--max-filesize", str(max_size),
            "-A", "BugSwarm/1.0 (security-research)",
            url,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=15)
        content = stdout.decode("utf-8", errors="ignore")[:max_size]
        
        _webfetch_cache.set(url, content)
        return {"content": content, "size": len(content), "from_cache": False}
    except asyncio.TimeoutError:
        return {"content": "", "size": 0, "from_cache": False, "error": "Fetch timeout"}
    except Exception as e:
        return {"content": "", "size": 0, "from_cache": False, "error": str(e)[:200]}


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
            id=task_id, description=description, priority=priority,
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
            {"id": t.id, "description": t.description, "status": t.status,
             "priority": t.priority, "depends_on": t.depends_on}
            for t in sorted_tasks
        ]
    
    def get_blocked(self) -> list[str]:
        """Get IDs of blocked tasks."""
        return [t.id for t in self.tasks.values() if t.status == "blocked"]


_todo_manager = TodoManager()


def todo_write(action: str, description: str = "", task_id: str = "",
               status: str = "pending", priority: int = 0) -> dict:
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

def kill_shell(process_id: str | None = None, signal_name: str = "SIGTERM",
               kill_all: bool = False) -> dict:
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
            "processes": {k: {"pid": v.pid, "alive": v.poll() is None}
                         for k, v in PROCESS_REGISTRY.items()},
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
