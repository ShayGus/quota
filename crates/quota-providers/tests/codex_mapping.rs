//! Codex payload decoding, over sanitized fixtures and inline payloads.
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
use quota_domain::quota::window::QuotaCategory;
use quota_providers::{decode_offline, decode_offline_with_headers};

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
        ProviderId::Codex,
        &fixture("codex_success.json"),
        "codex-local",
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
    // The evidence survives: the used value stays unrounded and unclamped.
    let Measurement::Percentage(reported) = &session.measurement else {
        panic!("expected a percentage");
    };
    assert!((reported.used_percent.value() - 28.0).abs() < f64::EPSILON);
}

/// The provider's stated 18000 seconds is what makes a window a session.
#[test]
fn only_the_exact_session_duration_is_labelled_a_session() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_success.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    let session = reading.category(QuotaCategory::Session);
    assert_eq!(session.len(), 1);
    assert_eq!(session[0].duration, Some(Duration::seconds(18_000)));
    let weekly = reading.category(QuotaCategory::Weekly);
    assert_eq!(weekly.len(), 1);
    assert_eq!(weekly[0].duration, Some(Duration::seconds(604_800)));
}

/// AC-03 and AC-04: Codex has no monthly allowance, and none is fabricated.
#[test]
fn codex_never_reports_a_monthly_window() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_success.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    assert!(reading.category(QuotaCategory::Monthly).is_empty());
}

/// A duration that is neither five hours nor a week keeps its own scope.
#[test]
fn a_bucket_with_an_unrecognised_duration_keeps_its_own_category_and_scope() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_multibucket.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    let custom = reading.category(QuotaCategory::Custom);
    let monthly_duration = custom
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("monthly-custom"))
        .expect("the 30-day bucket keeps its own scope");
    assert_eq!(
        monthly_duration.duration,
        Some(Duration::seconds(2_592_000))
    );
    assert_eq!(
        monthly_duration.scope.resource().as_str(),
        "monthly-custom",
        "a custom bucket keeps its own resource, not a session label"
    );
}

/// AC-22: a model-specific bucket keeps its own scope.
#[test]
fn a_model_specific_bucket_keeps_its_own_scope() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_multibucket.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    let bucket = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("gpt-5-codex"))
        .expect("the model bucket exists");
    assert_eq!(bucket.scope.resource().as_str(), "gpt-5-codex");
    assert_eq!(bucket.scope.label(), "GPT-5 Codex");
    assert_eq!(bucket.category, QuotaCategory::Custom);
}

/// AC-10: overspend above 100% used is preserved, and the display arc is clamped.
#[test]
fn overspend_is_preserved_and_only_the_arc_is_clamped() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_multibucket.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    let primary = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("primary"))
        .expect("the primary window exists");
    let remaining = primary.measurement.remaining_percent().unwrap();
    assert!((remaining.value() + 37.5).abs() < f64::EPSILON);
    assert!(remaining.clamped().abs() < f64::EPSILON);
    assert!(remaining.arc_fraction().abs() < f64::EPSILON);
    assert!(remaining.is_exhausted());
}

/// AC-14: a window with no reset time gets no guessed one.
#[test]
fn a_window_without_a_reset_time_has_no_boundary() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_multibucket.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    let bucket = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("gpt-5-codex"))
        .expect("the model bucket exists");
    assert_eq!(bucket.boundary, None);
}

/// A credit balance is a balance, never included quota.
#[test]
fn a_credit_balance_is_not_included_quota() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_multibucket.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    let credits = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("credits"))
        .expect("the credit balance exists");
    assert!(!credits.metric_role.is_included_allowance());
    let Measurement::Quantity(balance) = &credits.measurement else {
        panic!("expected a quantity balance");
    };
    assert_eq!(balance.remaining, Some(12.5));
    assert_eq!(balance.limit, None);
    assert!(credits.measurement.remaining_percent().is_none());
}

/// AC-04: a window the source was expected to report and did not is recorded.
#[test]
fn a_missing_window_is_recorded_rather_than_zeroed() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_session_only.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    assert!(!reading.is_complete());
    assert_eq!(reading.expected_but_missing.len(), 1);
    let missing = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("secondary"))
        .expect("the missing slot keeps its place");
    assert_eq!(
        missing.measurement,
        Measurement::Unavailable(UnavailableReason::NotReported)
    );
}

/// AC-24: an unusable number is a typed invalid state, never a fabricated zero.
#[test]
fn an_unusable_number_is_typed_and_never_a_zero() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_malformed.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    let primary = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("primary"))
        .expect("the window keeps its place");
    assert_eq!(
        primary.measurement,
        Measurement::Unavailable(UnavailableReason::InvalidResponse)
    );
    assert!(!primary.measurement.has_number());
    assert_eq!(primary.issues.len(), 1);
    assert_eq!(primary.issues[0].code(), "non_finite_value");
}

/// AC-24: HTML from a broken endpoint is a typed invalid state.
#[test]
fn an_html_body_is_refused_as_invalid_data() {
    let error = decode_offline(
        ProviderId::Codex,
        &fixture("not_json.html"),
        "codex-local",
        received_at(),
    )
    .expect_err("an HTML body is not a reading");
    assert_eq!(error.diagnostic_code(), "invalid_data");
}

/// Two accounts of the same provider never share a pool or a window identity.
#[test]
fn two_local_accounts_of_one_provider_do_not_share_identity() {
    let payload = fixture("codex_success.json");
    let first = decode_offline(ProviderId::Codex, &payload, "codex-a", received_at()).unwrap();
    let second = decode_offline(ProviderId::Codex, &payload, "codex-b", received_at()).unwrap();
    assert_ne!(first.windows[0].pool_id, second.windows[0].pool_id);
    assert_ne!(first.windows[0].id, second.windows[0].id);
}

/// The same account decoded twice yields the same identities.
#[test]
fn one_local_account_keeps_stable_identities_across_reads() {
    let payload = fixture("codex_success.json");
    let first = decode_offline(ProviderId::Codex, &payload, "codex-a", received_at()).unwrap();
    let second = decode_offline(
        ProviderId::Codex,
        &payload,
        "codex-a",
        received_at() + Duration::hours(1),
    )
    .unwrap();
    assert_eq!(first.windows[0].id, second.windows[0].id);
    assert_eq!(first.windows[0].pool_id, second.windows[0].pool_id);
}

/// An address is masked in any label the reading carries.
#[test]
fn an_address_is_never_reported_in_full() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_success.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    let label = reading.principal_label.unwrap();
    assert!(!label.contains("synthetic.user@"));
    assert_eq!(label, "s***@example.invalid");
}

/// An unknown field must not fail the parse or change the reading.
#[test]
fn an_unknown_field_is_ignored() {
    let payload = r#"{
        "rate_limit": {
            "primary_window": {
                "used_percent": 28,
                "limit_window_seconds": 18000,
                "some_future_field": {"nested": [1, 2, 3]}
            }
        },
        "brand_new_top_level": "ignored"
    }"#;
    let reading = decode_offline(ProviderId::Codex, payload, "codex-local", received_at()).unwrap();
    let session = reading.category(QuotaCategory::Session);
    assert_eq!(session.len(), 1);
    assert!(
        (session[0].measurement.remaining_percent().unwrap().value() - 72.0).abs() < f64::EPSILON
    );
}

/// A body that carries its review and extra allowances at the root keeps them.
#[test]
fn a_root_review_and_extra_pair_are_not_dropped() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_root_review_and_extras.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    assert!(reading.is_complete(), "every reported window arrived");
    let buckets: Vec<&str> = reading
        .windows
        .iter()
        .filter_map(|window| window.provider_bucket_id.as_deref())
        .collect();
    assert_eq!(
        buckets,
        vec![
            "primary",
            "secondary",
            "code-review",
            "gpt-5-4-codex-spark",
            "gpt-5-4-codex-spark-secondary",
        ]
    );
    let review = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("code-review"))
        .expect("the review allowance exists");
    assert_eq!(review.category, QuotaCategory::Weekly);
}

/// A prepaid plan reports credits and no allowance at all.
#[test]
fn a_credits_only_payload_connects_without_an_allowance() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_credits_only.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    assert!(reading.is_complete(), "no allowance was expected of it");
    assert_eq!(reading.windows.len(), 1, "only the balance is reported");
    let credits = &reading.windows[0];
    assert_eq!(credits.provider_bucket_id.as_deref(), Some("credits"));
    assert!(!credits.metric_role.is_included_allowance());
    assert_eq!(credits.measurement.remaining_percent(), None);
}

/// A plan whose only allowance covers a month reports one window, not two.
#[test]
fn a_lone_monthly_allowance_does_not_invent_a_second_one() {
    let reading = decode_offline(
        ProviderId::Codex,
        &fixture("codex_monthly_only.json"),
        "codex-local",
        received_at(),
    )
    .unwrap();
    assert!(reading.is_complete(), "nothing was missing from it");
    assert_eq!(reading.windows.len(), 1);
    assert_eq!(reading.windows[0].category, QuotaCategory::Monthly);
    assert_eq!(
        reading.windows[0].duration,
        Some(Duration::seconds(2_592_000))
    );
}

/// A reported header wins over the body, and only when it is readable.
#[test]
fn a_response_header_overrides_the_body_and_ignores_nonsense() {
    let payload = fixture("codex_success.json");
    let mut headers = std::collections::BTreeMap::new();
    headers.insert("x-codex-primary-used-percent".to_owned(), "77".to_owned());
    let reading = decode_offline_with_headers(
        ProviderId::Codex,
        &payload,
        "codex-local",
        received_at(),
        &headers,
    )
    .unwrap();
    let session = &reading.category(QuotaCategory::Session)[0];
    let remaining = session.measurement.remaining_percent().unwrap();
    assert!((remaining.value() - 23.0).abs() < f64::EPSILON);

    headers.insert(
        "x-codex-primary-used-percent".to_owned(),
        "not-a-number".to_owned(),
    );
    let kept = decode_offline_with_headers(
        ProviderId::Codex,
        &payload,
        "codex-local",
        received_at(),
        &headers,
    )
    .unwrap();
    let body = kept.category(QuotaCategory::Session)[0]
        .measurement
        .remaining_percent()
        .unwrap();
    assert!(
        (body.value() - 72.0).abs() < f64::EPSILON,
        "an unreadable header leaves the body's own value standing"
    );
}

/// A header may also carry the remaining credit balance.
#[test]
fn a_response_header_carries_the_credit_balance() {
    let mut headers = std::collections::BTreeMap::new();
    headers.insert("x-codex-credits-balance".to_owned(), "7.5".to_owned());
    let reading = decode_offline_with_headers(
        ProviderId::Codex,
        &fixture("codex_multibucket.json"),
        "codex-local",
        received_at(),
        &headers,
    )
    .unwrap();
    let credits = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("credits"))
        .expect("the credit balance exists");
    let Measurement::Quantity(balance) = &credits.measurement else {
        panic!("expected a quantity balance");
    };
    assert_eq!(balance.remaining, Some(7.5));
}

#[test]
fn empty_containers_cannot_become_a_complete_reading() {
    for payload in [
        r#"{}"#,
        r#"{"rate_limit":{}}"#,
        r#"{"rateLimits":{}}"#,
        r#"{"code_review_rate_limit":{}}"#,
        r#"{"rate_limit":{"code_review_rate_limit":{}}}"#,
        r#"{"additional_rate_limits":[{"id":"model"}]}"#,
        r#"{"additional_rate_limits":[{"id":"model","rate_limit":{}}]}"#,
        r#"{"rate_limit":{"additional_rate_limits":[{"id":"model","rate_limit":{}}]}}"#,
        r#"{"credits":{}}"#,
        r#"{"rate_limit":{},"credits":{"unlimited":false}}"#,
    ] {
        let error = decode_offline(ProviderId::Codex, payload, "codex-local", received_at())
            .expect_err("an empty wrapper is not a reading");
        assert_eq!(error.diagnostic_code(), "invalid_data");
    }
}

#[test]
fn empty_containers_do_not_hide_a_reported_credit_balance() {
    for credits in [r#"{"balance":0}"#, r#"{"unlimited":true}"#] {
        let payload =
            format!(r#"{{"rate_limit":{{}},"code_review_rate_limit":{{}},"credits":{credits}}}"#);
        let reading =
            decode_offline(ProviderId::Codex, &payload, "codex-local", received_at()).unwrap();
        assert_eq!(reading.windows.len(), 1);
        assert!(reading.is_complete());
    }
}
