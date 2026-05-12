"""Tool Registry — plugin-based dispatch with retry, cache, circuit breaker.

Rule: No raw subprocess.run(). Every external call goes through a ToolDefinition
with timeout, retry budget, and graceful degradation path.
"""

from __future__ import annotations

import asyncio
import hashlib
import random
import time
from collections import OrderedDict
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


class ToolRegistry:
    """Plugin-based tool dispatch with retry, cache, and circuit breaker."""

    def __init__(self):
        self._tools: dict[str, ToolDefinition] = {}
        self._cache = ToolCache()
        self._breakers: dict[str, CircuitBreaker] = {}
        self._retry = RetryPolicy()
        self._stats: dict[str, dict] = {}

    def register(self, tool: ToolDefinition) -> None:
        self._tools[tool.name] = tool
        self._breakers[tool.name] = CircuitBreaker()
        self._stats[tool.name] = {"calls": 0, "successes": 0, "failures": 0, "total_latency": 0.0}

    async def execute(self, name: str, args: dict) -> ToolResult:
        if name not in self._tools:
            return ToolResult(False, f"Unknown tool: {name}")

        tool = self._tools[name]
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
