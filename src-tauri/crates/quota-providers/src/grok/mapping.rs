//! Turn the Grok CLI's billing answers into quota windows.
//!
//! A `SuperGrok` account reports weekly credits: the share used overall, and
//! each product's share (Grok Build, the API). An account on unified billing
//! reports a monthly included quota instead. Either may carry an on-demand
//! spend cap, which is a cap, not included quota.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::scope::ACCOUNT_RESOURCE;
use quota_domain::quota::units::DecimalPrecision;
use quota_domain::quota::window::{MetricRole, QuotaCategory, QuotaWindow, WindowSemantics};

use crate::decode::{self, DecodedUsage, Numberish, WindowDraft};
use crate::grok::wire::{Amount, BillingConfig};

/// Decodes the weekly answer and, when the account has one, the monthly one.
pub(crate) fn decode(
    weekly: Option<&BillingConfig>,
    monthly: Option<&BillingConfig>,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let mut decoded = DecodedUsage::new();
    let current = weekly.and_then(|config| Some((config, weekly_period(config, received_at)?)));
    if let Some((config, WeeklyPeriod(ends))) = current {
        let used = config
            .credit_usage_percent
            .as_ref()
            .and_then(Numberish::field)
            .map_or(0.0, |field| field.value);
        let spec = Spec::included(
            QuotaCategory::Weekly,
            ACCOUNT_RESOURCE,
            "SuperGrok",
            "credits",
        );
        decoded.push(spec.percent(pool, used, ends, received_at)?);
        for product in &config.product_usage {
            let Some(name) = product
                .product
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
            else {
                continue;
            };
            let used = product
                .usage_percent
                .as_ref()
                .and_then(Numberish::field)
                .map_or(0.0, |field| field.value);
            let resource = decode::identifier(name);
            let label = product_label(name);
            let spec = Spec::included(QuotaCategory::Weekly, &resource, &label, &resource);
            decoded.push(spec.percent(pool, used, ends, received_at)?);
        }
        push_on_demand(&mut decoded, config, pool, received_at)?;
    }
    if let Some(config) = monthly {
        let limit = config.monthly_limit.as_ref().and_then(Amount::value);
        let used = config.used.as_ref().and_then(Amount::value);
        if let Some((used, limit)) = used.zip(limit.filter(|limit| *limit > 0.0)) {
            let ends = config.billing_period_end.as_deref().and_then(instant);
            let spec = Spec::included(
                QuotaCategory::Monthly,
                ACCOUNT_RESOURCE,
                "SuperGrok",
                "included",
            );
            decoded.push(spec.percent(pool, used / limit * 100.0, ends, received_at)?);
            if weekly.is_none() {
                push_on_demand(&mut decoded, config, pool, received_at)?;
            }
        }
    }
    if decoded.windows.is_empty() {
        return Err(ProviderError::InvalidData {
            detail: "the billing answer carried no usage".to_owned(),
        });
    }
    Ok(decoded)
}

/// A current weekly period, and its end when the answer gives one.
struct WeeklyPeriod(Option<DateTime<Utc>>);

/// The weekly period, when the answer describes one that has not ended.
fn weekly_period(config: &BillingConfig, now: DateTime<Utc>) -> Option<WeeklyPeriod> {
    let period = config.current_period.as_ref()?;
    let weekly = period
        .kind
        .as_deref()
        .is_some_and(|kind| kind.to_ascii_uppercase().contains("WEEK"));
    let ends = period.end.as_deref().and_then(instant);
    // A fresh period with no usage yet omits the percentage: that is zero,
    // but only while the period is current.
    let current = ends.is_some_and(|ends| ends > now) || config.credit_usage_percent.is_some();
    (weekly && current).then_some(WeeklyPeriod(ends))
}

/// The on-demand cap, when the account has one.
fn push_on_demand(
    decoded: &mut DecodedUsage,
    config: &BillingConfig,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<(), ProviderError> {
    let cap = config.on_demand_cap.as_ref().and_then(Amount::value);
    let used = config.on_demand_used.as_ref().and_then(Amount::value);
    if let Some((used, cap)) = used.zip(cap.filter(|cap| *cap > 0.0)) {
        let spec = Spec {
            category: QuotaCategory::Custom,
            resource: "on-demand",
            label: "On-demand",
            bucket: "on-demand",
            role: MetricRole::ExtraSpendCap,
        };
        decoded.push(spec.percent(pool, used / cap * 100.0, None, received_at)?);
    }
    Ok(())
}

/// One window's description.
struct Spec<'a> {
    category: QuotaCategory,
    resource: &'a str,
    label: &'a str,
    bucket: &'a str,
    role: MetricRole,
}

impl<'a> Spec<'a> {
    const fn included(
        category: QuotaCategory,
        resource: &'a str,
        label: &'a str,
        bucket: &'a str,
    ) -> Self {
        Self {
            category,
            resource,
            label,
            bucket,
            role: MetricRole::IncludedAllowance,
        }
    }

    fn percent(
        &self,
        pool: &QuotaPoolId,
        used: f64,
        ends: Option<DateTime<Utc>>,
        received_at: DateTime<Utc>,
    ) -> Result<QuotaWindow, ProviderError> {
        let draft = WindowDraft {
            provider: ProviderId::Grok,
            pool_id: pool,
            category: self.category,
            resource: self.resource,
            resource_label: self.label,
            bucket_id: Some(self.bucket),
            metric_role: self.role,
            semantics: WindowSemantics::AnchoredPeriod,
            duration_seconds: (self.category == QuotaCategory::Weekly).then_some(604_800),
            received_at,
        };
        match decode::percentage(used, DecimalPrecision::ONE_PLACE, self.bucket) {
            Ok(measurement) => draft.build(measurement, ends, Vec::new()),
            Err(issue) => draft.invalid(vec![issue]),
        }
    }
}

/// A product's name as a person reads it.
fn product_label(product: &str) -> String {
    match product {
        "GrokBuild" => "Grok Build".to_owned(),
        "Api" => "API".to_owned(),
        other => other.to_owned(),
    }
}

/// A date string as an instant.
fn instant(text: &str) -> Option<DateTime<Utc>> {
    decode::reset_instant(&Numberish::Text(text.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grok::wire::BillingEnvelope;

    fn config(body: serde_json::Value) -> BillingConfig {
        serde_json::from_value::<BillingEnvelope>(body)
            .expect("the body decodes")
            .config
            .expect("a config")
    }

    fn pool() -> QuotaPoolId {
        QuotaPoolId::new("pool-1").expect("a pool id")
    }

    #[test]
    fn weekly_credits_give_the_account_and_each_product_a_window() {
        let weekly = config(serde_json::json!({ "config": {
            "currentPeriod": { "start": "2026-10-01T00:00:00Z", "end": "2026-10-08T00:00:00Z",
                               "type": "BILLING_PERIOD_TYPE_WEEKLY" },
            "creditUsagePercent": 30,
            "productUsage": [ { "product": "GrokBuild", "usagePercent": 20 } ],
            "onDemandCap": { "val": 1000 }, "onDemandUsed": { "val": 250 }
        }}));
        let now = DateTime::parse_from_rfc3339("2026-10-03T00:00:00Z")
            .expect("a date")
            .with_timezone(&Utc);
        let usage = decode(Some(&weekly), None, &pool(), now).expect("the usage decodes");
        assert_eq!(usage.windows.len(), 3);
        let account = &usage.windows[0];
        assert_eq!(account.category, QuotaCategory::Weekly);
        let left = account
            .measurement
            .remaining_percent()
            .expect("a percentage");
        assert!((left.value() - 70.0).abs() < f64::EPSILON);
        assert_eq!(usage.windows[1].scope.label(), "Grok Build");
        assert_eq!(usage.windows[2].metric_role, MetricRole::ExtraSpendCap);
        assert!(usage.windows[2].measurement.remaining_percent().is_some());
    }

    #[test]
    fn unified_billing_reads_the_monthly_included_quota() {
        let monthly = config(serde_json::json!({ "config": {
            "isUnifiedBillingUser": true,
            "billingPeriodStart": "2026-10-01T00:00:00Z", "billingPeriodEnd": "2026-11-01T00:00:00Z",
            "monthlyLimit": { "val": 2000 }, "used": { "val": 500 }
        }}));
        let usage = decode(None, Some(&monthly), &pool(), Utc::now()).expect("the usage decodes");
        assert_eq!(usage.windows[0].category, QuotaCategory::Monthly);
        let left = usage.windows[0]
            .measurement
            .remaining_percent()
            .expect("a percentage");
        assert!((left.value() - 75.0).abs() < f64::EPSILON);
    }
}
