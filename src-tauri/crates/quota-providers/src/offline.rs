//! Offline decoding of a captured provider payload.
//!
//! A reading normally arrives over this crate's bounded HTTP boundary.
//! This entry point exposes the same decoding for a payload this process did not
//! fetch, which is what the adapter tests drive with sanitized fixtures. It
//! performs no I/O and makes no network call; the caller supplies the pool seed
//! and the receipt instant.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::{QuotaPoolId, QuotaWindowId};
use quota_domain::provider::ProviderId;
use quota_domain::quota::window::QuotaWindow;

use crate::decode::{self, DecodedUsage};
use crate::{claude, codex, cursor, grok, kimi, minimax, muse, opencode_go, openrouter, zai};

/// A decoded payload, before an identity is attached.
#[derive(Clone, Debug, PartialEq)]
pub struct OfflineReading {
    /// The normalised windows, in a stable order.
    pub windows: Vec<QuotaWindow>,
    /// Windows the provider was expected to report and did not.
    pub expected_but_missing: Vec<QuotaWindowId>,
    /// A plan label, when the payload carried one.
    pub plan_label: Option<String>,
    /// A principal label, when the payload carried one. It is already masked.
    pub principal_label: Option<String>,
}

impl OfflineReading {
    /// Whether every expected window arrived.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.expected_but_missing.is_empty()
    }

    /// The windows of one period category.
    #[must_use]
    pub fn category(
        &self,
        category: quota_domain::quota::window::QuotaCategory,
    ) -> Vec<&QuotaWindow> {
        self.windows
            .iter()
            .filter(|window| window.category == category)
            .collect()
    }

    /// Converts the decoded payload into the shared shape.
    fn from_decoded(decoded: DecodedUsage) -> Self {
        Self {
            windows: decoded.windows,
            expected_but_missing: decoded.expected_but_missing,
            plan_label: decoded.plan_label,
            principal_label: decoded.principal_label,
        }
    }
}

/// Decodes a captured payload for one provider.
///
/// `pool_seed` names the local account the payload belongs to, so the derived
/// window identities stay stable for that account. `received_at` is the instant
/// this process read the payload, not the instant the provider observed it.
///
/// # Errors
/// Returns [`ProviderError::InvalidData`] for a body that is not a JSON object,
/// [`ProviderError::UnsupportedSchema`] for a JSON object this build does not
/// recognise, and the provider-specific failure for a recognised payload that
/// carries no usable window. A malformed payload never yields a fabricated zero.
pub fn decode_offline(
    provider: ProviderId,
    payload: &str,
    pool_seed: &str,
    received_at: DateTime<Utc>,
) -> Result<OfflineReading, ProviderError> {
    let trimmed = payload.trim_start_matches('\u{feff}').trim_start();
    if !trimmed.starts_with('{') {
        return Err(ProviderError::InvalidData {
            detail: "the provider body was not a JSON object".to_owned(),
        });
    }
    let document: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|_| ProviderError::InvalidData {
            detail: "the provider body was not valid JSON".to_owned(),
        })?;
    let pool: QuotaPoolId = decode::pool_id(provider, pool_seed);
    let decoded = match provider {
        ProviderId::Codex => {
            let envelope: codex::wire::CodexEnvelope =
                serde_json::from_value(document).map_err(|_| ProviderError::UnsupportedSchema {
                    detail: "the payload did not match the supported Codex shape".to_owned(),
                })?;
            codex::mapping::decode(&envelope, &pool, received_at)?
        }
        ProviderId::Claude => {
            let usage: claude::wire::ClaudeUsage =
                serde_json::from_value(document).map_err(|_| ProviderError::UnsupportedSchema {
                    detail: "the payload did not match the supported Claude usage shape".to_owned(),
                })?;
            claude::mapping::decode(&usage, &pool, received_at)?
        }
        ProviderId::OpenCodeGo => {
            let envelope: opencode_go::wire::OpenCodeGoEnvelope = serde_json::from_value(document)
                .map_err(|_| ProviderError::UnsupportedSchema {
                    detail: "the payload did not match the supported OpenCode Go shape".to_owned(),
                })?;
            opencode_go::mapping::decode(&envelope, &pool, received_at)?
        }
        ProviderId::Openrouter => {
            // The key answer alone; the credit balance is a second endpoint.
            let key = serde_json::from_value::<openrouter::wire::KeyEnvelope>(document)
                .ok()
                .and_then(|envelope| envelope.data)
                .ok_or_else(|| ProviderError::UnsupportedSchema {
                    detail: "the payload did not match the supported OpenRouter key shape"
                        .to_owned(),
                })?;
            openrouter::mapping::decode(&key, None, &pool, received_at)?
        }
        ProviderId::Zai => {
            let envelope = offline_shape::<zai::wire::QuotaEnvelope>(document, "Z.ai quota")?;
            let data = envelope.data.ok_or_else(|| unsupported("Z.ai quota"))?;
            zai::mapping::decode(&data, &pool, received_at)?
        }
        ProviderId::Minimax => {
            let envelope =
                offline_shape::<minimax::wire::RemainsEnvelope>(document, "MiniMax plan")?;
            minimax::mapping::decode(envelope.buckets(), &pool, received_at)?
        }
        ProviderId::Kimi => {
            let envelope = offline_shape::<kimi::wire::UsageEnvelope>(document, "Kimi usage")?;
            kimi::mapping::decode(&envelope, &pool, received_at)?
        }
        ProviderId::Grok => {
            let envelope = offline_shape::<grok::wire::BillingEnvelope>(document, "Grok billing")?;
            grok::mapping::decode(envelope.config.as_ref(), None, &pool, received_at)?
        }
        ProviderId::MuseCode => {
            let answer =
                offline_shape::<muse::wire::SubscriptionAnswer>(document, "Muse Code usage")?;
            muse::mapping::decode(&answer, &pool, received_at)?
        }
        ProviderId::Cursor => {
            let summary = offline_shape::<cursor::wire::UsageSummary>(document, "Cursor usage")?;
            cursor::mapping::decode(&summary, &pool, received_at)?
        }
        ProviderId::Fixture => {
            return Err(ProviderError::UnsupportedSchema {
                detail: "this build has no offline decoder for that provider".to_owned(),
            });
        }
    };
    Ok(OfflineReading::from_decoded(decoded))
}

/// Reads a captured payload as one provider's wire shape.
fn offline_shape<T: serde::de::DeserializeOwned>(
    document: serde_json::Value,
    name: &str,
) -> Result<T, ProviderError> {
    serde_json::from_value(document).map_err(|_| unsupported(name))
}

/// The failure for a payload that is not the named provider's shape.
fn unsupported(name: &str) -> ProviderError {
    ProviderError::UnsupportedSchema {
        detail: format!("the payload did not match the supported {name} shape"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_that_is_not_json_is_refused() {
        let result = decode_offline(
            ProviderId::Codex,
            "<html><body>maintenance</body></html>",
            "seed",
            Utc::now(),
        );
        assert!(matches!(result, Err(ProviderError::InvalidData { .. })));
    }

    #[test]
    fn a_provider_without_a_decoder_is_explicitly_unsupported() {
        let result = decode_offline(ProviderId::Fixture, "{}", "seed", Utc::now());
        assert!(matches!(
            result,
            Err(ProviderError::UnsupportedSchema { .. })
        ));
    }
}
