//! Credentials Quota owns itself: checked when a connection starts, stored when
//! the person adds the account, and removed when they disconnect it.
//!
//! A pasted credential lives in memory while the account is verified and
//! reviewed. It reaches the system credential store only when the person adds
//! the account, and leaves it when they disconnect it, so a declined or
//! abandoned candidate leaves nothing behind.

use std::sync::Arc;

use quota_contracts::CommandError;
use quota_contracts::commands::BeginConnectionRequest;
use quota_core::ports::{ProviderAdapter, Secret, SecretStore, SecretStoreError};
use quota_domain::ids::ConnectionId;

/// The credential a connection attempt signs in with, when it carries one.
///
/// A provider Quota signs in to itself needs one, and any other provider must
/// not be sent one, so either mismatch is refused before any request is made.
pub(super) fn supplied(
    adapter: &Arc<dyn ProviderAdapter>,
    request: &BeginConnectionRequest,
) -> Result<Option<Secret>, CommandError> {
    let capabilities = adapter.capabilities();
    let credential = request
        .credential
        .as_ref()
        .map(|credential| credential.expose().to_owned())
        .filter(|value| !value.is_empty());
    if request.browser_sign_in {
        // The browser grants the credential; none may be pasted alongside.
        return if credential.is_some() || !capabilities.supports_app_owned_authorization {
            Err(CommandError::ValidationFailed {
                field: "credential".into(),
                reason: "a browser sign-in takes no pasted credential".into(),
            })
        } else {
            Ok(None)
        };
    }
    match credential {
        Some(_) if !capabilities.supports_app_owned_authorization => {
            Err(CommandError::ValidationFailed {
                field: "credential".into(),
                reason: "this provider signs in through its own client".into(),
            })
        }
        None if capabilities.supports_app_owned_authorization
            && !capabilities.supports_external_profile =>
        {
            Err(CommandError::ValidationFailed {
                field: "credential".into(),
                reason: "this provider needs an API key".into(),
            })
        }
        credential => Ok(credential.map(Secret::new)),
    }
}

/// Saves the credential for a connection the person just added.
pub(super) async fn store(
    secrets: &Arc<dyn SecretStore>,
    connection: &ConnectionId,
    secret: Secret,
) -> Result<(), CommandError> {
    secrets
        .write(connection, &secret)
        .await
        .map_err(store_error)
}

/// Removes a connection's credential, logging rather than failing: the
/// account is already gone, and a leftover entry names nothing but an unused
/// connection identifier.
pub(crate) async fn forget(secrets: &Arc<dyn SecretStore>, connection: ConnectionId) {
    let removed = secrets.delete(&connection).await;
    if !matches!(removed, Ok(())) {
        tracing::warn!("a disconnected account's credential was not removed from the store");
    }
}

fn store_error(error: SecretStoreError) -> CommandError {
    match error {
        SecretStoreError::Unavailable => CommandError::SecureStoreUnavailable,
        SecretStoreError::Refused => CommandError::Internal {
            code: "credential_store_refused".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use quota_contracts::commands::PastedCredential;
    use quota_core::ports::{
        ConnectionBinding, DiscoveredAccount, FetchOutcome, ProviderError, ProviderFuture,
        ReadContext,
    };
    use quota_domain::account::AccountCardinality;
    use quota_domain::polling::{FixedIntervalPolicy, PollingStrategy, ProviderPollingPolicy};
    use quota_domain::provider::{ProviderCapabilities, ProviderId};

    use super::*;

    /// An adapter that only declares how it signs in.
    #[derive(Debug)]
    struct Declared {
        app_owned: bool,
        external: bool,
    }

    impl ProviderAdapter for Declared {
        fn provider_id(&self) -> ProviderId {
            ProviderId::Openrouter
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                provider_id: ProviderId::Openrouter,
                cardinality: AccountCardinality::Independent,
                supports_app_owned_authorization: self.app_owned,
                supports_external_profile: self.external,
                reports_monthly_window: false,
                minimum_interval_seconds: 300,
            }
        }

        fn policy(&self) -> ProviderPollingPolicy {
            ProviderPollingPolicy {
                provider_id: ProviderId::Openrouter,
                strategy: PollingStrategy::FixedInterval(FixedIntervalPolicy {
                    visible_seconds: 300,
                    background_seconds: 300,
                    battery_saver_seconds: 900,
                    minimum_seconds: 300,
                }),
                request_timeout_seconds: 10,
                helper_timeout_seconds: 0,
                backoff_minutes: Vec::new(),
                max_concurrent_remote_reads: 1,
                version: 1,
            }
        }

        fn discover_accounts(
            &self,
        ) -> ProviderFuture<'_, Result<Vec<DiscoveredAccount>, ProviderError>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn read_quota(
            &self,
            _binding: &ConnectionBinding,
            _context: ReadContext,
        ) -> ProviderFuture<'_, Result<FetchOutcome, ProviderError>> {
            Box::pin(async { Err(ProviderError::Cancelled) })
        }
    }

    fn request(credential: Option<&str>) -> BeginConnectionRequest {
        BeginConnectionRequest {
            provider_id: ProviderId::Openrouter,
            profile_label: None,
            nickname: "Work".into(),
            credential: credential.map(|value| PastedCredential::new(value.to_owned())),
            browser_sign_in: false,
        }
    }

    fn adapter(app_owned: bool, external: bool) -> Arc<dyn ProviderAdapter> {
        Arc::new(Declared {
            app_owned,
            external,
        })
    }

    #[test]
    fn a_pasted_key_is_trimmed_for_a_provider_quota_signs_in_to() {
        let secret = supplied(&adapter(true, false), &request(Some("  sk-or-v1-abc \n")))
            .expect("accepted")
            .expect("a key");
        assert_eq!(secret.expose(), "sk-or-v1-abc");
    }

    #[test]
    fn a_provider_quota_signs_in_to_refuses_a_missing_or_blank_key() {
        for credential in [None, Some("   ")] {
            let refused = supplied(&adapter(true, false), &request(credential));
            assert!(matches!(
                refused,
                Err(CommandError::ValidationFailed { ref field, .. }) if field == "credential"
            ));
        }
    }

    #[test]
    fn a_provider_with_its_own_client_is_never_sent_a_key() {
        let refused = supplied(&adapter(false, true), &request(Some("sk-anything")));
        assert!(matches!(
            refused,
            Err(CommandError::ValidationFailed { .. })
        ));
        assert!(
            supplied(&adapter(false, true), &request(None))
                .expect("accepted")
                .is_none()
        );
    }
}
