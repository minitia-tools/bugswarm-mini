"""Structured Output Parser — multi-strategy parse chain with JSON Schema validation.

Rule: Parse, don't validate. LLM output is parsed through a chain of strategies.
Native function calling → JSON fences → inline JSON → freeform heuristics (last resort).
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from enum import Enum
from typing import Any

import structlog

logger = structlog.get_logger(__name__)


class OutputType(str, Enum):
    FINDING = "finding"
    TOOL_CALL = "tool_call"
    POC = "poc"
    UNKNOWN = "unknown"


@dataclass
class ParsedFinding:
    claim: str
    location: str
    mechanism: str = ""
    prediction: str = ""
    severity: int = 5
    verified: bool = False
    source: str = "unknown"  # json_block, inline_json, freeform, native_function

    def to_dict(self) -> dict:
        return {
            "type": "finding",
            "claim": self.claim,
            "location": self.location,
            "mechanism": self.mechanism,
            "prediction": self.prediction,
            "severity_estimate": self.severity,
            "verified": self.verified,
        }


@dataclass
class ParsedToolCall:
    tool_name: str
    args: dict[str, Any]

    def to_dict(self) -> dict:
        return {"type": "tool", "tool": self.tool_name, "args": self.args}


@dataclass
class ParsedPoC:
    code: str
    prediction: str = ""

    def to_dict(self) -> dict:
        return {"type": "poc", "code": self.code, "prediction": self.prediction}


@dataclass
class ParsedOutput:
    output_type: OutputType
    finding: ParsedFinding | None = None
    tool_call: ParsedToolCall | None = None
    poc: ParsedPoC | None = None
    raw: str = ""


# JSON Schema for findings (validated on every accepted finding)
FINDING_SCHEMA = {
    "type": "object",
    "required": ["claim", "location"],
    "properties": {
        "type": {"const": "finding"},
        "claim": {"type": "string", "minLength": 5, "maxLength": 500},
        "location": {"type": "string"},
        "mechanism": {"type": "string"},
        "prediction": {"type": "string"},
        "severity_estimate": {"type": "integer", "minimum": 1, "maximum": 10},
        "verified": {"type": "boolean"},
    },
}


class OutputParser:
    """Multi-strategy parser with fallback chain.

    Strategy order (first success wins):
    1. Native function calling (OpenAI tool_calls, Anthropic tool_use)
    2. JSON-in-code-fence (```json {...} ```)
    3. Inline JSON ({"type":"finding",...} in freeform text)
    4. Freeform heuristics (keyword matching, last resort)
    """

    # Location regex: matches "file.py:123" or "path/to/file.py:456"
    LOCATION_PATTERN = re.compile(r'(\S+\.(?:py|js|go|java|rb|rs|c|cpp|h|ts|tsx)):(\d+)')

    # Freeform confirmation keywords
    CONFIRMED_KEYWORDS = [
        "bug confirmed", "confirmed by sandbox", "confirmed!",
        "vulnerability confirmed", "injection confirmed", "**confirmed",
        "successfully exploited", "exploit confirmed", "verified by sandbox",
    ]

    def parse(self, content: str) -> ParsedOutput:
        """Parse LLM output through the strategy chain."""
        # Strategy 1: Native function calling (handled at gateway level, pass-through here)
        # For now, skip to JSON parsing

        # Strategy 2: JSON-in-code-fence
        result = self._parse_json_fence(content)
        if result:
            return result

        # Strategy 3: Inline JSON
        result = self._parse_inline_json(content)
        if result:
            return result

        # Strategy 4: Freeform heuristics
        return self._parse_freeform(content)

    def _parse_json_fence(self, content: str) -> ParsedOutput | None:
        """Extract from ```json ... ``` blocks."""
        remaining = content
        while '```json' in remaining:
            try:
                start = remaining.index('```json') + 7
                end = remaining.index('```', start) if '```' in remaining[start:] else len(remaining)
                json_str = remaining[start:end].strip()
                remaining = remaining[end+3:] if end+3 < len(remaining) else ""

                data = json.loads(json_str)
                return self._classify_json(data, content, "json_block")
            except (json.JSONDecodeError, ValueError, IndexError):
                continue
        return None

    def _parse_inline_json(self, content: str) -> ParsedOutput | None:
        """Extract from raw {"type":"finding"...} in text."""
        remaining = content
        while '{"type"' in remaining:
            try:
                start = remaining.index('{"type"')
                depth = 0
                end = start
                for i, c in enumerate(remaining[start:]):
                    if c == '{': depth += 1
                    elif c == '}':
                        depth -= 1
                        if depth == 0:
                            end = start + i + 1
                            break
                json_str = remaining[start:end]
                remaining = remaining[end:]

                data = json.loads(json_str)
                result = self._classify_json(data, content, "inline_json")
                if result:
                    return result
            except (json.JSONDecodeError, ValueError):
                continue
        return None

    def _classify_json(self, data: dict, raw: str, source: str) -> ParsedOutput | None:
        """Classify a parsed JSON dict as finding, tool call, or PoC."""
        typ = data.get("type", "")

        if typ == "finding":
            finding = ParsedFinding(
                claim=data.get("claim", ""),
                location=data.get("location", ""),
                mechanism=data.get("mechanism", ""),
                prediction=data.get("prediction", ""),
                severity=data.get("severity_estimate", 5),
                verified=data.get("verified", False),
                source=source,
            )
            return ParsedOutput(OutputType.FINDING, finding=finding, raw=raw)

        elif typ == "tool":
            return ParsedOutput(
                OutputType.TOOL_CALL,
                tool_call=ParsedToolCall(
                    tool_name=data.get("tool", ""),
                    args=data.get("args", {}),
                ),
                raw=raw,
            )

        elif typ == "poc":
            return ParsedOutput(
                OutputType.POC,
                poc=ParsedPoC(
                    code=data.get("code", ""),
                    prediction=data.get("prediction", ""),
                ),
                raw=raw,
            )

        return None

    def _parse_freeform(self, content: str) -> ParsedOutput:
        """Last resort: keyword-based freeform parsing for bug confirmations."""
        lower = content.lower()

        # Check for confirmation keywords
        is_confirmed = any(k in lower for k in self.CONFIRMED_KEYWORDS)
        if not is_confirmed:
            return ParsedOutput(OutputType.UNKNOWN, raw=content)

        # Extract claim from first significant line
        lines = [l.strip() for l in content.split('\n')
                 if l.strip() and not l.strip().startswith('```')]

        claim = ""
        bug_keywords = ["bug", "vulnerability", "injection", "overflow",
                       "bypass", "leak", "traversal", "confirmed", "rce"]
        for line in lines:
            lw = line.lower()
            if any(kw in lw for kw in bug_keywords):
                claim = line.strip('# *-').strip()
                if len(claim) > 20:
                    break
        if not claim:
            claim = lines[0][:200] if lines else content[:200]

        # Extract location
        loc_match = self.LOCATION_PATTERN.search(content)
        location = f"{loc_match.group(1)}:{loc_match.group(2)}" if loc_match else "unknown"

        finding = ParsedFinding(
            claim=claim[:200],
            location=location,
            mechanism="Detected via freeform confirmation",
            severity=7,  # Conservative estimate for freeform
            verified=True,
            source="freeform",
        )
        return ParsedOutput(OutputType.FINDING, finding=finding, raw=content)

    @staticmethod
    def validate_finding(finding: ParsedFinding) -> list[str]:
        """Validate a finding against the schema. Returns list of error messages."""
        errors = []
        if not finding.claim or len(finding.claim) < 5:
            errors.append("Claim too short (min 5 chars)")
        if not finding.location:
            errors.append("Location required")
        if finding.severity < 1 or finding.severity > 10:
            errors.append(f"Severity {finding.severity} out of range (1-10)")
        return errors
