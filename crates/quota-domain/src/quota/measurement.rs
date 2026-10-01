//! A normalised reading of one allowance.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::DomainError;
use crate::percent::Percent;
use crate::quota::money::MoneyMeasurement;
use crate::quota::units::{DecimalPrecision, QuotaUnit};

/// A provider-reported percentage of an allowance, with its original polarity.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct PercentageMeasurement {
    /// The provider's own used percentage, kept verbatim.
    pub used_percent: Percent,
    /// `100 - used_percent`, kept unrounded.
    pub remaining_percent: Percent,
    /// Decimal places the provider reported.
    pub precision: DecimalPrecision,
}

impl PercentageMeasurement {
    /// Builds a reading from a provider-reported used percentage.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidNumber`] when the used value is not finite.
    pub fn from_used_percent(
        used_percent: f64,
        precision: DecimalPrecision,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            used_percent: Percent::new(used_percent)?,
            remaining_percent: Percent::from_used_percent(used_percent)?,
            precision,
        })
    }
}

/// A counted allowance in a provider unit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct QuantityMeasurement {
    /// The unit the count is expressed in.
    pub unit: QuotaUnit,
    /// Decimal places the provider reported.
    pub precision: DecimalPrecision,
    /// Amount consumed, when reported.
    pub used: Option<f64>,
    /// Amount still available, when reported.
    pub remaining: Option<f64>,
    /// The allowance itself, when reported.
    pub limit: Option<f64>,
}

impl QuantityMeasurement {
    /// Builds a reading from counts with a known positive limit.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidNumber`] for non-finite values.
    pub fn with_limit(
        unit: QuotaUnit,
        precision: DecimalPrecision,
        used: f64,
        remaining: f64,
        limit: f64,
    ) -> Result<Self, DomainError> {
        for (field, value) in [("used", used), ("remaining", remaining), ("limit", limit)] {
            if !value.is_finite() {
                return Err(DomainError::InvalidNumber {
                    field,
                    reason: "not finite",
                });
            }
        }
        Ok(Self {
            unit,
            precision,
            used: Some(used),
            remaining: Some(remaining),
            limit: Some(limit),
        })
    }

    /// A remaining percentage, only for countable units with a positive limit.
    ///
    /// A count without a denominator stays in its native unit and never acquires
    /// an invented percentage.
    #[must_use]
    pub fn remaining_percent(&self) -> Option<Percent> {
        if !self.unit.is_count() {
            return None;
        }
        Percent::from_counts(self.remaining?, self.limit?).ok()
    }
}

/// Why an allowance has no usable number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    /// The provider was expected to report this window and did not.
    NotReported,
    /// The source cannot express this window at all.
    Unsupported,
    /// The provider answered, but the payload could not be interpreted.
    InvalidResponse,
    /// The window does not apply to this account.
    NotApplicable,
}

/// A normalised reading of one allowance.
///
/// The variants are mutually exclusive on purpose: exhausted, not entitled,
/// unlimited, and unknown are different facts and must never collapse into a
/// fabricated zero.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Measurement {
    /// A reported or derived percentage.
    Percentage(PercentageMeasurement),
    /// A count in a provider unit.
    Quantity(QuantityMeasurement),
    /// An amount of money.
    Money(MoneyMeasurement),
    /// The capability has no ceiling.
    Unlimited,
    /// The plan does not include this capability at all.
    NotEntitled,
    /// No usable number is available.
    Unavailable(UnavailableReason),
}

impl Measurement {
    /// The remaining percentage, when this measurement can produce one.
    #[must_use]
    pub fn remaining_percent(&self) -> Option<Percent> {
        match self {
            Self::Percentage(value) => Some(value.remaining_percent),
            Self::Quantity(value) => value.remaining_percent(),
            Self::Money(value) => value.remaining_percent().ok().flatten(),
            Self::Unlimited | Self::NotEntitled | Self::Unavailable(_) => None,
        }
    }

    /// Whether the provider reported a usable numeric value.
    #[must_use]
    pub const fn has_number(&self) -> bool {
        matches!(
            self,
            Self::Percentage(_) | Self::Quantity(_) | Self::Money(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::units::UnitSymbol;

    fn requests() -> QuotaUnit {
        QuotaUnit::Requests
    }

    #[test]
    fn percentage_keeps_its_polarity() {
        let value =
            PercentageMeasurement::from_used_percent(28.0, DecimalPrecision::new(0).unwrap())
                .unwrap();
        assert!((value.remaining_percent.value() - 72.0).abs() < f64::EPSILON);
        assert!((value.used_percent.value() - 28.0).abs() < f64::EPSILON);
    }

    #[test]
    fn quantity_with_a_limit_yields_a_percentage() {
        let value = QuantityMeasurement::with_limit(
            requests(),
            DecimalPrecision::new(0).unwrap(),
            40.0,
            60.0,
            100.0,
        )
        .unwrap();
        assert!((value.remaining_percent().unwrap().value() - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn quantity_without_a_denominator_stays_native() {
        let value = QuantityMeasurement {
            unit: requests(),
            precision: DecimalPrecision::new(0).unwrap(),
            used: Some(12.0),
            remaining: Some(340.0),
            limit: None,
        };
        assert_eq!(value.remaining_percent(), None);
    }

    #[test]
    fn non_count_units_never_produce_a_percentage() {
        let value = QuantityMeasurement::with_limit(
            QuotaUnit::Custom(UnitSymbol::new("agent-sec").unwrap()),
            DecimalPrecision::new(0).unwrap(),
            1.0,
            9.0,
            10.0,
        )
        .unwrap();
        assert_eq!(value.remaining_percent(), None);
    }

    #[test]
    fn rejects_non_finite_counts() {
        let result = QuantityMeasurement::with_limit(
            requests(),
            DecimalPrecision::new(0).unwrap(),
            f64::NAN,
            1.0,
            2.0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn unlimited_and_not_entitled_have_no_percentage() {
        assert_eq!(Measurement::Unlimited.remaining_percent(), None);
        assert_eq!(Measurement::NotEntitled.remaining_percent(), None);
        assert!(!Measurement::Unlimited.has_number());
        assert!(!Measurement::Unavailable(UnavailableReason::NotReported).has_number());
    }
}
