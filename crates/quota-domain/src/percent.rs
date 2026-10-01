//! A finite percentage that keeps its original evidence.

use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::DomainError;

/// A finite percentage of an allowance.
///
/// The stored value is never rounded, clamped, or re-polaritised. Display
/// formatting and the donut arc are derived from it, so overspend above 100%
/// used survives the round trip.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize, Type)]
#[serde(try_from = "f64", into = "f64")]
pub struct Percent(f64);

impl Percent {
    /// Builds a percentage from a remaining fraction that may fall outside `0..=100`.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidNumber`] for NaN or infinite input.
    pub fn new(value: f64) -> Result<Self, DomainError> {
        if !value.is_finite() {
            return Err(DomainError::InvalidNumber {
                field: "Percent",
                reason: "not finite",
            });
        }
        Ok(Self(value))
    }
    /// A percentage of exactly zero.
    pub const ZERO: Self = Self(0.0);

    /// A percentage of exactly one hundred.
    pub const HUNDRED: Self = Self(100.0);

    /// Converts a provider-reported used percentage into a remaining percentage.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidNumber`] when the used value is not finite.
    pub fn from_used_percent(used: f64) -> Result<Self, DomainError> {
        Self::new(100.0 - used)
    }

    /// Derives a remaining percentage from a counted allowance and a positive limit.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidNumber`] for a non-positive limit or a
    /// non-finite count. A zero allowance is an explicit `not entitled` state at
    /// the call site, never a division here.
    pub fn from_counts(remaining: f64, limit: f64) -> Result<Self, DomainError> {
        if !remaining.is_finite() {
            return Err(DomainError::InvalidNumber {
                field: "remaining",
                reason: "not finite",
            });
        }
        if !limit.is_finite() || limit <= 0.0 {
            return Err(DomainError::InvalidNumber {
                field: "limit",
                reason: "denominator must be finite and positive",
            });
        }
        Self::new(100.0 * (remaining / limit))
    }

    /// The unrounded value, used for every comparison, threshold and rank.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }

    /// The value clamped into `0..=100`, for the visible arc only.
    #[must_use]
    pub fn clamped(self) -> f64 {
        self.0.clamp(0.0, 100.0)
    }

    /// The value as a `0.0..=1.0` arc fraction.
    #[must_use]
    pub fn arc_fraction(self) -> f64 {
        self.clamped() / 100.0
    }

    /// Whether the value is a positive remainder below one percent.
    #[must_use]
    pub fn is_just_above_zero(self) -> bool {
        self.0 > 0.0 && self.0 < 1.0
    }

    /// Whether the allowance is fully exhausted.
    #[must_use]
    pub fn is_exhausted(self) -> bool {
        self.0 <= 0.0
    }
}

impl TryFrom<f64> for Percent {
    type Error = DomainError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Percent> for f64 {
    fn from(value: Percent) -> Self {
        value.0
    }
}

impl Display for Percent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.is_just_above_zero() {
            return f.write_str("<1%");
        }
        let clamped = self.clamped();
        // One decimal place near a warning boundary, so the label cannot contradict
        // a threshold that the unrounded value actually crossed.
        if (0.0..=1.0).contains(&clamped) || (99.0..=100.0).contains(&clamped) {
            return write!(f, "{clamped:.1}%");
        }
        write!(f, "{clamped:.0}%")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_used_percent_to_remaining() {
        let remaining = Percent::from_used_percent(28.0).unwrap();
        assert!((remaining.value() - 72.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_non_finite_values() {
        assert!(Percent::new(f64::NAN).is_err());
        assert!(Percent::new(f64::INFINITY).is_err());
        assert!(Percent::from_counts(1.0, 0.0).is_err());
        assert!(Percent::from_counts(1.0, f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn preserves_overspend_evidence() {
        let remaining = Percent::from_used_percent(137.5).unwrap();
        assert!((remaining.value() + 37.5).abs() < f64::EPSILON);
        assert!(remaining.clamped().abs() < f64::EPSILON);
        assert_eq!(format!("{remaining}"), "0.0%");
    }

    #[test]
    fn labels_a_positive_sub_one_remainder() {
        let remaining = Percent::new(0.42).unwrap();
        assert!(remaining.is_just_above_zero());
        assert_eq!(format!("{remaining}"), "<1%");
    }

    #[test]
    fn keeps_a_decimal_near_warning_boundaries() {
        assert_eq!(format!("{}", Percent::new(99.6).unwrap()), "99.6%");
        assert_eq!(format!("{}", Percent::new(0.4).unwrap()), "<1%");
        assert_eq!(format!("{}", Percent::new(1.0).unwrap()), "1.0%");
        assert_eq!(format!("{}", Percent::new(72.4).unwrap()), "72%");
    }

    #[test]
    fn distinguishes_exhaustion_from_unknown() {
        assert!(Percent::new(0.0).unwrap().is_exhausted());
        assert!(!Percent::new(0.001).unwrap().is_exhausted());
    }

    #[test]
    fn deserialization_rejects_non_numeric_text() {
        assert!(serde_json::from_str::<Percent>("12.5").is_ok());
        assert!(serde_json::from_str::<Percent>("\"NaN\"").is_err());
        assert!(serde_json::from_str::<Percent>("null").is_err());
    }
}
