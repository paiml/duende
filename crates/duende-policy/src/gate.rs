//! Quality gate enforcement.
//!
//! # Toyota Way: Jidoka (自働化)
//! Automatic stop when quality problems detected.

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Quality gate configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    /// Maximum cyclomatic complexity.
    pub max_complexity: u32,
    /// SATD (Self-Admitted Technical Debt) tolerance.
    pub satd_tolerance: u32,
    /// Maximum dead code percentage.
    pub dead_code_max_percent: f64,
    /// Minimum quality score (0-100).
    pub min_quality_score: f64,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            max_complexity: 20,
            satd_tolerance: 0,
            dead_code_max_percent: 10.0,
            min_quality_score: 80.0,
        }
    }
}

/// Quality gate for daemon code analysis.
pub struct QualityGate {
    config: GateConfig,
}

impl QualityGate {
    /// Creates a new quality gate with given configuration.
    #[must_use]
    pub const fn new(config: GateConfig) -> Self {
        Self { config }
    }

    /// Analyzes code quality and returns result.
    ///
    /// # Errors
    /// Returns an error if analysis fails.
    pub fn analyze(&self, analysis: &QualityAnalysis) -> Result<GateResult> {
        let mut violations = Vec::new();

        // Check complexity
        if analysis.max_complexity > self.config.max_complexity {
            violations.push(QualityViolation::Complexity {
                actual: analysis.max_complexity,
                threshold: self.config.max_complexity,
                location: analysis.complexity_hotspot.clone(),
            });
        }

        // Check SATD
        if analysis.satd_count > self.config.satd_tolerance {
            violations.push(QualityViolation::TechnicalDebt {
                count: analysis.satd_count,
                tolerance: self.config.satd_tolerance,
            });
        }

        // Check dead code
        if analysis.dead_code_percent > self.config.dead_code_max_percent {
            violations.push(QualityViolation::DeadCode {
                percent: analysis.dead_code_percent,
                threshold: self.config.dead_code_max_percent,
            });
        }

        // Check quality score
        if analysis.quality_score < self.config.min_quality_score {
            violations.push(QualityViolation::QualityScore {
                score: analysis.quality_score,
                minimum: self.config.min_quality_score,
            });
        }

        if violations.is_empty() {
            Ok(GateResult::Passed)
        } else {
            Ok(GateResult::Failed { violations })
        }
    }

    /// Returns the gate configuration.
    #[must_use]
    pub const fn config(&self) -> &GateConfig {
        &self.config
    }
}

impl Default for QualityGate {
    fn default() -> Self {
        Self::new(GateConfig::default())
    }
}

/// Result of quality gate check.
#[derive(Debug, Clone)]
pub enum GateResult {
    /// Gate passed.
    Passed,
    /// Gate failed with violations.
    Failed {
        /// List of violations.
        violations: Vec<QualityViolation>,
    },
}

impl GateResult {
    /// Returns true if the gate passed.
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self, Self::Passed)
    }
}

/// Quality violation.
#[derive(Debug, Clone)]
pub enum QualityViolation {
    /// Complexity threshold exceeded.
    Complexity {
        /// Actual complexity.
        actual: u32,
        /// Threshold.
        threshold: u32,
        /// Location of hotspot.
        location: Option<String>,
    },
    /// Technical debt tolerance exceeded.
    TechnicalDebt {
        /// SATD count.
        count: u32,
        /// Tolerance.
        tolerance: u32,
    },
    /// Dead code threshold exceeded.
    DeadCode {
        /// Actual percentage.
        percent: f64,
        /// Threshold.
        threshold: f64,
    },
    /// Quality score below minimum.
    QualityScore {
        /// Actual score.
        score: f64,
        /// Minimum required.
        minimum: f64,
    },
}

/// Quality analysis input.
#[derive(Debug, Clone, Default)]
pub struct QualityAnalysis {
    /// Maximum cyclomatic complexity found.
    pub max_complexity: u32,
    /// Location of complexity hotspot.
    pub complexity_hotspot: Option<String>,
    /// SATD comment count.
    pub satd_count: u32,
    /// Dead code percentage.
    pub dead_code_percent: f64,
    /// Overall quality score (0-100).
    pub quality_score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_passes_good_code() {
        let gate = QualityGate::default();
        let analysis = QualityAnalysis {
            max_complexity: 10,
            satd_count: 0,
            dead_code_percent: 5.0,
            quality_score: 90.0,
            ..Default::default()
        };

        let result = gate.analyze(&analysis).ok();
        assert!(result.is_some_and(|r| r.passed()));
    }

    #[test]
    fn test_gate_fails_on_complexity() {
        let gate = QualityGate::new(GateConfig {
            max_complexity: 10,
            ..Default::default()
        });

        let analysis = QualityAnalysis {
            max_complexity: 25,
            ..Default::default()
        };

        let result = gate.analyze(&analysis).ok();
        assert!(result.is_some_and(|r| !r.passed()));
    }

    #[test]
    fn test_gate_fails_on_satd() {
        let gate = QualityGate::new(GateConfig {
            satd_tolerance: 0,
            ..Default::default()
        });

        let analysis = QualityAnalysis {
            satd_count: 5,
            quality_score: 90.0,
            ..Default::default()
        };

        let result = gate.analyze(&analysis).ok();
        assert!(result.is_some_and(|r| !r.passed()));
    }
}
