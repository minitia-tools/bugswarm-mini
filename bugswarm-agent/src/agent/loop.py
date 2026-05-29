"""IEP Engine — Issue → Evidence → Proof execution loop.

Rule: Every agent turn follows the IEP cycle. Hypothesis ranking prioritizes what
to investigate. Context management prevents bloat. Self-reflection learns within a run.
"""

from __future__ import annotations

import hashlib
import time
from dataclasses import dataclass, field
from pathlib import Path

import structlog
from gateway.client import LLMClient
from gateway.types import ChatMessage, ChatRequest, MessageRole, ProviderType

from agent.parser import OutputParser, OutputType, ParsedFinding
from agent.persistence import PersistenceManager
from agent.prompts import Persona, PersonaEngine
from agent.scanner import UnifiedScanner
from agent.tools import ToolRegistry, ToolResult

logger = structlog.get_logger(__name__)


# ═══════════════════════════════════════════════════════════════
# Types
# ═══════════════════════════════════════════════════════════════


@dataclass
class Hypothesis:
    """A candidate bug to investigate, ranked by priority."""

    id: str
    claim: str
    location: str
    priority_score: float = 0.0
    source: str = "agent_generated"
    investigated: bool = False
    investigation_result: str = ""  # confirmed, refuted, inconclusive


@dataclass
class IEPConfig:
    repo_path: Path
    run_id: str = field(default_factory=lambda: hashlib.sha256(str(time.time()).encode()).hexdigest()[:12])
    max_rounds: int = 5
    max_turns_per_round: int = 10
    persona: Persona = Persona.CAUSAL
    model: str = "deepseek-v4-flash"
    provider: ProviderType = field(default_factory=lambda: ProviderType.DEEPSEEK)
    prompt_version: str = "v2.0.0"
    db_path: str = ":memory:"

    @property
    def persona_str(self) -> str:
        return self.persona.value if isinstance(self.persona, Persona) else str(self.persona)


@dataclass
class IEPState:
    """Full agent state at any point in the IEP loop."""

    run_id: str
    round: int = 1
    turn: int = 1
    messages: list[ChatMessage] = field(default_factory=list)
    hypotheses: list[Hypothesis] = field(default_factory=list)
    findings: list[ParsedFinding] = field(default_factory=list)
    context_usage: float = 0.0  # 0.0–1.0
    last_tool_results: list[ToolResult] = field(default_factory=list)


@dataclass
class IEPReport:
    run_id: str
    total_rounds: int = 0
    total_turns: int = 0
    hypotheses_generated: int = 0
    hypotheses_investigated: int = 0
    findings: list[ParsedFinding] = field(default_factory=list)
    verified_count: int = 0
    tokens_consumed: int = 0
    cost_usd: float = 0.0
    duration_secs: float = 0.0
    persona: str = ""
    prompt_version: str = ""
    tool_stats: dict = field(default_factory=dict)

    def to_dict(self) -> dict:
        return {
            "run_id": self.run_id,
            "total_rounds": self.total_rounds,
            "total_turns": self.total_turns,
            "findings": [f.to_dict() for f in self.findings],
            "verified_count": self.verified_count,
            "tokens_consumed": self.tokens_consumed,
            "cost_usd": round(self.cost_usd, 4),
            "duration_secs": round(self.duration_secs, 1),
        }


# ═══════════════════════════════════════════════════════════════
# Hypothesis Ranker
# ═══════════════════════════════════════════════════════════════


class HypothesisRanker:
    """Ranks what to investigate next using multi-factor scoring."""

    # Weights for the priority score
    TAINT_WEIGHT = 0.40  # Taint path exists from source to sink
    COMPLEXITY_WEIGHT = 0.20  # Code complexity (cyclomatic proxy via nesting)
    REACHABILITY_WEIGHT = 0.30  # Reachable from external input
    NOVELTY_WEIGHT = 0.10  # Not already investigated

    @classmethod
    def score(
        cls,
        hypothesis: Hypothesis,
        taint_exists: bool = False,
        code_lines: int = 0,
        reachable: bool = True,
        already_investigated: bool = False,
    ) -> float:
        score = 0.0
        if taint_exists:
            score += cls.TAINT_WEIGHT
        if code_lines > 0:
            complexity = min(1.0, code_lines / 200.0)  # Normalize to 0-1
            score += cls.COMPLEXITY_WEIGHT * complexity
        if reachable:
            score += cls.REACHABILITY_WEIGHT
        if not already_investigated:
            score += cls.NOVELTY_WEIGHT
        return score

    @classmethod
    def rank(cls, hypotheses: list[Hypothesis]) -> list[Hypothesis]:
        for h in hypotheses:
            h.priority_score = cls.score(h, already_investigated=h.investigated)
        return sorted(hypotheses, key=lambda h: h.priority_score, reverse=True)


# ═══════════════════════════════════════════════════════════════
# Context Manager
# ═══════════════════════════════════════════════════════════════


class ContextManager:
    """Manages agent context window to prevent bloat."""

    def __init__(self, max_tokens: int = 128000, compression_trigger_pct: float = 0.80):
        self.max_tokens = max_tokens
        self.compression_trigger_pct = compression_trigger_pct
        self._messages: list[ChatMessage] = []

    def add(self, msg: ChatMessage) -> None:
        self._messages.append(msg)

    @property
    def estimated_tokens(self) -> int:
        return sum(len(m.content) // 4 + 4 for m in self._messages)

    @property
    def usage_pct(self) -> float:
        return self.estimated_tokens / self.max_tokens if self.max_tokens > 0 else 0.0

    def should_compress(self) -> bool:
        return self.usage_pct >= self.compression_trigger_pct

    def get_messages(self) -> list[ChatMessage]:
        return list(self._messages)

    def compress_summary(self) -> str:
        """Generate a compressed summary of older messages."""
        if len(self._messages) <= 10:
            return ""
        keep_recent = self._messages[-5:]
        older = self._messages[:-5]
        claims = []
        for m in older:
            c = m.content
            if "finding" in c.lower() or "bug" in c.lower():
                claims.append(c[:150])
        summary_parts = [f"[{len(older)} older messages compressed. Key claims:"]
        for cl in claims[:5]:
            summary_parts.append(f"  - {cl}")
        summary_parts.append("]")
        return "\n".join(summary_parts)


# ═══════════════════════════════════════════════════════════════
# IEP Engine
# ═══════════════════════════════════════════════════════════════


class IEPEngine:
    """Core IEP (Issue → Evidence → Proof) execution loop."""

    def __init__(
        self,
        config: IEPConfig,
        tools: ToolRegistry,
        parser: OutputParser,
        gateway: LLMClient,
        persistence: PersistenceManager | None = None,
    ):
        self.config = config
        self.tools = tools
        self.parser = parser
        self.gateway = gateway
        self.persistence = persistence
        self.context = ContextManager()
        self.state = IEPState(run_id=config.run_id)
        self.scanner = UnifiedScanner()

    async def run(self) -> IEPReport:
        t0 = time.perf_counter()

        # Initialize context with system prompt and initial user message
        persona = self.config.persona if isinstance(self.config.persona, Persona) else Persona(self.config.persona_str)
        system_prompt = PersonaEngine.build(persona, str(self.config.repo_path))
        self.context.add(ChatMessage(role=MessageRole.SYSTEM, content=system_prompt))
        self.context.add(ChatMessage(role=MessageRole.USER, content=self._initial_prompt()))

        if self.persistence:
            self.persistence.create_run(self.config.run_id, self.config.persona_str, self.config.prompt_version)

        for round_num in range(1, self.config.max_rounds + 1):
            self.state.round = round_num
            logger.info("iep_round_start", run_id=self.config.run_id, round=round_num)

            for turn in range(1, self.config.max_turns_per_round + 1):
                self.state.turn = turn
                result = await self._execute_turn()
                if result:
                    self.state.findings.append(result)
                    if self.persistence:
                        self.persistence.log_finding(
                            self.config.run_id,
                            result.claim,
                            result.location,
                            result.mechanism,
                            result.severity,
                            result.verified,
                        )

                if self._should_stop():
                    break

            if self.persistence:
                self.persistence.save_checkpoint(
                    self.config.run_id,
                    round_num,
                    {
                        "findings": len(self.state.findings),
                        "verified": sum(1 for f in self.state.findings if f.verified),
                    },
                )

            if self._should_stop():
                break

        elapsed = time.perf_counter() - t0

        return IEPReport(
            run_id=self.config.run_id,
            total_rounds=self.state.round,
            total_turns=self.state.turn,
            findings=self.state.findings,
            verified_count=sum(1 for f in self.state.findings if f.verified),
            tokens_consumed=self.gateway.registry.total_tokens.total_tokens,
            cost_usd=self.gateway.registry.total_cost,
            duration_secs=elapsed,
            persona=self.config.persona_str,
            prompt_version=self.config.prompt_version,
            tool_stats=self.tools.stats(),
        )

    async def _execute_turn(self) -> ParsedFinding | None:
        """Execute one turn: call LLM → parse response → execute tools."""
        request = ChatRequest(
            messages=self.context.get_messages(),
            model=self.config.model,
            temperature=0.7,
            max_tokens=2048,
        )

        try:
            response = await self.gateway.chat(request, self.config.provider)
        except Exception as e:
            logger.error("llm_call_failed", error=str(e)[:200])
            return None

        content = response.content
        self.context.add(ChatMessage(role=MessageRole.ASSISTANT, content=content))

        if self.persistence:
            self.persistence.log_message(
                self.config.run_id,
                self.state.round,
                self.state.turn,
                "assistant",
                content,
                response.usage.total_tokens,
            )

        # Parse output
        parsed = self.parser.parse(content)

        if parsed.output_type == OutputType.FINDING and parsed.finding:
            return self._process_finding(parsed.finding, content)

        elif parsed.output_type == OutputType.TOOL_CALL and parsed.tool_call:
            result = await self.tools.execute(parsed.tool_call.tool_name, parsed.tool_call.args)
            self.state.last_tool_results.append(result)
            self.context.add(ChatMessage(role=MessageRole.USER, content=result.to_message()))
            if self.persistence:
                self.persistence.log_message(
                    self.config.run_id,
                    self.state.round,
                    self.state.turn,
                    "tool_result",
                    result.to_message(),
                )

        elif parsed.output_type == OutputType.POC and parsed.poc:
            result = await self.tools.execute("exec_sandbox", {"poc_code": parsed.poc.code})
            self.state.last_tool_results.append(result)
            self.context.add(ChatMessage(role=MessageRole.USER, content=result.to_message()))

        # Context compression check
        if self.context.should_compress():
            summary = self.context.compress_summary()
            if summary:
                self.context.add(ChatMessage(role=MessageRole.SYSTEM, content=summary))
                logger.info("context_compressed", run_id=self.config.run_id, usage_pct=self.context.usage_pct)

        return None

    def _process_finding(self, finding: ParsedFinding, content: str) -> ParsedFinding:
        """Process and verify a finding."""
        # Auto-verify from sandbox confirmation in content
        if not finding.verified:
            lower = content.lower()
            has_sandbox = "sandbox" in lower and ("confirmed" in lower or "pass" in lower)
            has_confirmed = any(kw in lower for kw in OutputParser.CONFIRMED_KEYWORDS)
            if has_sandbox or has_confirmed:
                finding.verified = True
        return finding

    def _should_stop(self) -> bool:
        """Check termination conditions."""
        verified = sum(1 for f in self.state.findings if f.verified)
        if verified >= 3:
            logger.info("iep_stop_verified", count=verified)
            return True
        if self.gateway.is_budget_exhausted():
            logger.info("iep_stop_budget")
            return True
        return False

    def _initial_prompt(self) -> str:
        return f"""Analyze the codebase at `{self.config.repo_path}` for bugs.

Follow the IEP cycle:
1. ISSUE: Examine the codebase structure. Query the CPG to discover functions, then read source files.
2. EVIDENCE: For each suspicious pattern, formulate a falsifiable prediction.
3. PROOF: Submit a PoC to the sandbox. Only sandbox-verified findings count.

Start by listing the directory and querying the CPG for the codebase structure."""
