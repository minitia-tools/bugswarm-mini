"""Feature Extractor — 10 features from CPG, git, and PatternDB.

C6.2 PEAK: Static (7 CPG features) + Historical (2 git features) + Cross-run (1 PatternDB feature).
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path

import structlog

logger = structlog.get_logger(__name__)


@dataclass
class FunctionFeatures:
    function_name: str
    file_path: str

    # CPG-derived (static analysis)
    cyclomatic_complexity: int = 0
    nesting_depth: int = 0
    parameter_count: int = 0
    lines_of_code: int = 0
    taint_source_count: int = 0
    taint_sink_count: int = 0
    external_call_count: int = 0

    # Git-derived (historical)
    author_count: int = 0
    commit_frequency: float = 0.0

    # PatternDB-derived (cross-run learning)
    bug_density_historical: float = 0.0

    def to_array(self) -> list[float]:
        return [
            float(self.cyclomatic_complexity),
            float(self.nesting_depth),
            float(self.parameter_count),
            float(self.lines_of_code),
            float(self.taint_source_count),
            float(self.taint_sink_count),
            float(self.external_call_count),
            float(self.author_count),
            float(self.commit_frequency),
            float(self.bug_density_historical),
        ]

    @classmethod
    def feature_names(cls) -> list[str]:
        return [
            "cyclomatic_complexity",
            "nesting_depth",
            "parameter_count",
            "lines_of_code",
            "taint_source_count",
            "taint_sink_count",
            "external_call_count",
            "author_count",
            "commit_frequency",
            "bug_density_historical",
        ]

    @classmethod
    def from_cpg_node(cls, node: dict) -> FunctionFeatures:
        """Build features from a CPG function node dict."""
        return cls(
            function_name=node.get("name", "unknown"),
            file_path=node.get("file", "unknown"),
            cyclomatic_complexity=node.get("cyclomatic_complexity", 0),
            nesting_depth=node.get("nesting_depth", 0),
            parameter_count=node.get("parameter_count", 0),
            lines_of_code=node.get("lines_of_code", 0),
            taint_source_count=node.get("taint_source_count", 0),
            taint_sink_count=node.get("taint_sink_count", 0),
            external_call_count=node.get("external_call_count", 0),
        )


class FeatureExtractor:
    """Extracts 10 features for ML bug probability prediction."""

    @classmethod
    def extract_from_git(cls, file_path: str, repo_root: Path) -> tuple[int, float]:
        """Extract author_count and commit_frequency from git log.

        Returns (author_count, commits_per_week).
        Gracefully returns (0, 0.0) if git is unavailable.
        """
        try:
            # Author count
            result = subprocess.run(
                ["git", "-C", str(repo_root), "log", "--follow", "--format=%an", "--", file_path],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if result.returncode == 0 and result.stdout.strip():
                authors = set(result.stdout.strip().split("\n"))
                author_count = len(authors)
            else:
                author_count = 0

            # Commit frequency (per week over last 90 days)
            result2 = subprocess.run(
                [
                    "git",
                    "-C",
                    str(repo_root),
                    "log",
                    "--follow",
                    "--format=%ct",
                    "--since=90.days.ago",
                    "--",
                    file_path,
                ],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if result2.returncode == 0 and result2.stdout.strip():
                commit_count = len(result2.stdout.strip().split("\n"))
                commit_frequency = commit_count / 13.0  # 13 weeks in 90 days
            else:
                commit_frequency = 0.0

            return author_count, round(commit_frequency, 2)
        except (subprocess.TimeoutExpired, FileNotFoundError, OSError):
            return 0, 0.0

    @classmethod
    def extract_bug_density(cls, file_path: str, hotspot_tracker=None) -> float:
        """Extract historical bug density from HotspotTracker."""
        if hotspot_tracker is None:
            return 0.0
        return round(hotspot_tracker.get_score(file_path), 2)

    @classmethod
    def enrich(cls, features: FunctionFeatures, repo_root: Path, hotspot_tracker=None) -> FunctionFeatures:
        """Enrich features with git + historical data."""
        authors, freq = cls.extract_from_git(features.file_path, repo_root)
        features.author_count = authors
        features.commit_frequency = freq
        features.bug_density_historical = cls.extract_bug_density(features.file_path, hotspot_tracker)
        return features
