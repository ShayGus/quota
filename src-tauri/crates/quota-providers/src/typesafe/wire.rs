//! Wire shapes for the `TypeSafe` console's billing overview.
//!
//! The console's billing page answers a Next.js server action with the
//! account's credit balance, its spend in the current cycle, and its credit
//! grants. The same answer also carries the billing address, the invoice
//! address, the payment card and the tax identifier. These shapes have no
//! field for any of them, so they are never read into Quota, kept, or logged.
//! The action is undocumented, so every field is optional and unknown fields
//! are ignored.

use serde::Deserialize;

use crate::decode::{Numberish, null_as_default};

/// The action's result: whether it succeeded, and what it returned.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct BillingResult {
    /// Whether the console answered the request.
    #[serde(default)]
    pub(crate) ok: Option<bool>,
    /// What it returned.
    #[serde(default)]
    pub(crate) data: Option<BillingData>,
}

/// The overview, without the payment history.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct BillingData {
    /// The account's billing summary.
    #[serde(default)]
    pub(crate) billing: Option<Billing>,
}

/// What the billing page shows about credit, with nothing personal.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Billing {
    /// The credit left, in dollars.
    #[serde(default)]
    pub(crate) balance: Option<Numberish>,
    /// Spent in the current cycle, in dollars.
    #[serde(default)]
    pub(crate) spent: Option<Numberish>,
    /// The current cycle, as the console names it, such as `October 2026`.
    #[serde(default, rename = "cycleLabel")]
    pub(crate) cycle_label: Option<String>,
    /// The plan, as a snake-case identifier such as `free_plan`.
    #[serde(default)]
    pub(crate) plan: Option<String>,
    /// The credit grants.
    #[serde(default, deserialize_with = "null_as_default")]
    pub(crate) credits: Vec<Credit>,
}

/// One credit grant.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Credit {
    /// How much it granted, in dollars.
    #[serde(default)]
    pub(crate) amount: Option<Numberish>,
    /// How much of it is left, in dollars.
    #[serde(default)]
    pub(crate) remaining: Option<Numberish>,
    /// When it expires.
    #[serde(default, rename = "expiresAt")]
    pub(crate) expires_at: Option<String>,
    /// What it was for, such as `free_tier_credit` or `purchased_credits`.
    #[serde(default)]
    pub(crate) reason: Option<String>,
}
