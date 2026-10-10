//! Tests for the prepaid-balance ledger.

use super::*;

fn usd() -> CurrencyCode {
    CurrencyCode::new("USD").expect("a currency")
}

fn reading(loaded: i64, spent: i64) -> BalanceReading {
    BalanceReading {
        currency: usd(),
        scale: 2,
        loaded_minor: loaded,
        spent_minor: spent,
        credits: Vec::new(),
        cycle_spend: None,
    }
}

fn at(hours: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(1_790_000_000, 0).expect("an instant") + Duration::hours(hours)
}

fn left(ledger: &BalanceLedger) -> (Option<i64>, Option<i64>) {
    match ledger.measurement() {
        Measurement::Money(money) => (money.remaining_minor_units, money.limit_minor_units),
        other => panic!("not money: {other:?}"),
    }
}

#[test]
fn before_a_top_up_the_balance_is_measured_from_the_first_reading() {
    let first = BalanceLedger::record(None, &reading(10_000, 6_000), at(0));
    assert_eq!(first.baseline_kind, BaselineKind::SinceAdded);
    assert_eq!(left(&first), (Some(4_000), Some(4_000)));
    let later = BalanceLedger::record(Some(&first), &reading(10_000, 7_000), at(1));
    assert_eq!(later.baseline_kind, BaselineKind::SinceAdded);
    assert_eq!(left(&later), (Some(3_000), Some(4_000)));
    assert!(later.top_ups.is_empty());
}

#[test]
fn a_rise_in_the_amount_loaded_is_a_top_up_of_exactly_that_much() {
    let first = BalanceLedger::record(None, &reading(10_000, 9_000), at(0));
    // $50 loaded, and $2 spent since the last reading.
    let topped = BalanceLedger::record(Some(&first), &reading(15_000, 9_200), at(2));
    assert_eq!(topped.baseline_kind, BaselineKind::TopUp);
    assert_eq!(topped.baseline_at, at(2));
    assert_eq!(
        topped.top_ups,
        vec![TopUp {
            detected_at: at(2),
            amount_minor: 5_000,
            balance_after_minor: 6_000,
        }]
    );
    assert_eq!(left(&topped), (Some(5_800), Some(6_000)));
}

#[test]
fn a_fall_in_the_amount_loaded_measures_the_balance_from_now() {
    let first = BalanceLedger::record(None, &reading(10_000, 2_000), at(0));
    let refunded = BalanceLedger::record(Some(&first), &reading(9_000, 2_500), at(1));
    assert_eq!(refunded.baseline_kind, BaselineKind::Adjusted);
    assert_eq!(left(&refunded), (Some(6_500), Some(6_500)));
    assert!(refunded.top_ups.is_empty());
}

#[test]
fn a_refunded_charge_never_shows_more_than_full() {
    let first = BalanceLedger::record(None, &reading(10_000, 4_000), at(0));
    let lower = BalanceLedger::record(Some(&first), &reading(10_000, 5_000), at(1));
    let refunded = BalanceLedger::record(Some(&lower), &reading(10_000, 3_000), at(2));
    assert_eq!(left(&refunded), (Some(7_000), Some(7_000)));
}

#[test]
fn an_empty_baseline_has_no_share() {
    let empty = BalanceLedger::record(None, &reading(500, 500), at(0));
    assert_eq!(left(&empty), (Some(0), None));
    assert!(empty.measurement().remaining_percent().is_none());
}

#[test]
fn the_top_up_history_is_bounded() {
    let mut ledger = BalanceLedger::record(None, &reading(0, 0), at(0));
    for step in 1..=30 {
        ledger = BalanceLedger::record(Some(&ledger), &reading(step * 100, 0), at(step));
    }
    assert_eq!(ledger.top_ups.len(), TOP_UP_HISTORY);
    assert_eq!(ledger.top_ups[0].amount_minor, 100);
    assert_eq!(ledger.top_ups[0].detected_at, at(30));
}

#[test]
fn the_runway_needs_a_day_of_history_and_some_spending() {
    let first = BalanceLedger::record(None, &reading(10_000, 0), at(0));
    let hours_later = BalanceLedger::record(Some(&first), &reading(10_000, 300), at(6));
    assert_eq!(hours_later.runway(at(6)), None);
    let idle = BalanceLedger::record(Some(&first), &reading(10_000, 0), at(48));
    assert_eq!(idle.runway(at(48)), None);
    // $6 spent over two days is $3 a day, and $94 lasts 31 more days.
    let spending = BalanceLedger::record(Some(&first), &reading(10_000, 600), at(48));
    assert_eq!(
        spending.runway(at(48)),
        Some(Runway {
            spend_per_day_minor: 300,
            days_left: 31,
        })
    );
}

#[test]
fn the_pace_is_measured_over_the_last_week_only() {
    let mut ledger = BalanceLedger::record(None, &reading(100_000, 0), at(0));
    // A big week, then a quiet one at $1 a day.
    ledger = BalanceLedger::record(Some(&ledger), &reading(100_000, 50_000), at(24 * 7));
    for day in 8..=14 {
        ledger = BalanceLedger::record(
            Some(&ledger),
            &reading(100_000, 50_000 + (day - 7) * 100),
            at(24 * day),
        );
    }
    let runway = ledger.runway(at(24 * 14)).expect("a pace");
    assert_eq!(runway.spend_per_day_minor, 100);
    assert!(ledger.samples.len() <= 9, "older samples are dropped");
}

#[test]
fn samples_are_kept_at_most_hourly() {
    let mut ledger = BalanceLedger::record(None, &reading(10_000, 0), at(0));
    for minutes in [5, 10, 30, 59] {
        ledger = BalanceLedger::record(
            Some(&ledger),
            &reading(10_000, minutes),
            at(0) + Duration::minutes(minutes),
        );
    }
    assert_eq!(ledger.samples.len(), 1);
    ledger = BalanceLedger::record(Some(&ledger), &reading(10_000, 70), at(1));
    assert_eq!(ledger.samples.len(), 2);
}

#[test]
fn a_new_currency_starts_a_new_ledger() {
    let first = BalanceLedger::record(None, &reading(10_000, 0), at(0));
    let mut euros = reading(20_000, 0);
    euros.currency = CurrencyCode::new("EUR").expect("a currency");
    let next = BalanceLedger::record(Some(&first), &euros, at(1));
    assert_eq!(next.baseline_kind, BaselineKind::SinceAdded);
    assert!(next.top_ups.is_empty());
}

fn grant(amount: i64, remaining: i64, expires_hours: i64) -> CreditGrant {
    CreditGrant {
        kind: CreditKind::Purchased,
        amount_minor: amount,
        remaining_minor: remaining,
        expires_at: at(expires_hours),
    }
}

/// A reading that lists its grants: loaded is their sum.
fn granted(credits: Vec<CreditGrant>, balance: i64) -> BalanceReading {
    let loaded: i64 = credits.iter().map(|grant| grant.amount_minor).sum();
    BalanceReading {
        credits,
        cycle_spend: Some(CycleSpend {
            label: "October 2026".to_owned(),
            spent_minor: 120,
        }),
        ..reading(loaded, loaded - balance)
    }
}

#[test]
fn listed_grants_measure_the_balance_from_their_sum_from_the_first_reading() {
    let first = BalanceLedger::record(
        None,
        &granted(
            vec![grant(1_000, 420, 600), grant(4_000, 3_300, 9_000)],
            3_720,
        ),
        at(0),
    );
    assert_eq!(first.baseline_kind, BaselineKind::Credits);
    assert_eq!(left(&first), (Some(3_720), Some(5_000)));
    let summary = first.summary(at(0));
    assert_eq!(summary.credits.len(), 2);
    assert_eq!(
        summary.credits[0].expires_at,
        at(600),
        "soonest to expire first"
    );
    assert_eq!(
        summary.cycle_spend.map(|spend| spend.spent_minor),
        Some(120)
    );
}

#[test]
fn a_new_grant_is_a_top_up_and_an_expired_one_leaves_the_gauge() {
    let first = BalanceLedger::record(None, &granted(vec![grant(1_000, 500, 600)], 500), at(0));
    let bought = BalanceLedger::record(
        Some(&first),
        &granted(
            vec![grant(1_000, 500, 600), grant(2_000, 2_000, 9_000)],
            2_500,
        ),
        at(1),
    );
    assert_eq!(bought.top_ups.len(), 1);
    assert_eq!(bought.top_ups[0].amount_minor, 2_000);
    assert_eq!(bought.baseline_kind, BaselineKind::Credits);
    assert_eq!(left(&bought), (Some(2_500), Some(3_000)));
    let expired = BalanceLedger::record(
        Some(&bought),
        &granted(vec![grant(2_000, 1_900, 9_000)], 1_900),
        at(700),
    );
    assert_eq!(expired.baseline_kind, BaselineKind::Credits);
    assert_eq!(left(&expired), (Some(1_900), Some(2_000)));
    assert_eq!(expired.top_ups.len(), 1, "an expiry is not a top-up");
}

#[test]
fn a_ledger_saved_before_grants_existed_still_loads() {
    let ledger = BalanceLedger::record(None, &reading(10_000, 2_500), at(0));
    let mut saved = serde_json::to_value(&ledger).expect("serializes");
    let object = saved.as_object_mut().expect("an object");
    object.remove("credits");
    object.remove("cycle_spend");
    let loaded: BalanceLedger = serde_json::from_value(saved).expect("loads");
    assert!(loaded.credits.is_empty());
    assert_eq!(loaded.cycle_spend, None);
}

#[test]
fn the_summary_carries_the_balance_and_lifetime_amounts() {
    let ledger = BalanceLedger::record(None, &reading(10_000, 2_500), at(0));
    let summary = ledger.summary(at(0));
    assert_eq!(summary.balance_minor, 7_500);
    assert_eq!(summary.loaded_minor, 10_000);
    assert_eq!(summary.runway, None);
}
