//! Turn a Codex usage payload into normalised quota windows.
//!
//! The rules this mapping will not break: a window is only labelled a session
//! when the source itself reported a 18000-second duration, and only labelled
//! weekly at 604800 seconds; anything else keeps its own scope and its own
//! category. Codex has no monthly allowance, so no monthly window is ever
//! created for it. A credit balance is a balance, never included quota.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::{Measurement, QuantityMeasurement, UnavailableReason};
use quota_domain::quota::units::{DecimalPrecision, QuotaUnit};
use quota_domain::quota::window::{MetricRole, QuotaCategory, QuotaWindow, WindowSemantics};

use crate::codex::wire::{CodexCredits, CodexEnvelope, CodexWindow};
use crate::decode::{self, DecodedUsage, Numberish, WindowDraft, masked_address, percentage};

/// The duration that proves a session window, in seconds.
const SESSION_SECONDS: i64 = 18_000;

/// The duration that proves a weekly window, in seconds.
const WEEKLY_SECONDS: i64 = 604_800;

/// The field name reported when the used percentage is unusable.
const USED_FIELD: &str = "used_percent";

/// Decodes one Codex payload into windows.
pub(crate) fn decode(
    envelope: &CodexEnvelope,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let limits = envelope.rate_limit.as_ref().unwrap_or(&envelope.root);
    if limits.is_empty() {
        return Err(ProviderError::InvalidData {
            detail: "the payload carried no rate-limit block".to_owned(),
        });
    }
    let mut usage = DecodedUsage::new();
    usage.plan_label = envelope
        .plan_type
        .clone()
        .or_else(|| limits.plan_type.clone());
    usage.principal_label = envelope.email.as_deref().map(masked_address);

    // The pair the source reports: the first window and, when the source offers
    // a second period, the window after it. A pair member the source did not
    // report keeps its place as a not-reported window, named by the pair slot
    // rather than by a period nobody claimed.
    for (bucket, wire) in [
        ("primary", limits.primary_window.as_ref()),
        ("secondary", limits.secondary_window.as_ref()),
    ] {
        match wire {
            Some(window) => {
                let duration = window.duration_seconds();
                let category = category_for(duration);
                let draft = account_draft(pool, bucket, category, duration, received_at);
                let (measurement, boundary, issues) = read_window(window, received_at);
                usage.push(draft.build(measurement, boundary, issues)?);
            }
            None => {
                let draft = account_draft(pool, bucket, QuotaCategory::Custom, None, received_at);
                usage.push(draft.reported_missing()?);
            }
        }
    }

    if let Some(window) = limits.code_review_rate_limit.as_ref() {
        usage.push(named_window(
            window,
            pool,
            "code-review",
            "Code review",
            received_at,
        )?);
    }

    for (bucket, label, window) in limits.buckets() {
        usage.push(named_window(window, pool, &bucket, &label, received_at)?);
    }

    if let Some(credits) = envelope.credits.as_ref() {
        usage.push(credit_window(credits, pool, received_at)?);
    }

    Ok(usage)
}

/// Builds one named bucket window, such as a model-specific allowance.
fn named_window(
    wire: &CodexWindow,
    pool: &QuotaPoolId,
    bucket: &str,
    label: &str,
    received_at: DateTime<Utc>,
) -> Result<QuotaWindow, ProviderError> {
    let duration = wire.duration_seconds();
    let draft = WindowDraft {
        provider: ProviderId::Codex,
        pool_id: pool,
        category: category_for(duration),
        resource: bucket,
        resource_label: label,
        bucket_id: Some(bucket),
        metric_role: MetricRole::IncludedAllowance,
        // The source never says what its reported boundary means, so the
        // boundary stays an unspecified provider event.
        semantics: WindowSemantics::Unknown,
        duration_seconds: duration,
        received_at,
    };
    let (measurement, boundary, issues) = read_window(wire, received_at);
    draft.build(measurement, boundary, issues)
}

/// Builds the draft for an account-wide primary or secondary window.
fn account_draft(
    pool: &QuotaPoolId,
    bucket: &str,
    category: QuotaCategory,
    duration_seconds: Option<i64>,
    received_at: DateTime<Utc>,
) -> WindowDraft<'_> {
    WindowDraft {
        provider: ProviderId::Codex,
        pool_id: pool,
        category,
        resource: "account",
        resource_label: "Codex account",
        bucket_id: Some(bucket),
        metric_role: MetricRole::IncludedAllowance,
        semantics: WindowSemantics::Unknown,
        duration_seconds,
        received_at,
    }
}

/// Builds the informational credit balance, which is never included quota.
fn credit_window(
    credits: &CodexCredits,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<QuotaWindow, ProviderError> {
    let draft = WindowDraft {
        provider: ProviderId::Codex,
        pool_id: pool,
        category: QuotaCategory::Custom,
        resource: "credits",
        resource_label: "Credit balance",
        bucket_id: Some("credits"),
        metric_role: MetricRole::CreditBalance,
        semantics: WindowSemantics::Unknown,
        duration_seconds: None,
        received_at,
    };
    if credits.unlimited.unwrap_or(false) {
        return draft.build(Measurement::Unlimited, None, Vec::new());
    }
    match credits.balance.as_ref() {
        Some(balance) => match balance.field() {
            Some(field) if field.value >= 0.0 => draft.build(
                Measurement::Quantity(QuantityMeasurement {
                    unit: QuotaUnit::Credits,
                    precision: DecimalPrecision::new(field.decimals)
                        .unwrap_or(DecimalPrecision::MAX),
                    used: None,
                    remaining: Some(field.value),
                    // A balance has no ceiling, so it never yields a percentage.
                    limit: None,
                }),
                None,
                Vec::new(),
            ),
            _ => draft.invalid(vec![QuotaIssue::NonFiniteValue {
                field: "credits.balance".to_owned(),
            }]),
        },
        None => draft.reported_missing(),
    }
}

/// The category a reported duration proves, never a guess.
fn category_for(duration_seconds: Option<i64>) -> QuotaCategory {
    match duration_seconds {
        Some(SESSION_SECONDS) => QuotaCategory::Session,
        Some(WEEKLY_SECONDS) => QuotaCategory::Weekly,
        _ => QuotaCategory::Custom,
    }
}

/// Reads the used percentage, the reported boundary, and any validation issue.
fn read_window(
    wire: &CodexWindow,
    received_at: DateTime<Utc>,
) -> (Measurement, Option<DateTime<Utc>>, Vec<QuotaIssue>) {
    let boundary = wire
        .reset_at
        .as_ref()
        .and_then(decode::reset_instant)
        .or_else(|| {
            wire.reset_after_seconds
                .as_ref()
                .and_then(|value| decode::reset_after(value, received_at))
        });
    let Some(reported) = wire.used_percent.as_ref() else {
        return (
            Measurement::Unavailable(UnavailableReason::NotReported),
            boundary,
            Vec::new(),
        );
    };
    match decode_used(reported) {
        Ok(measurement) => (measurement, boundary, Vec::new()),
        Err(issue) => (
            Measurement::Unavailable(UnavailableReason::InvalidResponse),
            boundary,
            vec![issue],
        ),
    }
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
