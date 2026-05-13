# Phase 19: Bug Probability Prediction — ML on Historical Bugs

**Status**: NOT_STARTED
**Estimated Effort**: 12 hours
**Depends On**: Phase 18 (Pattern Database)
**Unblocks**: Phase 15 (Self-Configuring Swarm — allocation algorithm consumption)

---

## A1. What is being built?

A supervised ML model (XGBoost) trained on every confirmed bug in the Pattern Database. For every function in a new repository, the model predicts bug probability based on 10 features extracted from the CPG, git history, and historical bug density. The top-N highest-probability functions feed into the scout agent's investigation priority and the allocation algorithm's agent distribution, saving 30-50% of token budget by targeting where bugs are most likely.

---

## A2. Which specific gap does it fill?

**Gap ID**: INV-011 (from `invincible.md`)
**Current behavior**: Agents investigate randomly or based on CPG taint alone. A 3-line utility function gets the same investigation priority as a 200-line auth handler with 5 taint sources.
**Target behavior**: `auth.py:login()` gets 94% probability and is investigated first. `utils.py:format_date()` gets 3% and is deprioritized.

---

## A3. What is the success criteria?

| Metric | Target | Measurement |
|--------|--------|-------------|
| Top-10% probability functions contain bugs | >70% of all bugs found | Compare model ranking vs actual bug locations across 10 repos |
| Token savings | >30% vs unprioritized investigation | Compare tokens/verified-bug with and without probability guidance |
| Training time | <5 min for 10K findings | XGBoost fit() wall-clock |
| Inference time | <1ms per function | `model.predict_proba()` per function |
| Model size | <10MB | Serialized XGBoost model |
| Retraining cadence | After every 50 new confirmed bugs | Incremental or full retrain |

---

## A4. What is the priority and why?

**Priority**: 4th in the invincibility stack. Depends on Phase 18 (needs data to train on). Immediately unlocks smarter allocation in Phase 15.

---

## A5. What is NOT being built?

- NOT a deep learning model (XGBoost is sufficient for tabular features)
- NOT real-time prediction during code editing (batch prediction after CPG indexing)
- NOT per-line probability (per-function granularity)
- NOT a model that requires GPU (XGBoost runs on CPU in <5 min)
- NOT automated model retraining pipeline (manual trigger after 50 new bugs)

---

## B1. Integration point?

**Primary**: New Python module `swarm/probability.py`
**New files**:
- `swarm/probability.py` — BugProbabilityModel: train, predict, save, load
- `swarm/probability/features.py` — Feature extractor: 10 features from CPG + git

**Modified files**:
- `swarm/orchestrator.py` — After run: trigger retrain if 50+ new bugs since last train
- `agent/cli/wiring.py` — Assessment phase calls `model.predict_all()` and feeds top-N to scout

---

## B2. Data flow?

```
TRAINING (after every 50 new confirmed bugs):
  │
  ├─→ Query Pattern DB for all confirmed bugs with locations
  ├─→ For each bug location, extract features from CPG + git
  ├─→ Label: 1 (bug confirmed at this function)
  ├─→ For each non-buggy function (random sample), extract features
  ├─→ Label: 0 (no bug found)
  ├─→ Train XGBoost classifier
  └─→ Save model to ~/.bugswarm/probability_model.json

INFERENCE (at start of each run):
  │
  ├─→ CPG indexed → all functions extracted
  ├─→ For each function, extract 10 features
  ├─→ model.predict_proba(features) → probability 0.0-1.0
  ├─→ Rank functions by probability descending
  └─→ Scout agent receives top-20 functions as investigation priority
```

---

## B3. New types/schemas?

### Feature Vector

```python
@dataclass
class FunctionFeatures:
    function_name: str
    file_path: str
    # CPG-derived
    cyclomatic_complexity: int        # if/for/while count
    nesting_depth: int                # max AST depth
    parameter_count: int              # number of parameters
    lines_of_code: int                # function body length
    taint_source_count: int           # number of taint sources in function
    taint_sink_count: int             # number of taint sinks in function
    external_call_count: int          # calls to external libraries
    # Git-derived
    author_count: int                 # distinct authors who modified this function
    commit_frequency: float           # commits per week
    bug_density_historical: float     # bugs per line in this file (from Pattern DB)
    
    def to_array(self) -> list[float]:
        return [self.cyclomatic_complexity, self.nesting_depth, ...]
```

---

## C1. Core algorithm?

```
function train_model():
    X, y = [], []
    
    for bug in pattern_db.get_all_confirmed():
        features = extract_features(bug.location)
        X.append(features.to_array())
        y.append(1)  # bug
    
    # Negative samples: random non-buggy functions
    for func in random.sample(all_functions, len(y) * 2):
        if func not in buggy_functions:
            X.append(extract_features(func).to_array())
            y.append(0)
    
    model = XGBClassifier(
        n_estimators=100,
        max_depth=5,
        learning_rate=0.1,
        objective='binary:logistic'
    )
    model.fit(X, y)
    return model
```

---

## C6. Algorithmic Peak Analysis

### C6.1 Algorithm Inventory

| # | Component | Current Approach | Naive/Peak |
|---|-----------|-----------------|------------|
| 1 | Model selection | XGBoost | **PEAK** — best tabular classifier, handles missing values, feature importance built-in |
| 2 | Feature extraction | CPG + git log | **PEAK** — 10 features cover static, dynamic, and historical dimensions |
| 3 | Negative sampling | Random non-buggy functions | NAIVE → Stratified sampling by complexity (C6.2.1) |
| 4 | Retraining | Full retrain after 50 bugs | NAIVE → Incremental (C6.2.2) |

---

## D1. Aggressive unit tests? (10 tests)

| Test | Attack | Expected |
|------|--------|----------|
| `test_model_train_predict` | Train on 100 samples, predict on 20 | AUC >0.7 on training data |
| `test_feature_extraction` | Extract features from known function | All 10 features non-null |
| `test_model_serialization` | Save → load → predict same input | Identical predictions |
| `test_empty_training_data` | **AGGRESSIVE**: Train with 0 bugs | Returns prior probability (0.0). No crash. |
| `test_single_class_training` | **AGGRESSIVE**: All samples are bugs | Model trains. Predicts 1.0 for all. |
| `test_large_feature_values` | **AGGRESSIVE**: Complexity=999999, lines=999999 | Model handles. No overflow. |
| `test_feature_extraction_on_empty_function` | **AGGRESSIVE**: 0-line function | Features all 0. No crash. |
| `test_prediction_ranking` | 10 functions, 2 known bugs | Top-2 predictions are the buggy functions |
| `test_incremental_retrain_trigger` | 49 bugs → no retrain. 50 → retrain. | Threshold enforced |
| `test_model_save_load_roundtrip` | Train, save, load, predict | Feature importance preserved |

---

## D3. Extreme gate test — The Prediction Gauntlet? (6 attack vectors)

1. 100 known-buggy + 400 clean functions. Top-10% predictions must contain >70% of bugs.
2. Model trained on repo A, tested on repo B. Cross-repo generalization.
3. Retrain 5 times with incremental data. AUC must not degrade.
4. 10K-function inference in <1 second.
5. Missing features (git not available) → model still predicts.
6. Corrupted model file → falls back to uniform prior.

---

## Gate Receipt

```json
{"phase": 19, "gate": "prediction_gauntlet", "status": "PENDING"}
```
