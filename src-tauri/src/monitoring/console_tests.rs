//! The website sign-in's wait, against a stand-in browser: it ends with the
//! session the provider accepts, and with "closed" when the person closes the
//! browser first.

use quota_core::ports::{
    ConnectionBinding, DiscoveredAccount, FetchOutcome, ProviderError, ProviderFuture, ReadContext,
};
use quota_domain::account::{
    AccountCardinality, ConnectionState, CredentialOwnership, VerifiedIdentity,
};
use quota_domain::ids::QuotaPoolId;
use quota_domain::polling::{FixedIntervalPolicy, PollingStrategy, ProviderPollingPolicy};
use quota_domain::provider::ProviderCapabilities;
use quota_domain::quota::window::SourceKind;

use super::*;
use crate::monitoring::browser_session::BrowserSession;
use crate::monitoring::browser_session::tests::{fake_browser, idle_child};

/// A provider that accepts only the stand-in browser's fictional session.
#[derive(Debug)]
struct AcceptsFictional;

impl ProviderAdapter for AcceptsFictional {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Typesafe
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            provider_id: ProviderId::Typesafe,
            cardinality: AccountCardinality::Independent,
            supports_app_owned_authorization: true,
            supports_external_profile: false,
            reports_monthly_window: false,
            minimum_interval_seconds: 600,
        }
    }

    fn policy(&self) -> ProviderPollingPolicy {
        ProviderPollingPolicy {
            provider_id: ProviderId::Typesafe,
            strategy: PollingStrategy::FixedInterval(FixedIntervalPolicy {
                visible_seconds: 600,
                background_seconds: 600,
                battery_saver_seconds: 1200,
                minimum_seconds: 600,
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

    fn discover_with<'a>(
        &'a self,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<Vec<DiscoveredAccount>, ProviderError>> {
        let accepted = credential.expose() == "session=fictional";
        Box::pin(async move {
            if !accepted {
                return Err(ProviderError::Authentication);
            }
            Ok(vec![DiscoveredAccount {
                principal_id: None,
                workspace_id: None,
                entitlement_id: None,
                pool_id: QuotaPoolId::new("pool-1").map_err(|_| ProviderError::Authorization)?,
                profile_label: None,
                identity: VerifiedIdentity {
                    principal_label: "TypeSafe console".to_owned(),
                    workspace_label: None,
                    plan_label: None,
                    source: SourceKind::ObservedWebEndpoint,
                },
                cardinality: AccountCardinality::Independent,
                credential_ownership: CredentialOwnership::AppOwned,
                state: ConnectionState::Connecting,
                nickname: "TypeSafe".to_owned(),
            }])
        })
    }
}

fn site() -> tauri::Url {
    tauri::Url::parse("https://console.example.test/").expect("an address")
}

#[tokio::test]
async fn the_wait_ends_with_the_session_the_provider_accepts() {
    let (port, server) = fake_browser(1);
    let session =
        BrowserSession::connect(idle_child(), port, "/devtools/browser/test").expect("connected");
    let source = Source::Browser(Running::new(session));
    let adapter: Arc<dyn ProviderAdapter> = Arc::new(AcceptsFictional);
    let (_keep, mut cancelled) = watch::channel(false);
    let outcome = wait(&adapter, &source, &site(), &mut cancelled).await;
    assert_eq!(outcome, Ok(Some("session=fictional".to_owned())));
    source.finish().await;
    drop(server.join());
}

#[tokio::test]
async fn closing_the_browser_first_ends_the_sign_in() {
    let (port, server) = fake_browser(0);
    let session =
        BrowserSession::connect(idle_child(), port, "/devtools/browser/test").expect("connected");
    let source = Source::Browser(Running::new(session));
    let adapter: Arc<dyn ProviderAdapter> = Arc::new(AcceptsFictional);
    let (_keep, mut cancelled) = watch::channel(false);
    let outcome = wait(&adapter, &source, &site(), &mut cancelled).await;
    assert_eq!(
        outcome,
        Err(CommandError::Internal {
            code: "console_sign_in_closed".into()
        })
    );
    source.finish().await;
    drop(server.join());
}

#[test]
fn nothing_is_tried_until_the_page_has_left_sign_in() {
    let console = site();
    for page in [
        "https://console.example.test/login?returnTo=%2Fsettings%2Fbilling",
        "https://console.example.test/auth/callback",
        "https://accounts.google.com/signin",
        "http://console.example.test/settings/billing",
        "not an address",
    ] {
        assert!(!past_sign_in(page, &console), "{page} is still sign-in");
    }
    for page in [
        "https://console.example.test/",
        "https://console.example.test/settings/billing",
    ] {
        assert!(past_sign_in(page, &console), "{page} is past sign-in");
    }
}
