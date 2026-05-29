"""Dependency Injection — wires all modules into a complete IEPEngine."""

from __future__ import annotations

import base64
import structlog
from pathlib import Path
from gateway.types import GatewayConfig, ProviderType
from gateway.client import LLMClient

from agent.loop import IEPEngine, IEPConfig
from agent.tools import ToolRegistry, ToolDefinition, ToolResult
from agent.parser import OutputParser
from agent.cpg_client import CPGClient
from agent.sandbox_client import SandboxClient
from agent.evidence_client import EvidenceClient
from agent.persistence import PersistenceManager
from agent.scanner import UnifiedScanner
from agent.prompts import Persona

from .config import CLIConfig

logger = structlog.get_logger(__name__)


def wire_everything(config: CLIConfig) -> tuple[IEPEngine, PersistenceManager, LLMClient, EvidenceClient]:
    """Build the full dependency tree. Called once at startup.

    Returns (engine, persistence, gateway) for lifecycle management.
    """
    # 1. Persistence
    persistence = PersistenceManager(config.db_path)

    # 2. Scanner (use shared config if provided)
    scanner = UnifiedScanner(config.scanner_config if config.scanner_config else None)

    # 3. Gateway
    gw_config = GatewayConfig.from_env()
    try:
        gw_config.default_provider = ProviderType(config.provider)
    except ValueError:
        logger.warning("unknown_provider", provider=config.provider, fallback="deepseek")
        gw_config.default_provider = ProviderType.DEEPSEEK

    # G3: Read learning data for investigation prioritization (Phase 18.5)
    learning_context = ""
    try:
        from swarm.pattern_db import BugPatternDB, HotspotTracker
        ht = HotspotTracker()
        pdb = BugPatternDB()
        hotspots = ht.get_top(20)
        patterns = pdb.query_similar(f"Repository: {config.repo}", top_k=5)
        if hotspots:
            learning_context += f"\nHOTSPOT FILES (investigate first — these had bugs in previous runs):\n"
            for fp, score in hotspots[:5]:
                learning_context += f"  {fp} (hotspot score: {score:.1f})\n"
        if patterns:
            learning_context += f"\nSIMILAR PAST BUGS (these patterns were found in similar code before):\n"
            for pat in patterns[:3]:
                learning_context += f"  [{pat.get('cwe','?')}] {pat.get('snippet','')[:100]}\n"

        # Phase 19: ML probability prediction feed — inject top-20 into scout prompt
        if getattr(config, 'probability_enabled', True):
            try:
                from swarm.probability import BugProbabilityModel, FunctionFeatures
                model = BugProbabilityModel(
                    model_path=getattr(config, 'probability_model_path',
                                       "~/.bugswarm/probability_model.json")
                )
                if model.model is not None:
                    # Extract features from all functions in repo via CPG
                    functions = _extract_cpg_functions(cpg, config.repo)
                    prediction = model.predict_all(functions)
                    prompt_block = prediction.to_prompt(
                        top_n=getattr(config, 'probability_top_k', 20),
                        threshold=getattr(config, 'probability_confidence_threshold', 0.5),
                    )
                    if prompt_block:
                        learning_context += "\n" + prompt_block
                        logger.info("probability_feed_injected", top_functions=prediction.get_top(5))
            except Exception as e:
                logger.debug("probability_feed_unavailable", error=str(e)[:100])
    except Exception as e:
        logger.debug("learning_context_unavailable", error=str(e)[:100])
    gateway = LLMClient(gw_config)
    gateway.register_default_adapters()

    # 4. Service clients
    cpg = CPGClient(binary=config.cpg_binary)
    sandbox = SandboxClient(binary=config.sandbox_binary)
    evidence = EvidenceClient()

    # 5. Tools
    tools = ToolRegistry()
    tools.register(ToolDefinition(
        name="read_file", description="Read lines from a file",
        parameters={"type": "object", "properties": {
            "path": {"type": "string"}, "start_line": {"type": "integer"},
            "end_line": {"type": "integer"}}, "required": ["path"]},
        handler=lambda args: _read_file(config.repo, scanner, args),
        timeout_secs=10.0, cache_ttl_secs=30.0,
    ))
    tools.register(ToolDefinition(
        name="query_cpg", description="Query the Code Property Graph",
        parameters={"type": "object", "properties": {
            "name": {"type": "string"}, "kind": {"type": "string"}}, "required": []},
        handler=lambda args: _query_cpg(cpg, config.repo, args),
        timeout_secs=30.0, cache_ttl_secs=10.0,
    ))
    tools.register(ToolDefinition(
        name="exec_sandbox", description="Execute PoC in isolated sandbox",
        parameters={"type": "object", "properties": {
            "poc_code": {"type": "string"}}, "required": ["poc_code"]},
        handler=lambda args: _exec_sandbox(sandbox, scanner, args),
        timeout_secs=130.0, max_retries=1, cache_ttl_secs=0.0,
    ))
    tools.register(ToolDefinition(
        name="list_dir", description="List directory contents",
        parameters={"type": "object", "properties": {
            "path": {"type": "string"}}, "required": []},
        handler=lambda args: _list_dir(config.repo, args),
        timeout_secs=5.0, cache_ttl_secs=30.0,
    ))
    tools.register(ToolDefinition(
        name="trace_dependency", description="Trace call dependencies",
        parameters={"type": "object", "properties": {
            "function_name": {"type": "string"}, "radius": {"type": "integer"}},
            "required": ["function_name"]},
        handler=lambda args: _trace_dependency(cpg, config.repo, args),
        timeout_secs=30.0, cache_ttl_secs=15.0,
    ))
    tools.register(ToolDefinition(
        name="delta_debug",
        description="Minimize a crashing input using delta debugging (ddmin). Returns the minimal reproduction by systematically removing irrelevant bytes.",
        parameters={"type": "object", "properties": {
            "input_base64": {"type": "string", "description": "Base64-encoded crashing input to minimize"},
            "max_iterations": {"type": "integer", "description": "Maximum ddmin iterations (default: 200)"},
        }, "required": ["input_base64"]},
        handler=lambda args: _delta_debug(sandbox, args),
        timeout_secs=60.0,
        max_retries=1,
        cache_ttl_secs=300.0,
    ))
    tools.register(ToolDefinition(
        name="describe_trigger",
        description="Document a trigger condition for a confirmed bug. Records what input, environment, timing, data state, concurrency, configuration, dependency version, or OS/arch triggers the bug.",
        parameters={"type": "object", "properties": {
            "bug_id": {"type": "string", "description": "The bug identifier"},
            "dimension": {"type": "string", "description": "Trigger dimension: Input, Environment, Timing, DataState, Concurrency, Configuration, DependencyVersion, OsArch"},
            "description": {"type": "string", "description": "Human-readable description of the trigger condition"},
        }, "required": ["bug_id", "dimension", "description"]},
        handler=lambda args: _describe_trigger_evidence(evidence, args),
        timeout_secs=10.0,
        cache_ttl_secs=0.0,  # No cache — each call is unique
    ))
    tools.register(ToolDefinition(
        name="get_trigger_matrix",
        description="Retrieve the complete trigger matrix for a confirmed bug. Returns all documented trigger conditions organized by dimension.",
        parameters={"type": "object", "properties": {
            "bug_id": {"type": "string", "description": "The bug identifier"},
        }, "required": ["bug_id"]},
        handler=lambda args: _get_trigger_matrix_evidence(evidence, args),
        timeout_secs=10.0,
        cache_ttl_secs=30.0,
    ))

    # Phase 24: Differential Analysis tool
    tools.register(ToolDefinition(
        name="diff_execute",
        description="Compare two outputs using differential analysis. Detects semantic regressions by normalizing volatile fields and computing diff magnitude.",
        parameters={"type": "object", "properties": {
            "input": {"type": "string", "description": "First (baseline) output to compare"},
            "reference": {"type": "string", "description": "Second (changed) output to compare"},
            "normalizer": {"type": "string", "description": "Output normalizer: Json, Xml, Dict, Text, Binary (default: Text)"},
        }, "required": ["input", "reference"]},
        handler=lambda args: _diff_execute(sandbox, args),
        timeout_secs=10.0,
        cache_ttl_secs=0.0,
    ))

    # Phase 25: Invariant Mining tool
    tools.register(ToolDefinition(
        name="mine_invariants",
        description="Mine implicit invariants from a function by running it with thousands of property-based inputs. Discovers range, type, ordering, relationship, exception, and state invariants. Flags violations as potential silent bugs.",
        parameters={"type": "object", "properties": {
            "function_name": {"type": "string", "description": "Target function name to mine invariants from"},
            "param_types": {"type": "array", "items": {"type": "string"}, "description": "Parameter types: int, float, bool, string, array, object"},
            "count": {"type": "integer", "description": "Number of inputs to generate (default: 100)"},
        }, "required": ["function_name"]},
        handler=lambda args: _mine_invariants(sandbox, args),
        timeout_secs=30.0,
        cache_ttl_secs=0.0,
    ))

    # Phase 26: Mutation Testing tool
    tools.register(ToolDefinition(
        name="run_mutations",
        description="Inject artificial bugs (mutants) into code and run tests against them. Surviving mutants are untested code paths — bugs-in-waiting. Returns mutation score, survivor list, and prescribed tests.",
        parameters={"type": "object", "properties": {
            "source_code": {"type": "string", "description": "Source code to mutate"},
            "file_path": {"type": "string", "description": "File path for identification"},
            "operators": {"type": "array", "items": {"type": "string"}, "description": "Mutation operators: Arithmetic, Comparison, Logical, Constant, NullCheck, ControlFlow"},
        }, "required": ["source_code"]},
        handler=lambda args: _run_mutations(sandbox, args),
        timeout_secs=30.0,
        cache_ttl_secs=300.0,
    ))

    # Phase 27: Symbolic Execution tool
    tools.register(ToolDefinition(
        name="solve_reachability",
        description="Use symbolic execution to find the exact input that reaches a target code location. Given path conditions (variable constraints from a code path), produces concrete input values that satisfy all constraints.",
        parameters={"type": "object", "properties": {
            "target_location": {"type": "string", "description": "Target code location (file:line) to reach"},
            "path_conditions": {"type": "array", "items": {"type": "object", "properties": {
                "line": {"type": "integer"},
                "condition": {"type": "string"},
            }}, "description": "List of (line_number, condition) pairs along the path to the target"},
        }, "required": ["target_location", "path_conditions"]},
        handler=lambda args: _solve_reachability(sandbox, args),
        timeout_secs=30.0,
        cache_ttl_secs=60.0,
    ))

    # Phase 28: Concolic Execution tool
    tools.register(ToolDefinition(
        name="explore_paths",
        description="Systematically explore all code paths around a target using concolic execution. Runs concrete inputs, collects constraints, negates them one at a time to discover new paths. Achieves 95%+ branch coverage for functions under 200 LOC.",
        parameters={"type": "object", "properties": {
            "target_location": {"type": "string", "description": "Target code location"},
            "path_conditions": {"type": "array", "items": {"type": "object", "properties": {
                "line": {"type": "integer"}, "condition": {"type": "string"},
            }}},
            "max_queries": {"type": "integer", "description": "Max negation queries (default: 100)"},
        }, "required": ["target_location", "path_conditions"]},
        handler=lambda args: _explore_paths(sandbox, args),
        timeout_secs=60.0,
        cache_ttl_secs=0.0,
    ))

    # Phase 29: Vulnerability Chaining tool
    tools.register(ToolDefinition(
        name="suggest_chain",
        description="Analyze confirmed bugs and discover multi-step exploit chains. Connects individual bugs via effect→precondition semantic matching (BFS traversal), computes chain severity using weighted calculus, identifies RCE-capable chains, and auto-generates combined PoCs when all members have individual sandbox PoCs.",
        parameters={"type": "object", "properties": {
            "bug_ids": {"type": "array", "items": {"type": "string"}, "description": "List of confirmed bug IDs to analyze for chains"},
            "max_hops": {"type": "integer", "description": "Maximum chain length (default: 10)"},
        }, "required": ["bug_ids"]},
        handler=lambda args: _suggest_chain(evidence, args),
        timeout_secs=30.0,
        cache_ttl_secs=60.0,
    ))

    # Phase 30: Fix-Induced Bug Prediction tool (CAPSTONE)
    tools.register(ToolDefinition(
        name="predict_fix_impact",
        description="Predict whether a proposed bug fix will introduce new bugs. Analyzes all callers via CPG call graph, computes value range overlap, generates regression tests for flagged callers, and produces a Bayesian confidence score. The final safety net before merging any fix.",
        parameters={"type": "object", "properties": {
            "bug_id": {"type": "string", "description": "The bug being fixed"},
            "function_name": {"type": "string", "description": "The function being changed"},
            "file_path": {"type": "string", "description": "File path of the changed function"},
            "original_line": {"type": "string", "description": "The original code line"},
            "replacement_line": {"type": "string", "description": "The proposed replacement line"},
            "line_number": {"type": "integer", "description": "Line number of the change"},
            "language": {"type": "string", "description": "Programming language (python, c, rust, etc.)"},
            "description": {"type": "string", "description": "Description of the fix"},
        }, "required": ["bug_id", "function_name", "original_line", "replacement_line"]},
        handler=lambda args: _predict_fix_impact(evidence, args),
        timeout_secs=30.0,
        cache_ttl_secs=0.0,
    ))

    # ── Batch 2A: Enterprise Tool Suite (M029-M035) ─────────────────────
    
    tools.register(ToolDefinition(
        name="grep",
        description="Search the entire codebase with regex pattern matching. Returns ranked results with context lines. Use for finding function calls, imports, dangerous API usage, vulnerability patterns across files.",
        parameters={"type": "object", "properties": {
            "pattern": {"type": "string", "description": "Regex pattern to search for"},
            "path_filter": {"type": "string", "description": "Glob filter for files (e.g., '**/*.py', 'src/**/*.rs')"},
            "max_results": {"type": "integer", "description": "Max results (default: 500)"},
        }, "required": ["pattern"]},
        handler=lambda args: _grep_handler(config.repo, args),
        timeout_secs=10.0,
        cache_ttl_secs=30.0,
    ))
    
    tools.register(ToolDefinition(
        name="glob",
        description="Recursive file discovery with pattern matching. Returns file listings with metadata (size, modification time, extension). Use to discover project structure before reading files.",
        parameters={"type": "object", "properties": {
            "pattern": {"type": "string", "description": "Glob pattern (e.g., '**/*.py', 'src/**/*.rs')"},
            "max_results": {"type": "integer", "description": "Max results (default: 200)"},
        }, "required": []},
        handler=lambda args: _glob_handler(config.repo, args),
        timeout_secs=5.0,
        cache_ttl_secs=30.0,
    ))
    
    tools.register(ToolDefinition(
        name="write_file",
        description="Write a file to the repository with atomic writes, automatic versioning, and path traversal protection. Previous versions are preserved for rollback.",
        parameters={"type": "object", "properties": {
            "path": {"type": "string", "description": "Relative file path within repository"},
            "content": {"type": "string", "description": "Content to write"},
        }, "required": ["path", "content"]},
        handler=lambda args: _write_file_handler(config.repo, args),
        timeout_secs=10.0,
        cache_ttl_secs=0.0,
    ))
    
    tools.register(ToolDefinition(
        name="edit_file",
        description="Surgical find-and-replace code modification. Supports single or all-occurrence replacement with preview and automatic versioning. Use for causal interventions and fix proposals.",
        parameters={"type": "object", "properties": {
            "path": {"type": "string", "description": "Relative file path within repository"},
            "old_string": {"type": "string", "description": "String to find"},
            "new_string": {"type": "string", "description": "String to replace with"},
            "replace_all": {"type": "boolean", "description": "Replace all occurrences (default: first only)"},
        }, "required": ["path", "old_string", "new_string"]},
        handler=lambda args: _edit_file_handler(config.repo, args),
        timeout_secs=10.0,
        cache_ttl_secs=0.0,
    ))
    
    tools.register(ToolDefinition(
        name="web_fetch",
        description="Security-hardened web client for fetching CVE data, documentation, and known exploit patterns. URL allowlist enforced. Results cached for 1 hour.",
        parameters={"type": "object", "properties": {
            "url": {"type": "string", "description": "URL to fetch (must be in allowlist)"},
        }, "required": ["url"]},
        handler=lambda args: _web_fetch_handler(args),
        timeout_secs=15.0,
        cache_ttl_secs=3600.0,
    ))
    
    tools.register(ToolDefinition(
        name="todo_write",
        description="Structured investigation task tracker. Create, update, and list tasks with priorities and dependencies. Auto-unblocks dependent tasks when prerequisites complete.",
        parameters={"type": "object", "properties": {
            "action": {"type": "string", "description": "Action: add, update, list, or status"},
            "description": {"type": "string", "description": "Task description (for add)"},
            "task_id": {"type": "string", "description": "Task ID (for update)"},
            "status": {"type": "string", "description": "New status: pending, in_progress, done, blocked"},
            "priority": {"type": "integer", "description": "Task priority 0-10"},
        }, "required": ["action"]},
        handler=lambda args: _todo_write_handler(args),
        timeout_secs=5.0,
        cache_ttl_secs=0.0,
    ))
    
    tools.register(ToolDefinition(
        name="kill_shell",
        description="Process lifecycle manager. List, gracefully terminate (SIGTERM with 5s timeout, then SIGKILL), or kill all registered background processes.",
        parameters={"type": "object", "properties": {
            "process_id": {"type": "string", "description": "Process ID to kill (omit to list all)"},
            "signal": {"type": "string", "description": "Signal: SIGTERM (default) or SIGKILL"},
            "kill_all": {"type": "boolean", "description": "Kill all registered processes"},
        }, "required": []},
        handler=lambda args: _kill_shell_handler(args),
        timeout_secs=10.0,
        cache_ttl_secs=0.0,
    ))

    # 6. Parser
    parser = OutputParser()

    # 7. Engine
    iep_config = IEPConfig(
        repo_path=config.repo,
        persona=config.persona,
        model=config.model,
        provider=ProviderType(config.provider) if config.provider in {"openai","anthropic","deepseek","google","ollama"} else ProviderType.DEEPSEEK,
        max_rounds=config.rounds,
        max_turns_per_round=config.turns,
        db_path=config.db_path,
    )

    engine = IEPEngine(iep_config, tools, parser, gateway, persistence)
    return engine, persistence, gateway, evidence


def _extract_cpg_functions(cpg, repo_path):
    """Extract function features from repo for probability prediction.

    Note: Uses heuristic file-walking (regex for def/class). Full CPG node-listing
    API is deferred (Phase 19.5). When CPG query API supports per-function extraction,
    this function should be replaced with actual CPG queries.

    Returns list of FunctionFeatures objects with best-effort extraction.
    """
    from swarm.probability import FunctionFeatures
    features = []
    try:
        rp = Path(repo_path) if hasattr(repo_path, 'iterdir') else Path(repo_path)
        for py_file in list(rp.rglob("*.py"))[:200]:  # Cap increased for larger repos
            try:
                content = py_file.read_text(errors='ignore')
                lines = content.splitlines()
                for i, line in enumerate(lines):
                    stripped = line.strip()
                    if stripped.startswith("def "):
                        func_name = stripped[4:].split("(")[0].strip()
                        if not func_name.startswith('_'):  # Skip private functions
                            features.append(FunctionFeatures(
                                function_name=func_name,
                                file_path=str(py_file.relative_to(rp)),
                                lines_of_code=len(lines),
                            ))
                    elif stripped.startswith("class "):
                        class_name = stripped[6:].split("(")[0].split(":")[0].strip()
                        features.append(FunctionFeatures(
                            function_name=class_name,
                            file_path=str(py_file.relative_to(rp)),
                            lines_of_code=len(lines),
                        ))
            except Exception:
                continue
    except Exception:
        pass
    return features


# ─── Tool Handlers (async wrapped for sync ToolRegistry) ───

import asyncio

def _run_async(coro):
    try:
        loop = asyncio.get_running_loop()
    except RuntimeError:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
    return loop.run_until_complete(coro)


async def _read_file_async(repo, scanner, args):
    try:
        path = args.get("path", "")
        start = int(args.get("start_line", 1))
        end = int(args.get("end_line", start + 50))

        repo_resolved = repo.resolve()

        # H12: Path traversal prevention (multi-layer defense)
        if not path:
            return ToolResult(False, "Path traversal blocked: empty path")
        if "\0" in path:
            return ToolResult(False, "Path traversal blocked: null byte in path")
        stripped_path = path.lstrip()
        if len(stripped_path) >= 2 and stripped_path[1] == ":":
            return ToolResult(False, "Path traversal blocked: Windows drive letter paths not allowed")
        if path.startswith("/"):
            return ToolResult(False, "Path traversal blocked: absolute paths are not allowed")
        normalized = path.replace("\\", "/")
        segments = normalized.split("/")
        if ".." in segments:
            return ToolResult(False, "Path traversal blocked: parent directory navigation not allowed")

        full = (repo / path).resolve()

        if not str(full).startswith(str(repo_resolved) + "/"):
            return ToolResult(False, f"Path traversal blocked: {path} resolves outside repository")

        final_path = full.resolve()
        if final_path != full:
            if not str(final_path).startswith(str(repo_resolved) + "/"):
                return ToolResult(False, f"Path traversal blocked: {path} resolves outside repository via symlink")

        if not full.exists():
            return ToolResult(False, f"File not found: {path}")

        lines = full.read_text().splitlines()
        result_lines = [f"{i+1}: {lines[i]}" for i in range(max(0, start-1), min(len(lines), end))]
        content = "\n".join(result_lines)
        redacted, count = scanner.redact(content)
        return ToolResult(True, redacted, {"lines": f"{start}-{end}", "total": len(lines)})
    except Exception as e:
        return ToolResult(False, str(e))

def _read_file(repo, scanner, args):
    return _run_async(_read_file_async(repo, scanner, args))


async def _query_cpg_async(cpg, repo, args):
    try:
        stats = await cpg.stats(repo)
        import json
        data = {"files": stats.total_files, "functions": stats.total_functions,
                "sources": stats.sources, "sinks": stats.sinks,
                "taint_paths": stats.taint_paths}
        return ToolResult(True, json.dumps(data, indent=2), {"source": "cpg"})
    except Exception as e:
        return ToolResult(False, str(e))

def _query_cpg(cpg, repo, args):
    return _run_async(_query_cpg_async(cpg, repo, args))


async def _exec_sandbox_async(sandbox, scanner, args):
    poc = args.get("poc_code", "")
    if not poc:
        return ToolResult(False, "No PoC code provided")
    if scanner.has_escape_attempt(poc):
        return ToolResult(False, "PoC contains sandbox escape patterns — REJECTED")
    # G4: Detect language and pass to sandbox for sanitizer image selection
    lang = "python"
    poc_lower = poc[:200].lower()
    if any(kw in poc_lower for kw in ["#include", "malloc", "free(", "int main", "printf"]):
        lang = "c"
    elif any(kw in poc_lower for kw in ["cout", "std::", "template<"]):
        lang = "cpp"
    receipt = await sandbox.execute(poc, env={"BGSWARM_LANGUAGE": lang})
    import json
    return ToolResult(True, json.dumps(receipt.to_summary()),
                      {"exit_code": receipt.exit_code, "status": receipt.status, "language": lang})

def _exec_sandbox(sandbox, scanner, args):
    return _run_async(_exec_sandbox_async(sandbox, scanner, args))


async def _list_dir_async(repo, args):
    try:
        path = args.get("path", ".")
        if not path:
            return ToolResult(False, "Path traversal blocked: empty path")
        if "\0" in path:
            return ToolResult(False, "Path traversal blocked: null byte in path")
        stripped_path = path.lstrip()
        if len(stripped_path) >= 2 and stripped_path[1] == ":":
            return ToolResult(False, "Path traversal blocked: Windows drive letter paths not allowed")
        if path.startswith("/"):
            return ToolResult(False, "Path traversal blocked: absolute paths are not allowed")
        normalized = path.replace("\\", "/")
        segments = normalized.split("/")
        if ".." in segments:
            return ToolResult(False, "Path traversal blocked: parent directory navigation not allowed")
        target = repo / path if path != "." else repo
        entries = [f"  [{'DIR' if e.is_dir() else 'FILE'}] {e.name}"
                   for e in sorted(target.iterdir())]
        return ToolResult(True, f"Contents of {target}:\n" + "\n".join(entries))
    except Exception as e:
        return ToolResult(False, str(e))

def _list_dir(repo, args):
    return _run_async(_list_dir_async(repo, args))


async def _trace_dependency_async(cpg, repo, args):
    try:
        paths = await cpg.taint_paths(repo)
        lines = [f"Path: {p.source} → {p.sink} (len={p.length})" for p in paths[:10]]
        return ToolResult(True, "\n".join(lines) or "No taint paths found")
    except Exception as e:
        return ToolResult(False, str(e))

def _trace_dependency(cpg, repo, args):
    return _run_async(_trace_dependency_async(cpg, repo, args))


async def _delta_debug_async(sandbox, args):
    try:
        input_b64 = args.get("input_base64", "")
        input_bytes = base64.b64decode(input_b64)
        max_iter = int(args.get("max_iterations", 200))
        result = await sandbox.delta_debug(input_bytes, max_iter)
        import json
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"delta_debug failed: {e}")


def _delta_debug(sandbox, args):
    return _run_async(_delta_debug_async(sandbox, args))


async def _describe_trigger_async(evidence, args):
    try:
        import json
        bug_id = args.get("bug_id", "unknown")
        dim = args.get("dimension", "Input")
        desc = args.get("description", "")
        result = await evidence.add_trigger_condition(bug_id, dim, desc, "agent")
        return ToolResult(True, json.dumps(result, indent=2), {"bug_id": bug_id})
    except Exception as e:
        return ToolResult(False, f"describe_trigger failed: {e}")

def _describe_trigger_evidence(evidence, args):
    return _run_async(_describe_trigger_async(evidence, args))


async def _get_trigger_matrix_async(evidence, args):
    try:
        import json
        bug_id = args.get("bug_id", "unknown")
        result = await evidence.get_trigger_matrix(bug_id)
        return ToolResult(True, json.dumps(result, indent=2), {"bug_id": bug_id})
    except Exception as e:
        return ToolResult(False, f"get_trigger_matrix failed: {e}")

def _get_trigger_matrix_evidence(evidence, args):
    return _run_async(_get_trigger_matrix_async(evidence, args))


async def _diff_execute_async(sandbox, args):
    try:
        input_str = args.get("input", "")
        reference = args.get("reference", "")
        normalizer = args.get("normalizer", "Text")
        result = await sandbox.diff_execute(input_str, reference, normalizer)
        import json
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"diff_execute failed: {e}")

def _diff_execute(sandbox, args):
    return _run_async(_diff_execute_async(sandbox, args))


async def _mine_invariants_async(sandbox, args):
    try:
        function_name = args.get("function_name", "unknown")
        param_types = args.get("param_types", [])
        count = int(args.get("count", 100))
        import json
        # Call sandbox daemon's mine_invariants handler
        result = await sandbox.mine_invariants(function_name, param_types, count)
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"mine_invariants failed: {e}")

def _mine_invariants(sandbox, args):
    return _run_async(_mine_invariants_async(sandbox, args))


async def _run_mutations_async(sandbox, args):
    try:
        source_code = args.get("source_code", "")
        file_path = args.get("file_path", "unknown")
        operators = args.get("operators", [])
        import json
        result = await sandbox.run_mutations(source_code, file_path, operators)
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"run_mutations failed: {e}")

def _run_mutations(sandbox, args):
    return _run_async(_run_mutations_async(sandbox, args))


async def _solve_reachability_async(sandbox, args):
    try:
        target = args.get("target_location", "")
        path_conditions = args.get("path_conditions", [])
        import json
        result = await sandbox.solve_reachability(target, path_conditions)
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"solve_reachability failed: {e}")

def _solve_reachability(sandbox, args):
    return _run_async(_solve_reachability_async(sandbox, args))


async def _explore_paths_async(sandbox, args):
    try:
        target = args.get("target_location", "")
        path_conditions = args.get("path_conditions", [])
        max_queries = int(args.get("max_queries", 100))
        import json
        result = await sandbox.explore_paths(target, path_conditions, max_queries)
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"explore_paths failed: {e}")

def _explore_paths(sandbox, args):
    return _run_async(_explore_paths_async(sandbox, args))


async def _suggest_chain_async(evidence, args):
    try:
        import json
        bug_ids = args.get("bug_ids", [])
        max_hops = int(args.get("max_hops", 10))
        result = await evidence.suggest_chain(bug_ids, max_hops)
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"suggest_chain failed: {e}")

def _suggest_chain(evidence, args):
    return _run_async(_suggest_chain_async(evidence, args))


async def _predict_fix_impact_async(evidence, args):
    try:
        import json
        bug_id = args.get("bug_id", "unknown")
        function_name = args.get("function_name", "")
        file_path = args.get("file_path", "")
        original = args.get("original_line", "")
        replacement = args.get("replacement_line", "")
        line_number = int(args.get("line_number", 0))
        language = args.get("language", "python")
        description = args.get("description", "")
        
        result = await evidence.predict_fix_impact(
            bug_id, function_name, file_path, original, replacement,
            line_number, language, description
        )
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"predict_fix_impact failed: {e}")

def _predict_fix_impact(evidence, args):
    return _run_async(_predict_fix_impact_async(evidence, args))


# ── Batch 2A Enterprise Tool Handlers ────────────────────────────────────

def _grep_handler(repo, args):
    from agent.enterprise_tools import grep
    try:
        import json
        result = grep(repo, args.get("pattern", ""),
                      args.get("path_filter", "**/*"),
                      int(args.get("max_results", 500)))
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"grep failed: {e}")


def _glob_handler(repo, args):
    from agent.enterprise_tools import glob
    try:
        import json
        result = glob(repo, args.get("pattern", "**/*"),
                      int(args.get("max_results", 200)))
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"glob failed: {e}")


def _write_file_handler(repo, args):
    from agent.enterprise_tools import write_file
    try:
        import json
        result = write_file(repo, args.get("path", ""), args.get("content", ""))
        return ToolResult(result.success, json.dumps({"path": result.path, "size": result.size, "version": result.version, "message": result.message}))
    except Exception as e:
        return ToolResult(False, f"write_file failed: {e}")


def _edit_file_handler(repo, args):
    from agent.enterprise_tools import edit_file
    try:
        import json
        result = edit_file(repo, args.get("path", ""),
                           args.get("old_string", ""), args.get("new_string", ""),
                           args.get("replace_all", False))
        return ToolResult(result.get("success", False), json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"edit_file failed: {e}")


def _web_fetch_handler(args):
    from agent.enterprise_tools import web_fetch
    try:
        import json
        result = _run_async(web_fetch(args.get("url", "")))
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"web_fetch failed: {e}")


def _todo_write_handler(args):
    from agent.enterprise_tools import todo_write
    try:
        import json
        result = todo_write(args.get("action", "list"),
                            args.get("description", ""),
                            args.get("task_id", ""),
                            args.get("status", "pending"),
                            int(args.get("priority", 0)))
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"todo_write failed: {e}")


def _kill_shell_handler(args):
    from agent.enterprise_tools import kill_shell
    try:
        import json
        result = kill_shell(args.get("process_id"),
                            args.get("signal", "SIGTERM"),
                            args.get("kill_all", False))
        return ToolResult(True, json.dumps(result, indent=2))
    except Exception as e:
        return ToolResult(False, f"kill_shell failed: {e}")
