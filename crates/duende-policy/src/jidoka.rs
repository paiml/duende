//! Jidoka (自働化) automation - stop-on-error with recommendations.
//!
//! "Stop to fix problems, to get quality right the first time."
//! — Taiichi Ohno, Toyota Production System (1988)

use serde::{Deserialize, Serialize};

/// Jidoka gate for stop-on-error automation.
pub struct JidokaGate {
    checks: Vec<Box<dyn JidokaCheck>>,
    stop_on_first: bool,
}

impl JidokaGate {
    /// Creates a new Jidoka gate.
    #[must_use]
    pub fn new(stop_on_first: bool) -> Self {
        Self {
            checks: Vec::new(),
            stop_on_first,
        }
    }

    /// Adds a check to the gate.
    pub fn add_check(&mut self, check: impl JidokaCheck + 'static) {
        self.checks.push(Box::new(check));
    }

    /// Runs all checks and returns result.
    pub fn check(&self, evidence: &Evidence) -> JidokaResult {
        let mut violations = Vec::new();

        for check in &self.checks {
            if let Some(violation) = check.verify(evidence) {
                violations.push(violation);
                if self.stop_on_first {
                    let recommendation = self.recommend(&violations);
                    return JidokaResult::Stop {
                        violations,
                        recommendation,
                    };
                }
            }
        }

        if violations.is_empty() {
            JidokaResult::Pass
        } else {
            let recommendation = self.recommend(&violations);
            JidokaResult::Stop {
                violations,
                recommendation,
            }
        }
    }

    /// Generates recommendation based on violations.
    #[allow(clippy::unused_self)] // May use self in future for custom recommendations
    fn recommend(&self, violations: &[JidokaViolation]) -> String {
        if violations.is_empty() {
            return String::new();
        }

        let mut recommendations = Vec::new();

        for violation in violations {
            match &violation.kind {
                ViolationKind::Invariant => {
                    recommendations.push("Review invariant conditions and add assertions");
                }
                ViolationKind::Precondition => {
                    recommendations.push("Validate inputs at function boundaries");
                }
                ViolationKind::Postcondition => {
                    recommendations.push("Verify outputs meet expected conditions");
                }
                ViolationKind::ResourceLeak => {
                    recommendations.push("Ensure resources are properly cleaned up (RAII pattern)");
                }
                ViolationKind::Timeout => {
                    recommendations.push("Add timeout handling and circuit breakers");
                }
            }
        }

        recommendations.dedup();
        recommendations.join("; ")
    }
}

impl Default for JidokaGate {
    fn default() -> Self {
        Self::new(true)
    }
}

/// Jidoka check trait.
pub trait JidokaCheck: Send + Sync {
    /// Verifies evidence and returns violation if check fails.
    fn verify(&self, evidence: &Evidence) -> Option<JidokaViolation>;

    /// Returns the check name.
    fn name(&self) -> &str;
}

/// Evidence for Jidoka checks.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// Check items and their results.
    pub items: Vec<CheckItem>,
}

impl Evidence {
    /// Creates new evidence.
    #[must_use]
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Adds a check item.
    pub fn add(&mut self, name: impl Into<String>, passed: bool, message: Option<String>) {
        self.items.push(CheckItem {
            name: name.into(),
            passed,
            message,
        });
    }

    /// Returns true if all items passed.
    #[must_use]
    pub fn all_passed(&self) -> bool {
        self.items.iter().all(|item| item.passed)
    }
}

/// A single check item in evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckItem {
    /// Check name.
    pub name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Optional message.
    pub message: Option<String>,
}

/// Result of Jidoka gate check.
#[derive(Debug, Clone)]
pub enum JidokaResult {
    /// All checks passed.
    Pass,
    /// Checks failed - stop and fix.
    Stop {
        /// Violations found.
        violations: Vec<JidokaViolation>,
        /// Recommended fix.
        recommendation: String,
    },
}

impl JidokaResult {
    /// Returns true if all checks passed.
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self, Self::Pass)
    }
}

/// A Jidoka violation.
#[derive(Debug, Clone)]
pub struct JidokaViolation {
    /// Check that failed.
    pub check_name: String,
    /// Kind of violation.
    pub kind: ViolationKind,
    /// Description.
    pub description: String,
}

/// Kind of Jidoka violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationKind {
    /// Invariant violated.
    Invariant,
    /// Precondition not met.
    Precondition,
    /// Postcondition not met.
    Postcondition,
    /// Resource leak detected.
    ResourceLeak,
    /// Operation timed out.
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PassingCheck;
    impl JidokaCheck for PassingCheck {
        fn verify(&self, _: &Evidence) -> Option<JidokaViolation> {
            None
        }
        fn name(&self) -> &'static str {
            "passing"
        }
    }

    struct FailingCheck {
        kind: ViolationKind,
    }

    impl FailingCheck {
        fn new(kind: ViolationKind) -> Self {
            Self { kind }
        }
    }

    impl JidokaCheck for FailingCheck {
        fn verify(&self, _: &Evidence) -> Option<JidokaViolation> {
            Some(JidokaViolation {
                check_name: "failing".to_string(),
                kind: self.kind,
                description: "test failure".to_string(),
            })
        }
        fn name(&self) -> &'static str {
            "failing"
        }
    }

    #[test]
    fn test_jidoka_gate_passes() {
        let mut gate = JidokaGate::new(true);
        gate.add_check(PassingCheck);

        let evidence = Evidence::new();
        let result = gate.check(&evidence);
        assert!(result.passed());
    }

    #[test]
    fn test_jidoka_gate_default() {
        let gate = JidokaGate::default();
        let evidence = Evidence::new();
        let result = gate.check(&evidence);
        assert!(result.passed());
    }

    #[test]
    fn test_jidoka_gate_stops_on_failure() {
        let mut gate = JidokaGate::new(true);
        gate.add_check(FailingCheck::new(ViolationKind::Invariant));

        let evidence = Evidence::new();
        let result = gate.check(&evidence);
        assert!(!result.passed());

        if let JidokaResult::Stop {
            violations,
            recommendation,
        } = result
        {
            assert_eq!(violations.len(), 1);
            assert!(!recommendation.is_empty());
        } else {
            panic!("Expected Stop result");
        }
    }

    #[test]
    fn test_jidoka_gate_collects_all_without_stop_on_first() {
        let mut gate = JidokaGate::new(false);
        gate.add_check(FailingCheck::new(ViolationKind::Invariant));
        gate.add_check(FailingCheck::new(ViolationKind::Precondition));

        let evidence = Evidence::new();
        let result = gate.check(&evidence);

        if let JidokaResult::Stop { violations, .. } = result {
            assert_eq!(violations.len(), 2, "Should collect all violations");
        } else {
            panic!("Expected Stop result");
        }
    }

    #[test]
    fn test_jidoka_gate_stops_on_first_when_configured() {
        let mut gate = JidokaGate::new(true);
        gate.add_check(FailingCheck::new(ViolationKind::Invariant));
        gate.add_check(FailingCheck::new(ViolationKind::Precondition));

        let evidence = Evidence::new();
        let result = gate.check(&evidence);

        if let JidokaResult::Stop { violations, .. } = result {
            assert_eq!(violations.len(), 1, "Should stop on first");
        } else {
            panic!("Expected Stop result");
        }
    }

    #[test]
    fn test_evidence_default() {
        let evidence = Evidence::default();
        assert!(evidence.items.is_empty());
        assert!(evidence.all_passed());
    }

    #[test]
    fn test_evidence_all_passed() {
        let mut evidence = Evidence::new();
        evidence.add("check1", true, None);
        evidence.add("check2", true, None);
        assert!(evidence.all_passed());

        evidence.add("check3", false, Some("failed".to_string()));
        assert!(!evidence.all_passed());
    }

    #[test]
    fn test_evidence_debug_clone() {
        let mut evidence = Evidence::new();
        evidence.add("test", true, Some("msg".to_string()));

        let debug = format!("{:?}", evidence);
        assert!(debug.contains("Evidence"));

        let cloned = evidence.clone();
        assert_eq!(evidence.items.len(), cloned.items.len());
    }

    #[test]
    fn test_check_item_serialization() {
        let item = CheckItem {
            name: "test".to_string(),
            passed: true,
            message: Some("ok".to_string()),
        };

        let json = serde_json::to_string(&item).expect("serialize");
        let deserialized: CheckItem = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(item.name, deserialized.name);
        assert_eq!(item.passed, deserialized.passed);
        assert_eq!(item.message, deserialized.message);
    }

    #[test]
    fn test_jidoka_result_debug_clone() {
        let result = JidokaResult::Pass;
        let debug = format!("{:?}", result);
        assert!(debug.contains("Pass"));

        let cloned = result.clone();
        assert!(cloned.passed());
    }

    #[test]
    fn test_jidoka_violation_debug_clone() {
        let violation = JidokaViolation {
            check_name: "test".to_string(),
            kind: ViolationKind::Timeout,
            description: "timed out".to_string(),
        };

        let debug = format!("{:?}", violation);
        assert!(debug.contains("Timeout"));

        let cloned = violation.clone();
        assert_eq!(cloned.check_name, "test");
    }

    #[test]
    fn test_violation_kind_variants() {
        let kinds = [
            ViolationKind::Invariant,
            ViolationKind::Precondition,
            ViolationKind::Postcondition,
            ViolationKind::ResourceLeak,
            ViolationKind::Timeout,
        ];

        for kind in kinds {
            let _ = format!("{:?}", kind);
            let cloned = kind;
            assert_eq!(kind, cloned);
        }
    }

    #[test]
    fn test_recommendations_for_all_violation_kinds() {
        let gate = JidokaGate::new(false);

        // Test each violation kind generates a recommendation
        let kinds = [
            ViolationKind::Invariant,
            ViolationKind::Precondition,
            ViolationKind::Postcondition,
            ViolationKind::ResourceLeak,
            ViolationKind::Timeout,
        ];

        for kind in kinds {
            let violations = vec![JidokaViolation {
                check_name: "test".to_string(),
                kind,
                description: "test".to_string(),
            }];

            let recommendation = gate.recommend(&violations);
            assert!(
                !recommendation.is_empty(),
                "Should have recommendation for {:?}",
                kind
            );
        }
    }

    #[test]
    fn test_recommend_empty_violations() {
        let gate = JidokaGate::new(true);
        let recommendation = gate.recommend(&[]);
        assert!(recommendation.is_empty());
    }

    #[test]
    fn test_recommend_deduplicates() {
        let gate = JidokaGate::new(false);

        // Two invariant violations should produce single recommendation
        let violations = vec![
            JidokaViolation {
                check_name: "test1".to_string(),
                kind: ViolationKind::Invariant,
                description: "test1".to_string(),
            },
            JidokaViolation {
                check_name: "test2".to_string(),
                kind: ViolationKind::Invariant,
                description: "test2".to_string(),
            },
        ];

        let recommendation = gate.recommend(&violations);
        // Should only contain the invariant recommendation once
        let count = recommendation.matches("Review invariant").count();
        assert_eq!(count, 1, "Should deduplicate recommendations");
    }
}
