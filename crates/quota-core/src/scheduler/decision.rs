//! The pure scheduling decision.
//!
//! This function takes a typed context and returns a typed decision. It never
//! starts a timer, performs a request, or reads a clock directly, so every
//! policy is testable without a runtime or a network.

use chrono::{DateTime, Duration, Utc};

use quota_domain::account::ConnectionState;
use quota_domain::ids::{AccountId, ConnectionId};
use quota_domain::polling::{
    FixedIntervalPolicy, PollDecision, PollingStrategy, ProviderPollingPolicy,
};

/// What the host is currently doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// An overview window is on screen.
    OverviewVisible,
    /// Only the tray is running.
    TrayOnly,
    /// The machine is conserving power.
    BatterySaver,
}

/// Everything a policy is allowed to consider.
#[derive(Clone, Debug)]
pub struct PollContext {
    /// The account the decision is for.
    pub account_id: AccountId,
    /// The connection the account belongs to.
    pub connection_id: ConnectionId,
    /// The generation the decision belongs to.
    pub generation: u32,
    /// The account's monitoring switch.
    pub monitoring_enabled: bool,
    /// Whether the user paused monitoring application-wide.
    pub monitoring_paused: bool,
    /// Where the connection stands.
    pub connection_state: ConnectionState,
    /// What the host is doing.
    pub visibility: Visibility,
    /// When the last accepted reading arrived.
    pub last_success_at: Option<DateTime<Utc>>,
    /// The next instant the provider permits another read.
    pub provider_not_before: Option<DateTime<Utc>>,
    /// The earliest instant backoff permits another read.
    pub backoff_not_before: Option<DateTime<Utc>>,
    /// The next reported boundary, when one is known.
    pub next_boundary_at: Option<DateTime<Utc>>,
    /// How long the boundary grace period is.
    pub boundary_grace_seconds: u64,
    /// The current instant, injected by the caller.
    pub now: DateTime<Utc>,
}

impl PollContext {
    /// A context for an account that has never been read.
    #[must_use]
    pub fn new(account_id: AccountId, connection_id: ConnectionId, now: DateTime<Utc>) -> Self {
        Self {
            account_id,
            connection_id,
            generation: 0,
            monitoring_enabled: true,
            monitoring_paused: false,
            connection_state: ConnectionState::Connected,
            visibility: Visibility::TrayOnly,
            last_success_at: None,
            provider_not_before: None,
            backoff_not_before: None,
            next_boundary_at: None,
            boundary_grace_seconds: 30,
            now,
        }
    }
}

/// A deterministic, documented jitter derived from the account identity.
///
/// The same account always receives the same offset, so a restart does not
/// reshuffle the schedule, and different accounts do not all fire together.
#[must_use]
pub fn stable_jitter(account_id: &AccountId, window_seconds: u64) -> Duration {
    if window_seconds == 0 {
        return Duration::zero();
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in account_id.as_str().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let span = window_seconds.saturating_mul(2).max(1);
    Duration::seconds(i64::try_from(hash % span).unwrap_or(0))
}

/// Decides what to do next for one account.
#[must_use]
pub fn decide(context: &PollContext, policy: &ProviderPollingPolicy) -> PollDecision {
    if !context.monitoring_enabled || context.monitoring_paused {
        return PollDecision::Suspend;
    }
    if matches!(
        context.connection_state,
        ConnectionState::ReauthenticationRequired | ConnectionState::Disconnected
    ) {
        return PollDecision::RequireReconnect;
    }

    let floor = floor_at(context);
    let interval = interval_for(&policy.strategy, context.visibility);
    let interval = clamp_to_provider_floor(interval, policy.strategy.minimum_interval());

    // A boundary that has just passed deserves one verification attempt, inside
    // the same budget. It never refills anything on its own.
    if boundary_due(context) {
        let grace = context.now + Duration::seconds(seconds(context.boundary_grace_seconds));
        return PollDecision::ScheduleAt(grace.max(floor).max(context.now));
    }

    let Some(last) = context.last_success_at else {
        return PollDecision::PollNow;
    };

    let window = interval.num_seconds().unsigned_abs();
    let earliest_due = last + interval + stable_jitter(&context.account_id, window);
    PollDecision::ScheduleAt(earliest_due.max(floor).max(context.now))
}

/// The earliest instant a provider or a backoff deadline permits.
fn floor_at(context: &PollContext) -> DateTime<Utc> {
    match (context.provider_not_before, context.backoff_not_before) {
        (Some(a), Some(b)) => a.max(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => DateTime::<Utc>::MIN_UTC,
    }
}

/// Narrows an unsigned count to the signed duration seconds expect.
fn seconds(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Whether a reported boundary has just passed and needs verification.
fn boundary_due(context: &PollContext) -> bool {
    let Some(boundary) = context.next_boundary_at else {
        return false;
    };
    let grace = Duration::seconds(seconds(context.boundary_grace_seconds));
    boundary <= context.now && context.now < boundary + grace
}

fn interval_for(strategy: &PollingStrategy, visibility: Visibility) -> Duration {
    let policy = match strategy {
        PollingStrategy::FixedInterval(value) => *value,
        PollingStrategy::BoundaryAware(value) => value.base,
        PollingStrategy::Adaptive(value) => from_adaptive(*value),
        PollingStrategy::EventAssisted(value) => from_event_assisted(*value),
    };
    let seconds = match visibility {
        Visibility::OverviewVisible => policy.visible_seconds,
        Visibility::TrayOnly => policy.background_seconds,
        Visibility::BatterySaver => policy.battery_saver_seconds,
    };
    Duration::seconds(i64::from(seconds))
}

fn from_adaptive(value: quota_domain::polling::AdaptivePolicy) -> FixedIntervalPolicy {
    FixedIntervalPolicy {
        visible_seconds: value.minimum_seconds,
        background_seconds: value.minimum_seconds.saturating_mul(2),
        battery_saver_seconds: value.maximum_seconds,
        minimum_seconds: value.minimum_seconds,
    }
}

fn from_event_assisted(value: quota_domain::polling::EventAssistedPolicy) -> FixedIntervalPolicy {
    FixedIntervalPolicy {
        visible_seconds: value.minimum_seconds,
        background_seconds: value.verification_seconds,
        battery_saver_seconds: value.verification_seconds.saturating_mul(3),
        minimum_seconds: value.minimum_seconds,
    }
}

fn clamp_to_provider_floor(interval: Duration, floor: std::time::Duration) -> Duration {
    let floor = Duration::from_std(floor).unwrap_or(interval);
    if interval < floor { floor } else { interval }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quota_domain::polling::{
        AdaptivePolicy, BoundaryAwarePolicy, EventAssistedPolicy, FixedIntervalPolicy,
    };
    use quota_domain::provider::ProviderId;

    fn ids() -> (AccountId, ConnectionId) {
        (
            AccountId::new("a").unwrap(),
            ConnectionId::new("c").unwrap(),
        )
    }

    fn fixed(minimum_seconds: u32) -> PollingStrategy {
        PollingStrategy::FixedInterval(FixedIntervalPolicy {
            visible_seconds: 60,
            background_seconds: 300,
            battery_saver_seconds: 900,
            minimum_seconds,
        })
    }

    fn policy(strategy: PollingStrategy) -> ProviderPollingPolicy {
        ProviderPollingPolicy {
            provider_id: ProviderId::Codex,
            strategy,
            request_timeout_seconds: 10,
            helper_timeout_seconds: 20,
            backoff_minutes: vec![1, 2, 5],
            max_concurrent_remote_reads: 2,
            version: 1,
        }
    }

    #[test]
    fn an_account_that_was_never_read_polls_immediately() {
        let (account, connection) = ids();
        let now = DateTime::UNIX_EPOCH;
        let context = PollContext::new(account, connection, now);
        assert_eq!(decide(&context, &policy(fixed(30))), PollDecision::PollNow);
    }

    #[test]
    fn a_disabled_account_suspends() {
        let (account, connection) = ids();
        let now = DateTime::UNIX_EPOCH;
        let mut context = PollContext::new(account, connection, now);
        context.monitoring_enabled = false;
        assert_eq!(decide(&context, &policy(fixed(30))), PollDecision::Suspend);
    }

    #[test]
    fn a_paused_application_suspends_every_account() {
        let (account, connection) = ids();
        let now = DateTime::UNIX_EPOCH;
        let mut context = PollContext::new(account, connection, now);
        context.monitoring_paused = true;
        assert_eq!(decide(&context, &policy(fixed(30))), PollDecision::Suspend);
    }

    #[test]
    fn an_expired_credential_requires_reconnect_not_a_retry_loop() {
        let (account, connection) = ids();
        let now = DateTime::UNIX_EPOCH;
        let mut context = PollContext::new(account, connection, now);
        context.connection_state = ConnectionState::ReauthenticationRequired;
        assert_eq!(
            decide(&context, &policy(fixed(30))),
            PollDecision::RequireReconnect
        );
    }

    #[test]
    fn a_provider_floor_is_never_shortened() {
        let (account, connection) = ids();
        let now = DateTime::UNIX_EPOCH;
        let mut context = PollContext::new(account, connection, now);
        context.last_success_at = Some(now);
        // The strategy asks for 60 s, the provider permits 300 s.
        let decision = decide(&context, &policy(fixed(300)));
        let PollDecision::ScheduleAt(at) = decision else {
            panic!("expected a schedule");
        };
        assert!(at >= now + Duration::seconds(300));
    }

    #[test]
    fn a_backoff_deadline_holds_the_next_read() {
        let (account, connection) = ids();
        let now = DateTime::UNIX_EPOCH;
        let mut context = PollContext::new(account, connection, now);
        context.last_success_at = Some(now);
        context.backoff_not_before = Some(now + Duration::minutes(5));
        let PollDecision::ScheduleAt(at) = decide(&context, &policy(fixed(30))) else {
            panic!("expected a schedule");
        };
        assert!(at >= now + Duration::minutes(5));
    }

    #[test]
    fn a_passed_boundary_gets_one_bounded_verification() {
        let (account, connection) = ids();
        let now = DateTime::UNIX_EPOCH + Duration::hours(1);
        let mut context = PollContext::new(account, connection, now);
        context.last_success_at = Some(now - Duration::hours(2));
        context.next_boundary_at = Some(now - Duration::seconds(5));
        let strategy = PollingStrategy::BoundaryAware(BoundaryAwarePolicy {
            base: FixedIntervalPolicy {
                visible_seconds: 300,
                background_seconds: 300,
                battery_saver_seconds: 900,
                minimum_seconds: 30,
            },
            boundary_grace_seconds: 30,
            max_boundary_attempts: 2,
        });
        let PollDecision::ScheduleAt(at) = decide(&context, &policy(strategy)) else {
            panic!("expected a boundary verification");
        };
        assert_eq!(at, now + Duration::seconds(30));
    }

    #[test]
    fn an_adaptive_strategy_stays_inside_its_bounds() {
        let (account, connection) = ids();
        let now = DateTime::UNIX_EPOCH;
        let mut context = PollContext::new(account, connection, now);
        context.last_success_at = Some(now);
        let strategy = PollingStrategy::Adaptive(AdaptivePolicy {
            minimum_seconds: 120,
            maximum_seconds: 900,
            step_seconds: 60,
        });
        let PollDecision::ScheduleAt(at) = decide(&context, &policy(strategy)) else {
            panic!("expected a schedule");
        };
        assert!(at >= now + Duration::seconds(120));
        assert!(at <= now + Duration::seconds(900 + 240));
    }

    #[test]
    fn an_event_assisted_strategy_still_verifies_on_its_interval() {
        let (account, connection) = ids();
        let now = DateTime::UNIX_EPOCH;
        let mut context = PollContext::new(account, connection, now);
        context.last_success_at = Some(now);
        let strategy = PollingStrategy::EventAssisted(EventAssistedPolicy {
            minimum_seconds: 60,
            verification_seconds: 300,
        });
        let PollDecision::ScheduleAt(at) = decide(&context, &policy(strategy)) else {
            panic!("expected a schedule");
        };
        assert!(at >= now + Duration::seconds(300));
    }

    #[test]
    fn jitter_is_stable_per_account_and_zero_for_a_zero_window() {
        let (first, _) = ids();
        let second = AccountId::new("b").unwrap();
        assert_eq!(stable_jitter(&first, 300), stable_jitter(&first, 300));
        assert_eq!(stable_jitter(&first, 0), Duration::zero());
        assert!(stable_jitter(&first, 300) < Duration::seconds(600));
        let _ = second;
    }
}
