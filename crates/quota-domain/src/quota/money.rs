//! Monetary measurements, kept in exact minor units rather than floating point.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::DomainError;
use crate::percent::Percent;
use crate::quota::units::CurrencyCode;

/// The largest absolute minor-unit value that survives a JavaScript `number`
/// round trip exactly. Money beyond this uses the decimal-string wire format.
pub const MAX_SAFE_MINOR_UNITS: i64 = 9_007_199_254_740_991;

/// A monetary allowance, such as a monthly extra-spend cap.
///
/// Amounts are integer minor units (for example cents) with the scale recorded
/// alongside, so no rounding is introduced by the application.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MoneyMeasurement {
    /// The currency the provider reported.
    pub currency: CurrencyCode,
    /// Number of decimal places in one major unit, for example 2 for cents.
    pub scale: u8,
    /// Amount consumed, when the provider reported it.
    #[specta(type = f64)]
    pub used_minor_units: Option<i64>,
    /// Amount still available, when the provider reported it.
    #[specta(type = f64)]
    pub remaining_minor_units: Option<i64>,
    /// The cap itself, when the provider reported one.
    #[specta(type = f64)]
    pub limit_minor_units: Option<i64>,
}

impl MoneyMeasurement {
    /// A remaining percentage, only when the provider gave both a remaining
    /// amount and a positive cap.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidNumber`] when the denominator is not a
    /// positive finite number.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the percentage is a display projection; the exact amount stays i64"
    )]
    pub fn remaining_percent(&self) -> Result<Option<Percent>, DomainError> {
        match (self.remaining_minor_units, self.limit_minor_units) {
            (Some(remaining), Some(limit)) => {
                Percent::from_counts(remaining as f64, limit as f64).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// Whether every amount fits the exact JavaScript numeric range.
    #[must_use]
    pub fn fits_safe_integer_range(&self) -> bool {
        [
            self.used_minor_units,
            self.remaining_minor_units,
            self.limit_minor_units,
        ]
        .into_iter()
        .flatten()
        .all(|value| value.abs() <= MAX_SAFE_MINOR_UNITS)
    }

    /// Whether this amount needs the decimal-string wire format.
    #[must_use]
    pub fn requires_decimal_string_wire_format(&self) -> bool {
        !self.fits_safe_integer_range()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(remaining: i64, limit: i64) -> MoneyMeasurement {
        MoneyMeasurement {
            currency: CurrencyCode::new("USD").unwrap(),
            scale: 2,
            used_minor_units: Some(limit - remaining),
            remaining_minor_units: Some(remaining),
            limit_minor_units: Some(limit),
        }
    }

    #[test]
    fn derives_a_percentage_from_minor_units() {
        let percent = money(2000, 5000).remaining_percent().unwrap().unwrap();
        assert!((percent.value() - 40.0).abs() < f64::EPSILON);
    }

    #[test]
    fn refuses_a_percentage_without_a_cap() {
        let mut value = money(2000, 5000);
        value.limit_minor_units = None;
        assert_eq!(value.remaining_percent().unwrap(), None);
    }

    #[test]
    fn refuses_a_percentage_for_a_zero_cap() {
        assert!(money(0, 0).remaining_percent().is_err());
    }

    #[test]
    fn detects_amounts_that_need_the_string_wire_format() {
        assert!(!money(2000, 5000).requires_decimal_string_wire_format());
        assert!(money(i64::MAX, i64::MAX).requires_decimal_string_wire_format());
    }
}
