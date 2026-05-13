# Phase 19: Bug Probability Prediction — ML on Historical Bugs

**Status**: NOT_STARTED
**Estimated Effort**: 12 hours
**Depends On**: Phase 18 (Pattern Database — needs training data)
**Unblocks**: Phase 15 (Self-Configuring Swarm allocation), Phase 21 (Taint-guided fuzzer seed selection)

---

## A1. What is being built?

XGBoost classifier trained on every confirmed bug in the Pattern Database. For every function in a new repo, predicts bug probability from 10 CPG+git features. Top-N high-probability functions feed scout agent investigation priority and allocation algorithm agent distribution. Saves 30-50% of token budget.

## A2. Which gap does it fill?

**Gap ID**: INV-011. Current: agents investigate randomly or by CPG taint. Target: `auth.py:login()` gets 94% probability, investigated first. `utils.py:format_date()` gets 3%.

## A3. Success criteria?

| Metric | Target |
|--------|--------|
| Top-10% functions contain bugs | >70% of all bugs found |
| Token savings | >30% vs unprioritized |
| Training time | <5min for 10K samples |
| Inference time | <1ms per function |
| Model size | <10MB |
| Retraining | After every 50 new confirmed bugs |

## A4. Priority?

4th in invincibility stack. Depends on Phase 18 (needs historical data). Unlocks Phase 15 allocation optimization and Phase 21 seed selection.

## A5. Scope boundary?

NOT: deep learning (XGBoost sufficient for tabular features), real-time prediction (batch after CPG), per-line granularity (per-function), GPU training (CPU <5min), automated retraining pipeline (manual trigger).

---

## B1. Integration point?

**New files**: `swarm/probability.py`, `swarm/probability/features.py`
**Modified**: `swarm/orchestrator.py` (post-run: trigger retrain if 50+ new bugs), `agent/cli/wiring.py` (pre-run: predict_all, feed top-N to scout)

## B2. Data flow?

```
TRAINING: Pattern DB → extract 10 features per function → label (1=bug, 0=clean) → XGBoost.fit() → save model
INFERENCE: CPG indexed → extract features per function → model.predict_proba() → rank → top-N to scout agent
```

## B3. New types?

`FunctionFeatures`: cyclomatic_complexity, nesting_depth, parameter_count, lines_of_code, taint_source_count, taint_sink_count, external_call_count, author_count, commit_frequency, bug_density_historical

## B4. Modified modules?

`swarm/probability.py` (NEW), `swarm/probability/features.py` (NEW), `swarm/orchestrator.py` (retrain trigger), `agent/cli/wiring.py` (prediction feed)

## B5. Dependencies?

| Dep | Version | Purpose |
|-----|---------|---------|
| `xgboost` | >=2.0 | Classifier |
| `scikit-learn` | >=1.4 | Train/test split, metrics |
| `numpy` | >=1.26 | Feature arrays |

---

## C1. Core algorithm?

```
train_model():
    X, y = [], []
    for bug in pattern_db.get_all_confirmed():
        X.append(extract_features(bug.location).to_array())
        y.append(1)
    for func in random.sample(all_functions, len(y)*2):
        if func not in buggy: X.append(extract_features(func).to_array()); y.append(0)
    model = XGBClassifier(n_estimators=100, max_depth=5, learning_rate=0.1)
    model.fit(X, y)
    return model
```

**Complexity**: XGBoost O(N × D × T) where N=samples, D=features, T=trees. 10K samples × 10 features × 100 trees = <5min CPU.

## C2. Failure modes?

| Failure | Handling | Recovery |
|---------|----------|----------|
| <50 bugs in DB | Skip training. Use uniform prior (all functions equal probability) | Retrain when threshold met |
| Single-class data (all bugs) | Model trains. Predicts 1.0. | Flag WARN. Need negative samples. |
| Feature extraction fails (git unavailable) | Git features set to 0. CPG features still valid. | Partial prediction still useful |
| Model file corrupted | Delete corrupted file. Revert to uniform prior. | Retrain on next trigger |
| NaN in features | Replace NaN with 0. Log WARN. | Isolated to that function |

## C3. Edge cases?

| Edge Case | Behavior |
|-----------|----------|
| Empty function (0 lines) | All features 0. Low probability. |
| Function with only comments | Lines >0 but complexity=0. Low probability. |
| Generated code (100K lines, 0 authors) | High LOC, low authors → unusual feature combo. Model handles. |
| New language (no historical bugs) | Bug density = 0. Other features still predictive. |
| First run (empty DB, no model) | Uniform prior. All functions equal. |

## C4. Concurrency?

Model is read-only during inference. Single-writer during training (after swarm terminates). No race conditions.

## C5. Performance budget?

| Metric | Target |
|--------|--------|
| Training | <5min for 10K samples |
| Inference | <1ms per function |
| Model load | <100ms from disk |
| Feature extraction | <10ms per function (CPG query + git log) |

---

## C6. Algorithmic Peak Analysis

### C6.1 Algorithm Inventory

| # | Component | Approach | Status | Peak Reference |
|---|-----------|----------|--------|----------------|
| 1 | Model | XGBoost | **PEAK** | Best tabular classifier, handles missing values |
| 2 | Features | 10-feature vector from CPG+git | **PEAK** | Covers static, dynamic, historical dimensions |
| 3 | Negative sampling | Random non-buggy functions | NAIVE → Stratified by complexity | C6.2.1 |
| 4 | Retraining | Full retrain after 50 new bugs | NAIVE → Incremental update | C6.2.2 |

### C6.2.1 Stratified Negative Sampling

**Peak**: Sample negative examples proportional to function complexity strata. Without stratification, model sees mostly simple non-buggy functions (majority of codebase) and learns "complex = buggy" — missing simple bugs and flagging complex clean code.

**Quantitative**: Stratified sampling improves precision by ~15% on simple functions without losing recall on complex ones.

**Edge cases**: When no bugs in a complexity stratum, sample uniformly from that stratum.

**Verification**: Compare precision@top-K with stratified vs random negative sampling on 10 repos.

### C6.2.2 Incremental Retraining

**Peak**: Instead of full retrain from scratch after 50 new bugs, use XGBoost's `xgb_model` parameter to continue training from previous model. Reduces training time from 5min to 30s.

**Edge cases**: If model architecture changes (new features), full retrain required. Detect via feature count mismatch.

**Verification**: Incremental retrain AUC must be within 2% of full retrain AUC.

### C6.3 Zero-Gap Guarantee

```
Component: Model — [x] PEAK via XGBoost
Component: Features — [x] PEAK via 10-dim CPG+git vector
Component: Negative sampling — [x] PEAK specified (C6.2.1)
Component: Retraining — [x] PEAK specified (C6.2.2)
```

---

## D1. Aggressive unit tests? (12 tests, 7 aggressive)

| # | Test | Attack | Expected |
|---|------|-------|----------|
| 1 | `test_train_predict` | 100 samples, 20 predict | AUC >0.7 |
| 2 | `test_feature_extraction` | Known function | All 10 features non-null |
| 3 | `test_model_serialization` | Save→load→predict same | Identical |
| 4 | `test_empty_training` | **AGGRESSIVE**: 0 bugs | Uniform prior. No crash. |
| 5 | `test_single_class` | **AGGRESSIVE**: All samples=bug | Predicts 1.0. Logs WARN. |
| 6 | `test_large_features` | **AGGRESSIVE**: Complexity=999999 | No overflow. |
| 7 | `test_empty_function` | **AGGRESSIVE**: 0 lines | Features all 0. No crash. |
| 8 | `test_prediction_ranking` | 10 funcs, 2 known bugs | Top-2 are buggy ones |
| 9 | `test_incremental_retrain` | 49→no, 50→yes | Threshold enforced |
| 10 | `test_model_roundtrip` | Train→save→load→predict | Feature importance preserved |
| 11 | `test_stratified_sampling` | **AGGRESSIVE**: Uneven complexity distribution | All strata represented |
| 12 | `test_git_unavailable` | **AGGRESSIVE**: No git repo | Git features=0. Model works. |

## D2. Aggressive integration tests? (4 tests, 2 aggressive)

| # | Test | Attack | Expected |
|---|------|-------|----------|
| 1 | `test_probability_feeds_scout` | Predict → scout receives top-N | Scout investigates top function first |
| 2 | `test_retrain_triggers` | 49 bugs→no, 50→yes | Post-run check correct |
| 3 | `test_cross_repo_prediction` | **AGGRESSIVE**: Train A, test B | AUC >0.6 (generalization) |
| 4 | `test_10k_inference_performance` | **AGGRESSIVE**: 10K functions | <1s total |

## D3. Extreme gate test — The Prediction Gauntlet? (6 attack vectors)

1. **Recall**: 100 buggy + 400 clean. Top-10% predictions contain >70% bugs.
2. **Cross-repo**: Model trained on repo A, tested on repo B. AUC >0.6.
3. **No-degradation**: Retrain 5× with incremental data. AUC does not degrade.
4. **Performance**: 10K-function inference <1s.
5. **Missing features**: Git unavailable → model still predicts (features=0).
6. **Corrupted model**: Corrupted file → falls back to uniform prior.

## D4. Golden dataset?

Applicable: Train on 150 golden bugs, test on 50. AUC must exceed 0.75. Feature importance report generated.

## D5. Regression test?

`test_probability_regression`: Prediction Gauntlet runs on every CI push. Any vector fails → build fails.

---

## E1. Estimated cost?

| Cost | Estimate |
|------|----------|
| Development | 12 hours |
| Training compute | <5min CPU per retrain |
| Model storage | <10MB |
| Inference | <1ms per function, negligible |
| Token savings | 30-50% reduction |

## E2. Observability?

**Logs**: model_trained (samples, auc), model_loaded, prediction_complete (functions, top_score), retrain_skipped (insufficient_data), feature_extraction_failed
**Metrics**: `bugswarm_probability_model_auc`, `bugswarm_probability_inference_count`, `bugswarm_probability_top10_recall`
**Alerts**: AUC drops >10% from previous → WARN

## E3. Configuration?

| Parameter | Default | Env | Flag |
|-----------|---------|-----|------|
| `probability_enabled` | `true` | `BGSWARM_PROBABILITY` | `--[no-]probability` |
| `retrain_threshold` | `50` | — | — |
| `model_path` | `~/.bugswarm/probability_model.json` | `BGSWARM_MODEL_PATH` | `--model-path` |
| `top_k_predictions` | `20` | — | — |

## E4. Migration?

Full backward compat. First run: no model → uniform prior. After 50 bugs: first model trained. Model file is additive.

## E5. Documentation?

ADR-019: ML Bug Probability Prediction. User docs: "Prediction System." Changelog.

---

## Gate Receipt

```json
{"phase":19,"gate":"prediction_gauntlet","attack_vectors":6,"passed":0,"failed":0,"verdict":"PHASE 19 NOT YET EXECUTED"}
```
