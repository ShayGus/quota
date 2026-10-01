//! Turn a Claude usage payload into normalised quota windows.
//!
//! The rules this mapping will not break: a usable named-limits array replaces
//! the three fixed windows outright; every entry keeps its own model scope; the
//! paid extra-usage summary becomes an extra-spend cap, never included quota,
//! and never takes part in ranking. An amount without a usable scale stays
//! uninterpreted instead of inventing a currency.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::{Measurement, UnavailableReason};
use quota_domain::quota::money::MoneyMeasurement;
use quota_domain::quota::units::CurrencyCode;
use quota_domain::quota::window::{MetricRole, QuotaCategory, QuotaWindow, WindowSemantics};

use crate::claude::wire::{ClaudeExtraUsage, ClaudeLimit, ClaudeUsage, ClaudeWindow};
use crate::decode::{self, DecodedUsage, Numberish, WindowDraft, percentage};

/// The duration of the short rolling window, in seconds.
const SESSION_SECONDS: i64 = 18_000;

/// The duration of the weekly window, in seconds.
const WEEKLY_SECONDS: i64 = 604_800;

/// The scale used when the payload omits the decimal-place count.
const DEFAULT_SCALE: u8 = 2;

/// The largest decimal scale this crate will interpret.
const MAX_SCALE: u8 = 9;

/// The field name reported when a utilisation value is unusable.
const USED_FIELD: &str = "utilization";

/// The currency assumed when the payload omits one.
///
/// The observed payload carries no currency, and every extra-usage amount this
/// route reports is denominated in United States dollars. A payload that names
/// a currency uses that one instead.
const DEFAULT_CURRENCY: &str = "USD";

/// Decodes one Claude usage payload into windows.
pub(crate) fn decode(
    usage: &ClaudeUsage,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    if usage.is_empty() {
        return Err(ProviderError::InvalidData {
            detail: "the payload carried no usage window".to_owned(),
        });
    }
    let mut decoded = DecodedUsage::new();
    if usable_limits(&usage.limits) {
        for (index, limit) in usage.limits.iter().enumerate() {
            // An entry without a percentage states nothing about a limit.
            if limit.percent.is_none() {
                continue;
            }
            decoded.push(named_limit(limit, index, pool, received_at)?);
        }
    } else {
        fixed_windows(usage, pool, received_at, &mut decoded)?;
    }
    if let Some(extra) = usage.extra_usage.as_ref() {
        decoded.push(extra_usage(extra, pool, received_at)?);
    }
    Ok(decoded)
}

/// Whether the named-limits array is usable and replaces the fixed windows.
pub(crate) fn usable_limits(limits: &[ClaudeLimit]) -> bool {
    !limits.is_empty() && limits.iter().any(|entry| entry.percent.is_some())
}

/// Decodes the three fixed windows, keeping the missing ones explicit.
fn fixed_windows(
    usage: &ClaudeUsage,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
    decoded: &mut DecodedUsage,
) -> Result<(), ProviderError> {
    let session = WindowDraft {
        provider: ProviderId::Claude,
        pool_id: pool,
        category: QuotaCategory::Session,
        resource: "account",
        resource_label: "Claude account",
        bucket_id: Some("five-hour"),
        metric_role: MetricRole::IncludedAllowance,
        semantics: WindowSemantics::RollingPeriod,
        duration_seconds: Some(SESSION_SECONDS),
        received_at,
    };
    // The five-hour allowance is the one window this source always offers.
    match usage.five_hour.as_ref() {
        Some(window) => decoded.push(fixed_window(window, &session)?),
        None => decoded.push(session.reported_missing()?),
    }

    let weekly = WindowDraft {
        provider: ProviderId::Claude,
        pool_id: pool,
        category: QuotaCategory::Weekly,
        resource: "account",
        resource_label: "Claude account",
        bucket_id: Some("weekly"),
        metric_role: MetricRole::IncludedAllowance,
        semantics: WindowSemantics::RollingPeriod,
        duration_seconds: Some(WEEKLY_SECONDS),
        received_at,
    };
    match usage.seven_day.as_ref() {
        Some(window) => decoded.push(fixed_window(window, &weekly)?),
        None => decoded.push(weekly.reported_missing()?),
    }

    // The Opus weekly allowance is model-scoped and optional: a plan without it
    // simply has no such window, so nothing is fabricated when it is absent.
    if let Some(window) = usage.seven_day_opus.as_ref() {
        let opus = WindowDraft {
            provider: ProviderId::Claude,
            pool_id: pool,
            category: QuotaCategory::Weekly,
            resource: "opus",
            resource_label: "Claude Opus",
            bucket_id: Some("weekly-opus"),
            metric_role: MetricRole::IncludedAllowance,
            semantics: WindowSemantics::RollingPeriod,
            duration_seconds: Some(WEEKLY_SECONDS),
            received_at,
        };
        decoded.push(fixed_window(window, &opus)?);
    }
    Ok(())
}

/// Builds one fixed window from its reported utilisation.
fn fixed_window(
    wire: &ClaudeWindow,
    draft: &WindowDraft<'_>,
) -> Result<QuotaWindow, ProviderError> {
    let boundary = wire.reset().and_then(decode::reset_instant);
    let Some(reported) = wire.utilization.as_ref() else {
        return draft.build(
            Measurement::Unavailable(UnavailableReason::NotReported),
            boundary,
            Vec::new(),
        );
    };
    match decode_used(reported) {
        Ok(measurement) => draft.build(measurement, boundary, Vec::new()),
        Err(issue) => draft.build(
            Measurement::Unavailable(UnavailableReason::InvalidResponse),
            boundary,
            vec![issue],
        ),
    }
}

/// Builds one named limit, keeping its own group and model scope.
fn named_limit(
    limit: &ClaudeLimit,
    index: usize,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<QuotaWindow, ProviderError> {
    let model = limit.scope.as_ref().and_then(|scope| scope.model.as_ref());
    let model_id = model.and_then(|model| model.id.as_deref());
    let model_label = model.and_then(|model| model.display_name.as_deref());
    let group = limit.group.as_deref().or(limit.kind.as_deref());
    let resource = model_id.unwrap_or("account");
    let label = model_label.or(group).unwrap_or("Claude limit").to_owned();
    // A bucket named "primary" is the provider's own name for this slot, so it
    // keeps the reported bucket identifier rather than inventing a period name.
    let bucket = format!("{}-{index}", decode::identifier(group.unwrap_or(resource)));
    let draft = WindowDraft {
        provider: ProviderId::Claude,
        pool_id: pool,
        category: category_for(group),
        resource,
        resource_label: &label,
        bucket_id: Some(&bucket),
        metric_role: MetricRole::IncludedAllowance,
        semantics: semantics_for(group),
        duration_seconds: duration_for(group),
        received_at,
    };
    let boundary = limit.resets_at.as_ref().and_then(decode::reset_instant);
    let Some(reported) = limit.percent.as_ref() else {
        return draft.build(
            Measurement::Unavailable(UnavailableReason::NotReported),
            boundary,
            Vec::new(),
        );
    };
    match decode_used(reported) {
        Ok(measurement) => draft.build(measurement, boundary, Vec::new()),
        Err(issue) => draft.build(
            Measurement::Unavailable(UnavailableReason::InvalidResponse),
            boundary,
            vec![issue],
        ),
    }
}

/// The category a named limit's own group text proves, never a guess.
///
/// The provider names these groups after the period they cover, so the name is
/// the evidence: `five_hour` is the five-hour allowance and `seven_day` is the
/// seven-day one. A name outside this vocabulary keeps the custom category
/// rather than being forced into a period nobody claimed.
fn category_for(group: Option<&str>) -> QuotaCategory {
    let Some(group) = group else {
        return QuotaCategory::Custom;
    };
    let text = group.to_ascii_lowercase().replace([' ', '-'], "_");
    if text.contains("five_hour") || text.contains("5h") || text.contains("session") {
        return QuotaCategory::Session;
    }
    if text.contains("seven_day") || text.contains("7d") || text.contains("week") {
        return QuotaCategory::Weekly;
    }
    if text.contains("month") {
        return QuotaCategory::Monthly;
    }
    // A one-day group is checked after the seven-day one, so `seven_day` never
    // lands here.
    if text.contains("day") || text.contains("24h") {
        return QuotaCategory::Daily;
    }
    QuotaCategory::Custom
}

/// What a named limit's group text says about its period.
fn semantics_for(group: Option<&str>) -> WindowSemantics {
    match category_for(group) {
        QuotaCategory::Session | QuotaCategory::Weekly => WindowSemantics::RollingPeriod,
        QuotaCategory::Monthly => WindowSemantics::CalendarCycle,
        _ => WindowSemantics::Unknown,
    }
}

/// The duration a named limit's group text proves, when it proves one.
fn duration_for(group: Option<&str>) -> Option<i64> {
    match category_for(group) {
        QuotaCategory::Session => Some(SESSION_SECONDS),
        QuotaCategory::Weekly => Some(WEEKLY_SECONDS),
        _ => None,
    }
}

/// Builds the paid extra-usage cap, which never takes part in ranking.
fn extra_usage(
    extra: &ClaudeExtraUsage,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<QuotaWindow, ProviderError> {
    let draft = WindowDraft {
        provider: ProviderId::Claude,
        pool_id: pool,
        category: QuotaCategory::Monthly,
        resource: "extra-usage",
        resource_label: "Extra usage",
        bucket_id: Some("extra-usage"),
        metric_role: MetricRole::ExtraSpendCap,
        semantics: WindowSemantics::CalendarCycle,
        duration_seconds: None,
        received_at,
    };
    if extra.is_enabled == Some(false) {
        return draft.build(Measurement::NotEntitled, None, Vec::new());
    }
    let Some(scale) = scale(extra) else {
        return draft.invalid(vec![QuotaIssue::NonFiniteValue {
            field: "extra_usage.decimal_places".to_owned(),
        }]);
    };
    let Some(currency) = currency(extra) else {
        return draft.invalid(vec![QuotaIssue::UnsupportedSchemaVersion { version: 0 }]);
    };
    let limit = minor_units(extra.monthly_limit.as_ref());
    let used = minor_units(extra.used_credits.as_ref());
    let mut issues = Vec::new();
    for (reported, decoded, field) in [
        (&extra.monthly_limit, limit, "extra_usage.monthly_limit"),
        (&extra.used_credits, used, "extra_usage.used_credits"),
    ] {
        if reported.is_some() && decoded.is_none() {
            issues.push(QuotaIssue::NonFiniteValue {
                field: field.to_owned(),
            });
        }
    }
    if !issues.is_empty() {
        return draft.invalid(issues);
    }
    if limit.is_none() && used.is_none() {
        // No amount arrived, so no amount is invented.
        return draft.reported_missing();
    }
    draft.build(
        Measurement::Money(MoneyMeasurement {
            currency,
            scale,
            used_minor_units: used,
            remaining_minor_units: limit.zip(used).map(|(limit, used)| limit - used),
            limit_minor_units: limit,
        }),
        None,
        Vec::new(),
    )
}

/// The decimal scale the amounts use, when it is interpretable.
fn scale(extra: &ClaudeExtraUsage) -> Option<u8> {
    let Some(reported) = extra.decimal_places.as_ref() else {
        return Some(DEFAULT_SCALE);
    };
    let field = reported.field()?;
    if field.value < 0.0 || field.value > f64::from(MAX_SCALE) || field.value.fract() != 0.0 {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is a small non-negative decimal-place count"
    )]
    let places = field.value.trunc() as u8;
    if places > MAX_SCALE {
        return None;
    }
    Some(places)
}

/// The currency the amounts are in, when the payload names a usable one.
fn currency(extra: &ClaudeExtraUsage) -> Option<CurrencyCode> {
    match extra.currency.as_deref() {
        Some(text) => CurrencyCode::new(text.trim().to_uppercase()).ok(),
        None => CurrencyCode::new(DEFAULT_CURRENCY).ok(),
    }
}

/// Converts a reported amount into whole minor units at the reported scale.
///
/// The reported amounts are already minor units, and `decimal_places` is the
/// scale that turns them into major units: 5000 with a scale of two is 50.00.
/// The value is therefore stored unchanged, and the scale travels with it. See
/// the extra-usage note in `README.md` for what this interpretation rests on.
fn minor_units(reported: Option<&Numberish>) -> Option<i64> {
    let field = reported?.field()?;
    if field.value < 0.0 || field.value.fract().abs() > f64::EPSILON {
        return None;
    }
    if field.value.abs() >= 9_007_199_254_740_992.0 {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the value is a bounded, integral minor-unit amount"
    )]
    Some(field.value.trunc() as i64)
}

/// Decodes the reported used percentage, or the issue that makes it unusable.
fn decode_used(reported: &Numberish) -> Result<Measurement, QuotaIssue> {
    let Some(field) = reported.field() else {
        return Err(QuotaIssue::NonFiniteValue {
            field: USED_FIELD.to_owned(),
        });
    };
    percentage(field.value, field.decimals, USED_FIELD)
}
