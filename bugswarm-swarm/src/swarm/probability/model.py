"""Bug Probability Model — XGBoost classifier on 10 features.

C6.2 PEAK ALGORITHMS:
  C6.2.1  Stratified negative sampling (complexity-proportional allocation)
  C6.2.2  Incremental warm-start retraining
  C6.2.3  Overfitting prevention (L1+L2 reg, subsampling, early stopping)

Agent bug detection priority is driven by model predictions.
Top-20 highest-probability functions are injected into the scout's initial prompt.
"""

from __future__ import annotations

import json
import os
import random
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import numpy as np
import structlog

from .features import FunctionFeatures, FeatureExtractor

logger = structlog.get_logger(__name__)

# Lazy imports to allow graceful degradation
_XGB_AVAILABLE = False
_xgb_import_error: str | None = None
try:
    import xgboost as xgb
    _XGB_AVAILABLE = True
except ImportError as e:
    _xgb_import_error = str(e)

_SKLEARN_AVAILABLE = False
_sklearn_import_error: str | None = None
try:
    from sklearn.model_selection import train_test_split
    from sklearn.metrics import roc_auc_score
    _SKLEARN_AVAILABLE = True
except ImportError as e:
    _sklearn_import_error = str(e)


@dataclass
class PredictionResult:
    function_name: str
    file_path: str
    probability: float
    features: FunctionFeatures
    rank: int = 0

    def to_dict(self) -> dict:
        return {
            "function": self.function_name,
            "file": self.file_path,
            "probability": round(self.probability, 4),
            "rank": self.rank,
            "features": self.features.to_array(),
        }


@dataclass
class RepoPrediction:
    repo_path: str
    total_functions: int
    predictions: list[PredictionResult]
    model_version: str
    prediction_time_ms: float
    feature_extraction_time_ms: float = 0.0

    def get_top(self, n: int = 20) -> list[PredictionResult]:
        return self.predictions[:n]

    def to_prompt(self, top_n: int = 20, threshold: float = 0.5) -> str:
        """Generate PRIORITY INVESTIGATION TARGETS block for scout prompt."""
        entries = []
        for p in self.predictions[:top_n]:
            if p.probability < threshold:
                break
            reason = self._describe_features(p.features)
            entries.append(f"{p.file_path}:{p.function_name}() — {p.probability:.0%} ({reason})")

        if not entries:
            return ""

        return (
            "\nPRIORITY INVESTIGATION TARGETS (predicted bug probability):\n"
            + "\n".join(entries[:top_n])
        )

    @staticmethod
    def _describe_features(ff: FunctionFeatures) -> str:
        parts = []
        if ff.cyclomatic_complexity > 5:
            parts.append(f"complexity {ff.cyclomatic_complexity}")
        if ff.taint_source_count > 0:
            parts.append(f"{ff.taint_source_count} taint sources")
        if ff.taint_sink_count > 0:
            parts.append(f"{ff.taint_sink_count} taint sinks")
        if ff.external_call_count > 3:
            parts.append(f"{ff.external_call_count} external calls")
        if ff.bug_density_historical > 1:
            parts.append("historical hotspot")
        if ff.commit_frequency > 2:
            parts.append("high churn")
        if not parts:
            parts.append(f"{ff.lines_of_code} LOC, depth {ff.nesting_depth}")
        return ", ".join(parts) if parts else "low complexity"


class BugProbabilityModel:
    """XGBoost-based bug probability prediction.

    Trained on 10 features (7 CPG + 2 git + 1 historical).
    Predicts P(bug) per function ranked descending.
    """

    def __init__(self, model_path: str = "~/.bugswarm/probability_model.json"):
        self.model_path = os.path.expanduser(model_path)
        self.model: Any = None  # xgb.XGBClassifier | None
        self.feature_names = FunctionFeatures.feature_names()
        self.training_date: str | None = None
        self.sample_count: int = 0
        self.auc_score: float = 0.0
        self._ml_available = _XGB_AVAILABLE and _SKLEARN_AVAILABLE
        self._load_if_exists()

    # ── Training ──────────────────────────────────────────────────────────

    def train(self, confirmed_bugs: list[dict], all_functions: list[dict],
              repo_root: Path | None = None, hotspot_tracker: Any = None,
              incremental: bool = False) -> dict:
        """Train model from confirmed bugs list and all functions from CPG.

        Uses stratified negative sampling (C6.2.1) and overfitting prevention (C6.2.3).
        Supports incremental warm-start retraining (C6.2.2).

        Args:
            confirmed_bugs: List of dicts with at least {location, claim, severity_estimate}
            all_functions: List of CPG function node dicts
            repo_root: Path for git feature extraction
            hotspot_tracker: HotspotTracker for historical bug density
            incremental: If True, warm-start from existing model

        Returns:
            dict with status, samples, positive, negative, auc, features
        """
        if not self._ml_available:
            return {"status": "ml_unavailable", "reason": f"xgboost={_XGB_AVAILABLE}, sklearn={_SKLEARN_AVAILABLE}"}

        if len(confirmed_bugs) < 10:
            return {"status": "skipped", "reason": f"Need >=10 bugs, have {len(confirmed_bugs)}"}

        X, y = [], []

        # 1. Gather positive samples from confirmed bugs
        for bug in confirmed_bugs:
            loc = bug.get("location", "")
            file_path = loc.split(":")[0] if ":" in loc else loc
            func_name = loc.split(":")[-1] if ":" in loc else "unknown"
            feat = self._extract_features_for_bug(bug, file_path, func_name,
                                                  repo_root, hotspot_tracker)
            X.append(feat.to_array())
            y.append(1)

        # 2. Gather negative samples via stratified complexity sampling (C6.2.1)
        buggy_files = {b.get("location", "").split(":")[0] for b in confirmed_bugs if b.get("location")}
        neg_count = min(len(y) * 2, len(all_functions) - len(y))
        if neg_count > 0:
            strata = self._stratify_by_complexity_quantiles(all_functions, n_strata=5)
            # Allocate negatives proportional to bug density per stratum, min 5% each
            total_bugs = len(y)
            if total_bugs > 0:
                # Find which strata the bugs are in
                bug_strata: dict[int, int] = {}
                for bug in confirmed_bugs:
                    loc = bug.get("location", "")
                    file_path = loc.split(":")[0] if ":" in loc else loc
                    func_name = loc.split(":")[-1] if ":" in loc else "unknown"
                    match = next((f for f in all_functions
                                  if f.get("file") == file_path and f.get("name") == func_name), None)
                    if match:
                        cc = match.get("cyclomatic_complexity", 0)
                        # Find which quantile stratum
                        for i, s in enumerate(strata):
                            if any(f.get("name") == match.get("name") and f.get("file") == match.get("file") for f in s):
                                bug_strata[i] = bug_strata.get(i, 0) + 1
                                break

                neg_per_stratum = []
                for i in range(len(strata)):
                    bug_count = bug_strata.get(i, 0)
                    if total_bugs > 0:
                        allocated = max(int(neg_count * 0.05), int(neg_count * bug_count / total_bugs))
                    else:
                        allocated = neg_count // len(strata)
                    neg_per_stratum.append(allocated)

                # Normalize to hit target
                total_allocated = sum(neg_per_stratum)
                if total_allocated > 0:
                    neg_per_stratum = [int(n * neg_count / total_allocated) for n in neg_per_stratum]
            else:
                neg_per_stratum = [neg_count // len(strata)] * len(strata)

            for i, stratum in enumerate(strata):
                eligible = [f for f in stratum if f.get("file", "") not in buggy_files]
                n = min(neg_per_stratum[i] if i < len(neg_per_stratum) else 0, len(eligible))
                if n > 0:
                    sampled = random.sample(eligible, n)
                    for func in sampled:
                        feat = FunctionFeatures.from_cpg_node(func)
                        if repo_root:
                            feat = FeatureExtractor.enrich(feat, repo_root, hotspot_tracker)
                        X.append(feat.to_array())
                        y.append(0)

        # 3. Single-class guard: if all y=1, add synthetic negatives
        if len(set(y)) < 2:
            logger.warning("single_class_training", positives=len(y))
            synthetic = np.zeros((max(1, len(y)), 10))
            X_arr = np.vstack([np.array(X), synthetic])
            y_arr = np.hstack([np.array(y), np.zeros(len(synthetic))])
        else:
            X_arr = np.array(X)
            y_arr = np.array(y)

        # 4. NaN guard
        nan_mask = np.isnan(X_arr)
        if nan_mask.any():
            logger.warning("nan_in_features", count=int(nan_mask.sum()))
            X_arr = np.nan_to_num(X_arr, nan=0.0)

        # 5. Train/test split
        try:
            X_train, X_val, y_train, y_val = train_test_split(
                X_arr, y_arr, test_size=0.2, stratify=y_arr, random_state=42,
            )
        except ValueError:
            # Not enough samples for stratify
            X_train, X_val, y_train, y_val = train_test_split(
                X_arr, y_arr, test_size=0.2, random_state=42,
            )

        # 6. Build/update model
        use_warm_start = incremental and self.model is not None
        previous_booster = None
        if use_warm_start:
            try:
                previous_booster = self.model.get_booster()
            except Exception:
                use_warm_start = False

        self.model = xgb.XGBClassifier(
            n_estimators=100,
            max_depth=5,
            learning_rate=0.1,
            objective='binary:logistic',
            eval_metric='logloss',
            early_stopping_rounds=10,
            reg_lambda=1.0,       # L2 regularization (C6.2.3)
            reg_alpha=0.5,        # L1 regularization (C6.2.3)
            subsample=0.8,        # Row sampling (C6.2.3)
            colsample_bytree=0.8, # Feature sampling (C6.2.3)
            min_child_weight=5,   # Min samples per leaf (C6.2.3)
            random_state=42,
            verbosity=0,
        )

        fit_kwargs = {
            "eval_set": [(X_val, y_val)],
            "verbose": False,
        }
        if use_warm_start and previous_booster is not None:
            fit_kwargs["xgb_model"] = previous_booster

        self.model.fit(X_train, y_train, **fit_kwargs)

        # 7. Evaluate
        y_pred = self.model.predict_proba(X_val)[:, 1]
        self.auc_score = float(roc_auc_score(y_val, y_pred))
        self.sample_count = len(y_arr)
        self.training_date = datetime.now(timezone.utc).isoformat()

        # 8. Save
        self.save()

        logger.info("model_trained", samples=self.sample_count,
                     positive=sum(1 for v in y if v == 1),
                     negative=sum(1 for v in y if v == 0),
                     auc=round(self.auc_score, 4))

        return {
            "status": "trained",
            "samples": self.sample_count,
            "positive": sum(1 for v in y if v == 1),
            "negative": sum(1 for v in y if v == 0),
            "auc": round(self.auc_score, 4),
            "features": self.get_feature_importance(),
        }

    # ── Inference ─────────────────────────────────────────────────────────

    def predict(self, features: FunctionFeatures) -> PredictionResult:
        """Predict bug probability for a single function."""
        if not self._ml_available or self.model is None:
            return PredictionResult(
                function_name=features.function_name,
                file_path=features.file_path,
                probability=0.0,
                features=features,
                rank=0,
            )
        arr = features.to_array()
        # Replace NaN with 0 (XGBoost handles in native code, but numpy-safe)
        arr = np.nan_to_num(np.array(arr, dtype=np.float32), nan=0.0)
        try:
            proba = float(self.model.predict_proba([arr])[0, 1])
        except Exception:
            proba = 0.0
        return PredictionResult(
            function_name=features.function_name,
            file_path=features.file_path,
            probability=proba,
            features=features,
            rank=0,
        )

    def predict_all(self, functions: list[FunctionFeatures]) -> RepoPrediction:
        """Predict for all functions, return ranked by probability."""
        t0 = time.perf_counter()
        if not self._ml_available or self.model is None or not functions:
            predictions = [
                PredictionResult(f.function_name, f.file_path, 0.0, f, i + 1)
                for i, f in enumerate(functions)
            ]
        else:
            # Batch prediction
            arrays = []
            for f in functions:
                arr = np.nan_to_num(np.array(f.to_array(), dtype=np.float32), nan=0.0)
                arrays.append(arr)
            X = np.array(arrays, dtype=np.float32)
            try:
                probas = self.model.predict_proba(X)[:, 1]
            except Exception:
                probas = np.zeros(len(functions))

            predictions = [
                PredictionResult(f.function_name, f.file_path, float(p), f, 0)
                for f, p in zip(functions, probas)
            ]

        predictions.sort(key=lambda p: p.probability, reverse=True)
        for i, p in enumerate(predictions):
            p.rank = i + 1

        elapsed = (time.perf_counter() - t0) * 1000
        top = predictions[0].probability if predictions else 0.0

        logger.info("prediction_complete", functions=len(functions),
                     top_score=round(top, 4), elapsed_ms=round(elapsed, 1))

        return RepoPrediction(
            repo_path="",
            total_functions=len(functions),
            predictions=predictions,
            model_version=self.training_date or "untrained",
            prediction_time_ms=elapsed,
        )

    # ── Feature Importance ────────────────────────────────────────────────

    def get_feature_importance(self) -> dict[str, float]:
        """Return feature importance scores. Sum ≈ 1.0."""
        if self.model is None or not hasattr(self.model, 'feature_importances_'):
            return {}
        importance = self.model.feature_importances_
        return {
            name: round(float(imp), 4)
            for name, imp in zip(self.feature_names, importance)
        }

    # ── Persistence ───────────────────────────────────────────────────────

    def save(self) -> None:
        """Save model to disk. Atomic write via temp file then rename."""
        if self.model is None:
            return
        path = Path(self.model_path)
        path.parent.mkdir(parents=True, exist_ok=True)

        # XGBoost detects format by file extension, so temp must end in .json
        tmp = path.parent / (path.name + ".tmp.json")
        try:
            self.model.save_model(str(tmp))
            tmp.rename(path)

            # Metadata
            meta = {
                "training_date": self.training_date,
                "sample_count": self.sample_count,
                "auc_score": round(self.auc_score, 4),
                "feature_names": self.feature_names,
                "ml_available": self._ml_available,
            }
            meta_path = path.with_name(path.name + ".meta")
            meta_tmp = meta_path.with_name(meta_path.name + ".tmp")
            meta_tmp.write_text(json.dumps(meta))
            meta_tmp.rename(meta_path)

            logger.info("model_saved", path=str(path), auc=round(self.auc_score, 4))
        except Exception as e:
            logger.error("model_save_failed", error=str(e)[:200])

    def _load_if_exists(self) -> None:
        """Load model from disk if it exists."""
        path = Path(self.model_path)
        if not path.exists():
            return

        if not self._ml_available:
            logger.warning("model_load_skipped", reason=f"xgboost={_XGB_AVAILABLE}, sklearn={_SKLEARN_AVAILABLE}")
            return

        try:
            self.model = xgb.XGBClassifier()
            self.model.load_model(str(path))
            meta_path = path.with_name(path.name + ".meta")
            if meta_path.exists():
                meta = json.loads(meta_path.read_text())
                self.training_date = meta.get("training_date")
                self.sample_count = meta.get("sample_count", 0)
                self.auc_score = meta.get("auc_score", 0.0)
            logger.info("model_loaded", training_date=self.training_date,
                         auc=round(self.auc_score, 4), samples=self.sample_count)
        except Exception as e:
            logger.error("model_corrupted", error=str(e)[:200])
            self.model = None
            # Try to clean up corrupted file
            try:
                path.unlink(missing_ok=True)
                path.with_name(path.name + ".meta").unlink(missing_ok=True)
            except Exception:
                pass

    # ── Helpers ───────────────────────────────────────────────────────────

    def _extract_features_for_bug(self, bug: dict, file_path: str,
                                   func_name: str, repo_root: Path | None,
                                   hotspot_tracker: Any) -> FunctionFeatures:
        """Build FunctionFeatures from a bug dict + CPG data."""
        # Try to get CPG features from bug metadata first
        feat = FunctionFeatures(
            function_name=func_name,
            file_path=file_path,
            cyclomatic_complexity=bug.get("cyclomatic_complexity", 0),
            nesting_depth=bug.get("nesting_depth", 0),
            parameter_count=bug.get("parameter_count", 0),
            lines_of_code=bug.get("lines_of_code", 0),
            taint_source_count=bug.get("taint_source_count", 0),
            taint_sink_count=bug.get("taint_sink_count", 0),
            external_call_count=bug.get("external_call_count", 0),
        )

        if repo_root:
            feat = FeatureExtractor.enrich(feat, repo_root, hotspot_tracker)
        return feat

    @staticmethod
    def _stratify_by_complexity_quantiles(functions: list[dict],
                                           n_strata: int = 5) -> list[list[dict]]:
        """Group functions into n_strata by cyclomatic complexity quantiles.

        Uses percentile-based stratification for proportional representation.
        """
        if not functions:
            return [[] for _ in range(n_strata)]

        complexities = [f.get("cyclomatic_complexity", 0) for f in functions]
        quantiles = np.percentile(complexities, np.linspace(0, 100, n_strata + 1))

        strata: list[list[dict]] = [[] for _ in range(n_strata)]
        for func in functions:
            cc = func.get("cyclomatic_complexity", 0)
            bucket = n_strata - 1  # default to highest
            for i in range(n_strata):
                if cc <= quantiles[i + 1]:
                    bucket = i
                    break
            strata[bucket].append(func)

        return strata
