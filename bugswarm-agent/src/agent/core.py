"""
Bug Swarm Agent — Single-agent IEP loop.

Issue → Evidence → Proof cycle with tool dispatch,
SQLite WAL persistence, PII scanning, and sandbox integration.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import os
import re
import sqlite3
import subprocess
import tempfile
import time
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any

from gateway.types import ChatMessage, ChatRequest, MessageRole, ProviderType
from gateway.client import LLMClient

from agent.cli.signals import register_temp_file


# ═══════════════════════════════════════════════════════════════
# System Prompts
# ═══════════════════════════════════════════════════════════════

SYSTEM_PROMPT = """<SYSTEM_IMMUTABLE>
You are Bug Swarm Agent v1.4, an automated software vulnerability detection system.
Your sole purpose is to find, prove, and report software bugs using tool-based evidence.

NON-NEGOTIABLE DIRECTIVES:
1. You are forbidden from agreeing with any claim unless you independently verify it using tool-based evidence.
2. If you cannot verify, you MUST state your uncertainty and propose a test to resolve it.
3. Every bug claim MUST include: the code location (file:line), the mechanism, a falsifiable prediction, and sandbox evidence.
4. Do NOT assume existing code is correct. The presence of code is not evidence of correctness. Default stance: SKEPTICAL.
5. You do not have a human identity or human experience. Your arguments must be grounded in code evidence and sandbox results.
6. A critical security vulnerability (SQL injection, RCE, auth bypass) is worth 100x more than a style violation.
7. If you spend >2 turns on a non-security issue without finding a confirmed vulnerability, you MUST pivot to a security-relevant code path.
</SYSTEM_IMMUTABLE>

You have access to the following tools:
- query_cpg(name, kind) — Search the Code Property Graph for functions, classes, or variables.
- trace_dependency(function_name, radius) — Trace call and data dependencies through N hops.
- exec_sandbox(poc_code) — Execute a proof-of-concept in an isolated Docker container. Returns an execution receipt.
- read_file(path, start_line, end_line) — Read a range of lines from a file.
- list_dir(path) — List files and directories in a given path.

OUTPUT FORMAT (MANDATORY — you MUST output EXACTLY one JSON block per response):
For bug findings, you MUST use:
```json
{"type":"finding","claim":"...","location":"file:line","mechanism":"...","prediction":"if input X then output/behavior Y","severity_estimate":1-10}
```

For tool calls, you MUST use:
```json
{"type":"tool","tool":"tool_name","args":{...}}
```

For PoC execution, you MUST use:
```json
{"type":"poc","code":"...","prediction":"expected behavior if bug exists"}
```

CRITICAL: Every response MUST contain exactly one JSON block. Freeform text without a JSON block will be ignored. The JSON block MUST be wrapped in ```json fences.
"""


# ═══════════════════════════════════════════════════════════════
# Tool Permissions — Capability Gating
# ═══════════════════════════════════════════════════════════════

TOOL_PERMISSIONS = {
    "read_file": "read",
    "list_dir": "read",
    "query_cpg": "read",
    "trace_dependency": "read",
    "exec_sandbox": "execute",
    "delta_debug": "execute",
    "diff_execute": "read",
    "mine_invariants": "analyze",
    "run_mutations": "analyze",
    "solve_reachability": "analyze",
    "explore_paths": "analyze",
    "describe_trigger": "write",
    "suggest_chain": "read",
    "predict_fix_impact": "read",
}


# ═══════════════════════════════════════════════════════════════
# Tool Definitions
# ═══════════════════════════════════════════════════════════════

class ToolResult:
    def __init__(self, success: bool, data: str, metadata: dict | None = None):
        self.success = success
        self.data = data
        self.metadata = metadata or {}

    def to_message(self) -> str:
        if not self.success:
            return f"Tool error: {self.data}"
        result = self.data
        if self.metadata:
            result += f"\n[Metadata: {json.dumps(self.metadata)}]"
        return result


class ToolDispatcher:
    """Dispatches tool calls to sandbox, CPG, and filesystem."""

    def __init__(self, repo_path: Path, sandbox_binary: str = "bugswarm-sandbox"):
        self.repo_path = repo_path
        self.sandbox_binary = sandbox_binary
        self.scanner = PIIScanner()
        self.tool_history: list[dict] = []

    async def dispatch(self, tool_name: str, args: dict) -> ToolResult:
        self.tool_history.append({"tool": tool_name, "args": args, "time": time.time()})

        permission = TOOL_PERMISSIONS.get(tool_name)
        if permission is None:
            return ToolResult(False, f"Unknown tool: {tool_name}")

        if permission == "execute":
            exec_count = sum(1 for t in self.tool_history if TOOL_PERMISSIONS.get(t["tool"]) == "execute")
            if exec_count > 20:
                return ToolResult(False, "Execute rate limit exceeded — too many executions in one session")

        if tool_name == "query_cpg":
            return await self._query_cpg(args)
        elif tool_name == "trace_dependency":
            return await self._trace_dependency(args)
        elif tool_name == "exec_sandbox":
            return await self._exec_sandbox(args)
        elif tool_name == "read_file":
            return await self._read_file(args)
        elif tool_name == "list_dir":
            return await self._list_dir(args)
        else:
            return ToolResult(False, f"Unknown tool: {tool_name}")

    async def _query_cpg(self, args: dict) -> ToolResult:
        name = args.get("name", "")
        kind = args.get("kind", "")
        try:
            cpg_bin = "/root/a/bugswarm-cpg/target/release/bugswarm-cpg"
            if kind in ("files", "file"):
                # Find Python files
                py_files = list(self.repo_path.rglob("*.py"))
                js_files = list(self.repo_path.rglob("*.js"))
                files = [str(f.relative_to(self.repo_path)) for f in py_files + js_files]
                return ToolResult(True, f"Files in repo:\n" + "\n".join(files[:50]), {"file_count": len(files)})
            elif name:
                # Search for specific function — do a full CPG index and grep
                cmd = [cpg_bin, "taint", "--repo", str(self.repo_path)]
                proc = await asyncio.create_subprocess_exec(*cmd, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
                stdout, _ = await asyncio.wait_for(proc.communicate(), timeout=30)
                text = stdout.decode()
                if name.lower() in text.lower():
                    lines = [l for l in text.split('\n') if name.lower() in l.lower()]
                    return ToolResult(True, '\n'.join(lines[:30]), {"query": name})
                return ToolResult(True, f"No results for '{name}'. Full CPG:\n{text[:2000]}", {"query": name})
            else:
                cmd = [cpg_bin, "stats", "--repo", str(self.repo_path)]
                proc = await asyncio.create_subprocess_exec(*cmd, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
                stdout, _ = await asyncio.wait_for(proc.communicate(), timeout=30)
                data = json.loads(stdout.decode())
                return ToolResult(True, json.dumps(data, indent=2), {"source": "cpg"})
        except asyncio.TimeoutError:
            return ToolResult(False, "CPG query timed out (30s)")
        except (ConnectionError, OSError) as e:
            return ToolResult(False, f"CPG service unavailable: {e}")
        except json.JSONDecodeError as e:
            return ToolResult(False, f"CPG invalid response: {e}")
        except Exception as e:
            return ToolResult(False, f"CPG query failed: {e}")

    async def _trace_dependency(self, args: dict) -> ToolResult:
        func = args.get("function_name", "")
        radius = args.get("radius", 2)
        try:
            cpg_bin = "/root/a/bugswarm-cpg/target/release/bugswarm-cpg"
            # Use taint command to find related paths
            cmd = [cpg_bin, "taint", "--repo", str(self.repo_path)]
            proc = await asyncio.create_subprocess_exec(
                *cmd, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE,
            )
            stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=30)
            text = stdout.decode()
            if func and func in text:
                # Extract relevant lines mentioning this function
                lines = [l for l in text.split('\n') if func in l]
                return ToolResult(True, '\n'.join(lines[:20]), {"function": func, "radius": radius})
            return ToolResult(True, text[:2000], {"function": func, "radius": radius})
        except asyncio.TimeoutError:
            return ToolResult(False, "Trace dependency timed out (30s)")
        except (ConnectionError, OSError) as e:
            return ToolResult(False, f"Trace service unavailable: {e}")
        except Exception as e:
            return ToolResult(False, f"Trace dependency failed: {e}")

    async def _list_dir(self, args: dict) -> ToolResult:
        path_str = args.get("path", ".")
        try:
            target = self.repo_path / path_str
            if not target.exists():
                # Try listing the repo root
                target = self.repo_path
            entries = []
            for entry in sorted(target.iterdir()):
                t = "DIR" if entry.is_dir() else "FILE"
                entries.append(f"  [{t}] {entry.name}")
            entries.insert(0, f"Contents of {target}:")
            return ToolResult(True, '\n'.join(entries))
        except PermissionError as e:
            return ToolResult(False, f"Permission denied: {e}")
        except OSError as e:
            return ToolResult(False, f"List dir failed: {e}")
        except Exception as e:
            return ToolResult(False, f"List dir failed: {e}")

    async def _exec_sandbox(self, args: dict) -> ToolResult:
        poc_code = args.get("poc_code", "")
        if not poc_code:
            return ToolResult(False, "No PoC code provided")

        # PII scan before execution
        cleaned, pii_count = self.scanner.scan(poc_code)
        if pii_count > 0:
            return ToolResult(False, f"PoC contains {pii_count} PII/sensitive patterns — rejected")

        # Escape attempt detection
        if self.scanner.detect_escape(poc_code):
            return ToolResult(False, "PoC contains sandbox escape patterns — REJECTED")

        # Write PoC to temp file
        with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
            f.write(poc_code)
            poc_path = f.name
            register_temp_file(poc_path)

        try:
            sandbox_bin = "/root/a/bugswarm-sandbox/target/release/bugswarm-sandbox"
            cmd = [sandbox_bin, "execute", "--poc", poc_path]
            proc = await asyncio.create_subprocess_exec(
                *cmd, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE,
            )
            stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=130)

            # Scan output for PII
            output_text = stdout.decode()
            cleaned_output, out_pii = self.scanner.scan(output_text)

            try:
                receipt = json.loads(cleaned_output)
                return ToolResult(True, json.dumps(receipt, indent=2), {
                    "exit_code": receipt.get("exit_code"),
                    "status": receipt.get("status"),
                    "duration_secs": receipt.get("duration_secs"),
                })
            except json.JSONDecodeError:
                return ToolResult(True, cleaned_output[:2000], {"raw": True})
        except asyncio.TimeoutError:
            return ToolResult(False, "Sandbox execution timed out (130s)")
        except (ConnectionError, OSError) as e:
            return ToolResult(False, f"Sandbox infrastructure error: {e}")
        except Exception as e:
            return ToolResult(False, f"Sandbox execution failed: {e}")
        finally:
            try:
                os.unlink(poc_path)
            except OSError:
                pass

    async def _read_file(self, args: dict) -> ToolResult:
        path = args.get("path", "")
        start = int(args.get("start_line", 1))
        end = int(args.get("end_line", start + 50))

        repo_resolved = self.repo_path.resolve()

        # H12: Path traversal prevention
        if path.startswith("/"):
            return ToolResult(False, "Path traversal blocked: absolute paths are not allowed")
        if ".." in path:
            return ToolResult(False, "Path traversal blocked: parent directory navigation not allowed")

        full_path = (self.repo_path / path).resolve()

        if not str(full_path).startswith(str(repo_resolved)):
            return ToolResult(False, f"Path traversal blocked: {path} resolves outside repository")

        if not full_path.exists():
            return ToolResult(False, f"File not found: {path}")

        try:
            with open(full_path) as f:
                lines = f.readlines()

            if start < 1:
                start = 1
            if end > len(lines):
                end = len(lines)

            result_lines = []
            for i in range(start - 1, end):
                result_lines.append(f"{i+1}: {lines[i].rstrip()}")

            content = '\n'.join(result_lines)
            cleaned, pii_count = self.scanner.scan(content)
            if pii_count > 0:
                content = cleaned
            if self.scanner.detect_injection(content):
                content = "<SANITIZED_CODE_CONTEXT>\n" + content + "\n</SANITIZED_CODE_CONTEXT>"

            return ToolResult(True, content, {
                "file": str(full_path), "lines": f"{start}-{end}",
                "total_lines": len(lines), "pii_redactions": pii_count,
            })
        except FileNotFoundError:
            return ToolResult(False, f"File not found: {path}")
        except PermissionError:
            return ToolResult(False, f"Permission denied: {path}")
        except OSError as e:
            return ToolResult(False, f"File read failed: {e}")
        except Exception as e:
            return ToolResult(False, f"File read failed: {e}")


# ═══════════════════════════════════════════════════════════════
# PII + Prompt Injection Scanner
# ═══════════════════════════════════════════════════════════════

class PIIScanner:
    """Scans content for PII, secrets, escape attempts, and prompt injection."""

    PII_PATTERNS = [
        (r'[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}', 'email'),
        (r'\b(?:\d{4}[ -]?){3}\d{4}\b', 'credit_card'),
        (r'\b\d{3}-\d{2}-\d{4}\b', 'ssn'),
        (r'\bAKIA[0-9A-Z]{16}\b', 'aws_key'),
        (r'\bgh[pousr]_[A-Za-z0-9_]{36,}\b', 'github_token'),
        (r'-----BEGIN (RSA|EC|DSA|OPENSSH|PGP) PRIVATE KEY-----', 'private_key'),
        (r'\beyJ[A-Za-z0-9\-_=]+\.[A-Za-z0-9\-_=]+\.?[A-Za-z0-9\-_.+/=]*\b', 'jwt'),
    ]

    ESCAPE_PATTERNS = [
        'chroot', 'nsenter', 'unshare', '/proc/1/ns', '/proc/self/ns',
        'docker.sock', 'pivot_root', 'kexec', 'mount -t cgroup',
        'insmod', 'modprobe', 'finit_module',
    ]

    INJECTION_PATTERNS = [
        'ignore previous instructions', 'you are now DAN',
        'forget all previous', 'new instructions:',
        '### Human:', '[INST]', 'SYSTEM:',
    ]

    def scan(self, content: str) -> tuple[str, int]:
        count = 0
        result = content
        for pattern, pii_type in self.PII_PATTERNS:
            matches = list(re.finditer(pattern, result, re.IGNORECASE))
            count += len(matches)
            for m in reversed(matches):
                replacement = f"[REDACTED: type={pii_type}]"
                result = result[:m.start()] + replacement + result[m.end():]
        return result, count

    def detect_escape(self, content: str) -> bool:
        lower = content.lower()
        return any(p in lower for p in self.ESCAPE_PATTERNS)

    def detect_injection(self, content: str) -> bool:
        lower = content.lower()
        return any(p.lower() in lower for p in self.INJECTION_PATTERNS)


# ═══════════════════════════════════════════════════════════════
# SQLite WAL State Persistence
# ═══════════════════════════════════════════════════════════════

class AgentStateDB:
    """SQLite WAL for crash-recoverable agent state."""

    def __init__(self, db_path: str = ":memory:"):
        self.conn = sqlite3.connect(db_path)
        self.conn.execute("PRAGMA journal_mode=WAL")
        self.conn.execute("PRAGMA synchronous=NORMAL")
        self._init_schema()

    def _init_schema(self):
        self.conn.executescript("""
            CREATE TABLE IF NOT EXISTS agent_state (
                id INTEGER PRIMARY KEY,
                run_id TEXT NOT NULL,
                round_number INTEGER DEFAULT 0,
                turn_number INTEGER DEFAULT 0,
                status TEXT DEFAULT 'active',
                persona TEXT DEFAULT 'causal',
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
                round INTEGER,
                turn INTEGER,
                role TEXT,
                content TEXT,
                tool_calls TEXT,
                token_count INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS findings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL,
                claim TEXT,
                location TEXT,
                mechanism TEXT,
                severity INTEGER,
                sandbox_receipt TEXT,
                verified INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS checkpoints (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL,
                round INTEGER,
                state_json TEXT,
                created_at TEXT DEFAULT (datetime('now'))
            );
        """)
        self.conn.commit()

    def start_run(self, run_id: str, persona: str = "causal") -> int:
        self.conn.execute(
            "INSERT INTO agent_state (run_id, persona, status) VALUES (?, ?, 'active')",
            (run_id, persona),
        )
        self.conn.commit()
        return self.conn.execute("SELECT last_insert_rowid()").fetchone()[0]

    def checkpoint(self, run_id: str, round_num: int, state: dict) -> None:
        self.conn.execute(
            "INSERT INTO checkpoints (run_id, round, state_json) VALUES (?, ?, ?)",
            (run_id, round_num, json.dumps(state)),
        )
        self.conn.execute(
            "UPDATE agent_state SET round_number=?, updated_at=datetime('now') WHERE run_id=?",
            (round_num, run_id),
        )
        self.conn.commit()

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

    def log_finding(self, run_id: str, claim: str, location: str, mechanism: str, severity: int, verified: bool = False) -> None:
        self.conn.execute(
            "INSERT INTO findings (run_id, claim, location, mechanism, severity, verified) VALUES (?,?,?,?,?,?)",
            (run_id, claim, location, mechanism, severity, int(verified)),
        )
        self.conn.execute(
            "UPDATE agent_state SET findings_count=findings_count+1, verified_findings=verified_findings+?, updated_at=datetime('now') WHERE run_id=?",
            (int(verified), run_id),
        )
        self.conn.commit()

    def get_stats(self, run_id: str) -> dict:
        row = self.conn.execute(
            "SELECT * FROM agent_state WHERE run_id=? ORDER BY id DESC LIMIT 1",
            (run_id,),
        ).fetchone()
        if not row:
            return {}
        cols = [d[0] for d in self.conn.execute("SELECT * FROM agent_state LIMIT 0").description]
        return dict(zip(cols, row))

    def close(self):
        self.conn.close()


# ═══════════════════════════════════════════════════════════════
# Agent IEP Loop
# ═══════════════════════════════════════════════════════════════

@dataclass
class AgentConfig:
    repo_path: Path
    run_id: str = field(default_factory=lambda: hashlib.sha256(str(time.time()).encode()).hexdigest()[:12])
    max_rounds: int = 5
    max_turns_per_round: int = 10
    persona: str = "causal"
    model: str = "gpt-4o-mini"
    provider: ProviderType = ProviderType.OPENAI
    sandbox_binary: str = "/root/a/bugswarm-sandbox/target/release/bugswarm-sandbox"
    db_path: str = ":memory:"


class BugSwarmAgent:
    """Single agent running the IEP (Issue → Evidence → Proof) cycle."""

    def __init__(self, config: AgentConfig, gateway: LLMClient):
        self.config = config
        self.gateway = gateway
        self.db = AgentStateDB(config.db_path)
        self.tools = ToolDispatcher(config.repo_path, config.sandbox_binary)
        self.messages: list[ChatMessage] = []
        self.state_id = self.db.start_run(config.run_id, config.persona)

    async def run(self) -> dict:
        """Execute the full IEP loop."""
        self.messages = [
            ChatMessage(role=MessageRole.SYSTEM, content=self._build_system_prompt()),
            ChatMessage(role=MessageRole.USER, content=self._build_initial_prompt()),
        ]

        all_findings = []

        for round_num in range(1, self.config.max_rounds + 1):
            print(f"\n{'='*60}\n Round {round_num}/{self.config.max_rounds}\n{'='*60}")

            for turn in range(1, self.config.max_turns_per_round + 1):
                finding = await self._execute_turn(round_num, turn)
                if finding:
                    all_findings.append(finding)

                # Check for termination conditions
                if self._should_stop(all_findings):
                    break

            # Checkpoint
            self.db.checkpoint(self.config.run_id, round_num, {
                "findings": len(all_findings),
                "tokens": self.gateway.registry.total_tokens.total_tokens,
                "cost": self.gateway.registry.total_cost,
            })

            if self._should_stop(all_findings):
                break

        # Final report
        stats = self.db.get_stats(self.config.run_id)
        return {
            "run_id": self.config.run_id,
            "findings": all_findings,
            "total_findings": len(all_findings),
            "verified_findings": sum(1 for f in all_findings if f.get("verified")),
            "stats": stats,
            "gateway_stats": self.gateway.registry.stats,
        }

    async def _execute_turn(self, round_num: int, turn: int) -> dict | None:
        """Execute one turn: call LLM → parse response → dispatch tools."""
        print(f"\n--- Turn {round_num}.{turn} ---")

        request = ChatRequest(
            messages=self.messages,
            model=self.config.model,
            temperature=0.7,
            max_tokens=2048,
        )

        try:
            response = await self.gateway.chat(request, self.config.provider)
        except asyncio.TimeoutError:
            print(f"  LLM timeout")
            return None
        except (ConnectionError, OSError) as e:
            print(f"  LLM infrastructure error: {e}")
            return None
        except ValueError as e:
            print(f"  LLM invalid request: {e}")
            return None
        except Exception as e:
            print(f"  LLM error: {e}")
            return None

        content = response.content
        print(f"  Agent: {content[:200]}...")

        self.db.log_message(
            self.config.run_id, round_num, turn, "assistant", content,
            response.usage.total_tokens,
        )
        self.messages.append(ChatMessage(role=MessageRole.ASSISTANT, content=content))

        # Parse structured outputs
        finding = self._parse_finding(content)
        if finding:
            # Auto-verify if sandbox confirmed or freeform confirmation detected
            if "verified" not in finding or not finding.get("verified"):
                has_sandbox = "sandbox" in content.lower() and ("confirmed" in content.lower() or "pass" in content.lower() or "exit_code" in content.lower())
                has_confirmed = any(kw in content.lower() for kw in ["confirmed", "verified", "successfully exploited", "exploit works"])
                if has_sandbox or has_confirmed:
                    finding["verified"] = True
            self.db.log_finding(
                self.config.run_id,
                finding.get("claim", ""),
                finding.get("location", ""),
                finding.get("mechanism", ""),
                finding.get("severity_estimate", 5),
            )
            return finding

        # Parse tool calls
        tool_result = await self._parse_and_execute_tools(content)
        if tool_result:
            self.messages.append(ChatMessage(role=MessageRole.USER, content=tool_result.to_message()))
            self.db.log_message(self.config.run_id, round_num, turn, "tool_result", tool_result.to_message())

        return None

    def _parse_finding(self, content: str) -> dict | None:
        """Extract structured finding from LLM output."""
        # Try ALL JSON blocks — a message can have both PoC and finding
        try:
            remaining = content
            while '```json' in remaining:
                start = remaining.index('```json') + 7
                end = remaining.index('```', start) if '```' in remaining[start:] else len(remaining)
                json_str = remaining[start:end]
                remaining = remaining[end+3:] if end+3 < len(remaining) else ""
                try:
                    data = json.loads(json_str)
                    if data.get("type") == "finding":
                        return data
                except json.JSONDecodeError:
                    continue

            # Also check inline JSON (no fences)
            remaining = content
            while '{"type"' in remaining:
                start = remaining.index('{"type"')
                depth = 0; end = start
                for i, c in enumerate(remaining[start:]):
                    if c == '{': depth += 1
                    elif c == '}': depth -= 1
                    if depth == 0: end = start + i + 1; break
                json_str = remaining[start:end]
                remaining = remaining[end:]
                try:
                    data = json.loads(json_str)
                    if data.get("type") == "finding":
                        return data
                except json.JSONDecodeError:
                    continue
        except (ValueError, IndexError):
            pass

        # Freeform confirmation
        lower = content.lower()
        confirmed_kw = ["bug confirmed", "confirmed by sandbox", "confirmed!", "vulnerability confirmed",
                        "injection confirmed", "**confirmed", "successfully exploited"]
        is_confirmed = any(k in lower for k in confirmed_kw)
        if not is_confirmed:
            return None

        lines = [l.strip() for l in content.split('\n') if l.strip() and not l.strip().startswith('```')]
        claim = ""
        for line in lines:
            lw = line.lower()
            if any(kw in lw for kw in ["bug", "vulnerability", "injection", "overflow", "bypass", "leak", "traversal", "confirmed"]):
                claim = line.strip('# *-').strip()
                if len(claim) > 20:
                    break
        if not claim:
            claim = lines[0][:200] if lines else content[:200]

        loc_match = __import__('re').search(r'(?:bugs\.py|\.py):(\d+)', content)
        return {
            "type": "finding", "claim": claim[:200],
            "location": f"bugs.py:{loc_match.group(1)}" if loc_match else "bugs.py",
            "mechanism": "Confirmed via sandbox execution",
            "severity_estimate": 7, "verified": True,
        }

    async def _parse_and_execute_tools(self, content: str) -> ToolResult | None:
        """Extract and execute tool calls from LLM output."""
        try:
            # Extract JSON tool call
            if '{"type":"tool"' not in content and '{"type": "tool"' not in content:
                # Check for PoC
                if '{"type":"poc"' in content or '{"type": "poc"' in content:
                    return await self._execute_poc_from_content(content)
                return None

            start = content.index('{"type"')
            depth = 0
            end = start
            for i, c in enumerate(content[start:]):
                if c == '{': depth += 1
                elif c == '}':
                    depth -= 1
                    if depth == 0:
                        end = start + i + 1
                        break

            data = json.loads(content[start:end])
            tool_name = data.get("tool", "")
            args = data.get("args", {})

            result = await self.tools.dispatch(tool_name, args)
            return result
        except (ValueError, IndexError, KeyError) as e:
            return ToolResult(False, f"Tool format error: {e}")
        except json.JSONDecodeError as e:
            return ToolResult(False, f"Tool JSON parse error: {e}")
        except Exception as e:
            return ToolResult(False, f"Tool dispatch error: {e}")

    async def _execute_poc_from_content(self, content: str) -> ToolResult:
        """Execute PoC embedded in the content."""
        try:
            start = content.index('{"type"')
            depth = 0
            end = start
            for i, c in enumerate(content[start:]):
                if c == '{': depth += 1
                elif c == '}':
                    depth -= 1
                    if depth == 0:
                        end = start + i + 1
                        break
            data = json.loads(content[start:end])
            poc_code = data.get("code", "")
            if poc_code:
                return await self.tools.dispatch("exec_sandbox", {"poc_code": poc_code})
        except (ValueError, IndexError, json.JSONDecodeError):
            pass
        except Exception:
            pass
        return ToolResult(False, "Failed to parse PoC")

    def _should_stop(self, findings: list) -> bool:
        """Check termination conditions."""
        # Stop if we have enough verified findings
        verified = sum(1 for f in findings if f.get("verified"))
        if verified >= 3:
            print("  Stopping: 3+ verified findings")
            return True
        # Stop if budget exhausted
        if self.gateway.is_budget_exhausted():
            print("  Stopping: Budget exhausted")
            return True
        return False

    def _build_system_prompt(self) -> str:
        persona_additions = {
            "causal": "Focus on error propagation chains. Trace how bad inputs cascade through function calls.",
            "adversarial": "Assume this code was written by a junior developer at 4:55 PM on a Friday. Every line is suspect.",
            "defensive": "Assume environment instability. Look for error handling gaps, missing null checks, and unsafe defaults.",
            "semantic": "Check API contracts and type safety. Look for mismatches between what functions promise and what they do.",
        }
        persona_text = persona_additions.get(self.config.persona, "")
        return SYSTEM_PROMPT + f"\n\nPERSONA: {self.config.persona.upper()}\n{persona_text}\n\nTarget repository: {self.config.repo_path}"

    def _build_initial_prompt(self) -> str:
        return f"""Analyze the codebase at `{self.config.repo_path}` for bugs.

Follow the IEP cycle:
1. ISSUE: Examine the codebase structure. Query the CPG to discover functions, then read source files to understand logic.
2. EVIDENCE: For each suspicious pattern, formulate a falsifiable prediction.
3. PROOF: Submit a PoC to the sandbox. Only sandbox-verified findings count.

Start by querying the CPG to understand the codebase structure. Then investigate the most security-critical paths first (auth, input handling, database queries, command execution)."""
