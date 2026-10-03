//! Turn Cursor's usage summary into quota windows.
//!
//! Every allowance runs over the billing cycle, so each window is monthly and
//! ends at `billingCycleEnd`. A plan reports its included usage as two
//! shares, Cursor's own models and other models; an older answer gives one
//! total; an enterprise seat gives an amount. On-demand spend beyond the plan
//! is a spend cap, not included quota.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::measurement::Measurement;
use quota_domain::quota::money::MoneyMeasurement;
use quota_domain::quota::scope::ACCOUNT_RESOURCE;
use quota_domain::quota::units::CurrencyCode;
use quota_domain::quota::window::{MetricRole, QuotaCategory, WindowSemantics};

use crate::cursor::wire::{Bucket, UsageSummary};
use crate::decode::{self, DecodedUsage, Numberish, WindowDraft};

/// Decodes one usage summary.
pub(crate) fn decode(
    summary: &UsageSummary,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let usage = summary.individual_usage.as_ref();
    let ends = summary
        .billing_cycle_end
        .as_ref()
        .and_then(|end| decode::reset_epoch(end).or_else(|| decode::reset_instant(end)));
    let window = |resource: &str, label: &str, role, measurement| {
        draft(pool, resource, label, role, received_at).build(measurement, ends, Vec::new())
    };
    let mut decoded = DecodedUsage::new();
    let included = MetricRole::IncludedAllowance;
    if let Some(amount) = usage
        .and_then(|usage| usage.overall.as_ref())
        .and_then(cents)
    {
        decoded.push(window(ACCOUNT_RESOURCE, "Plan usage", included, amount?)?);
    } else if let Some(plan) = usage
        .and_then(|usage| usage.plan.as_ref())
        .filter(|plan| plan.enabled != Some(false))
    {
        let auto = percent(plan.auto_percent_used.as_ref());
        let api = percent(plan.api_percent_used.as_ref());
        if auto.is_none() && api.is_none() {
            let total = percent(plan.total_percent_used.as_ref()).or_else(|| cents(plan));
            if let Some(total) = total {
                decoded.push(window(ACCOUNT_RESOURCE, "Plan usage", included, total?)?);
            }
        }
        if let Some(auto) = auto {
            decoded.push(window("cursor-models", "Cursor models", included, auto?)?);
        }
        if let Some(api) = api {
            decoded.push(window("other-models", "Other models", included, api?)?);
        }
    }
    if let Some(on_demand) = usage
        .and_then(|usage| usage.on_demand.as_ref())
        .filter(|bucket| positive(bucket.limit.as_ref()))
        .and_then(cents)
    {
        decoded.push(window(
            "on-demand",
            "On-demand",
            MetricRole::ExtraSpendCap,
            on_demand?,
        )?);
    }
    if decoded.windows.is_empty() {
        return Err(ProviderError::InvalidData {
            detail: "the usage summary carried no usage".to_owned(),
        });
    }
    decoded.plan_label = summary
        .membership_type
        .as_deref()
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
        .map(plan_label);
    Ok(decoded)
}

fn draft<'a>(
    pool: &'a QuotaPoolId,
    resource: &'a str,
    label: &'a str,
    role: MetricRole,
    received_at: DateTime<Utc>,
) -> WindowDraft<'a> {
    WindowDraft {
        provider: ProviderId::Cursor,
        pool_id: pool,
        category: QuotaCategory::Monthly,
        resource,
        resource_label: label,
        bucket_id: Some(resource),
        metric_role: role,
        semantics: WindowSemantics::CalendarCycle,
        duration_seconds: None,
        received_at,
    }
}

/// A reported percentage used, or `None` when the share was not reported.
fn percent(reported: Option<&Numberish>) -> Option<Result<Measurement, ProviderError>> {
    let field = reported?.field();
    Some(
        field
            .and_then(|field| decode::percentage(field.value, field.decimals, "percent").ok())
            .ok_or_else(|| ProviderError::InvalidData {
                detail: "a usage share was not a number".to_owned(),
            }),
    )
}

/// An amount in cents against an allowance, or `None` without a usable one.
fn cents(bucket: &Bucket) -> Option<Result<Measurement, ProviderError>> {
    if bucket.enabled == Some(false) {
        return None;
    }
    let value = |field: Option<&Numberish>| field.and_then(Numberish::whole);
    let limit = value(bucket.limit.as_ref()).filter(|limit| *limit > 0)?;
    let used = value(bucket.used.as_ref())
        .or_else(|| Some(limit - value(bucket.remaining.as_ref())?))?
        .max(0);
    Some(
        CurrencyCode::new("USD")
            .map(|currency| {
                Measurement::Money(MoneyMeasurement {
                    currency,
                    scale: 2,
                    used_minor_units: Some(used),
                    remaining_minor_units: Some((limit - used).max(0)),
                    limit_minor_units: Some(limit),
                })
            })
            .map_err(|_| ProviderError::InvalidData {
                detail: "the currency code was rejected".to_owned(),
            }),
    )
}

fn positive(reported: Option<&Numberish>) -> bool {
    reported
        .and_then(Numberish::whole)
        .is_some_and(|value| value > 0)
}

/// The plan's name: `pro` reads "Pro".
pub(crate) fn plan_label(membership: &str) -> String {
    let mut letters = membership.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + letters.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(body: serde_json::Value) -> DecodedUsage {
        let summary: UsageSummary = serde_json::from_value(body).expect("the body decodes");
        let pool = QuotaPoolId::new("pool-1").expect("a pool id");
        decode(&summary, &pool, Utc::now()).expect("the summary decodes")
    }

    #[test]
    fn a_plan_split_by_model_kind_gives_two_monthly_shares_and_the_on_demand_cap() {
        let usage = decoded(serde_json::json!({
            "billingCycleEnd": "2026-11-01T00:00:00Z",
            "membershipType": "pro",
            "individualUsage": {
                "plan": { "enabled": true, "limit": 2000, "autoPercentUsed": 30, "apiPercentUsed": 55 },
                "onDemand": { "enabled": true, "used": 500, "limit": 2000 }
            }
        }));
        assert_eq!(usage.plan_label.as_deref(), Some("Pro"));
        let labels: Vec<&str> = usage
            .windows
            .iter()
            .map(|window| window.scope.label())
            .collect();
        assert_eq!(labels, ["Cursor models", "Other models", "On-demand"]);
        assert!(
            usage
                .windows
                .iter()
                .all(|window| window.category == QuotaCategory::Monthly)
        );
        assert!(usage.windows.iter().all(|window| window.boundary.is_some()));
        assert_eq!(usage.windows[2].metric_role, MetricRole::ExtraSpendCap);
        let left = usage.windows[2]
            .measurement
            .remaining_percent()
            .expect("a percentage");
        assert!((left.value() - 75.0).abs() < f64::EPSILON);
    }

    #[test]
    fn an_enterprise_seat_reports_its_amount_as_the_plan() {
        let usage = decoded(serde_json::json!({
            "individualUsage": { "overall": { "used": 1250, "limit": 5000 } }
        }));
        assert_eq!(usage.windows.len(), 1);
        assert_eq!(usage.windows[0].scope.resource().as_str(), ACCOUNT_RESOURCE);
        assert!(matches!(
            usage.windows[0].measurement,
            Measurement::Money(_)
        ));
    }
}
