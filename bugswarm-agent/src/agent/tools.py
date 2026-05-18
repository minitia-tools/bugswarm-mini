"""Tool Registry — plugin-based dispatch with retry, cache, circuit breaker.

Rule: No raw subprocess.run(). Every external call goes through a ToolDefinition
with timeout, retry budget, and graceful degradation path.
"""

from __future__ import annotations

import asyncio
import hashlib
import random
import time
from collections import OrderedDict, defaultdict
from dataclasses import dataclass, field
from typing import Any, Callable

import structlog

logger = structlog.get_logger(__name__)


@dataclass
class ToolResult:
    success: bool
    data: str
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_message(self) -> str:
        if not self.success:
            return f"Tool error: {self.data}"
        result = self.data
        if self.metadata:
            import json
            result += f"\n[Metadata: {json.dumps(self.metadata)}]"
        return result


@dataclass
class ToolDefinition:
    name: str
    description: str
    parameters: dict  # JSON Schema for LLM function calling
    handler: Callable  # async (args: dict) -> ToolResult
    timeout_secs: float = 30.0
    max_retries: int = 2
    cache_ttl_secs: float = 60.0
    required_permissions: list[str] = field(default_factory=list)


class RetryPolicy:
    """Exponential backoff with jitter."""

    def __init__(self, max_retries: int = 3, base_delay: float = 1.0,
                 max_delay: float = 60.0):
        self.max_retries = max_retries
        self.base_delay = base_delay
        self.max_delay = max_delay

    def delay_for(self, attempt: int) -> float:
        d = min(self.base_delay * (2 ** (attempt - 1)), self.max_delay)
        return d * (0.5 + random.random())


class ToolCache:
    """LRU cache for tool results, keyed by (tool_name, args_hash)."""

    def __init__(self, max_size: int = 500):
        self.cache: OrderedDict[str, tuple[ToolResult, float]] = OrderedDict()
        self.max_size = max_size
        self.hits = 0
        self.misses = 0

    def _key(self, tool_name: str, args: dict) -> str:
        raw = f"{tool_name}:{sorted(args.items())}"
        return hashlib.sha256(raw.encode()).hexdigest()

    def get(self, tool_name: str, args: dict, ttl: float) -> ToolResult | None:
        key = self._key(tool_name, args)
        if key in self.cache:
            result, ts = self.cache[key]
            if time.time() - ts < ttl:
                self.hits += 1
                self.cache.move_to_end(key)
                return result
            del self.cache[key]
        self.misses += 1
        return None

    def set(self, tool_name: str, args: dict, result: ToolResult) -> None:
        key = self._key(tool_name, args)
        self.cache[key] = (result, time.time())
        if len(self.cache) > self.max_size:
            self.cache.popitem(last=False)

    @property
    def hit_rate(self) -> float:
        total = self.hits + self.misses
        return self.hits / total if total > 0 else 0.0

    def invalidate(self, tool_name: str | None = None) -> None:
        if tool_name:
            self.cache = OrderedDict(
                (k, v) for k, v in self.cache.items()
                if not k.startswith(hashlib.sha256(tool_name.encode()).hexdigest()[:8])
            )
        else:
            self.cache.clear()


class CircuitBreaker:
    """Opens after N consecutive failures, half-opens after cooldown."""

    def __init__(self, failure_threshold: int = 5, cooldown_secs: float = 30.0):
        self.threshold = failure_threshold
        self.cooldown = cooldown_secs
        self.failures = 0
        self.last_failure_time = 0.0
        self.state = "closed"  # closed, open, half_open

    def record_success(self) -> None:
        self.failures = 0
        self.state = "closed"

    def record_failure(self) -> None:
        self.failures += 1
        self.last_failure_time = time.time()
        if self.failures >= self.threshold:
            self.state = "open"
            logger.warning("circuit_breaker_open", failures=self.failures)

    def allow_request(self) -> bool:
        if self.state == "closed":
            return True
        if self.state == "open":
            if time.time() - self.last_failure_time > self.cooldown:
                self.state = "half_open"
                return True
            return False
        return True  # half_open



MAX_POC_SIZE_BYTES = 1_000_000
MAX_INPUT_SIZE_BYTES = 10_000_000
MAX_SOURCE_SIZE_BYTES = 5_000_000
MAX_OUTPUT_SIZE_BYTES = 5_000_000
MAX_COUNT = 10_000
MAX_QUERIES = 1_000
MAX_HOPS = 20
RATE_LIMIT_WINDOW_SECS = 60
MAX_CALLS_PER_WINDOW = 100


def _validate_exec_sandbox(args: dict) -> tuple[bool, str]:
    poc_code = args.get("poc_code", "")
    if not poc_code:
        return False, "exec_sandbox requires non-empty 'poc_code'"
    if len(poc_code.encode("utf-8")) > MAX_POC_SIZE_BYTES:
        return False, f"PoC code too large: {len(poc_code.encode('utf-8'))} bytes (max {MAX_POC_SIZE_BYTES})"
    return True, ""


def _validate_read_file(args: dict) -> tuple[bool, str]:
    path = args.get("path", "")
    if not path:
        return False, "read_file requires non-empty 'path'"
    if path.startswith("/"):
        return False, f"read_file rejected: absolute path '{path}' forbidden"
    if ".." in path.replace("\\", "/").split("/"):
        return False, f"read_file rejected: path traversal blocked for '{path}'"
    return True, ""


def _validate_fuzz_target(args: dict) -> tuple[bool, str]:
    target = args.get("target_path", "")
    if not target:
        return False, "fuzz_target requires non-empty 'target_path'"
    timeout = int(args.get("exec_timeout_ms", 1000))
    if timeout < 10 or timeout > 300_000:
        return False, f"fuzz_target timeout must be 10-300000ms, got {timeout}"
    return True, ""


def _validate_delta_debug(args: dict) -> tuple[bool, str]:
    input_data = args.get("input_base64", args.get("input_bytes", ""))
    if isinstance(input_data, bytes):
        if len(input_data) > MAX_INPUT_SIZE_BYTES:
            return False, f"Input too large: {len(input_data)} bytes (max {MAX_INPUT_SIZE_BYTES})"
    iterations = int(args.get("max_iterations", 200))
    if iterations < 1 or iterations > MAX_COUNT:
        return False, f"max_iterations must be 1-{MAX_COUNT}, got {iterations}"
    return True, ""


def _validate_diff_execute(args: dict) -> tuple[bool, str]:
    output_a = args.get("output_a", "")
    output_b = args.get("output_b", "")
    if not output_a and not output_b:
        return False, "diff_execute requires at least one output to compare"
    if len(output_a) > MAX_OUTPUT_SIZE_BYTES or len(output_b) > MAX_OUTPUT_SIZE_BYTES:
        return False, f"Output too large (max {MAX_OUTPUT_SIZE_BYTES} bytes)"
    return True, ""


def _validate_mine_invariants(args: dict) -> tuple[bool, str]:
    func = args.get("function_name", "")
    if not func:
        return False, "mine_invariants requires non-empty 'function_name'"
    count = int(args.get("count", 100))
    if count < 1 or count > MAX_COUNT:
        return False, f"count must be 1-{MAX_COUNT}, got {count}"
    return True, ""


def _validate_run_mutations(args: dict) -> tuple[bool, str]:
    source = args.get("source_code", "")
    if not source:
        return False, "run_mutations requires non-empty 'source_code'"
    if len(source.encode("utf-8")) > MAX_SOURCE_SIZE_BYTES:
        return False, f"Source too large: {len(source.encode('utf-8'))} bytes (max {MAX_SOURCE_SIZE_BYTES})"
    return True, ""


def _validate_solve_reachability(args: dict) -> tuple[bool, str]:
    target = args.get("target_location", "")
    if not target:
        return False, "solve_reachability requires non-empty 'target_location'"
    if ":" not in target:
        return False, f"target_location must be 'file:line' format, got '{target}'"
    return True, ""


def _validate_explore_paths(args: dict) -> tuple[bool, str]:
    target = args.get("target_location", "")
    if not target:
        return False, "explore_paths requires non-empty 'target_location'"
    queries = int(args.get("max_queries", 100))
    if queries < 1 or queries > MAX_QUERIES:
        return False, f"max_queries must be 1-{MAX_QUERIES}, got {queries}"
    return True, ""


def _validate_describe_trigger(args: dict) -> tuple[bool, str]:
    bug_id = args.get("bug_id", "")
    if not bug_id:
        return False, "describe_trigger requires non-empty 'bug_id'"
    dim = args.get("dimension", "")
    valid_dims = {"Input", "Environment", "Timing", "DataState", "Concurrency",
                  "Configuration", "DependencyVersion", "OsArch"}
    if dim and dim not in valid_dims:
        return False, f"Invalid dimension '{dim}'. Must be one of: {valid_dims}"
    return True, ""


def _validate_get_trigger_matrix(args: dict) -> tuple[bool, str]:
    bug_id = args.get("bug_id", "")
    if not bug_id:
        return False, "get_trigger_matrix requires non-empty 'bug_id'"
    return True, ""


def _validate_suggest_chain(args: dict) -> tuple[bool, str]:
    bug_ids = args.get("bug_ids", [])
    if not bug_ids:
        return False, "suggest_chain requires non-empty 'bug_ids' list"
    if len(bug_ids) > 100:
        return False, f"Too many bug_ids: {len(bug_ids)} (max 100)"
    hops = int(args.get("max_hops", 10))
    if hops < 1 or hops > MAX_HOPS:
        return False, f"max_hops must be 1-{MAX_HOPS}, got {hops}"
    return True, ""


def _validate_predict_fix_impact(args: dict) -> tuple[bool, str]:
    bug_id = args.get("bug_id", "")
    func = args.get("function_name", "")
    original = args.get("original_line", "")
    replacement = args.get("replacement_line", "")
    if not bug_id:
        return False, "predict_fix_impact requires non-empty 'bug_id'"
    if not func:
        return False, "predict_fix_impact requires non-empty 'function_name'"
    if not original:
        return False, "predict_fix_impact requires non-empty 'original_line'"
    if not replacement:
        return False, "predict_fix_impact requires non-empty 'replacement_line'"
    return True, ""


TOOL_VALIDATORS: dict[str, Callable] = {
    "exec_sandbox": _validate_exec_sandbox,
    "read_file": _validate_read_file,
    "fuzz_target": _validate_fuzz_target,
    "delta_debug": _validate_delta_debug,
    "diff_execute": _validate_diff_execute,
    "mine_invariants": _validate_mine_invariants,
    "run_mutations": _validate_run_mutations,
    "solve_reachability": _validate_solve_reachability,
    "explore_paths": _validate_explore_paths,
    "describe_trigger": _validate_describe_trigger,
    "get_trigger_matrix": _validate_get_trigger_matrix,
    "suggest_chain": _validate_suggest_chain,
    "predict_fix_impact": _validate_predict_fix_impact,
}


class ToolRegistry:
    """Plugin-based tool dispatch with retry, cache, circuit breaker, and rate limiting."""

    def __init__(self):
        self._tools: dict[str, ToolDefinition] = {}
        self._cache = ToolCache()
        self._breakers: dict[str, CircuitBreaker] = {}
        self._retry = RetryPolicy()
        self._stats: dict[str, dict] = {}
        self._rate_limit_counters: dict[str, list[float]] = defaultdict(list)

    def _check_rate_limit(self, name: str) -> tuple[bool, str]:
        """Enforce per-tool rate limiting within a sliding window."""
        now = time.time()
        window_start = now - RATE_LIMIT_WINDOW_SECS
        timestamps = self._rate_limit_counters[name]
        timestamps[:] = [t for t in timestamps if t > window_start]
        if len(timestamps) >= MAX_CALLS_PER_WINDOW:
            return False, f"Rate limit exceeded for {name}: {MAX_CALLS_PER_WINDOW} calls per {RATE_LIMIT_WINDOW_SECS}s"
        timestamps.append(now)
        return True, ""

    def register(self, tool: ToolDefinition) -> None:
        self._tools[tool.name] = tool
        self._breakers[tool.name] = CircuitBreaker()
        self._stats[tool.name] = {"calls": 0, "successes": 0, "failures": 0, "total_latency": 0.0}

    async def execute(self, name: str, args: dict) -> ToolResult:
        if name not in self._tools:
            return ToolResult(False, f"Unknown tool: {name}")

        tool = self._tools[name]

        validator = TOOL_VALIDATORS.get(name)
        if validator is not None:
            ok, reason = validator(args)
            if not ok:
                return ToolResult(False, f"Tool rejected by capability gate: {reason}")

        rate_ok, rate_reason = self._check_rate_limit(name)
        if not rate_ok:
            return ToolResult(False, f"Tool rejected by rate limit: {rate_reason}")

        breaker = self._breakers[name]

        if not breaker.allow_request():
            return ToolResult(False, f"Circuit breaker open for tool: {name}")

        # Check cache
        if tool.cache_ttl_secs > 0:
            cached = self._cache.get(name, args, tool.cache_ttl_secs)
            if cached:
                return cached

        # Execute with retry
        last_error = None
        for attempt in range(1, tool.max_retries + 2):
            try:
                t0 = time.perf_counter()
                result = await asyncio.wait_for(
                    tool.handler(args), timeout=tool.timeout_secs
                )
                elapsed = (time.perf_counter() - t0) * 1000

                self._stats[name]["calls"] += 1
                self._stats[name]["successes"] += 1
                self._stats[name]["total_latency"] += elapsed
                breaker.record_success()

                if tool.cache_ttl_secs > 0 and result.success:
                    self._cache.set(name, args, result)

                return result

            except asyncio.TimeoutError:
                last_error = f"Timeout after {tool.timeout_secs}s"
                logger.warning("tool_timeout", tool=name, attempt=attempt, timeout=tool.timeout_secs)
            except Exception as e:
                last_error = str(e)[:200]
                logger.warning("tool_error", tool=name, attempt=attempt, error=last_error)

            if attempt <= tool.max_retries:
                delay = self._retry.delay_for(attempt)
                await asyncio.sleep(delay)

        self._stats[name]["calls"] += 1
        self._stats[name]["failures"] += 1
        breaker.record_failure()
        return ToolResult(False, f"Tool '{name}' failed after {tool.max_retries + 1} attempts: {last_error}")

    def get_schema_for_llm(self) -> list[dict]:
        """Generate OpenAI-compatible function calling schema."""
        schemas = []
        for name, tool in self._tools.items():
            schemas.append({
                "type": "function",
                "function": {
                    "name": name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                },
            })
        return schemas

    def stats(self) -> dict:
        return {
            name: {
                **self._stats[name],
                "avg_latency_ms": round(
                    self._stats[name]["total_latency"] / max(self._stats[name]["calls"], 1), 1
                ),
                "breaker_state": self._breakers[name].state,
                "cache_hit_rate": round(self._cache.hit_rate, 3),
            }
            for name in self._tools
        }

    def invalidate_cache(self, tool_name: str | None = None) -> None:
        self._cache.invalidate(tool_name)
