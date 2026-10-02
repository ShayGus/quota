//! When one account may next be read.
//!
//! The strategy is pure data owned by the provider; this turns it into the one
//! instant the supervisor schedules against. Every source of a refresh, whether
//! a timer tick or a user pressing refresh, goes through here, so a saved
//! interval, the provider's floor, a reported boundary and a stale reading all
//! constrain the next read the same way.

use chrono::{DateTime, Duration, Utc};
use quota_domain::account::ConnectionState;
use quota_domain::polling::{PollingStrategy, ProviderPollingPolicy};

use super::RefreshReason;

impl RefreshReason {
    pub(super) fn for_connection(self, state: ConnectionState) -> Self {
        if state == ConnectionState::Connecting {
            Self::Reconnect
        } else {
            self
        }
    }
}

/// What the supervisor knows when it decides.
pub(crate) struct ReadSchedule<'a> {
    /// The policy the provider or the user saved.
    pub policy: &'a ProviderPollingPolicy,
    /// When this account last sent a real read, if it ever has.
    pub last_attempt: Option<DateTime<Utc>>,
    /// When the last accepted reading arrived, if there was one.
    pub last_success: Option<DateTime<Utc>>,
    /// When the current reading stops being current, if it expires.
    pub valid_until: Option<DateTime<Utc>>,
    /// Now.
    pub now: DateTime<Utc>,
    pub reason: RefreshReason,
}

/// The earliest instant this account may be read again.
///
/// Three floors apply at once, and the latest one wins: the strategy's own
/// spacing, the provider's minimum measured from the last accepted reading, and
/// the moment the current reading goes stale. A boundary-aware strategy moves a
/// read towards a reported reset but never past any of those floors.
///
/// The strategy's spacing is measured from the last real attempt, never from
/// `now`. Measuring it from `now` would make every check move the next read
/// another whole interval into the future, so an account would never become due
/// at all. An account that has never been read is due immediately.
pub(crate) fn next_read_at(schedule: &ReadSchedule<'_>) -> DateTime<Utc> {
    let ReadSchedule {
        policy,
        last_attempt,
        last_success,
        valid_until,
        now,
        reason,
    } = schedule;
    if *reason == RefreshReason::Reconnect {
        return *now;
    }
    let now = *now;
    let valid_until = *valid_until;

    let strategy_delay = match &policy.strategy {
        PollingStrategy::FixedInterval(fixed) => Duration::seconds(fixed.visible_seconds.into()),
        PollingStrategy::Adaptive(adaptive) => Duration::seconds(adaptive.step_seconds.into()),
        PollingStrategy::EventAssisted(assisted) => {
            Duration::seconds(assisted.verification_seconds.into())
        }
        PollingStrategy::BoundaryAware(boundary) => {
            let base = Duration::seconds(boundary.base.visible_seconds.into());
            match valid_until {
                // A boundary inside the grace period earns one early check, so
                // the reported reset is verified rather than assumed.
                Some(reset)
                    if reset > now
                        && reset - now
                            <= Duration::seconds(boundary.boundary_grace_seconds.into()) =>
                {
                    Duration::zero()
                }
                _ => base,
            }
        }
    };

    let anchor = last_attempt.or(*last_success);
    let strategy_floor = match anchor {
        Some(anchor) => anchor + strategy_delay.max(Duration::zero()),
        None => now,
    };

    // The provider's minimum is measured from the last accepted reading, so a
    // repeated manual refresh cannot outrun it.
    let provider_floor = last_attempt
        .iter()
        .chain(last_success)
        .max()
        .map_or(now, |last| {
            *last
                + chrono::Duration::from_std(policy.strategy.minimum_interval()).unwrap_or_default()
        });

    let mut next = strategy_floor.max(provider_floor);

    // A reading that is going stale is worth re-reading before the strategy says
    // to, so the overview never shows a value the backend already excludes.
    if let Some(valid_until) = valid_until
        && valid_until > now
        && next > valid_until
    {
        next = valid_until;
    }

    next.max(provider_floor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quota_domain::polling::{
        AdaptivePolicy, BoundaryAwarePolicy, EventAssistedPolicy, FixedIntervalPolicy,
    };

    /// The policy every production adapter uses: five minutes, no variation.
    fn policy() -> ProviderPollingPolicy {
        ProviderPollingPolicy {
            provider_id: quota_domain::provider::ProviderId::Codex,
            strategy: PollingStrategy::EventAssisted(EventAssistedPolicy {
                minimum_seconds: 300,
                verification_seconds: 300,
            }),
            request_timeout_seconds: 10,
            helper_timeout_seconds: 0,
            backoff_minutes: Vec::new(),
            max_concurrent_remote_reads: 1,
            version: 1,
        }
    }

    /// A fixed-interval policy, for the same five-minute spacing.
    fn fixed_policy() -> ProviderPollingPolicy {
        ProviderPollingPolicy {
            strategy: PollingStrategy::FixedInterval(FixedIntervalPolicy {
                visible_seconds: 300,
                background_seconds: 300,
                battery_saver_seconds: 900,
                minimum_seconds: 300,
            }),
            ..policy()
        }
    }

    fn all_policies() -> [ProviderPollingPolicy; 4] {
        let PollingStrategy::FixedInterval(base) = fixed_policy().strategy else {
            unreachable!();
        };
        [
            policy(),
            fixed_policy(),
            ProviderPollingPolicy {
                strategy: PollingStrategy::Adaptive(AdaptivePolicy {
                    minimum_seconds: 300,
                    maximum_seconds: 900,
                    step_seconds: 30,
                }),
                ..policy()
            },
            ProviderPollingPolicy {
                strategy: PollingStrategy::BoundaryAware(BoundaryAwarePolicy {
                    base,
                    boundary_grace_seconds: 60,
                    max_boundary_attempts: 1,
                }),
                ..policy()
            },
        ]
    }

    fn base() -> DateTime<Utc> {
        DateTime::from_timestamp(1_789_000_000, 0).expect("a fixed instant")
    }

    /// A schedule whose last real read and last accepted reading are both at the
    /// base instant, checked `elapsed_seconds` later.
    fn schedule(policy: &ProviderPollingPolicy, elapsed_seconds: i64) -> ReadSchedule<'_> {
        ReadSchedule {
            policy,
            last_attempt: Some(base()),
            last_success: Some(base()),
            valid_until: None,
            now: base() + Duration::seconds(elapsed_seconds),
            reason: RefreshReason::Scheduled,
        }
    }

    /// The reproduced defect: every check pushed the next read one interval on.
    #[test]
    fn a_repeated_check_does_not_move_the_due_time() {
        for elapsed in [0, 300, 3_600, 86_400] {
            let event_assisted = policy();
            let fixed = fixed_policy();
            assert_eq!(
                next_read_at(&schedule(&event_assisted, elapsed)),
                base() + Duration::seconds(300),
                "the due time stays one interval after the last real read"
            );
            assert_eq!(
                next_read_at(&schedule(&fixed, elapsed)),
                base() + Duration::seconds(300)
            );
        }
    }

    /// The second check after a due read is not due again.
    #[test]
    fn an_account_is_due_exactly_one_interval_after_its_last_read() {
        let fixed = fixed_policy();
        let mut current = ReadSchedule {
            policy: &fixed,
            last_attempt: None,
            last_success: None,
            valid_until: None,
            now: base(),
            reason: RefreshReason::Scheduled,
        };
        // An account that has never been read is due now, not in five minutes.
        assert_eq!(next_read_at(&current), base());
        current.last_attempt = Some(base());
        current.last_success = Some(base());
        current.now = base() + Duration::seconds(299);
        assert_eq!(next_read_at(&current), base() + Duration::seconds(300));
        current.now = base() + Duration::seconds(300);
        assert_eq!(next_read_at(&current), base() + Duration::seconds(300));
        current.now = base() + Duration::seconds(301);
        assert!(
            next_read_at(&current) <= current.now,
            "one second past the interval the account is due again"
        );
    }
    #[test]
    fn ordinary_refreshes_respect_the_minimum_even_before_expiry() {
        for reason in [
            RefreshReason::UserRequested,
            RefreshReason::Scheduled,
            RefreshReason::BoundaryVerification,
            RefreshReason::OverviewOpened,
            RefreshReason::Resumed,
        ] {
            for policy in all_policies() {
                let mut current = schedule(&policy, 1);
                current.reason = reason;
                current.valid_until = Some(base() + Duration::seconds(2));
                assert_eq!(next_read_at(&current), base() + Duration::seconds(300));
                assert!(next_read_at(&current) > current.now);
            }
        }
    }

    #[test]
    fn every_source_queued_during_reconnect_keeps_immediate_verification() {
        let policy = policy();
        for reason in [
            RefreshReason::UserRequested,
            RefreshReason::Scheduled,
            RefreshReason::BoundaryVerification,
            RefreshReason::OverviewOpened,
            RefreshReason::Resumed,
            RefreshReason::Reconnect,
        ] {
            let mut current = schedule(&policy, 1);
            current.reason = reason.for_connection(ConnectionState::Connecting);
            assert_eq!(current.reason, RefreshReason::Reconnect);
            assert_eq!(next_read_at(&current), current.now);
            for state in [
                ConnectionState::NeverConnected,
                ConnectionState::Connected,
                ConnectionState::ReauthenticationRequired,
                ConnectionState::Unsupported,
                ConnectionState::Disconnected,
            ] {
                assert_eq!(reason.for_connection(state), reason);
            }
        }
    }

    #[test]
    fn reconnect_is_due_inside_the_minimum_interval() {
        for policy in all_policies() {
            let mut current = schedule(&policy, 1);
            current.reason = RefreshReason::Reconnect;
            assert_eq!(next_read_at(&current), current.now);
        }
    }

    #[test]
    fn first_connection_verification_is_due_without_a_previous_read() {
        for policy in all_policies() {
            let current = ReadSchedule {
                policy: &policy,
                last_attempt: None,
                last_success: None,
                valid_until: None,
                now: base(),
                reason: RefreshReason::UserRequested,
            };
            assert_eq!(next_read_at(&current), current.now);
        }
    }
}
