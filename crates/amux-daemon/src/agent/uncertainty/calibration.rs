//! Calibration tracker for uncertainty quantification (UNCR-07).
//!
//! Records predicted confidence band vs actual outcome. Applies conservative
//! cold-start defaults until 50+ observations per band (research pitfall 5).

use std::collections::HashMap;

use crate::agent::explanation::ConfidenceBand;

use super::confidence::confidence_label;

/// A single calibration observation: predicted band + actual outcome.
#[derive(Debug, Clone)]
struct CalibrationEntry {
    #[allow(dead_code)]
    predicted_band: ConfidenceBand,
    actual_success: bool,
    #[allow(dead_code)]
    timestamp: u64,
}

/// Tracks predicted confidence bands vs actual outcomes for calibration feedback.
///
/// Conservative cold-start: shifts bands one step more cautious when there are
/// fewer than `conservative_threshold` observations for the given band.
#[derive(Debug, Clone)]
pub struct CalibrationTracker {
    /// Observations keyed by band label ("HIGH", "MEDIUM", "LOW").
    observations: HashMap<String, Vec<CalibrationEntry>>,
    /// Minimum observations before trusting the raw band (default 50).
    conservative_threshold: usize,
}

impl CalibrationTracker {
    /// Create a new tracker with the given conservative threshold.
    pub fn new(conservative_threshold: usize) -> Self {
        Self {
            observations: HashMap::new(),
            conservative_threshold,
        }
    }

    /// Record an observation of predicted band vs actual outcome.
    pub fn record_observation(
        &mut self,
        predicted_band: ConfidenceBand,
        actual_success: bool,
        timestamp: u64,
    ) {
        let label = confidence_label(predicted_band).to_string();
        let entry = CalibrationEntry {
            predicted_band,
            actual_success,
            timestamp,
        };
        self.observations
            .entry(label)
            .or_insert_with(Vec::new)
            .push(entry);
    }

    /// Get the number of observations for the given band.
    pub fn observation_count(&self, band: ConfidenceBand) -> usize {
        let label = confidence_label(band);
        self.observations.get(label).map(|v| v.len()).unwrap_or(0)
    }

    /// Get a calibrated band, applying conservative shift when data is sparse.
    ///
    /// If there are fewer than `conservative_threshold` observations for this band,
    /// shift one step more cautious: HIGH -> MEDIUM, MEDIUM -> LOW, LOW stays LOW.
    pub fn get_calibrated_band(&self, band: ConfidenceBand) -> ConfidenceBand {
        let count = self.observation_count(band);
        if count >= self.conservative_threshold {
            return band;
        }

        // Conservative shift: one step more cautious
        match band {
            ConfidenceBand::Confident => ConfidenceBand::Likely,
            ConfidenceBand::Likely => ConfidenceBand::Uncertain,
            ConfidenceBand::Uncertain => ConfidenceBand::Guessing,
            ConfidenceBand::Guessing => ConfidenceBand::Guessing,
        }
    }

    /// Get total observation count across all bands.
    pub fn total_observations(&self) -> usize {
        self.observations.values().map(|v| v.len()).sum()
    }

    /// Get calibration stats: count and accuracy per band.
    pub fn calibration_stats(&self) -> HashMap<String, (usize, f64)> {
        let mut stats = HashMap::new();
        for (label, entries) in &self.observations {
            let count = entries.len();
            let successes = entries.iter().filter(|e| e.actual_success).count();
            let accuracy = if count > 0 {
                successes as f64 / count as f64
            } else {
                0.0
            };
            stats.insert(label.clone(), (count, accuracy));
        }
        stats
    }
}

impl Default for CalibrationTracker {
    fn default() -> Self {
        Self::new(50)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_tracker_new_starts_with_zero_observations() {
        let tracker = CalibrationTracker::new(50);
        assert_eq!(tracker.total_observations(), 0);
        assert_eq!(tracker.observation_count(ConfidenceBand::Confident), 0);
        assert_eq!(tracker.observation_count(ConfidenceBand::Likely), 0);
        assert_eq!(tracker.observation_count(ConfidenceBand::Uncertain), 0);
    }

    #[test]
    fn record_observation_increments_count() {
        let mut tracker = CalibrationTracker::new(50);
        tracker.record_observation(ConfidenceBand::Confident, true, 1000);
        tracker.record_observation(ConfidenceBand::Confident, false, 2000);
        assert_eq!(tracker.observation_count(ConfidenceBand::Confident), 2);
        assert_eq!(tracker.total_observations(), 2);
    }

    #[test]
    fn get_calibrated_band_conservative_shift_below_threshold() {
        let tracker = CalibrationTracker::new(50);
        // No observations -> should shift conservatively
        // HIGH (Confident) -> MEDIUM (Likely)
        assert_eq!(
            tracker.get_calibrated_band(ConfidenceBand::Confident),
            ConfidenceBand::Likely
        );
        // MEDIUM (Likely) -> LOW (Uncertain)
        assert_eq!(
            tracker.get_calibrated_band(ConfidenceBand::Likely),
            ConfidenceBand::Uncertain
        );
    }

    #[test]
    fn get_calibrated_band_unchanged_above_threshold() {
        let mut tracker = CalibrationTracker::new(50);
        // Add 50 observations for Confident
        for i in 0..50 {
            tracker.record_observation(ConfidenceBand::Confident, true, i as u64);
        }
        // Now should return band unchanged
        assert_eq!(
            tracker.get_calibrated_band(ConfidenceBand::Confident),
            ConfidenceBand::Confident
        );
    }

    #[test]
    fn calibration_stats_returns_counts_and_accuracy() {
        let mut tracker = CalibrationTracker::new(50);
        // 3 HIGH: 2 success, 1 failure
        tracker.record_observation(ConfidenceBand::Confident, true, 1);
        tracker.record_observation(ConfidenceBand::Confident, true, 2);
        tracker.record_observation(ConfidenceBand::Confident, false, 3);
        // 1 LOW: 0 success
        tracker.record_observation(ConfidenceBand::Uncertain, false, 4);

        let stats = tracker.calibration_stats();

        let (high_count, high_acc) = stats.get("HIGH").unwrap();
        assert_eq!(*high_count, 3);
        assert!((high_acc - 2.0 / 3.0).abs() < 0.01);

        let (low_count, low_acc) = stats.get("LOW").unwrap();
        assert_eq!(*low_count, 1);
        assert_eq!(*low_acc, 0.0);
    }
}
