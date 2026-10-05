//! Wire shapes for `OpenRouter`'s documented key and credit endpoints.
//!
//! `GET /api/v1/key` describes the API key that made the request, and
//! `GET /api/v1/credits` the account's purchased credits and total spend.
//! Both wrap their body in `data`. Every field is optional and unknown fields
//! are ignored, so an added field can never fail the parse.

use serde::Deserialize;

use crate::decode::Numberish;

/// The body of `GET /api/v1/key`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct KeyEnvelope {
    /// The key's details.
    #[serde(default)]
    pub(crate) data: Option<KeyData>,
}

/// One API key, as `OpenRouter` describes it.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct KeyData {
    /// The key's display label, a masked form of the key.
    #[serde(default)]
    pub(crate) label: Option<String>,
    /// The key's spend limit in US dollars, or `null` for none.
    #[serde(default)]
    pub(crate) limit: Option<Numberish>,
    /// What is left of the limit in US dollars.
    #[serde(default)]
    pub(crate) limit_remaining: Option<Numberish>,
    /// How often the limit resets: `daily`, `weekly`, `monthly`, or `null`.
    #[serde(default)]
    pub(crate) limit_reset: Option<String>,
    /// Whether the account has never bought credits.
    #[serde(default)]
    pub(crate) is_free_tier: Option<bool>,
    /// What the key spent in the current UTC day, in US dollars.
    #[serde(default)]
    pub(crate) usage_daily: Option<Numberish>,
    /// What the key spent in the current UTC week, in US dollars.
    #[serde(default)]
    pub(crate) usage_weekly: Option<Numberish>,
    /// What the key spent in the current UTC month, in US dollars.
    #[serde(default)]
    pub(crate) usage_monthly: Option<Numberish>,
}

/// The body of `GET /api/v1/credits`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CreditsEnvelope {
    /// The account's credits.
    #[serde(default)]
    pub(crate) data: Option<CreditsData>,
}

/// The account's purchased credits and what has been spent of them.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CreditsData {
    /// Every credit ever bought, in US dollars.
    #[serde(default)]
    pub(crate) total_credits: Option<Numberish>,
    /// Everything ever spent, in US dollars.
    #[serde(default)]
    pub(crate) total_usage: Option<Numberish>,
}
