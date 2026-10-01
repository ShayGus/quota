//! `OpenCode` Go payload decoding, over sanitized fixtures and inline payloads.
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
use quota_domain::quota::window::{BoundaryKind, QuotaCategory};
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
        ProviderId::OpenCodeGo,
        &fixture("opencode_go_success.json"),
        "opencode-local",
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

/// This connector reports a monthly window, and it is kept.
#[test]
fn the_monthly_window_is_reported_and_not_dropped() {
    let reading = decode_offline(
        ProviderId::OpenCodeGo,
        &fixture("opencode_go_success.json"),
        "opencode-local",
        received_at(),
    )
    .unwrap();
    let monthly = reading.category(QuotaCategory::Monthly);
    assert_eq!(monthly.len(), 1);
    assert_eq!(monthly[0].provider_bucket_id.as_deref(), Some("monthly"));
    let remaining = monthly[0].measurement.remaining_percent().unwrap();
    assert!(remaining.is_exhausted());
}

/// The rolling window is five hours and the weekly one is seven days.
#[test]
fn the_stated_durations_are_mapped_exactly() {
    let reading = decode_offline(
        ProviderId::OpenCodeGo,
        &fixture("opencode_go_success.json"),
        "opencode-local",
        received_at(),
    )
    .unwrap();
    let session = &reading.category(QuotaCategory::Session)[0];
    let weekly = &reading.category(QuotaCategory::Weekly)[0];
    assert_eq!(session.duration, Some(Duration::seconds(18_000)));
    assert_eq!(weekly.duration, Some(Duration::seconds(604_800)));
}

/// A remaining percentage is converted, so the reading keeps one polarity.
#[test]
fn a_reported_remaining_percentage_is_converted_to_used() {
    let reading = decode_offline(
        ProviderId::OpenCodeGo,
        &fixture("opencode_go_success.json"),
        "opencode-local",
        received_at(),
    )
    .unwrap();
    let weekly = &reading.category(QuotaCategory::Weekly)[0];
    let Measurement::Percentage(reported) = &weekly.measurement else {
        panic!("expected a percentage");
    };
    assert!((reported.used_percent.value() - 64.5).abs() < f64::EPSILON);
    assert!((reported.remaining_percent.value() - 35.5).abs() < f64::EPSILON);
}

/// AC-13: a rolling window's boundary is a replenishment, not a full reset.
#[test]
fn a_rolling_window_reports_a_replenishment_boundary() {
    let reading = decode_offline(
        ProviderId::OpenCodeGo,
        &fixture("opencode_go_success.json"),
        "opencode-local",
        received_at(),
    )
    .unwrap();
    let session = &reading.category(QuotaCategory::Session)[0];
    let boundary = session.boundary.expect("a boundary was reported");
    assert_eq!(boundary.kind, BoundaryKind::NextReplenishment);
}

/// The payload may sit at the root instead of under its container.
#[test]
fn the_root_payload_shape_is_accepted() {
    let reading = decode_offline(
        ProviderId::OpenCodeGo,
        &fixture("opencode_go_root.json"),
        "opencode-local",
        received_at(),
    )
    .unwrap();
    assert_eq!(reading.category(QuotaCategory::Session).len(), 1);
    assert_eq!(reading.category(QuotaCategory::Weekly).len(), 1);
    assert_eq!(reading.category(QuotaCategory::Monthly).len(), 1);
    assert!(reading.is_complete());
}

/// AC-08 and AC-10: a sub-one-percent remainder and an exhausted window differ.
#[test]
fn a_sub_one_percent_remainder_is_labelled_and_exhaustion_stays_zero() {
    let reading = decode_offline(
        ProviderId::OpenCodeGo,
        &fixture("opencode_go_root.json"),
        "opencode-local",
        received_at(),
    )
    .unwrap();
    let session = &reading.category(QuotaCategory::Session)[0];
    let weekly = &reading.category(QuotaCategory::Weekly)[0];
    let just_above_zero = session.measurement.remaining_percent().unwrap();
    assert!(just_above_zero.is_just_above_zero());
    assert_eq!(just_above_zero.to_string(), "<1%");
    assert!(
        weekly
            .measurement
            .remaining_percent()
            .unwrap()
            .is_exhausted()
    );
    assert_eq!(
        weekly.measurement.remaining_percent().unwrap().to_string(),
        "0.0%"
    );
}

/// AC-14: no reset time means no guessed one.
#[test]
fn a_window_without_a_reset_time_has_no_boundary() {
    let reading = decode_offline(
        ProviderId::OpenCodeGo,
        &fixture("opencode_go_root.json"),
        "opencode-local",
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

/// AC-04: a window the source did not report is recorded, not zeroed.
#[test]
fn a_missing_window_is_recorded_rather_than_zeroed() {
    let reading = decode_offline(
        ProviderId::OpenCodeGo,
        &fixture("opencode_go_partial.json"),
        "opencode-local",
        received_at(),
    )
    .unwrap();
    assert!(!reading.is_complete());
    assert_eq!(reading.expected_but_missing.len(), 2);
    for bucket in ["weekly", "monthly"] {
        let window = reading
            .windows
            .iter()
            .find(|window| window.provider_bucket_id.as_deref() == Some(bucket))
            .unwrap_or_else(|| panic!("the {bucket} slot keeps its place"));
        assert_eq!(
            window.measurement,
            Measurement::Unavailable(UnavailableReason::NotReported)
        );
    }
}

/// AC-24: an HTML body is a typed invalid state.
#[test]
fn an_html_body_is_refused_as_invalid_data() {
    let error = decode_offline(
        ProviderId::OpenCodeGo,
        &fixture("not_json.html"),
        "opencode-local",
        received_at(),
    )
    .expect_err("an HTML body is not a reading");
    assert_eq!(error.diagnostic_code(), "invalid_data");
}

/// AC-24: an unusable number is typed, never a fabricated zero.
#[test]
fn an_unusable_number_is_typed_and_never_a_zero() {
    let payload = r#"{"usage": {"rollingUsage": {"percent": "not-a-number"}}}"#;
    let reading = decode_offline(
        ProviderId::OpenCodeGo,
        payload,
        "opencode-local",
        received_at(),
    )
    .unwrap();
    let rolling = &reading.category(QuotaCategory::Session)[0];
    assert_eq!(
        rolling.measurement,
        Measurement::Unavailable(UnavailableReason::InvalidResponse)
    );
    assert_eq!(rolling.issues.len(), 1);
}

/// An empty payload is invalid data, not an empty success.
#[test]
fn an_empty_payload_is_invalid_data() {
    let error = decode_offline(
        ProviderId::OpenCodeGo,
        "{}",
        "opencode-local",
        received_at(),
    )
    .expect_err("an empty payload is not a reading");
    assert_eq!(error.diagnostic_code(), "invalid_data");
}

/// Two accounts of the same provider never share a pool or a window identity.
#[test]
fn two_local_accounts_of_one_provider_do_not_share_identity() {
    let payload = fixture("opencode_go_success.json");
    let first = decode_offline(
        ProviderId::OpenCodeGo,
        &payload,
        "opencode-a",
        received_at(),
    )
    .unwrap();
    let second = decode_offline(
        ProviderId::OpenCodeGo,
        &payload,
        "opencode-b",
        received_at(),
    )
    .unwrap();
    assert_ne!(first.windows[0].pool_id, second.windows[0].pool_id);
    assert_ne!(first.windows[0].id, second.windows[0].id);
}

/// A window identity is stable for one account across reads.
#[test]
fn one_local_account_keeps_stable_identities_across_reads() {
    let payload = fixture("opencode_go_success.json");
    let first = decode_offline(
        ProviderId::OpenCodeGo,
        &payload,
        "opencode-a",
        received_at(),
    )
    .unwrap();
    let second = decode_offline(
        ProviderId::OpenCodeGo,
        &payload,
        "opencode-a",
        received_at() + Duration::hours(2),
    )
    .unwrap();
    assert_eq!(first.windows[0].id, second.windows[0].id);
}
