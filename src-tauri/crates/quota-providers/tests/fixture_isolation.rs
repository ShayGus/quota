//! Multi-account isolation, exercised through the compiled fixture adapter.
//!
//! This is the one test file that needs the fixture provider, so it selects the
//! non-default `test-fixtures` feature explicitly.
#![cfg(feature = "test-fixtures")]
// Integration tests are compiled without `cfg(test)`, so the crate-wide
// test-context allowance in `clippy.toml` does not reach this file. The scope is
// this file only: every allowance covers a fixture literal or an assertion that
// must fail loudly when a reading is wrong.
#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "fixture assertions and fixture reads must fail loudly"
)]

use std::future::Future;

use quota_core::ports::{FetchOutcome, ProviderAdapter};
use quota_domain::provider::ProviderId;
use quota_providers::fixture::{FixtureAdapter, FixtureProfile, fixture_binding, fixture_context};
use quota_providers::registry::ProviderRegistry;

/// The registry offers the fixture only when the feature is compiled in.
#[test]
fn the_fixture_provider_is_registered_for_this_build() {
    let registry = ProviderRegistry::with_fixture(quota_providers::secrets::unavailable()).unwrap();
    assert!(registry.provider(ProviderId::Fixture).is_some());
    assert_eq!(
        registry
            .provider(ProviderId::Fixture)
            .unwrap()
            .provider_id(),
        ProviderId::Fixture
    );
}

/// Every fixture profile is discovered as its own account with its own pool.
#[test]
fn every_profile_is_discovered_as_an_independent_account() {
    let adapter = FixtureAdapter::new();
    let accounts = block_on(adapter.discover_accounts()).unwrap();
    assert_eq!(accounts.len(), FixtureProfile::ALL.len());
    let mut pools = accounts
        .iter()
        .map(|account| account.pool_id.clone())
        .collect::<Vec<_>>();
    pools.sort();
    pools.dedup();
    assert_eq!(pools.len(), accounts.len(), "no two accounts share a pool");
    let mut principals = accounts
        .iter()
        .filter_map(|account| account.principal_id.clone())
        .collect::<Vec<_>>();
    principals.sort();
    principals.dedup();
    assert_eq!(
        principals.len(),
        accounts.len(),
        "no two accounts share a principal"
    );
}

/// Two accounts of one provider get different readings at the same instant.
#[test]
fn two_accounts_of_one_provider_report_different_readings() {
    let adapter = FixtureAdapter::new();
    let healthy = read(&adapter, FixtureProfile::Healthy);
    let exhausted = read(&adapter, FixtureProfile::MonthlyExhausted);
    let healthy_session = session_remaining(&healthy);
    let exhausted_session = session_remaining(&exhausted);
    assert!(
        (healthy_session - exhausted_session).abs() > f64::EPSILON,
        "the two accounts must not share a reading"
    );
    assert!(exhausted.windows.iter().any(|window| {
        window
            .measurement
            .remaining_percent()
            .is_some_and(quota_domain::percent::Percent::is_exhausted)
    }));
    assert!(healthy.windows.iter().all(|window| {
        window
            .measurement
            .remaining_percent()
            .is_some_and(|p| p.value() > 0.0)
    }));
}

/// A read for one account never answers with another account's reading.
#[test]
fn a_read_for_another_account_is_refused() {
    let adapter = FixtureAdapter::new();
    let mut binding = fixture_binding(FixtureProfile::Healthy);
    binding.principal_id = quota_domain::ids::ProviderPrincipalId::new("fixture-unlimited").ok();
    let outcome = block_on(adapter.read_quota(&binding, fixture_context()));
    assert_eq!(
        outcome.unwrap_err(),
        quota_core::ports::ProviderError::Authentication
    );
}

/// A binding for a profile this adapter was not built with is refused.
#[test]
fn a_read_for_an_unavailable_profile_is_refused() {
    let adapter = FixtureAdapter::with_profiles(vec![FixtureProfile::Healthy]);
    let binding = fixture_binding(FixtureProfile::Unlimited);
    let outcome = block_on(adapter.read_quota(&binding, fixture_context()));
    assert!(matches!(
        outcome,
        Err(quota_core::ports::ProviderError::InvalidData { .. })
    ));
}

/// One account's failure leaves its siblings' readings intact.
#[test]
fn one_accounts_failure_leaves_its_siblings_correct() {
    let adapter = FixtureAdapter::new();
    let good = read(&adapter, FixtureProfile::Healthy);
    let mut broken = fixture_binding(FixtureProfile::StaleBoundary);
    broken.profile_label = Some("not-a-profile".to_owned());
    let failure = block_on(adapter.read_quota(&broken, fixture_context()));
    failure.unwrap_err();
    let again = read(&adapter, FixtureProfile::Healthy);
    // The receipt instant legitimately differs between the two reads, so the
    // comparison covers the stable parts: identity, windows, and measures.
    assert_eq!(good.identity, again.identity);
    assert_eq!(
        good.windows
            .iter()
            .map(|window| (window.id.clone(), window.measurement.clone()))
            .collect::<Vec<_>>(),
        again
            .windows
            .iter()
            .map(|window| (window.id.clone(), window.measurement.clone()))
            .collect::<Vec<_>>(),
        "a sibling read is unaffected by the failure"
    );
}

/// The stale-boundary profile proves a passed boundary is detectable.
#[test]
fn a_passed_boundary_is_reported_and_detectable() {
    let adapter = FixtureAdapter::new();
    let reading = read(&adapter, FixtureProfile::StaleBoundary);
    let now = chrono::Utc::now();
    assert!(
        reading
            .windows
            .iter()
            .any(|window| window.boundary_has_passed(now)),
        "the stale boundary must be detectable"
    );
}

/// The unlimited profile is never a made-up percentage.
#[test]
fn the_unlimited_profile_is_not_an_invented_percentage() {
    let adapter = FixtureAdapter::new();
    let reading = read(&adapter, FixtureProfile::Unlimited);
    assert!(reading.windows.iter().all(
        |window| window.measurement == quota_domain::quota::measurement::Measurement::Unlimited
    ));
    assert!(
        reading
            .windows
            .iter()
            .all(|window| window.measurement.remaining_percent().is_none())
    );
}

/// The native-unit profile keeps its count and invents no percentage.
#[test]
fn the_native_unit_profile_keeps_its_count_without_a_percentage() {
    let adapter = FixtureAdapter::new();
    let reading = read(&adapter, FixtureProfile::NativeUnitsOnly);
    assert!(
        reading
            .windows
            .iter()
            .all(|window| window.measurement.remaining_percent().is_none())
    );
    assert!(
        reading
            .windows
            .iter()
            .any(|window| window.measurement.has_number())
    );
}

/// The fixture adapter performs no I/O, so a read needs no network at all.
#[test]
fn the_fixture_adapter_reports_its_own_capabilities_and_policy() {
    let adapter = FixtureAdapter::new();
    let capabilities = adapter.capabilities();
    assert_eq!(capabilities.provider_id, ProviderId::Fixture);
    assert!(capabilities.reports_monthly_window);
    assert_eq!(adapter.policy().provider_id, ProviderId::Fixture);
}

/// Runs one fixture read to completion.
fn read(adapter: &FixtureAdapter, profile: FixtureProfile) -> quota_core::ports::QuotaRead {
    let binding = fixture_binding(profile);
    match block_on(adapter.read_quota(&binding, fixture_context())).unwrap() {
        FetchOutcome::Complete(read) => read,
        other => panic!("expected a complete reading, got {other:?}"),
    }
}

/// The remaining percentage of the session window.
fn session_remaining(read: &quota_core::ports::QuotaRead) -> f64 {
    read.windows
        .iter()
        .find(|window| window.category == quota_domain::quota::window::QuotaCategory::Session)
        .and_then(|window| window.measurement.remaining_percent())
        .map(quota_domain::percent::Percent::value)
        .expect("a session window with a percentage")
}

/// Drives one future to completion on a small current-thread runtime.
fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(future)
}
