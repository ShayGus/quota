//! Claude payload decoding, over sanitized fixtures and inline payloads.
// Integration tests are compiled without `cfg(test)`, so the crate-wide
// test-context allowance in `clippy.toml` does not reach this file. The scope is
// this file only: every allowance covers a fixture literal or an assertion that
// must fail loudly when a decode is wrong.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fixture reads must fail loudly when a decode is wrong"
)]

use chrono::{DateTime, Duration, Utc};
use quota_domain::provider::ProviderId;
use quota_domain::quota::measurement::{Measurement, UnavailableReason};
use quota_domain::quota::units::CurrencyCode;
use quota_domain::quota::window::{MetricRole, QuotaCategory};
use quota_providers::decode_offline;

fn received_at() -> DateTime<Utc> {
    DateTime::from_timestamp(1_789_000_000, 0).unwrap()
}

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("reading {name}: {error}"))
}

/// AC-01: a reported 28% used leaves 72% remaining.
#[test]
fn a_reported_used_percentage_becomes_its_remaining_percentage() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_success.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    let session = reading
        .category(QuotaCategory::Session)
        .into_iter()
        .next()
        .expect("a session window");
    let remaining = session.measurement.remaining_percent().unwrap();
    assert!((remaining.value() - 72.0).abs() < f64::EPSILON);
}

/// The three fixed windows keep their own periods and their own durations.
#[test]
fn the_fixed_windows_keep_their_periods_and_durations() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_success.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    let session = &reading.category(QuotaCategory::Session)[0];
    let weekly = &reading.category(QuotaCategory::Weekly)[0];
    assert_eq!(session.duration, Some(Duration::seconds(18_000)));
    assert_eq!(weekly.duration, Some(Duration::seconds(604_800)));
}

/// AC-13: a rolling source's boundary is a replenishment, not a full reset.
#[test]
fn a_rolling_window_reports_a_replenishment_boundary() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_success.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    let session = &reading.category(QuotaCategory::Session)[0];
    let boundary = session.boundary.expect("a boundary was reported");
    assert_eq!(
        boundary.kind,
        quota_domain::quota::window::BoundaryKind::NextReplenishment
    );
}

/// AC-14: a window with no reset time gets no guessed one.
#[test]
fn a_window_without_a_reset_time_has_no_boundary() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_boundary.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    assert!(
        reading
            .windows
            .iter()
            .all(|window| window.boundary.is_none())
    );
}

/// AC-05: the paid extra-usage summary is an extra-spend cap, never included quota.
#[test]
fn extra_usage_is_an_extra_spend_cap_and_never_included_quota() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_extra_usage.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    let extra = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("extra-usage"))
        .expect("the extra-usage cap exists");
    assert_eq!(extra.metric_role, MetricRole::ExtraSpendCap);
    assert!(!extra.metric_role.is_included_allowance());
    let Measurement::Money(money) = &extra.measurement else {
        panic!("expected a money measurement");
    };
    // The amounts are minor units at the reported scale of two.
    assert_eq!(money.currency, CurrencyCode::new("USD").unwrap());
    assert_eq!(money.scale, 2);
    assert_eq!(money.limit_minor_units, Some(5000));
    assert_eq!(money.used_minor_units, Some(625));
    assert_eq!(money.remaining_minor_units, Some(4375));
}

/// The extra-usage cap never takes part in an included-quota comparison.
#[test]
fn the_extra_spend_cap_cannot_be_ranked() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_extra_usage.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    let rankable = reading
        .windows
        .iter()
        .filter(|window| window.metric_role.is_included_allowance())
        .count();
    assert_eq!(
        rankable,
        reading.windows.len() - 1,
        "only the extra-spend cap is excluded"
    );
}

/// A disabled extra-usage capability is not entitled, not a zero.
#[test]
fn a_disabled_extra_usage_cap_is_not_entitled() {
    let payload = r#"{
        "five_hour": {"utilization": 28},
        "extra_usage": {"is_enabled": false, "monthly_limit": 5000}
    }"#;
    let reading =
        decode_offline(ProviderId::Claude, payload, "claude-local", received_at()).unwrap();
    let extra = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("extra-usage"))
        .expect("the extra-usage window exists");
    assert_eq!(extra.measurement, Measurement::NotEntitled);
}

/// An amount without a usable scale is unusable, never a guessed currency.
#[test]
fn an_unusable_amount_scale_produces_a_typed_issue() {
    let payload = r#"{
        "five_hour": {"utilization": 28},
        "extra_usage": {"is_enabled": true, "monthly_limit": 5000, "decimal_places": 40}
    }"#;
    let reading =
        decode_offline(ProviderId::Claude, payload, "claude-local", received_at()).unwrap();
    let extra = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("extra-usage"))
        .expect("the extra-usage window exists");
    assert_eq!(
        extra.measurement,
        Measurement::Unavailable(UnavailableReason::InvalidResponse)
    );
    assert_eq!(extra.issues.len(), 1);
}

/// A usable named-limits array replaces the three fixed windows.
#[test]
fn a_usable_limits_array_replaces_the_fixed_windows() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_limits.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    // Three named limits and three windows: had the fixed set also been decoded,
    // there would be six. The identities come from each limit's own group and
    // model scope, so they do not depend on the order of the array.
    assert_eq!(reading.windows.len(), 3);
    let identities: Vec<&str> = reading
        .windows
        .iter()
        .filter_map(|window| window.provider_bucket_id.as_deref())
        .collect();
    assert!(identities.contains(&"five-hour"), "{identities:?}");
    assert!(identities.contains(&"seven-day"), "{identities:?}");
    assert!(
        identities.iter().any(|id| id.starts_with("seven-day-opus")),
        "{identities:?}"
    );
    let session = reading.category(QuotaCategory::Session);
    assert_eq!(session.len(), 1);
    assert_eq!(session[0].duration, Some(Duration::seconds(18_000)));
    let weekly = reading.category(QuotaCategory::Weekly);
    assert_eq!(
        weekly.len(),
        2,
        "the weekly and Opus-weekly limits both arrive"
    );
}

/// AC-22: a model-scoped limit keeps its own scope.
#[test]
fn a_model_scoped_limit_keeps_its_own_scope() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_limits.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    let opus = reading
        .windows
        .iter()
        .find(|window| window.scope.resource().as_str() == "claude-opus-4")
        .expect("the model-scoped limit exists");
    assert_eq!(opus.scope.label(), "Claude Opus 4");
    assert_ne!(opus.scope.resource().as_str(), "account");
}

/// AC-08 and AC-10: a sub-one-percent remainder and an exhausted window stay distinct.
#[test]
fn a_sub_one_percent_remainder_is_labelled_and_exhaustion_stays_zero() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_boundary.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    let session = &reading.category(QuotaCategory::Session)[0];
    let weekly = &reading.category(QuotaCategory::Weekly)[0];
    let just_above_zero = session.measurement.remaining_percent().unwrap();
    assert!(just_above_zero.is_just_above_zero());
    assert_eq!(just_above_zero.to_string(), "<1%");
    let exhausted = weekly.measurement.remaining_percent().unwrap();
    assert!(exhausted.is_exhausted());
    assert_eq!(exhausted.to_string(), "0.0%");
}

/// AC-04: a fixed window the source did not report is recorded, not zeroed.
#[test]
fn a_missing_fixed_window_is_recorded_rather_than_zeroed() {
    let payload = r#"{"five_hour": {"utilization": 28}}"#;
    let reading =
        decode_offline(ProviderId::Claude, payload, "claude-local", received_at()).unwrap();
    assert!(!reading.is_complete());
    assert_eq!(reading.expected_but_missing.len(), 1);
    let weekly = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("weekly"))
        .expect("the missing weekly slot keeps its place");
    assert_eq!(
        weekly.measurement,
        Measurement::Unavailable(UnavailableReason::NotReported)
    );
}

/// A plan with no Opus allowance simply has no Opus window.
#[test]
fn an_absent_opus_window_is_not_fabricated() {
    let payload = r#"{"five_hour": {"utilization": 28}, "seven_day": {"utilization": 10}}"#;
    let reading =
        decode_offline(ProviderId::Claude, payload, "claude-local", received_at()).unwrap();
    assert!(
        reading
            .windows
            .iter()
            .all(|window| window.provider_bucket_id.as_deref() != Some("weekly-opus"))
    );
    assert!(reading.is_complete());
}

/// AC-24: an HTML body is a typed invalid state.
#[test]
fn an_html_body_is_refused_as_invalid_data() {
    let error = decode_offline(
        ProviderId::Claude,
        &fixture("not_json.html"),
        "claude-local",
        received_at(),
    )
    .expect_err("an HTML body is not a reading");
    assert_eq!(error.diagnostic_code(), "invalid_data");
}

/// A payload with no window at all is invalid data, not an empty success.
#[test]
fn an_empty_payload_is_invalid_data() {
    let error = decode_offline(ProviderId::Claude, "{}", "claude-local", received_at())
        .expect_err("an empty payload is not a reading");
    assert_eq!(error.diagnostic_code(), "invalid_data");
}

/// Two accounts of the same provider never share a pool or a window identity.
#[test]
fn two_local_accounts_of_one_provider_do_not_share_identity() {
    let payload = fixture("claude_success.json");
    let first = decode_offline(ProviderId::Claude, &payload, "claude-a", received_at()).unwrap();
    let second = decode_offline(ProviderId::Claude, &payload, "claude-b", received_at()).unwrap();
    assert_ne!(first.windows[0].pool_id, second.windows[0].pool_id);
    assert_ne!(first.windows[0].id, second.windows[0].id);
}

/// The identity of a named limit comes from what it says, not where it sits.
#[test]
fn removing_an_unrelated_limit_does_not_rename_an_unchanged_allowance() {
    let full = fixture("claude_limits.json");
    let before: serde_json::Value = serde_json::from_str(&full).unwrap();
    let weekly_group = before["limits"][1]["group"].as_str().unwrap().to_owned();

    // Drop the first entry, as a plan that dropped the five-hour limit would.
    let mut trimmed = before.clone();
    trimmed["limits"] =
        serde_json::json!([before["limits"][1].clone(), before["limits"][2].clone()]);

    let reading = decode_offline(
        ProviderId::Claude,
        &serde_json::to_string(&trimmed).unwrap(),
        "claude-local",
        received_at(),
    )
    .unwrap();

    let weekly = reading.category(QuotaCategory::Weekly);
    let expected = weekly_group.replace('_', "-");
    assert!(
        weekly
            .iter()
            .any(|window| window.provider_bucket_id.as_deref() == Some(expected.as_str())),
        "the unchanged weekly allowance keeps its identity: {:?}",
        weekly
            .iter()
            .filter_map(|window| window.provider_bucket_id.clone())
            .collect::<Vec<_>>()
    );
}
