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
            "output_a": {"type": "string", "description": "First output to compare"},
            "output_b": {"type": "string", "description": "Second output to compare"},
            "normalizer": {"type": "string", "description": "Output normalizer: Json, Xml, Dict, Text, Binary (default: Text)"},
        }, "required": ["output_a", "output_b"]},
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
    """Extract function features from CPG for probability prediction.

    Best-effort: queries CPG for function nodes, falls back to empty list.
    Full implementation requires CPG node listing API (Phase 19.5 deferred).
    """
    from swarm.probability import FunctionFeatures
    features = []
    try:
        # Walk repo files and extract Python function names + basic features
        rp = repo_path if hasattr(repo_path, 'iterdir') else Path(repo_path)
        for py_file in list(rp.rglob("*.py"))[:100]:  # Cap at 100 files
            try:
                content = py_file.read_text()
                lines = content.splitlines()
                name = py_file.name
                # Heuristic: treat each top-level def as a function
                for line in lines:
                    stripped = line.strip()
                    if stripped.startswith("def "):
                        func_name = stripped[4:].split("(")[0].strip()
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
        full = repo / path
        if not full.exists() and Path(path).exists():
            full = Path(path)
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
        output_a = args.get("output_a", "")
        output_b = args.get("output_b", "")
        normalizer = args.get("normalizer", "Text")
        result = await sandbox.diff_execute(output_a, output_b, normalizer)
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
