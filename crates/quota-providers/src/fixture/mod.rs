//! A deterministic local provider for tests and developer runs.
//!
//! This module exists only when the `test-fixtures` feature is on, which release
//! builds leave off, so a release build contains no code that can serve a
//! fixture reading and the registry never offers this provider. It performs no
//! I/O at all: every reading is derived from the binding and the receipt instant.
//!
//! It supports several independent accounts of the same provider, each with its
//! own quota pool, so multi-account isolation is testable.

pub(crate) mod windows;

pub use windows::{fixture_binding, fixture_context};

use std::sync::Arc;

use chrono::Utc;
use quota_core::ports::{
    ConnectionBinding, DiscoveredAccount, FetchOutcome, ProviderAdapter, ProviderError,
    ProviderFuture, QuotaRead, ReadContext,
};
use quota_domain::account::{AccountCardinality, ConnectionState, CredentialOwnership};
use quota_domain::ids::{ProviderPrincipalId, QuotaPoolId};
use quota_domain::polling::{FixedIntervalPolicy, PollingStrategy, ProviderPollingPolicy};
use quota_domain::provider::{ProviderCapabilities, ProviderId};

use windows::{identity, windows};

/// The fixture profiles, one per account shape the tests need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureProfile {
    /// A healthy account with every window reported.
    Healthy,
    /// A healthy session window beside an exhausted monthly window.
    MonthlyExhausted,
    /// An account whose reported boundary has already passed.
    StaleBoundary,
    /// An account that reports only a native, uncountable unit.
    NativeUnitsOnly,
    /// An account whose session allowance has no ceiling.
    Unlimited,
}

impl FixtureProfile {
    /// Every profile, in a stable order.
    pub const ALL: [Self; 5] = [
        Self::Healthy,
        Self::MonthlyExhausted,
        Self::StaleBoundary,
        Self::NativeUnitsOnly,
        Self::Unlimited,
    ];

    /// The stable key used in the account identity and the binding label.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::MonthlyExhausted => "monthly-exhausted",
            Self::StaleBoundary => "stale-boundary",
            Self::NativeUnitsOnly => "native-units",
            Self::Unlimited => "unlimited",
        }
    }

    /// The principal identity this profile is discovered under.
    #[must_use]
    pub fn principal(self) -> String {
        format!("fixture-{}", self.key())
    }

    /// The nickname the fixture account is offered under.
    #[must_use]
    pub const fn nickname(self) -> &'static str {
        match self {
            Self::Healthy => "Fixture healthy",
            Self::MonthlyExhausted => "Fixture monthly exhausted",
            Self::StaleBoundary => "Fixture stale boundary",
            Self::NativeUnitsOnly => "Fixture native units",
            Self::Unlimited => "Fixture unlimited",
        }
    }

    /// Parses a profile key, for callers that address an account by name.
    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|profile| profile.key() == key)
    }
}

/// The deterministic adapter used by tests and developer runs.
#[derive(Debug)]
pub struct FixtureAdapter {
    profiles: Arc<Vec<FixtureProfile>>,
}

impl FixtureAdapter {
    /// Builds the adapter with every profile available.
    #[must_use]
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(FixtureProfile::ALL.to_vec()),
        }
    }

    /// Builds the adapter with a chosen set of profiles.
    #[must_use]
    pub fn with_profiles(profiles: Vec<FixtureProfile>) -> Self {
        Self {
            profiles: Arc::new(profiles),
        }
    }

    /// The pool identity for one profile, stable across runs.
    #[must_use]
    pub fn pool_for(profile: FixtureProfile) -> QuotaPoolId {
        QuotaPoolId::new(format!("fixture-{}", profile.key()))
            .unwrap_or_else(|_| QuotaPoolId::generate())
    }

    /// The profile a binding addresses, or a typed failure when it names none.
    fn profile_for(&self, binding: &ConnectionBinding) -> Result<FixtureProfile, ProviderError> {
        if binding.provider_id != ProviderId::Fixture {
            return Err(ProviderError::InvalidData {
                detail: "the binding names another provider".to_owned(),
            });
        }
        let key = binding
            .profile_label
            .as_deref()
            .ok_or_else(|| ProviderError::InvalidData {
                detail: "the fixture binding named no profile".to_owned(),
            })?;
        let profile = FixtureProfile::parse(key).ok_or_else(|| ProviderError::InvalidData {
            detail: "the fixture binding named an unknown profile".to_owned(),
        })?;
        if !self.profiles.contains(&profile) {
            return Err(ProviderError::InvalidData {
                detail: "the fixture binding named an unavailable profile".to_owned(),
            });
        }
        Ok(profile)
    }

    /// Checks the binding against the identity this profile reports.
    fn verify(profile: FixtureProfile, binding: &ConnectionBinding) -> Result<(), ProviderError> {
        let matches = binding
            .principal_id
            .as_ref()
            .is_some_and(|principal| principal.as_str() == profile.principal());
        if !matches {
            return Err(ProviderError::Authentication);
        }
        Ok(())
    }
}

impl Default for FixtureAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAdapter for FixtureAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Fixture
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            provider_id: ProviderId::Fixture,
            cardinality: AccountCardinality::Independent,
            supports_app_owned_authorization: false,
            supports_external_profile: true,
            reports_monthly_window: true,
            minimum_interval_seconds: 1,
        }
    }

    fn policy(&self) -> ProviderPollingPolicy {
        ProviderPollingPolicy {
            provider_id: ProviderId::Fixture,
            strategy: PollingStrategy::FixedInterval(FixedIntervalPolicy {
                visible_seconds: 60,
                background_seconds: 300,
                battery_saver_seconds: 900,
                minimum_seconds: 1,
            }),
            request_timeout_seconds: 1,
            helper_timeout_seconds: 0,
            backoff_minutes: vec![1],
            max_concurrent_remote_reads: 1,
            version: 1,
        }
    }

    fn discover_accounts(
        &self,
    ) -> ProviderFuture<'_, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let accounts = self
                .profiles
                .iter()
                .map(|profile| DiscoveredAccount {
                    principal_id: ProviderPrincipalId::new(profile.principal()).ok(),
                    workspace_id: None,
                    entitlement_id: None,
                    profile_label: Some(profile.key().to_owned()),
                    pool_id: Self::pool_for(*profile),
                    identity: identity(*profile),
                    cardinality: AccountCardinality::Independent,
                    credential_ownership: CredentialOwnership::ExternalClient,
                    state: ConnectionState::Connected,
                    nickname: profile.nickname().to_owned(),
                })
                .collect();
            Ok(accounts)
        })
    }

    fn read_quota(
        &self,
        binding: &ConnectionBinding,
        _context: ReadContext,
    ) -> ProviderFuture<'_, Result<FetchOutcome, ProviderError>> {
        let resolved = self
            .profile_for(binding)
            .and_then(|profile| Self::verify(profile, binding).map(|()| profile));
        Box::pin(async move {
            let profile = resolved?;
            let received_at = Utc::now();
            let pool = Self::pool_for(profile);
            Ok(FetchOutcome::Complete(QuotaRead {
                identity: identity(profile),
                windows: windows(profile, &pool, received_at)?,
                expected_but_missing: Vec::new(),
                debug_metadata: None,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quota_domain::quota::measurement::Measurement;
    use quota_domain::quota::window::QuotaCategory;

    #[test]
    fn each_profile_reports_its_own_pool_and_windows() {
        let received_at = Utc::now();
        for profile in FixtureProfile::ALL {
            let pool = FixtureAdapter::pool_for(profile);
            let windows = windows(profile, &pool, received_at).expect("the fixture is total");
            assert!(!windows.is_empty(), "{} reported no window", profile.key());
            for window in &windows {
                assert_eq!(window.pool_id, pool);
            }
        }
    }

    #[test]
    fn two_profiles_never_share_a_pool() {
        let healthy = FixtureAdapter::pool_for(FixtureProfile::Healthy);
        let unlimited = FixtureAdapter::pool_for(FixtureProfile::Unlimited);
        assert_ne!(healthy, unlimited);
    }

    #[test]
    fn the_monthly_exhausted_profile_keeps_a_healthy_session() {
        let pool = FixtureAdapter::pool_for(FixtureProfile::MonthlyExhausted);
        let windows = windows(FixtureProfile::MonthlyExhausted, &pool, Utc::now())
            .expect("the fixture is total");
        let session = windows
            .iter()
            .find(|window| window.category == QuotaCategory::Session)
            .expect("a session window exists");
        let monthly = windows
            .iter()
            .find(|window| window.category == QuotaCategory::Monthly)
            .expect("a monthly window exists");
        assert!(
            session
                .measurement
                .remaining_percent()
                .is_some_and(|remaining| remaining.value() > 90.0)
        );
        assert!(
            monthly
                .measurement
                .remaining_percent()
                .is_some_and(|remaining| remaining.value() <= 0.0)
        );
    }

    #[test]
    fn the_native_unit_profile_never_invents_a_percentage() {
        let pool = FixtureAdapter::pool_for(FixtureProfile::NativeUnitsOnly);
        let windows = windows(FixtureProfile::NativeUnitsOnly, &pool, Utc::now())
            .expect("the fixture is total");
        assert!(
            windows
                .iter()
                .all(|window| window.measurement.remaining_percent().is_none())
        );
        assert!(windows.iter().any(|window| window.measurement.has_number()));
    }

    #[test]
    fn the_unlimited_profile_is_not_a_full_percentage() {
        let pool = FixtureAdapter::pool_for(FixtureProfile::Unlimited);
        let windows =
            windows(FixtureProfile::Unlimited, &pool, Utc::now()).expect("the fixture is total");
        assert!(
            windows
                .iter()
                .all(|window| window.measurement == Measurement::Unlimited)
        );
    }

    #[test]
    fn the_stale_boundary_profile_reports_a_passed_boundary() {
        let pool = FixtureAdapter::pool_for(FixtureProfile::StaleBoundary);
        let windows = windows(FixtureProfile::StaleBoundary, &pool, Utc::now())
            .expect("the fixture is total");
        assert!(
            windows
                .iter()
                .any(|window| window.boundary_has_passed(Utc::now()))
        );
    }

    #[test]
    fn a_missing_or_unknown_profile_binding_is_refused() {
        let adapter = FixtureAdapter::new();
        let mut binding = fixture_binding(FixtureProfile::Healthy);
        binding.profile_label = None;
        assert!(adapter.profile_for(&binding).is_err());
        binding.profile_label = Some("not-a-profile".to_owned());
        assert!(adapter.profile_for(&binding).is_err());
    }

    #[test]
    fn a_reading_for_another_profile_is_refused() {
        let adapter = FixtureAdapter::new();
        let mut binding = fixture_binding(FixtureProfile::Healthy);
        binding.principal_id = ProviderPrincipalId::new("fixture-unlimited").ok();
        assert_eq!(
            adapter
                .profile_for(&binding)
                .and_then(|profile| FixtureAdapter::verify(profile, &binding)),
            Err(ProviderError::Authentication)
        );
    }
}
