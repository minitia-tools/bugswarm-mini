"""Swarm Orchestrator — manages 12 agents in parallel with real LLM-driven execution.

Rule: Agents are NOT simulated with random.random(). Each agent slot runs a real
IEPEngine instance connected to the LLM Gateway, CPG, and Sandbox.
"""

from __future__ import annotations

import asyncio
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

        self._tools = tools
        return tools

    async def _tool_read_file(self, repo: Path, scanner: UnifiedScanner, args: dict) -> "ToolResult":
        from agent.tools import ToolResult
        try:
            path = args.get("path", "")
            start = int(args.get("start_line", 1))
            end = int(args.get("end_line", start + 50))
            full = repo / path
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

        return {
            "rounds": len(round_results),
            "total_findings": len(all_findings),
            "verified_findings": sum(1 for f in all_findings if f.get("verified")),
            "agent_scores": scores,
            "ejections": sum(1 for a in self.agents.values() if a.status == AgentStatus.EJECTED),
            "round_results": round_results,
        }
