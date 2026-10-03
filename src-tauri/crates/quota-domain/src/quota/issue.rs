//! Typed validation findings attached to a quota window.

use serde::{Deserialize, Serialize};
use specta::Type;

/// A structured problem found while decoding or normalising a provider reading.
///
/// The variant is the machine-readable contract. Any display text is a
/// supplement and never a control-flow signal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "code", content = "context")]
pub enum QuotaIssue {
    /// A reported number was NaN or infinite.
    NonFiniteValue {
        /// The field that carried the unusable number.
        field: String,
    },
    /// A percentage was computed from a zero or negative denominator.
    NonPositiveDenominator,
    /// Reported remaining and limit values contradict each other.
    ContradictoryCounts {
        /// The reported remaining value.
        remaining: f64,
        /// The reported limit value.
        limit: f64,
    },
    /// Usage was reported as negative.
    NegativeUsage {
        /// The negative value the provider reported.
        value: f64,
    },
    /// A persisted or remote schema version this build cannot interpret.
    UnsupportedSchemaVersion {
        /// The version the provider or file declared.
        version: u32,
    },
    /// A resource scope did not match the scope the window is defined for.
    ScopeMismatch {
        /// The scope the window expects.
        expected: String,
        /// The scope the provider actually reported.
        actual: String,
    },
}

impl QuotaIssue {
    /// A stable, machine-readable code for logs and diagnostics.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NonFiniteValue { .. } => "non_finite_value",
            Self::NonPositiveDenominator => "non_positive_denominator",
            Self::ContradictoryCounts { .. } => "contradictory_counts",
            Self::NegativeUsage { .. } => "negative_usage",
            Self::UnsupportedSchemaVersion { .. } => "unsupported_schema_version",
            Self::ScopeMismatch { .. } => "scope_mismatch",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable_and_distinct() {
        let issues = [
            QuotaIssue::NonFiniteValue {
                field: "used".into(),
            },
            QuotaIssue::NonPositiveDenominator,
            QuotaIssue::ContradictoryCounts {
                remaining: 1.0,
                limit: 0.0,
            },
            QuotaIssue::NegativeUsage { value: -3.0 },
            QuotaIssue::UnsupportedSchemaVersion { version: 9 },
            QuotaIssue::ScopeMismatch {
                expected: "a".into(),
                actual: "b".into(),
            },
        ];
        let codes: Vec<_> = issues.iter().map(QuotaIssue::code).collect();
        let mut unique = codes.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), codes.len());
    }

    #[test]
    fn round_trips_through_serde() {
        let issue = QuotaIssue::NegativeUsage { value: -3.0 };
        let json = serde_json::to_string(&issue).unwrap();
        assert_eq!(serde_json::from_str::<QuotaIssue>(&json).unwrap(), issue);
    }
}
