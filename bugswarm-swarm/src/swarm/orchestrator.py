"""Swarm Orchestrator — manages 12 agents in parallel with real LLM-driven execution.

Rule: Agents are NOT simulated with random.random(). Each agent slot runs a real
IEPEngine instance connected to the LLM Gateway, CPG, and Sandbox.
"""

from __future__ import annotations

import asyncio
import hashlib
import time
from collections import defaultdict
from pathlib import Path

import structlog
from gateway.types import ProviderType
from gateway.client import LLMClient

from agent.loop import IEPEngine, IEPConfig
from agent.tools import ToolRegistry, ToolDefinition
from agent.parser import OutputParser
from agent.cpg_client import CPGClient
from agent.sandbox_client import SandboxClient
from agent.persistence import PersistenceManager
from agent.prompts import Persona as AgentPersona
from agent.scanner import UnifiedScanner

from .types import AgentSlot, AgentStatus, Persona, SwarmConfig, assign_personas
from .routing import mmr_critique_routing, text_similarity
from .monitor import PerformanceMonitor

logger = structlog.get_logger(__name__)


class SwarmOrchestrator:
    """Manages N agents in parallel, routing contributions to the evidence graph.

    Each agent slot runs a real IEPEngine with LLM-driven bug hunting.
    """

    def __init__(self, config: SwarmConfig, gateway: LLMClient):
        self.config = config
        self.gateway = gateway
        self.monitor = PerformanceMonitor(config)
        self.agents: dict[str, AgentSlot] = {}
        self.spare_pool: list[str] = []
        self.pair_history: dict[tuple[str, str], int] = defaultdict(int)
        self.semaphore = asyncio.Semaphore(config.batch_size)
        self.current_round = 0
        self._agent_engines: dict[str, IEPEngine] = {}  # Per-agent IEP instances
        self._persistence: PersistenceManager | None = None
        self._tools: ToolRegistry | None = None
        self._parser: OutputParser = OutputParser()

    def initialize_agents(self) -> None:
        personas = assign_personas(self.config.num_agents)
        for i in range(self.config.num_agents):
            aid = f"A{i+1}"
            self.agents[aid] = AgentSlot(id=aid, persona=personas[i], status=AgentStatus.IDLE)

        for i in range(self.config.spare_pool_size):
            sid = f"S{i+1}"
            self.spare_pool.append(sid)
            self.agents[sid] = AgentSlot(id=sid, persona=Persona.CAUSAL, status=AgentStatus.IDLE)

    def setup_tools(self, repo_path: Path) -> ToolRegistry:
        """Create tool registry with real CPG and Sandbox clients."""
        if self._tools:
            return self._tools

        cpg = CPGClient()
        sandbox = SandboxClient()
        scanner = UnifiedScanner()
        evidence = None  # Lazy-init

        def get_evidence():
            nonlocal evidence
            if evidence is None:
                from agent.evidence_client import EvidenceClient
                evidence = EvidenceClient()
            return evidence

        tools = ToolRegistry()
        tools.register(ToolDefinition(
            name="read_file", description="Read lines from a file",
            parameters={"type": "object", "properties": {
                "path": {"type": "string"}, "start_line": {"type": "integer"},
                "end_line": {"type": "integer"}},
                "required": ["path"]},
            handler=lambda args: asyncio.ensure_future(self._tool_read_file(repo_path, scanner, args)),
            timeout_secs=10.0, cache_ttl_secs=30.0,
        ))
        tools.register(ToolDefinition(
            name="query_cpg", description="Query the Code Property Graph",
            parameters={"type": "object", "properties": {
                "name": {"type": "string"}, "kind": {"type": "string"}},
                "required": []},
            handler=lambda args: asyncio.ensure_future(self._tool_query_cpg(cpg, repo_path, args)),
            timeout_secs=30.0, cache_ttl_secs=10.0,
        ))
        tools.register(ToolDefinition(
            name="exec_sandbox", description="Execute PoC in isolated sandbox",
            parameters={"type": "object", "properties": {
                "poc_code": {"type": "string"}},
                "required": ["poc_code"]},
            handler=lambda args: asyncio.ensure_future(self._tool_exec_sandbox(sandbox, scanner, args)),
            timeout_secs=130.0, max_retries=1, cache_ttl_secs=0.0,
        ))
        tools.register(ToolDefinition(
            name="list_dir", description="List directory contents",
            parameters={"type": "object", "properties": {
                "path": {"type": "string"}},
                "required": []},
            handler=lambda args: asyncio.ensure_future(self._tool_list_dir(repo_path, args)),
            timeout_secs=5.0, cache_ttl_secs=30.0,
        ))
        tools.register(ToolDefinition(
            name="trace_dependency", description="Trace call dependencies",
            parameters={"type": "object", "properties": {
                "function_name": {"type": "string"}, "radius": {"type": "integer"}},
                "required": ["function_name"]},
            handler=lambda args: asyncio.ensure_future(self._tool_trace(cpg, repo_path, args)),
            timeout_secs=30.0, cache_ttl_secs=15.0,
        ))

        # Phase 23: Trigger Matrix tools
        tools.register(ToolDefinition(
            name="describe_trigger",
            description="Document a trigger condition for a confirmed bug. Records input, environment, timing, data state, concurrency, configuration, dependency version, or OS/arch triggers.",
            parameters={"type": "object", "properties": {
                "bug_id": {"type": "string", "description": "The bug identifier"},
                "dimension": {"type": "string", "description": "Trigger dimension: Input, Environment, Timing, DataState, Concurrency, Configuration, DependencyVersion, OsArch"},
                "description": {"type": "string", "description": "Human-readable description of the trigger condition"},
            }, "required": ["bug_id", "dimension", "description"]},
            handler=lambda args: asyncio.ensure_future(self._tool_describe_trigger(args)),
            timeout_secs=10.0,
            cache_ttl_secs=0.0,
        ))
        tools.register(ToolDefinition(
            name="get_trigger_matrix",
            description="Retrieve the complete trigger matrix for a confirmed bug with all documented trigger conditions.",
            parameters={"type": "object", "properties": {
                "bug_id": {"type": "string", "description": "The bug identifier"},
            }, "required": ["bug_id"]},
            handler=lambda args: asyncio.ensure_future(self._tool_get_trigger_matrix(args)),
            timeout_secs=10.0,
            cache_ttl_secs=30.0,
        ))

        # Batch 3B: Additional sandbox and evidence tools
        tools.register(ToolDefinition(
            name="delta_debug",
            description="Minimize a crashing input using delta debugging (ddmin algorithm).",
            parameters={"type": "object", "properties": {
                "input_bytes_b64": {"type": "string", "description": "Base64-encoded crashing input bytes"},
                "max_iterations": {"type": "integer"},
                "timeout_secs": {"type": "integer"},
            }, "required": ["input_bytes_b64"]},
            handler=lambda args: asyncio.ensure_future(self._tool_delta_debug(sandbox, args)),
            timeout_secs=120.0, max_retries=1, cache_ttl_secs=0.0,
        ))
        tools.register(ToolDefinition(
            name="diff_execute",
            description="Compare two outputs using differential analysis.",
            parameters={"type": "object", "properties": {
                "output_a": {"type": "string"},
                "output_b": {"type": "string"},
                "normalizer": {"type": "string", "description": "Text, Json, Xml, Dict, or Binary"},
            }, "required": ["output_a", "output_b"]},
            handler=lambda args: asyncio.ensure_future(self._tool_diff_execute(sandbox, args)),
            timeout_secs=120.0, cache_ttl_secs=10.0,
        ))
        tools.register(ToolDefinition(
            name="mine_invariants",
            description="Mine invariants from function execution traces.",
            parameters={"type": "object", "properties": {
                "function_name": {"type": "string"},
                "param_types": {"type": "array", "items": {"type": "string"}},
                "count": {"type": "integer"},
            }, "required": ["function_name"]},
            handler=lambda args: asyncio.ensure_future(self._tool_mine_invariants(sandbox, args)),
            timeout_secs=60.0, cache_ttl_secs=30.0,
        ))
        tools.register(ToolDefinition(
            name="run_mutations",
            description="Run mutation testing against source code.",
            parameters={"type": "object", "properties": {
                "source_code": {"type": "string"},
                "file_path": {"type": "string"},
                "operators": {"type": "array", "items": {"type": "string"}},
            }, "required": ["source_code"]},
            handler=lambda args: asyncio.ensure_future(self._tool_run_mutations(sandbox, args)),
            timeout_secs=60.0, cache_ttl_secs=0.0,
        ))
        tools.register(ToolDefinition(
            name="solve_reachability",
            description="Solve for the exact input that reaches a target code location.",
            parameters={"type": "object", "properties": {
                "target_location": {"type": "string"},
                "path_conditions": {"type": "array", "items": {
                    "type": "object", "properties": {
                        "line": {"type": "integer"}, "condition": {"type": "string"},
                    }, "required": ["line", "condition"]}},
            }, "required": ["target_location"]},
            handler=lambda args: asyncio.ensure_future(self._tool_solve_reachability(sandbox, args)),
            timeout_secs=60.0, cache_ttl_secs=30.0,
        ))
        tools.register(ToolDefinition(
            name="explore_paths",
            description="Systematically explore all code paths using concolic execution.",
            parameters={"type": "object", "properties": {
                "target_location": {"type": "string"},
                "path_conditions": {"type": "array", "items": {
                    "type": "object", "properties": {
                        "line": {"type": "integer"}, "condition": {"type": "string"},
                    }, "required": ["line", "condition"]}},
                "max_queries": {"type": "integer"},
            }, "required": ["target_location"]},
            handler=lambda args: asyncio.ensure_future(self._tool_explore_paths(sandbox, args)),
            timeout_secs=120.0, cache_ttl_secs=60.0,
        ))
        tools.register(ToolDefinition(
            name="suggest_chain",
            description="Analyze bugs and discover exploit chains with severity escalations.",
            parameters={"type": "object", "properties": {
                "bug_ids": {"type": "array", "items": {"type": "string"}},
                "max_hops": {"type": "integer"},
            }, "required": ["bug_ids"]},
            handler=lambda args: asyncio.ensure_future(self._tool_suggest_chain(get_evidence, args)),
            timeout_secs=30.0, cache_ttl_secs=60.0,
        ))
        tools.register(ToolDefinition(
            name="predict_fix_impact",
            description="Predict whether a proposed fix will introduce new bugs.",
            parameters={"type": "object", "properties": {
                "bug_id": {"type": "string"},
                "function_name": {"type": "string"},
                "file_path": {"type": "string"},
                "original_line": {"type": "string"},
                "replacement_line": {"type": "string"},
                "line_number": {"type": "integer"},
                "language": {"type": "string"},
                "description": {"type": "string"},
            }, "required": ["bug_id", "function_name"]},
            handler=lambda args: asyncio.ensure_future(self._tool_predict_fix_impact(get_evidence, args)),
            timeout_secs=30.0, cache_ttl_secs=60.0,
        ))

        self._tools = tools
        return tools

    async def _tool_read_file(self, repo: Path, scanner: UnifiedScanner, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        try:
            path = args.get("path", "")
            start = int(args.get("start_line", 1))
            end = int(args.get("end_line", start + 50))

            repo_resolved = repo.resolve()

            # H12: Path traversal prevention
            if path.startswith("/"):
                return ToolResult(False, "Path traversal blocked: absolute paths are not allowed")
            if ".." in path:
                return ToolResult(False, "Path traversal blocked: parent directory navigation not allowed")

            full = (repo / path).resolve()

            if not str(full).startswith(str(repo_resolved)):
                return ToolResult(False, f"Path traversal blocked: {path} resolves outside repository")

            if not full.exists():
                return ToolResult(False, f"File not found: {path}")

            lines = full.read_text().splitlines()
            result_lines = [f"{i+1}: {lines[i]}" for i in range(max(0, start-1), min(len(lines), end))]
            content = "\n".join(result_lines)
            redacted, count = scanner.redact(content)
            return ToolResult(True, redacted, {"lines": f"{start}-{end}", "total": len(lines)})
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_query_cpg(self, cpg: CPGClient, repo: Path, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        try:
            stats = await cpg.stats(repo)
            data = {
                "files": stats.total_files, "functions": stats.total_functions,
                "sources": stats.sources, "sinks": stats.sinks,
                "taint_paths": stats.taint_paths, "languages": stats.by_language,
            }
            import json
            return ToolResult(True, json.dumps(data, indent=2), {"source": "cpg"})
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_exec_sandbox(self, sandbox: SandboxClient, scanner: UnifiedScanner, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        poc_code = args.get("poc_code", "")
        if not poc_code:
            return ToolResult(False, "No PoC code provided")
        # Security scan
        if scanner.has_escape_attempt(poc_code):
            return ToolResult(False, "PoC contains sandbox escape patterns — REJECTED")
        receipt = await sandbox.execute(poc_code)
        import json
        return ToolResult(True, json.dumps(receipt.to_summary()), {
            "exit_code": receipt.exit_code, "status": receipt.status,
        })

    async def _tool_list_dir(self, repo: Path, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        try:
            path = args.get("path", ".")
            target = repo / path if path != "." else repo
            entries = [f"  [{'DIR' if e.is_dir() else 'FILE'}] {e.name}"
                       for e in sorted(target.iterdir())]
            return ToolResult(True, f"Contents of {target}:\n" + "\n".join(entries))
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_trace(self, cpg: CPGClient, repo: Path, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        try:
            paths = await cpg.taint_paths(repo)
            lines = [f"Path: {p.source} -> {p.sink} (len={p.length}, san={p.sanitized})"
                      for p in paths[:10]]
            return ToolResult(True, "\n".join(lines) or "No taint paths found")
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_describe_trigger(self, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        import json
        try:
            bug_id = args.get("bug_id", "unknown")
            dimension = args.get("dimension", "Input")
            description = args.get("description", "")
            from agent.evidence_client import EvidenceClient
            ev = EvidenceClient()
            result = await ev.add_trigger_condition(bug_id, dimension, description, "agent")
            return ToolResult(True, json.dumps(result, indent=2), {"bug_id": bug_id})
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_get_trigger_matrix(self, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        import json
        try:
            bug_id = args.get("bug_id", "unknown")
            from agent.evidence_client import EvidenceClient
            ev = EvidenceClient()
            result = await ev.get_trigger_matrix(bug_id)
            return ToolResult(True, json.dumps(result, indent=2), {"bug_id": bug_id})
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_delta_debug(self, sandbox: SandboxClient, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        import base64, json
        try:
            input_b64 = args.get("input_bytes_b64", "")
            input_bytes = base64.b64decode(input_b64)
            max_iterations = int(args.get("max_iterations", 200))
            timeout_secs = int(args.get("timeout_secs", 30))
            result = await sandbox.delta_debug(input_bytes, max_iterations, timeout_secs)
            return ToolResult(True, json.dumps(result, indent=2))
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_diff_execute(self, sandbox: SandboxClient, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        import json
        try:
            output_a = args.get("output_a", "")
            output_b = args.get("output_b", "")
            normalizer = args.get("normalizer", "Text")
            result = await sandbox.diff_execute(output_a, output_b, normalizer)
            return ToolResult(True, json.dumps(result, indent=2))
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_mine_invariants(self, sandbox: SandboxClient, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        import json
        try:
            function_name = args.get("function_name", "")
            param_types = args.get("param_types", [])
            count = int(args.get("count", 100))
            result = await sandbox.mine_invariants(function_name, param_types, count)
            return ToolResult(True, json.dumps(result, indent=2))
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_run_mutations(self, sandbox: SandboxClient, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        import json
        try:
            source_code = args.get("source_code", "")
            file_path = args.get("file_path", "unknown")
            operators = args.get("operators")
            result = await sandbox.run_mutations(source_code, file_path, operators)
            return ToolResult(True, json.dumps(result, indent=2))
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_solve_reachability(self, sandbox: SandboxClient, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        import json
        try:
            target_location = args.get("target_location", "")
            path_conditions = args.get("path_conditions", [])
            result = await sandbox.solve_reachability(target_location, path_conditions)
            return ToolResult(True, json.dumps(result, indent=2))
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_explore_paths(self, sandbox: SandboxClient, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        import json
        try:
            target_location = args.get("target_location", "")
            path_conditions = args.get("path_conditions", [])
            max_queries = int(args.get("max_queries", 100))
            result = await sandbox.explore_paths(target_location, path_conditions, max_queries)
            return ToolResult(True, json.dumps(result, indent=2))
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_suggest_chain(self, get_evidence, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        import json
        try:
            bug_ids = args.get("bug_ids", [])
            max_hops = int(args.get("max_hops", 10))
            ev = get_evidence()
            result = await ev.suggest_chain(bug_ids, max_hops)
            return ToolResult(True, json.dumps(result, indent=2))
        except Exception as e:
            return ToolResult(False, str(e))

    async def _tool_predict_fix_impact(self, get_evidence, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        import json
        try:
            bug_id = args.get("bug_id", "")
            function_name = args.get("function_name", "")
            file_path = args.get("file_path", "")
            original_line = args.get("original_line", "")
            replacement_line = args.get("replacement_line", "")
            line_number = int(args.get("line_number", 0))
            language = args.get("language", "python")
            description = args.get("description", "")
            ev = get_evidence()
            result = await ev.predict_fix_impact(
                bug_id, function_name, file_path,
                original_line, replacement_line, line_number,
                language, description,
            )
            return ToolResult(True, json.dumps(result, indent=2))
        except Exception as e:
            return ToolResult(False, str(e))

    async def run_round(self, round_num: int) -> dict:
        self.current_round = round_num
        active = [a for a in self.agents.values() if a.status == AgentStatus.ACTIVE]
        logger.info("swarm_round_start", round=round_num, active_agents=len(active))
        results = {"round": round_num, "findings": [], "ejections": [], "loops": [], "echo_chambers": []}

        if round_num == 1:
            hypotheses = await self._round1_isolation(active)
            results["hypotheses"] = hypotheses
        else:
            hypotheses_text = {a.id: a.round_hypotheses.get(1, "") for a in active if 1 in a.round_hypotheses}
            pairings = mmr_critique_routing(active, hypotheses_text, self.pair_history)
            results["pairings"] = [(a, r) for a, r in pairings]
            results["findings"].extend(await self._execute_round(active, round_num))

        results["echo_chambers"] = self.monitor.detect_echo_chamber(active)
        results["loops"] = [aid for aid, a in self.agents.items() if a.status == AgentStatus.LOOPING]

        diversity = self._compute_diversity(active)
        if diversity < self.config.diversity_warning_threshold:
            logger.warning("diversity_collapse", round=round_num, diversity=diversity)
            results["diversity_warning"] = True

        return results

    async def _round1_isolation(self, agent_slots: list[AgentSlot]) -> dict[str, str]:
        """Round 1: Each agent generates hypotheses independently via real LLM."""
        hypotheses: dict[str, str] = {}
        tools = self.setup_tools(self.config.repo_path)

        async def agent_generate(slot: AgentSlot):
            async with self.semaphore:
                slot.status = AgentStatus.ACTIVE
                # Create IEPEngine for this agent
                persona = AgentPersona(slot.persona.value)
                iep_config = IEPConfig(
                    repo_path=self.config.repo_path,
                    run_id=f"{slot.id}-r1",
                    max_rounds=1, max_turns_per_round=3,
                    persona=persona, model=self.config.model,
                    provider=self.config.provider,
                )
                engine = IEPEngine(iep_config, tools, self._parser, self.gateway, self._persistence)
                self._agent_engines[slot.id] = engine

                # Run one turn to get a hypothesis
                report = await engine.run()
                findings = report.findings
                if findings:
                    hypothesis = findings[0].claim
                else:
                    hypothesis = f"Agent {slot.id} ({persona.value}) investigates {self.config.repo_path}"
                
                slot.round_hypotheses[1] = hypothesis
                hypotheses[slot.id] = hypothesis
                self.monitor.record_message(slot.id, hypothesis)
                logger.info("agent_hypothesis", agent=slot.id, persona=persona.value)

        tasks = [agent_generate(s) for s in agent_slots]
        await asyncio.gather(*tasks, return_exceptions=True)
        return hypotheses

    async def _execute_round(self, agents: list[AgentSlot], round_num: int) -> list[dict]:
        """Execute evidence gathering for a round using real IEPEngine agents."""
        findings: list[dict] = []
        tools = self.setup_tools(self.config.repo_path)

        async def agent_turn(slot: AgentSlot):
            async with self.semaphore:
                if self.monitor.detect_loop(slot.id):
                    slot.loop_count += 1
                    slot.status = AgentStatus.LOOPING
                    logger.warning("agent_looping", agent=slot.id, count=slot.loop_count)

                should_eject, reason = self.monitor.should_eject(slot)
                if should_eject:
                    slot.status = AgentStatus.EJECTED
                    slot.ejection_reason = reason
                    logger.warning("agent_ejected", agent=slot.id, reason=reason)
                    self._replace_agent(slot.id)
                    return

                engine = self._agent_engines.get(slot.id)
                if not engine:
                    persona = AgentPersona(slot.persona.value)
                    iep_config = IEPConfig(
                        repo_path=self.config.repo_path,
                        run_id=f"{slot.id}-r{round_num}",
                        max_rounds=1, max_turns_per_round=2,
                        persona=persona, model=self.config.model,
                        provider=self.config.provider,
                    )
                    engine = IEPEngine(iep_config, tools, self._parser, self.gateway, self._persistence)
                    self._agent_engines[slot.id] = engine

                report = await engine.run()
                for f in report.findings:
                    finding_dict = f.to_dict()
                    finding_dict["agent"] = slot.id
                    finding_dict["persona"] = slot.persona.value
                    finding_dict["round"] = round_num
                    findings.append(finding_dict)
                    slot.findings.append(finding_dict)
                    slot.contribution_score += 1
                    if f.verified:
                        slot.trust_score = min(1.0, slot.trust_score + 0.05)
                    else:
                        slot.hallucination_rate = min(1.0, slot.hallucination_rate * 0.8 + 0.2)
                    self.monitor.record_message(slot.id, f.claim)

        tasks = []
        for _ in range(min(3, self.config.max_turns_per_round)):
            for agent in agents:
                if agent.status == AgentStatus.ACTIVE:
                    tasks.append(agent_turn(agent))
        await asyncio.gather(*tasks, return_exceptions=True)
        return findings

    def _replace_agent(self, ejected_id: str) -> None:
        if self.spare_pool:
            replacement = self.spare_pool.pop(0)
            old_persona = self.agents[ejected_id].persona
            self.agents[replacement].status = AgentStatus.ACTIVE
            self.agents[replacement].persona = old_persona
            # Remove old engine
            self._agent_engines.pop(ejected_id, None)
            logger.info("agent_replaced", ejected=ejected_id, replacement=replacement)
        else:
            logger.warning("spare_pool_exhausted", ejected=ejected_id)

    @staticmethod
    def _maybe_retrain_model(pdb, ht) -> None:
        """Phase 19: Trigger ML model retraining after accumulating enough new bugs."""
        try:
            from swarm.probability import BugProbabilityModel
            model = BugProbabilityModel()
            confirmed = pdb.get_all_confirmed()
            # CPG function extraction deferred — model trains with confirmed bug data only
            # When CPG query API for function nodes is available (Phase 19.5), pass all_funcs
            all_funcs = []
            result = model.train(confirmed, all_funcs, hotspot_tracker=ht)
            if result.get("status") == "trained":
                pdb.reset_training_counter()
                logger.info("model_retrained_triggered", samples=result.get("samples", 0),
                             auc=result.get("auc", 0))
            else:
                logger.info("model_retrain_skipped", reason=result.get("reason", "unknown"))
        except Exception as e:
            logger.warning("model_retrain_failed", error=str(e)[:200])

    def reinstate_agent(self, agent_id: str) -> None:
        if agent_id in self.agents:
            self.agents[agent_id].status = AgentStatus.ACTIVE
            self.agents[agent_id].trust_score = min(1.0, self.agents[agent_id].trust_score + 0.2)
            logger.info("agent_reinstated", agent=agent_id)

    def _compute_diversity(self, agent_slots: list[AgentSlot]) -> float:
        active = [a for a in agent_slots if a.status == AgentStatus.ACTIVE]
        if len(active) < 2:
            return 1.0

        texts = []
        for a in active:
            h = a.round_hypotheses.get(self.current_round, "") or a.round_hypotheses.get(1, "")
            texts.append(h)

        similarities = []
        for i in range(len(texts)):
            for j in range(i + 1, len(texts)):
                similarities.append(text_similarity(texts[i], texts[j]))

        avg_sim = sum(similarities) / max(len(similarities), 1) if similarities else 0.0
        return 1.0 - avg_sim

    async def run(self) -> dict:
        self.initialize_agents()
        for aid in self.agents:
            if not aid.startswith("S"):
                self.agents[aid].status = AgentStatus.ACTIVE

        all_findings = []
        round_results = []

        for round_num in range(1, self.config.max_rounds + 1):
            logger.info("swarm_round", round=round_num)
            result = await self.run_round(round_num)
            all_findings.extend(result.get("findings", []))
            round_results.append(result)

            verified = sum(1 for f in all_findings if f.get("verified"))
            if verified >= 5:
                logger.info("swarm_terminated", reason="enough_verified", verified=verified)
                break

        scores = {}
        for a in self.agents.values():
            if a.status in (AgentStatus.ACTIVE, AgentStatus.COMPLETED):
                scores[a.id] = {
                    "persona": a.persona.value,
                    "trust": a.trust_score,
                    "contributions": a.contribution_score,
                    "hallucination_rate": a.hallucination_rate,
                    "loops": a.loop_count,
                    "status": a.status.value,
                }

        # G2: Push verified findings to PatternDB for persistent learning (Phase 18.5)
        try:
            from swarm.pattern_db import BugPatternDB, HotspotTracker, AgentHistory
            pdb = BugPatternDB()
            ht = HotspotTracker()
            ah = AgentHistory()
            for finding in all_findings:
                if finding.get("verified"):
                    loc = finding.get("location", "unknown:0")
                    file_path = loc.split(":")[0] if ":" in loc else loc
                    severity = finding.get("severity_estimate", 5)
                    claim = finding.get("claim", "")
                    mechanism = finding.get("mechanism", "")
                    pdb.store_finding(
                        {"claim": claim, "mechanism": mechanism, "severity_estimate": severity, "location": loc},
                        code_snippet=f"{claim}\n{mechanism}"[:500],
                        language="python",
                    )
                    ht.record_bug(file_path, severity)
            for aid, s in scores.items():
                ah.record_run(aid, s.get("persona", ""), self.config.model,
                               s.get("contributions", 0), 0,
                               self.gateway.registry.total_tokens.total_tokens,
                               sum(1 for f in all_findings if f.get("verified") and f.get("agent") == aid))
            logger.info("learning_persisted", patterns=pdb.count, hotspots=len(ht.entries), agents=len(ah.records))

            # Also push trigger conditions to Rust evidence daemon
            try:
                from agent.evidence_client import EvidenceClient
                ev_client = EvidenceClient()
                for finding in all_findings:
                    if finding.get("verified"):
                        bug_id = finding.get("id", hashlib.sha256(finding.get("claim", "").encode()).hexdigest()[:12])
                        asyncio.ensure_future(ev_client.add_trigger_condition(
                            bug_id, "Input", f"Agent: {finding.get('claim', '')[:200]}", "agent"
                        ))
            except Exception:
                pass

            # Phase 19: Trigger retrain if enough new bugs accumulated
            if pdb.count_since_last_train >= 50:
                self._maybe_retrain_model(pdb, ht)
        except Exception as e:
            logger.warning("learning_persist_failed", error=str(e)[:200])

        # Phase 23: Contribute trigger conditions for verified findings
        for finding in all_findings:
            if finding.get("verified"):
                bug_id = hashlib.sha256(
                    finding.get("claim", "unknown").encode()
                ).hexdigest()[:12]
                loc = finding.get("location", "unknown:0")
                claim = finding.get("claim", "")
                mechanism = finding.get("mechanism", "")
                
                # Input dimension: the location where the bug was found
                desc = f"Code location: {loc}"
                if claim:
                    desc += f". Claim: {claim[:200]}"
                asyncio.ensure_future(
                    self._tool_describe_trigger({
                        "bug_id": bug_id,
                        "dimension": "Input",
                        "description": desc,
                    })
                )
                
                # DataState: if mechanism mentions state conditions
                if mechanism and any(kw in mechanism.lower() for kw in ["null", "none", "empty", "state", "missing"]):
                    asyncio.ensure_future(
                        self._tool_describe_trigger({
                            "bug_id": bug_id,
                            "dimension": "DataState",
                            "description": f"State condition: {mechanism[:200]}",
                        })
                    )

        return {
            "rounds": len(round_results),
            "total_findings": len(all_findings),
            "verified_findings": sum(1 for f in all_findings if f.get("verified")),
            "agent_scores": scores,
            "ejections": sum(1 for a in self.agents.values() if a.status == AgentStatus.EJECTED),
            "round_results": round_results,
        }
