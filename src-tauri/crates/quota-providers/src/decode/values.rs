//! Reading the numbers, instants, and durations an undocumented source writes.
//!
//! These sources write one quantity many ways: a JSON number, a numeric string,
//! an epoch second, a date string, or a delay. Each reader answers one question
//! and returns `None` rather than guessing, so no reset time is invented and no
//! unusable number becomes a zero.

use chrono::{DateTime, Duration, Utc};
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::{Measurement, PercentageMeasurement};
use quota_domain::quota::units::DecimalPrecision;
use serde::Deserialize;

/// A number as an undocumented provider wrote it: a JSON number or a string.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Numberish {
    /// A real JSON number.
    Number(serde_json::Number),
    /// A number written as text.
    Text(String),
}

/// A numeric field, with the decimal places the provider used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NumberField {
    /// The decoded value.
    pub(crate) value: f64,
    /// Decimal places in the text the provider sent, capped at the domain limit.
    pub(crate) decimals: DecimalPrecision,
}

impl Numberish {
    /// Decodes the field, rejecting text that is not a finite number.
    pub(crate) fn field(&self) -> Option<NumberField> {
        let text = match self {
            Self::Number(number) => number.to_string(),
            Self::Text(text) => text.trim().to_owned(),
        };
        let value: f64 = text.parse().ok()?;
        if !value.is_finite() {
            return None;
        }
        // A count above the domain ceiling is clamped, so this never fails.
        let decimals = DecimalPrecision::new(decimal_places(&text)).ok()?;
        Some(NumberField { value, decimals })
    }

    /// Decodes an integral count, rejecting fractional values.
    pub(crate) fn whole(&self) -> Option<i64> {
        let field = self.field()?;
        if field.value.fract().abs() > f64::EPSILON {
            return None;
        }
        if field.value.abs() >= 9_007_199_254_740_992.0 {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the value is integral and inside the exact f64 integer range"
        )]
        Some(field.value.trunc() as i64)
    }
}

/// The decimal places in the text form of a number, ignoring an exponent form.
fn decimal_places(text: &str) -> u8 {
    if text.contains(['e', 'E']) {
        return 0;
    }
    let Some((_, fraction)) = text.split_once('.') else {
        return 0;
    };
    u8::try_from(fraction.len())
        .unwrap_or(DecimalPrecision::MAX)
        .min(DecimalPrecision::MAX)
}

/// Reads a reset instant written as epoch seconds or as a date string.
///
/// A value outside the representable range, or a time that is not a date at
/// all, yields `None`: this crate never guesses a reset time.
pub(crate) fn reset_instant(value: &Numberish) -> Option<DateTime<Utc>> {
    match value {
        Numberish::Number(_) => epoch_instant(value.whole()?),
        Numberish::Text(text) => {
            let text = text.trim();
            if let Ok(seconds) = text.parse::<i64>() {
                return epoch_instant(seconds);
            }
            DateTime::parse_from_rfc3339(text)
                .or_else(|_| DateTime::parse_from_rfc2822(text))
                .ok()
                .map(|parsed| parsed.with_timezone(&Utc))
        }
    }
}

/// Reads a reset instant written as epoch seconds or epoch milliseconds,
/// telling them apart by size as the sources that mix them require.
pub(crate) fn reset_epoch(value: &Numberish) -> Option<DateTime<Utc>> {
    let whole = value.whole()?;
    if whole <= 0 {
        return None;
    }
    if whole > 100_000_000_000 {
        return DateTime::from_timestamp_millis(whole);
    }
    epoch_instant(whole)
}

/// A count against a positive allowance, in its unit, or `None` when the
/// source did not give a usable allowance.
///
/// A missing used or remaining count is derived from the other, and a count
/// above the allowance is kept, so overspend stays visible.
pub(crate) fn counted(
    unit: quota_domain::quota::units::QuotaUnit,
    used: Option<f64>,
    remaining: Option<f64>,
    limit: Option<f64>,
) -> Option<Measurement> {
    let limit = limit.filter(|limit| limit.is_finite() && *limit > 0.0)?;
    let used = used.or_else(|| remaining.map(|remaining| limit - remaining))?;
    let remaining = remaining.unwrap_or(limit - used);
    quota_domain::quota::measurement::QuantityMeasurement::with_limit(
        unit,
        DecimalPrecision::WHOLE,
        used,
        remaining,
        limit,
    )
    .ok()
    .map(Measurement::Quantity)
}

/// Converts epoch seconds into an instant, or `None` when out of range.
fn epoch_instant(seconds: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(seconds, 0)
}

/// Reads a reset instant written as a delay in seconds from receipt.
pub(crate) fn reset_after(value: &Numberish, received_at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let seconds = value.whole()?;
    received_at.checked_add_signed(Duration::seconds(seconds.max(0)))
}

/// Reads a duration, converting from a per-unit scale when the source needs it.
pub(crate) fn scaled_seconds(value: &Numberish, per_unit: i64) -> Option<i64> {
    value.whole()?.checked_mul(per_unit)
}

/// Decodes a reported percentage of an allowance into a normalised measurement.
///
/// `value` is percent points USED, because that is what these sources report.
/// The remaining percentage is derived and kept unrounded, so overspend above
/// 100% used survives as evidence instead of being clamped away.
///
/// # Errors
/// Returns [`QuotaIssue::NonFiniteValue`] for a value that is not finite, and
/// [`QuotaIssue::NegativeUsage`] when usage was reported below zero. Neither
/// becomes a fabricated zero.
pub(crate) fn percentage(
    value: f64,
    decimals: DecimalPrecision,
    field: &str,
) -> Result<Measurement, QuotaIssue> {
    if !value.is_finite() {
        return Err(QuotaIssue::NonFiniteValue {
            field: field.to_owned(),
        });
    }
    if value < 0.0 {
        return Err(QuotaIssue::NegativeUsage { value });
    }
    PercentageMeasurement::from_used_percent(value, decimals)
        .map(Measurement::Percentage)
        .map_err(|_| QuotaIssue::NonFiniteValue {
            field: field.to_owned(),
        })
}
