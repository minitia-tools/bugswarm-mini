use serde::{Deserialize, Serialize};

/// A sorted map of (address, danger_score) pairs used for taint-guided
/// fuzzing prioritization.
#[derive(Debug, Clone)]
pub struct DangerMap {
    entries: Vec<(u64, f32)>,
}

impl DangerMap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn from_pairs(mut pairs: Vec<(u64, f32)>) -> Self {
        pairs.sort_by_key(|&(addr, _)| addr);
        Self { entries: pairs }
    }

    /// Binary search with floor semantics:
    /// - Exact match: return the score.
    /// - Between entries: return the score of the nearest lower address.
    /// - Below all entries: return 0.0.
    pub fn lookup(&self, address: u64) -> f32 {
        if self.entries.is_empty() {
            return 0.0;
        }
        match self.entries.binary_search_by_key(&address, |&(a, _)| a) {
            Ok(idx) => self.entries[idx].1,
            Err(idx) => {
                if idx == 0 {
                    0.0
                } else {
                    self.entries[idx - 1].1
                }
            }
        }
    }

    /// Normalize all scores to [0.0, 1.0] by dividing by the maximum score.
    pub fn normalize(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let max_score = self
            .entries
            .iter()
            .map(|&(_, s)| s)
            .fold(0.0_f32, f32::max);
        if max_score > 0.0 {
            for (_, score) in &mut self.entries {
                *score /= max_score;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for DangerMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration controlling how taint-guided danger scores are blended into
/// the fuzzer's power schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DangerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_taint_weight")]
    pub taint_weight: f32,
    #[serde(default = "default_coverage_weight")]
    pub coverage_weight: f32,
    #[serde(default = "default_decay_factor")]
    pub decay_factor: f32,
}

fn default_true() -> bool {
    true
}
fn default_taint_weight() -> f32 {
    0.7
}
fn default_coverage_weight() -> f32 {
    0.3
}
fn default_decay_factor() -> f32 {
    0.7
}

impl Default for DangerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            taint_weight: 0.7,
            coverage_weight: 0.3,
            decay_factor: 0.7,
        }
    }
}

impl DangerConfig {
    /// Blend coverage rarity and danger score into a power-schedule value
    /// clamped to [0.0, 1.0].
    pub fn compute_power_schedule(&self, coverage_rarity: f32, danger_score: f32) -> f32 {
        let score = coverage_rarity * self.coverage_weight + danger_score * self.taint_weight;
        score.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_map_returns_zero() {
        let map = DangerMap::new();
        assert_eq!(map.lookup(0x1000), 0.0);
    }

    #[test]
    fn test_exact_match() {
        let map = DangerMap::from_pairs(vec![(0x1000, 0.5), (0x2000, 0.8)]);
        assert_eq!(map.lookup(0x1000), 0.5);
        assert_eq!(map.lookup(0x2000), 0.8);
    }

    #[test]
    fn test_floor_semantics() {
        let map = DangerMap::from_pairs(vec![(0x1000, 0.3), (0x3000, 0.9)]);
        assert_eq!(map.lookup(0x2000), 0.3);
    }

    #[test]
    fn test_below_all() {
        let map = DangerMap::from_pairs(vec![(0x5000, 0.7)]);
        assert_eq!(map.lookup(0x1000), 0.0);
    }

    #[test]
    fn test_normalize() {
        let mut map = DangerMap::from_pairs(vec![(0x1000, 2.0), (0x2000, 4.0), (0x3000, 4.0)]);
        map.normalize();
        assert!((map.lookup(0x1000) - 0.5).abs() < 1e-6);
        assert!((map.lookup(0x2000) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_empty() {
        let mut map = DangerMap::new();
        map.normalize();
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_len() {
        let map = DangerMap::from_pairs(vec![(0x1000, 0.1), (0x2000, 0.2)]);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_danger_config_defaults() {
        let cfg = DangerConfig::default();
        assert!(cfg.enabled);
        assert!((cfg.taint_weight - 0.7).abs() < 1e-6);
        assert!((cfg.coverage_weight - 0.3).abs() < 1e-6);
        assert!((cfg.decay_factor - 0.7).abs() < 1e-6);
    }

    #[test]
    fn test_compute_power_schedule() {
        let cfg = DangerConfig::default();
        let score = cfg.compute_power_schedule(0.5, 0.8);
        let expected = 0.5 * 0.3 + 0.8 * 0.7;
        assert!((score - expected).abs() < 1e-6);
    }

    #[test]
    fn test_compute_power_schedule_clamped() {
        let cfg = DangerConfig::default();
        let score = cfg.compute_power_schedule(1.5, 2.0);
        assert!(score <= 1.0);
    }
}
