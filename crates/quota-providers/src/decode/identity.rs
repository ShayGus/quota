//! Deriving stable identities and safe labels from provider text.
//!
//! A pool identity is stable for one provider and one locally resolved account,
//! and a window identity is stable for one pool, scope, metric, and period
//! definition. Deriving them here, once, keeps two accounts of the same provider
//! from colliding and keeps one account's windows stable across reads.

use quota_core::ports::ProviderError;
use quota_domain::ids::{MAX_ID_LEN, QuotaPoolId, QuotaWindowId, ResourceId};
use quota_domain::provider::ProviderId;
use quota_domain::quota::scope::QuotaScope;
use quota_domain::quota::window::QuotaCategory;

/// The detail text used when a scope cannot be represented at all.
const SCOPE_DETAIL: &str = "a quota scope could not be represented";

/// A stable pool identity for one provider and one locally resolved account.
pub(crate) fn pool_id(provider: ProviderId, seed: &str) -> QuotaPoolId {
    let text = bounded(&format!("{}-{seed}", provider.as_str()), MAX_ID_LEN);
    QuotaPoolId::new(text).unwrap_or_else(|_| QuotaPoolId::generate())
}

/// A stable window identity for one pool, scope, metric, and period definition.
pub(crate) fn window_id(
    provider: ProviderId,
    category: QuotaCategory,
    pool: &QuotaPoolId,
    resource: &str,
    bucket: Option<&str>,
) -> QuotaWindowId {
    let mut text = format!(
        "{}:{}:{}:{}",
        provider.as_str(),
        category_key(category),
        pool.as_str(),
        resource
    );
    if let Some(bucket) = bucket {
        text.push(':');
        text.push_str(bucket);
    }
    let text = bounded(&text, MAX_ID_LEN);
    QuotaWindowId::new(text).unwrap_or_else(|_| QuotaWindowId::generate())
}

/// A resource identifier that always satisfies the domain invariants.
pub(crate) fn resource_id(text: &str) -> ResourceId {
    ResourceId::new(identifier(text)).unwrap_or_else(|_| ResourceId::generate())
}

/// A display label trimmed to the domain's scope-label budget.
pub(crate) fn label(text: &str) -> String {
    let trimmed = text.trim();
    let budget = quota_domain::quota::scope::MAX_SCOPE_LABEL_LEN;
    let shortened: String = trimmed.chars().take(budget).collect();
    if shortened.is_empty() {
        return "Usage".to_owned();
    }
    shortened
}

/// A lower-case, punctuation-free form of provider text, never empty.
pub(crate) fn identifier(text: &str) -> String {
    let mut sanitized = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            sanitized.push(character.to_ascii_lowercase());
        } else if !sanitized.ends_with('-') {
            sanitized.push('-');
        }
    }
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        return "unknown".to_owned();
    }
    bounded(trimmed, MAX_ID_LEN)
}

/// A masked form of an address, so no label carries the full address.
pub(crate) fn masked_address(text: &str) -> String {
    let trimmed = text.trim();
    let Some((local, domain)) = trimmed.split_once('@') else {
        return label(trimmed);
    };
    let Some(first) = local.chars().next() else {
        return label(domain);
    };
    label(&format!("{first}***@{domain}"))
}

/// Shortens text to a character budget, keeping it unique with a text hash.
///
/// The hash is a plain FNV-1a of the full text, so the shortened form is stable
/// across releases and process runs.
fn bounded(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max.saturating_sub(17)).collect();
    format!("{head}-{:016x}", fnv1a(text))
}

/// A stable 64-bit FNV-1a hash of the text.
fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A stable, readable key for a window category, used inside derived identities.
pub(crate) fn category_key(category: QuotaCategory) -> &'static str {
    match category {
        QuotaCategory::Session => "session",
        QuotaCategory::Weekly => "weekly",
        QuotaCategory::Monthly => "monthly",
        QuotaCategory::Daily => "daily",
        QuotaCategory::Custom => "custom",
    }
}

/// A scope for a provider-reported resource, or the typed failure when the
/// domain cannot represent it.
pub(crate) fn scope(resource: &str, resource_label: &str) -> Result<QuotaScope, ProviderError> {
    QuotaScope::new(resource_id(resource), label(resource_label)).map_err(|_| {
        ProviderError::InvalidData {
            detail: SCOPE_DETAIL.to_owned(),
        }
    })
}
