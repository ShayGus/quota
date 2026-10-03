//! Units, currencies, and provider precision for quota quantities.

use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::DomainError;
use crate::ids::validated_id;

validated_id!(
    /// A provider-defined unit that the closed vocabulary does not cover.
    UnitSymbol
);

/// The unit a counted allowance is expressed in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "symbol")]
pub enum QuotaUnit {
    /// A count of billable requests.
    Requests,
    /// A count of model tokens.
    Tokens,
    /// A count of conversation messages.
    Messages,
    /// A count of prepaid credits.
    Credits,
    /// A provider-defined unit, carried verbatim as text.
    Custom(UnitSymbol),
}

impl QuotaUnit {
    /// Whether this unit is a plain count, and so eligible for a percentage once a
    /// compatible positive denominator is known.
    #[must_use]
    pub const fn is_count(&self) -> bool {
        matches!(
            self,
            Self::Requests | Self::Tokens | Self::Messages | Self::Credits
        )
    }
}

/// An ISO 4217-style currency code.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(try_from = "String", into = "String")]
pub struct CurrencyCode(Box<str>);

impl CurrencyCode {
    /// Validates a three-letter uppercase currency code.
    ///
    /// # Errors
    /// Returns [`DomainError::UnsupportedValue`] unless the code is exactly three
    /// ASCII letters.
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let raw = raw.into();
        if raw.len() != 3 || !raw.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(DomainError::UnsupportedValue {
                field: "CurrencyCode",
            });
        }
        Ok(Self(raw.into_boxed_str()))
    }

    /// Borrows the currency code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CurrencyCode {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<CurrencyCode> for String {
    fn from(value: CurrencyCode) -> Self {
        value.0.into_string()
    }
}

impl Display for CurrencyCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for CurrencyCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "CurrencyCode({:?})", &*self.0)
    }
}

/// Decimal places a provider used when it reported a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(try_from = "u8", into = "u8")]
pub struct DecimalPrecision(u8);

impl DecimalPrecision {
    /// The largest provider precision this build understands.
    pub const MAX: u8 = 9;

    /// No decimal places, for a whole-number value.
    pub const WHOLE: Self = Self(0);

    /// One decimal place, for a percentage derived from counts.
    pub const ONE_PLACE: Self = Self(1);

    /// Validates a decimal-place count.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidNumber`] above [`DecimalPrecision::MAX`].
    pub fn new(places: u8) -> Result<Self, DomainError> {
        if places > Self::MAX {
            return Err(DomainError::InvalidNumber {
                field: "DecimalPrecision",
                reason: "more than 9 decimal places",
            });
        }
        Ok(Self(places))
    }

    /// The number of decimal places.
    #[must_use]
    pub const fn places(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for DecimalPrecision {
    type Error = DomainError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<DecimalPrecision> for u8 {
    fn from(value: DecimalPrecision) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_requires_three_uppercase_letters() {
        CurrencyCode::new("USD").unwrap();
        CurrencyCode::new("usd").unwrap_err();
        CurrencyCode::new("US").unwrap_err();
        CurrencyCode::new("USDD").unwrap_err();
    }

    #[test]
    fn currency_deserialization_enforces_validation() {
        serde_json::from_str::<CurrencyCode>("\"EUR\"").unwrap();
        serde_json::from_str::<CurrencyCode>("\"eur\"").unwrap_err();
    }

    #[test]
    fn custom_unit_is_validated() {
        UnitSymbol::new("agent-seconds").unwrap();
        UnitSymbol::new(" ").unwrap_err();
    }

    #[test]
    fn precision_is_bounded() {
        DecimalPrecision::new(9).unwrap();
        DecimalPrecision::new(10).unwrap_err();
    }

    #[test]
    fn custom_units_are_not_counts() {
        assert!(QuotaUnit::Requests.is_count());
        assert!(!QuotaUnit::Custom(UnitSymbol::new("s").unwrap()).is_count());
    }
}
