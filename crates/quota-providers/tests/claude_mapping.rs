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

/// A usable named-limits array fills the slots the fixed fields left empty.
#[test]
fn a_usable_limits_array_fills_the_slots_the_fixed_fields_left_empty() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_limits.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    // Three named limits and three windows: the fixed fields were not reported
    // here, so each named limit took the slot for its own period instead of
    // being reported as a missing window beside it. The identities come from
    // each limit's own group and model scope, not from where it sits.
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

/// Decodes one inline payload and returns the extra-usage window's measurement.
fn extra_usage_measurement(extra_usage: &str) -> Measurement {
    let payload = format!(
        r#"{{"limits":[{{"percent":50,"group":"five_hour","resets_at":"2026-09-30T12:00:00Z"}}],
            "extra_usage":{extra_usage}}}"#
    );
    let outcome =
        decode_offline(ProviderId::Claude, &payload, "claude-local", received_at()).unwrap();
    outcome
        .windows
        .iter()
        .find(|window| window.metric_role == MetricRole::ExtraSpendCap)
        .map_or_else(
            || panic!("the extra-usage window is missing"),
            |window| window.measurement.clone(),
        )
}

#[test]
fn a_fractional_decimal_places_is_rejected_rather_than_truncated() {
    let measurement =
        extra_usage_measurement(r#"{"monthlyLimit":5000,"usedCredits":1000,"decimalPlaces":2.5}"#);
    assert_eq!(
        measurement,
        Measurement::Unavailable(UnavailableReason::InvalidResponse),
        "a fractional scale was silently truncated into a complete measurement"
    );
}

#[test]
fn an_unusable_used_amount_invalidates_the_window_rather_than_reading_as_absent() {
    let measurement =
        extra_usage_measurement(r#"{"monthlyLimit":5000,"usedCredits":"not-a-number"}"#);
    assert_eq!(
        measurement,
        Measurement::Unavailable(UnavailableReason::InvalidResponse),
        "an invalid amount was collapsed into absence"
    );
}

#[test]
fn an_unusable_limit_invalidates_the_window_too() {
    let measurement =
        extra_usage_measurement(r#"{"monthlyLimit":"not-a-number","usedCredits":1000}"#);
    assert_eq!(
        measurement,
        Measurement::Unavailable(UnavailableReason::InvalidResponse),
        "an invalid limit was collapsed into absence"
    );
}

/// The two sources describe different allowances, so neither replaces the other.
#[test]
fn fixed_and_scoped_limits_are_merged_not_substituted() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_mixed_scoped.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    assert_eq!(
        reading.windows.len(),
        3,
        "session, weekly and the model window"
    );
    assert!(
        reading.is_complete(),
        "nothing the payload reported was lost"
    );
    let remaining = |bucket: &str| {
        reading
            .windows
            .iter()
            .find(|window| window.provider_bucket_id.as_deref() == Some(bucket))
            .expect("the window exists")
            .measurement
            .remaining_percent()
            .expect("a reported percentage")
            .value()
    };
    assert!((remaining("five-hour") - 72.0).abs() < f64::EPSILON);
    assert!((remaining("weekly") - 55.0).abs() < f64::EPSILON);
    assert!((remaining("weekly-scoped-claude-fable") - 87.0).abs() < f64::EPSILON);
}

/// An entry without a percentage states nothing, and must not shift the others.
#[test]
fn a_sparse_limits_array_never_panics_and_keeps_the_scoped_entry() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_sparse_limits.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    assert!(
        reading.windows.iter().any(
            |window| window.provider_bucket_id.as_deref() == Some("weekly-scoped-claude-fable")
        ),
        "the entry that does report a percentage survives the sparse array"
    );
}

/// The model and product allowances arrive as fixed fields of their own.
#[test]
fn sonnet_and_product_windows_are_reported() {
    let reading = decode_offline(
        ProviderId::Claude,
        &fixture("claude_product_windows.json"),
        "claude-local",
        received_at(),
    )
    .unwrap();
    let buckets: Vec<&str> = reading
        .windows
        .iter()
        .filter_map(|window| window.provider_bucket_id.as_deref())
        .collect();
    for expected in [
        "weekly-sonnet",
        "weekly-oauth-apps",
        "weekly-design",
        "weekly-routines",
    ] {
        assert!(buckets.contains(&expected), "{expected} in {buckets:?}");
    }
    let sonnet = reading
        .windows
        .iter()
        .find(|window| window.provider_bucket_id.as_deref() == Some("weekly-sonnet"))
        .expect("the Sonnet allowance exists");
    assert_eq!(sonnet.scope.label(), "Claude Sonnet");
    assert_eq!(sonnet.duration, Some(Duration::seconds(604_800)));
}

/// Two entries for one scope are separated rather than one being dropped.
#[test]
fn two_limits_for_the_same_scope_are_both_kept() {
    let payload = r#"{"limits":[
        {"kind":"seven_day","percent":10},
        {"kind":"seven_day","percent":20}]}"#;
    let reading =
        decode_offline(ProviderId::Claude, payload, "claude-local", received_at()).unwrap();
    let weekly: Vec<&str> = reading
        .category(QuotaCategory::Weekly)
        .iter()
        .filter_map(|window| window.provider_bucket_id.as_deref())
        .collect();
    assert_eq!(weekly.len(), 2, "neither entry was dropped: {weekly:?}");
}

/// Extra spend with no cap is still a cap-shaped window with no ceiling.
#[test]
fn extra_spend_without_a_cap_is_not_a_percentage() {
    let measurement = extra_usage_measurement(r#"{"usedCredits":2500}"#);
    let Measurement::Money(money) = measurement else {
        panic!("extra spend is an amount, not a percentage: {measurement:?}");
    };
    assert_eq!(money.limit_minor_units, None);
    assert_eq!(money.used_minor_units, Some(2500));
    assert_eq!(money.remaining_minor_units, None);
}

#[test]
fn account_wide_named_limits_fill_missing_slots_beside_every_product_scope() {
    for product in [
        "seven_day_opus",
        "seven_day_sonnet",
        "seven_day_oauth_apps",
        "seven_day_design",
        "seven_day_routines",
    ] {
        for (group, other) in [("seven_day", "five_hour"), ("five_hour", "seven_day")] {
            for scoped in [false, true] {
                let mut payload = serde_json::json!({
                    "limits": [{"group":group,"percent":20}]
                });
                payload[other] = serde_json::json!({"utilization":10});
                payload[product] = serde_json::json!({"utilization":30});
                if scoped {
                    payload["limits"][0]["scope"] = serde_json::json!({"model":{"id":"model"}});
                }
                let reading = decode_offline(
                    ProviderId::Claude,
                    &payload.to_string(),
                    "claude-local",
                    received_at(),
                )
                .unwrap();
                assert_eq!(reading.is_complete(), !scoped);
                assert_eq!(reading.expected_but_missing.len(), usize::from(scoped));
                assert_eq!(reading.windows.len(), 3 + usize::from(scoped));
            }
        }
    }
}
