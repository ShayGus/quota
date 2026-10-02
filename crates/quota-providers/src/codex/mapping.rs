//! Turn a Codex usage payload into normalised quota windows.
//!
//! The rules this mapping will not break: a window is only labelled a session
//! when the source itself reported a 18000-second duration, and only labelled
//! weekly at 604800 seconds; a named bucket keeps its own category. The one
//! addition is the account's own first window: a plan that reports a single
//! allowance covering at least one whole period is that allowance, so it is
//! labelled for the period it reports and no second allowance is invented for
//! it. This follows the TaskbarQuota provider investigation report, section 12,
//! Codex P1 row "Support credits-only and lone monthly responses", with acceptance
//! evidence "Credits-only connects. No fabricated secondary allowance."
//! A credit balance is a balance, never included quota.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::{Measurement, QuantityMeasurement, UnavailableReason};
use quota_domain::quota::units::QuotaUnit;
use quota_domain::quota::window::{MetricRole, QuotaCategory, QuotaWindow, WindowSemantics};

use crate::codex::wire::{CodexCredits, CodexEnvelope, CodexLimitSet, CodexWindow};
use crate::decode::{self, DecodedUsage, Numberish, WindowDraft, masked_address, percentage};

/// The duration that proves a session window, in seconds.
const SESSION_SECONDS: i64 = 18_000;

/// The duration that proves a weekly window, in seconds.
const WEEKLY_SECONDS: i64 = 604_800;

/// The shortest duration that covers a whole named period, in seconds.
const WHOLE_PERIOD_SECONDS: i64 = 86_400;

/// The duration that proves a monthly allowance, in seconds.
const MONTHLY_SECONDS: i64 = 20 * WHOLE_PERIOD_SECONDS;

/// The field name reported when the used percentage is unusable.
const USED_FIELD: &str = "used_percent";

/// Decodes one Codex payload into windows.
pub(crate) fn decode(
    envelope: &CodexEnvelope,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let sets = envelope.limit_sets();
    let mut usage = DecodedUsage::new();
    usage.plan_label = envelope
        .plan_type
        .clone()
        .or_else(|| {
            envelope
                .rate_limit
                .as_ref()
                .and_then(|set| set.plan_type.clone())
        })
        .or_else(|| sets.iter().find_map(|set| set.plan_type.clone()));
    usage.principal_label = envelope.email.as_deref().map(masked_address);
    account_windows(&sets, pool, received_at, &mut usage)?;
    named_windows(&sets, pool, received_at, &mut usage)?;
    if let Some(credits) = envelope.credits.as_ref() {
        usage.push(credit_window(credits, pool, received_at)?);
    }
    if !usage.windows.iter().any(|window| {
        window.metric_role != MetricRole::CreditBalance
            || matches!(
                &window.measurement,
                Measurement::Quantity(_) | Measurement::Unlimited
            )
    }) {
        return Err(ProviderError::InvalidData {
            detail: "the payload carried neither a rate-limit window nor a credit balance"
                .to_owned(),
        });
    }
    Ok(usage)
}

/// The account's own allowance windows, from the first block that reports one.
fn account_windows(
    sets: &[&CodexLimitSet],
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
    usage: &mut DecodedUsage,
) -> Result<(), ProviderError> {
    let reported = sets.iter().find_map(|set| {
        let (first, second) = set.pair();
        (first.is_some() || second.is_some()).then_some((first, second))
    });
    let Some((primary, secondary)) = reported else {
        return Ok(());
    };
    match (primary, secondary) {
        (Some(first), Some(second)) => {
            usage.push(account_window(first, "primary", pool, received_at)?);
            usage.push(account_window(second, "secondary", pool, received_at)?);
        }
        // A lone first window is promoted rather than paired, so a plan with one
        // allowance is not reported as permanently missing a second one.
        (Some(only), None) => {
            usage.push(account_window(only, "primary", pool, received_at)?);
            if only.duration_seconds().is_none_or(is_shorter_than) {
                usage.push(
                    account_draft(pool, "secondary", QuotaCategory::Custom, None, received_at)
                        .reported_missing()?,
                );
            }
        }
        // A lone second window is the account's only allowance, so it takes the
        // first slot rather than being reported as a missing pair.
        (None, Some(only)) => {
            usage.push(account_window(only, "primary", pool, received_at)?);
        }
        // Unreachable: the search only returns a pair that has a member.
        (None, None) => {}
    }
    Ok(())
}

/// Whether a duration is too short to stand as a whole period of its own.
fn is_shorter_than(duration: i64) -> bool {
    duration < WHOLE_PERIOD_SECONDS
}

/// The review, model, and other named allowances a body reported.
///
/// Every block is walked, because a body may report these at its root while the
/// account pair sits under a named container.
fn named_windows(
    sets: &[&CodexLimitSet],
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
    usage: &mut DecodedUsage,
) -> Result<(), ProviderError> {
    let mut reported = HashSet::new();
    for set in sets {
        if let Some(review) = set.code_review_rate_limit.as_ref() {
            for (suffix, window) in [("", review.pair().0), ("-secondary", review.pair().1)] {
                if let Some(window) = window {
                    let bucket = format!("code-review{suffix}");
                    if !reported.insert(bucket.clone()) {
                        continue;
                    }
                    usage.push(named_window(
                        window,
                        pool,
                        &bucket,
                        "Code review",
                        received_at,
                    )?);
                }
            }
        }
        for (identifier, label, pair) in set.buckets() {
            for (suffix, window) in [("", pair.0), ("-secondary", pair.1)] {
                let Some(window) = window else { continue };
                let bucket = format!("{identifier}{suffix}");
                if !reported.insert(bucket.clone()) {
                    continue;
                }
                usage.push(named_window(window, pool, &bucket, &label, received_at)?);
            }
        }
    }
    Ok(())
}

/// Builds one account-wide window, labelled by its own reported duration.
fn account_window(
    wire: &CodexWindow,
    bucket: &str,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<QuotaWindow, ProviderError> {
    let duration = wire.duration_seconds();
    let draft = account_draft(
        pool,
        bucket,
        account_category(duration),
        duration,
        received_at,
    );
    let (measurement, boundary, issues) = read_window(wire, received_at);
    draft.build(measurement, boundary, issues)
}

/// Builds the draft for an account-wide primary or secondary window.
fn account_draft<'a>(
    pool: &'a QuotaPoolId,
    bucket: &'a str,
    category: QuotaCategory,
    duration_seconds: Option<i64>,
    received_at: DateTime<Utc>,
) -> WindowDraft<'a> {
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
                    precision: field.decimals,
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

/// The category a named bucket's own duration proves, never a guess.
fn category_for(duration_seconds: Option<i64>) -> QuotaCategory {
    match duration_seconds {
        Some(SESSION_SECONDS) => QuotaCategory::Session,
        Some(WEEKLY_SECONDS) => QuotaCategory::Weekly,
        _ => QuotaCategory::Custom,
    }
}

/// The category the account's own first window proves from its duration.
///
/// A lone window covering twenty days or more is a monthly allowance. Named
/// buckets deliberately keep [`category_for`], because a bucket's name is what
/// identifies it, not its length.
fn account_category(duration_seconds: Option<i64>) -> QuotaCategory {
    match category_for(duration_seconds) {
        QuotaCategory::Custom if duration_seconds.is_some_and(is_monthly) => QuotaCategory::Monthly,
        other => other,
    }
}

/// Whether a duration is at least the longest period Codex reports.
fn is_monthly(duration_seconds: i64) -> bool {
    duration_seconds >= MONTHLY_SECONDS
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
