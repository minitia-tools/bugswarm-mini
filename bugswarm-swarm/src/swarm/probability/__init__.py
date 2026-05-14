"""Probability Package — Bug probability prediction via XGBoost.

C6.2 PEAK: Stratified negative sampling, incremental retrain, overfitting prevention.
"""

from .features import FunctionFeatures, FeatureExtractor
from .model import (
    BugProbabilityModel,
    PredictionResult,
    RepoPrediction,
)

__all__ = [
    "BugProbabilityModel",
    "PredictionResult",
    "RepoPrediction",
    "FunctionFeatures",
    "FeatureExtractor",
]
