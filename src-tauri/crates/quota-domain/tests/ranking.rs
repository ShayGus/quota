//! Least-remaining-first ordering, exercised through the public domain API.
// Integration tests are compiled without `cfg(test)`, so the crate-wide
// test-context allowance in `clippy.toml` does not reach this file. The scope
// is this file only, and every `unwrap` builds an inline literal fixture.
#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]
#![expect(clippy::unwrap_used, reason = "inline fixtures must be constructible")]
use chrono::{DateTime, Duration};
use proptest::prelude::*;
use quota_domain::account::ConnectionState;
use quota_domain::ids::{AccountId, DefinitionVersion, QuotaPoolId, QuotaWindowId, ResourceId};
use quota_domain::quota::measurement::{Measurement, PercentageMeasurement};
use quota_domain::quota::scope::QuotaScope;
use quota_domain::quota::units::DecimalPrecision;
use quota_domain::quota::window::{
    Boundary, BoundaryKind, Completeness, Enforcement, MetricRole, QuotaCategory, QuotaWindow,
    SourceKind, WindowSemantics,
};
use quota_domain::ranking::{
    AccountOrder, OrderSection, RankingInput, UnrankedReason, compute_order, rank_account,
};

fn id(name: &str) -> AccountId {
    AccountId::new(name).unwrap()
}

fn percent_window(window_id: &str, remaining: f64) -> QuotaWindow {
    QuotaWindow {
        id: QuotaWindowId::new(window_id).unwrap(),
        provider_bucket_id: None,
        pool_id: QuotaPoolId::new("pool").unwrap(),
        scope: QuotaScope::new(ResourceId::new("model_x").unwrap(), "Model X").unwrap(),
        category: QuotaCategory::Session,
        semantics: WindowSemantics::Unknown,
        duration: None,
        metric_role: MetricRole::IncludedAllowance,
        enforcement: Enforcement::Unknown,
        measurement: Measurement::Percentage(
            PercentageMeasurement::from_used_percent(
                100.0 - remaining,
                DecimalPrecision::new(0).unwrap(),
            )
            .unwrap(),
        ),
        period_started_at: None,
        boundary: None,
        observed_at: None,
        received_at: DateTime::UNIX_EPOCH,
        valid_until: None,
        source: SourceKind::DocumentedApi,
        completeness: Completeness::Complete,
        definition_version: DefinitionVersion::INITIAL,
        issues: Vec::new(),
    }
}

fn input<'a>(account: &'a AccountId, ordinal: u32, windows: &'a [QuotaWindow]) -> RankingInput<'a> {
    RankingInput {
        account_id: account,
        connection_ordinal: ordinal,
        windows,
        monitoring_enabled: true,
        monitoring_paused: false,
        connection_state: ConnectionState::Connected,
        now: DateTime::UNIX_EPOCH + Duration::hours(1),
    }
}

#[test]
fn uses_the_lowest_included_window_not_the_earliest_reset() {
    let account = id("a");
    let mut monthly = percent_window("m", 0.0);
    monthly.category = QuotaCategory::Monthly;
    monthly.boundary = Some(Boundary {
        at: DateTime::UNIX_EPOCH + Duration::days(12),
        kind: BoundaryKind::FullReset,
    });
    let windows = vec![
        percent_window("s", 86.0),
        percent_window("w", 53.0),
        monthly,
    ];
    let AccountOrder::Ranked(ranked) = rank_account(&input(&account, 1, &windows)) else {
        panic!("expected a comparable rank");
    };
    assert!(ranked.remaining_percent.value().abs() < f64::EPSILON);
    assert_eq!(ranked.controlling_window_id.as_str(), "m");
}

#[test]
fn a_three_percent_weekly_ranks_above_a_forty_one_percent_account() {
    let low = id("low");
    let high = id("high");
    let low_windows = vec![percent_window("s", 72.0), percent_window("w", 3.0)];
    let high_windows = vec![percent_window("s", 72.0), percent_window("w", 41.0)];
    let inputs = vec![input(&high, 1, &high_windows), input(&low, 2, &low_windows)];
    let order = compute_order(&inputs);
    assert_eq!(order[0].account_id, low);
    assert_eq!(order[1].account_id, high);
}

#[test]
fn ties_break_stably_and_are_independent_of_input_order() {
    let first = id("account-a");
    let second = id("account-b");
    let windows = vec![percent_window("s", 42.0)];
    let order = compute_order(&[input(&second, 5, &windows), input(&first, 4, &windows)]);
    assert_eq!(order[0].account_id, first);
    let reversed = compute_order(&[input(&first, 4, &windows), input(&second, 5, &windows)]);
    assert_eq!(order, reversed);
}

#[test]
fn extra_spend_and_credit_balances_never_take_part() {
    let account = id("a");
    let mut cap = percent_window("cap", 1.0);
    cap.metric_role = MetricRole::ExtraSpendCap;
    let mut balance = percent_window("bal", 2.0);
    balance.metric_role = MetricRole::CreditBalance;
    let order = rank_account(&input(&account, 1, &[cap, balance]));
    assert!(matches!(
        order,
        AccountOrder::Unranked(value) if value.reason == UnrankedReason::NoIncludedAllowance
    ));
}

#[test]
fn a_stale_reading_never_becomes_a_current_rank() {
    let account = id("a");
    let now = DateTime::UNIX_EPOCH + Duration::hours(3);
    let mut window = percent_window("s", 12.0);
    window.valid_until = Some(now - Duration::minutes(1));
    let one_window = [window];
    let mut probe = input(&account, 1, &one_window);
    probe.now = now;
    let order = rank_account(&probe);
    assert!(matches!(
        order,
        AccountOrder::Unranked(value) if value.reason == UnrankedReason::Stale
    ));
}

#[test]
fn an_expired_boundary_reads_as_reset_pending() {
    let account = id("a");
    let now = DateTime::UNIX_EPOCH + Duration::hours(3);
    let mut window = percent_window("s", 80.0);
    window.boundary = Some(Boundary {
        at: now,
        kind: BoundaryKind::FullReset,
    });
    let one_window = [window];
    let mut probe = input(&account, 1, &one_window);
    probe.now = now;
    let order = rank_account(&probe);
    assert!(matches!(
        order,
        AccountOrder::Unranked(value) if value.reason == UnrankedReason::ResetPending
    ));
}

#[test]
fn needs_checking_accounts_sort_above_ranked_ones() {
    let healthy = id("healthy");
    let broken = id("broken");
    let mut probe = input(&broken, 1, &[]);
    probe.connection_state = ConnectionState::ReauthenticationRequired;
    let order = compute_order(&[input(&healthy, 2, &[percent_window("s", 90.0)]), probe]);
    assert_eq!(order[0].account_id, broken);
    assert_eq!(order[0].section, OrderSection::NeedsChecking);
}

#[test]
fn disabled_accounts_sort_last() {
    let off = id("off");
    let on = id("on");
    let off_windows = [percent_window("s", 0.0)];
    let mut probe = input(&off, 1, &off_windows);
    probe.monitoring_enabled = false;
    let order = compute_order(&[input(&on, 2, &[percent_window("s", 90.0)]), probe]);
    assert_eq!(order[0].account_id, on);
    assert_eq!(order[1].section, OrderSection::MonitoringOff);
}

#[test]
fn a_known_zero_stays_visible_next_to_a_missing_window() {
    let account = id("a");
    let mut known_zero = percent_window("s", 0.0);
    known_zero.category = QuotaCategory::Monthly;
    let mut missing = percent_window("w", 50.0);
    missing.measurement =
        Measurement::Unavailable(quota_domain::quota::measurement::UnavailableReason::NotReported);
    let windows = vec![known_zero, missing];
    let order = rank_account(&input(&account, 1, &windows));
    let value = order.order_value().expect("a known zero stays ranked");
    assert!(value.value().abs() < f64::EPSILON);
}

proptest! {
    #[test]
    fn ranking_is_independent_of_input_order(
        a in 0u32..4,
        b in 0u32..4,
        x in 0.0f64..100.0,
        y in 0.0f64..100.0,
    ) {
        let first = id("account-a");
        let second = id("account-b");
        let low = percent_window("s", x);
        let high = percent_window("s", y);
        let (low_first, low_windows, high_windows) =
            if x <= y { (true, vec![low], vec![high]) } else { (false, vec![high], vec![low]) };
        let one = compute_order(&[
            input(&first, a, &low_windows),
            input(&second, b, &high_windows),
        ]);
        let two = compute_order(&[
            input(&second, b, &high_windows),
            input(&first, a, &low_windows),
        ]);
        prop_assert_eq!(&one, &two);
        prop_assert_eq!(one[0].section, OrderSection::Ranked);
        if low_first {
            prop_assert_eq!(&one[0].account_id, &first);
        }
    }
}
