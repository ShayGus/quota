//! Wire shapes for the Grok CLI's billing endpoint.
//!
//! `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits` answers the
//! weekly `SuperGrok` credits; without `format`, an account on unified billing
//! answers its monthly included quota instead. Both wrap the figures in
//! `config`. Amounts arrive as `{ "val": n }`. The endpoint is undocumented,
//! so every field is optional and unknown fields are ignored.

use serde::Deserialize;

use crate::decode::Numberish;

/// The response body.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct BillingEnvelope {
    /// The figures.
    #[serde(default)]
    pub(crate) config: Option<BillingConfig>,
}

/// The billing figures, weekly or monthly.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BillingConfig {
    /// The weekly period.
    #[serde(default)]
    pub(crate) current_period: Option<Period>,
    /// Percent of the weekly credits used.
    #[serde(default)]
    pub(crate) credit_usage_percent: Option<Numberish>,
    /// Percent used per product, such as Grok Build.
    #[serde(default, deserialize_with = "crate::decode::null_as_default")]
    pub(crate) product_usage: Vec<ProductUsage>,
    /// The on-demand spend cap.
    #[serde(default)]
    pub(crate) on_demand_cap: Option<Amount>,
    /// The on-demand spend so far.
    #[serde(default)]
    pub(crate) on_demand_used: Option<Amount>,
    /// Whether the account is on unified (monthly) billing.
    #[serde(default)]
    pub(crate) is_unified_billing_user: Option<bool>,
    /// The monthly period's end.
    #[serde(default)]
    pub(crate) billing_period_end: Option<String>,
    /// The monthly included quota.
    #[serde(default)]
    pub(crate) monthly_limit: Option<Amount>,
    /// What has been used of it.
    #[serde(default)]
    pub(crate) used: Option<Amount>,
}

/// The weekly period.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Period {
    /// Its end, as a date string.
    #[serde(default)]
    pub(crate) end: Option<String>,
    /// Its kind, such as `WEEKLY`.
    #[serde(default, rename = "type")]
    pub(crate) kind: Option<String>,
}

/// One product's share of the weekly credits.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductUsage {
    /// The product, such as `GrokBuild` or `Api`.
    #[serde(default)]
    pub(crate) product: Option<String>,
    /// Percent used.
    #[serde(default)]
    pub(crate) usage_percent: Option<Numberish>,
}

/// An amount, written as `{ "val": n }`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Amount {
    /// The amount.
    #[serde(default)]
    pub(crate) val: Option<Numberish>,
}

impl Amount {
    /// The amount, when it is a finite number.
    pub(crate) fn value(&self) -> Option<f64> {
        self.val.as_ref()?.field().map(|field| field.value)
    }
}
