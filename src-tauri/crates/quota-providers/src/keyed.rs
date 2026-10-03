//! What every provider signed in with a pasted key shares.
//!
//! Quota owns these keys. Each read takes the key from the system credential
//! store under the connection it signs in; during verification the host passes
//! the pasted key instead, before anything is stored. A key is told apart from
//! another, and checked against the one the person confirmed, by its
//! fingerprint, so no part of it is kept anywhere else.

use std::sync::Arc;

use quota_core::ports::{ConnectionBinding, ProviderError, ReadContext, Secret, SecretStore};
use quota_domain::account::AccountCardinality;
use quota_domain::polling::{FixedIntervalPolicy, PollingStrategy, ProviderPollingPolicy};
use quota_domain::provider::{ProviderCapabilities, ProviderId};

use crate::decode;

/// The shortest interval a pasted-key provider permits between reads.
const MINIMUM_SECONDS: u32 = 300;

/// The interval used during battery saver or prolonged idle.
const BATTERY_SAVER_SECONDS: u32 = 900;

/// The remote request deadline, in seconds.
const REQUEST_TIMEOUT_SECONDS: u32 = 10;
use crate::http::{GetRequest, HttpReply, ProviderHttp, classify_status};

/// The transport and key store one pasted-key adapter reads through.
#[derive(Debug)]
pub(crate) struct KeyedSource {
    http: ProviderHttp,
    secrets: Arc<dyn SecretStore>,
}

impl KeyedSource {
    /// Builds the source over the store its keys live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            http: ProviderHttp::new()?,
            secrets,
        })
    }

    /// The key stored for one connection.
    pub(crate) async fn stored_key(
        &self,
        binding: &ConnectionBinding,
    ) -> Result<Secret, ProviderError> {
        let secrets = Arc::clone(&self.secrets);
        let connection = binding.connection_id.clone();
        tokio::task::spawn_blocking(move || secrets.read(&connection))
            .await
            .map_err(|_| unavailable())?
            .map_err(|_| unavailable())?
            .ok_or(ProviderError::Authentication)
    }

    /// One GET with the given headers, refused on any non-success status.
    pub(crate) async fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        context: &ReadContext,
    ) -> Result<HttpReply, ProviderError> {
        let reply = self
            .http
            .get(GetRequest {
                url,
                headers,
                deadline: context.deadline,
            })
            .await?;
        match classify_status(reply.status, reply.retry_after) {
            Some(failure) => Err(failure),
            None => Ok(reply),
        }
    }
}

/// The profile label and pool seed of a key: its fingerprint.
pub(crate) fn profile(key: &Secret) -> String {
    decode::fingerprint(key.expose())
}

/// Checks that a read is for this provider and still uses the key the person
/// confirmed, and answers the key's profile.
pub(crate) fn check_binding(
    binding: &ConnectionBinding,
    provider: ProviderId,
    key: &Secret,
) -> Result<String, ProviderError> {
    let profile = profile(key);
    decode::ensure_binding(binding, provider, None, Some(&profile))?;
    Ok(profile)
}

/// The polling policy every pasted-key provider uses: a read every five
/// minutes, fifteen on battery saver.
pub(crate) fn policy(provider: ProviderId) -> ProviderPollingPolicy {
    ProviderPollingPolicy {
        provider_id: provider,
        strategy: PollingStrategy::FixedInterval(FixedIntervalPolicy {
            visible_seconds: MINIMUM_SECONDS,
            background_seconds: MINIMUM_SECONDS,
            battery_saver_seconds: BATTERY_SAVER_SECONDS,
            minimum_seconds: MINIMUM_SECONDS,
        }),
        request_timeout_seconds: REQUEST_TIMEOUT_SECONDS,
        helper_timeout_seconds: 0,
        backoff_minutes: quota_domain::polling::DEFAULT_BACKOFF_MINUTES.to_vec(),
        max_concurrent_remote_reads: 1,
        version: 1,
    }
}

/// What a pasted-key provider declares. `cli` says whether it can also read
/// a sign-in its own command-line tool keeps, so the key is optional.
pub(crate) const fn capabilities(
    provider: ProviderId,
    cli: bool,
    monthly: bool,
) -> ProviderCapabilities {
    ProviderCapabilities {
        provider_id: provider,
        cardinality: AccountCardinality::Independent,
        supports_app_owned_authorization: true,
        supports_external_profile: cli,
        reports_monthly_window: monthly,
        minimum_interval_seconds: MINIMUM_SECONDS,
    }
}

/// A context for a verification read, which has no scheduler deadline.
pub(crate) fn verification_context() -> ReadContext {
    ReadContext {
        attempt_id: quota_domain::ids::ConnectionAttemptId::generate(),
        deadline: None,
    }
}

fn unavailable() -> ProviderError {
    ProviderError::Transient {
        detail: "the credential store is unavailable".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_tells_keys_apart_and_keeps_none_of_their_text() {
        let first = profile(&Secret::new("sk-or-v1-first-key".to_owned()));
        let second = profile(&Secret::new("sk-or-v1-second-key".to_owned()));
        assert_ne!(first, second);
        assert!(!first.contains("first"));
        // Pasted with or without surrounding spaces, a key is the same key.
        assert_eq!(
            first,
            profile(&Secret::new(" sk-or-v1-first-key\n".to_owned()))
        );
    }
}
