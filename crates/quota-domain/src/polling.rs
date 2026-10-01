//! Typed polling policies and the decisions they produce.
//!
//! A policy is pure data. The shared supervisor owns task execution, budgets,
//! cancellation, and persistence; nothing here starts a timer or makes a
//! request.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ids::{AccountId, ConnectionId, QuotaPoolId};
use crate::provider::ProviderId;

/// How often an adapter wants to be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "settings")]
pub enum PollingStrategy {
    /// Separate validated intervals per visibility and power mode.
    FixedInterval(FixedIntervalPolicy),
    /// Frequency adjusted from approved change signals within bounds.
    Adaptive(AdaptivePolicy),
    /// Provider notifications plus periodic verification.
    EventAssisted(EventAssistedPolicy),
    /// Base polling plus a bounded verification near a reported boundary.
    BoundaryAware(BoundaryAwarePolicy),
}

impl PollingStrategy {
    /// The shortest interval this strategy may ever request.
    #[must_use]
    pub fn minimum_interval(&self) -> std::time::Duration {
        let seconds = match self {
            Self::FixedInterval(value) => value.minimum_seconds,
            Self::Adaptive(value) => value.minimum_seconds,
            Self::EventAssisted(value) => value.minimum_seconds,
            Self::BoundaryAware(value) => value.base.minimum_seconds,
        };
        std::time::Duration::from_secs(u64::from(seconds))
    }
}

/// Separate validated intervals for each display and power mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct FixedIntervalPolicy {
    /// Interval while an overview is visible and the adapter permits it.
    pub visible_seconds: u32,
    /// Interval while only the tray is running.
    pub background_seconds: u32,
    /// Interval during battery saver or prolonged idle.
    pub battery_saver_seconds: u32,
    /// The provider's floor. No other interval may be shorter.
    pub minimum_seconds: u32,
}

/// Bounds and signals for an adaptive strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AdaptivePolicy {
    /// The provider's floor.
    pub minimum_seconds: u32,
    /// The longest spacing the strategy may choose.
    pub maximum_seconds: u32,
    /// How much a single observed change moves the interval.
    pub step_seconds: u32,
}

/// A notification source plus its verification interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EventAssistedPolicy {
    /// The provider's floor, also used for the verification read.
    pub minimum_seconds: u32,
    /// How often a read verifies a push that may not have refreshed server data.
    pub verification_seconds: u32,
}

/// Base polling plus boundary verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct BoundaryAwarePolicy {
    /// Ordinary interval behaviour.
    pub base: FixedIntervalPolicy,
    /// How long after a reported boundary verification is attempted.
    pub boundary_grace_seconds: u32,
    /// How many verification attempts one boundary may receive.
    pub max_boundary_attempts: u32,
}

/// Backoff steps for transient failures, in minutes.
pub const DEFAULT_BACKOFF_MINUTES: [u32; 5] = [1, 2, 5, 15, 30];

/// The domain a rate limit or backoff applies to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum LimitScope {
    /// One account.
    Account(AccountId),
    /// One credential owner.
    Connection(ConnectionId),
    /// One provider across every account.
    Provider(ProviderId),
    /// One shared allowance pool.
    QuotaPool(QuotaPoolId),
    /// One source address, when the provider limits by origin.
    SourceAddress,
}

/// A validated policy attached to one provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ProviderPollingPolicy {
    /// The provider this policy belongs to.
    pub provider_id: ProviderId,
    /// The strategy the adapter declared.
    pub strategy: PollingStrategy,
    /// The remote request deadline.
    pub request_timeout_seconds: u32,
    /// The helper startup and read deadline, when a helper is used.
    pub helper_timeout_seconds: u32,
    /// Backoff steps for transient failures.
    pub backoff_minutes: Vec<u32>,
    /// The largest number of simultaneous remote reads.
    pub max_concurrent_remote_reads: u32,
    /// The version of this policy, so a stale persisted policy is detectable.
    pub version: u32,
}

impl ProviderPollingPolicy {
    /// The delay before retry number `attempt`, where the first retry is one.
    #[must_use]
    pub fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        let index = (attempt as usize).saturating_sub(1);
        let minutes = self
            .backoff_minutes
            .get(index)
            .copied()
            .or_else(|| self.backoff_minutes.last().copied())
            .unwrap_or(DEFAULT_BACKOFF_MINUTES[DEFAULT_BACKOFF_MINUTES.len() - 1]);
        Duration::minutes(i64::from(minutes))
    }
}

/// What the supervisor should do next for one account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum PollDecision {
    /// Read as soon as the supervisor has capacity.
    PollNow,
    /// Read at this instant.
    ScheduleAt(DateTime<Utc>),
    /// Wait for a provider notification, then verify on the policy interval.
    AwaitProviderEventWithVerification,
    /// Do not schedule; monitoring is paused or the account is disabled.
    Suspend,
    /// The credential must be renewed by the user before any read.
    RequireReconnect,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(backoff: Vec<u32>) -> ProviderPollingPolicy {
        ProviderPollingPolicy {
            provider_id: ProviderId::Fixture,
            strategy: PollingStrategy::FixedInterval(FixedIntervalPolicy {
                visible_seconds: 60,
                background_seconds: 300,
                battery_saver_seconds: 900,
                minimum_seconds: 30,
            }),
            request_timeout_seconds: 10,
            helper_timeout_seconds: 20,
            backoff_minutes: backoff,
            max_concurrent_remote_reads: 2,
            version: 1,
        }
    }

    #[test]
    fn backoff_walks_then_holds_at_the_last_step() {
        let value = policy(vec![1, 2, 5]);
        assert_eq!(value.backoff_for_attempt(1), Duration::minutes(1));
        assert_eq!(value.backoff_for_attempt(3), Duration::minutes(5));
        assert_eq!(value.backoff_for_attempt(9), Duration::minutes(5));
    }

    #[test]
    fn an_empty_backoff_list_still_produces_a_bounded_delay() {
        assert_eq!(
            policy(Vec::new()).backoff_for_attempt(1),
            Duration::minutes(30)
        );
    }

    #[test]
    fn the_shortest_interval_comes_from_every_strategy() {
        let fixed = PollingStrategy::FixedInterval(FixedIntervalPolicy {
            visible_seconds: 60,
            background_seconds: 300,
            battery_saver_seconds: 900,
            minimum_seconds: 30,
        });
        assert_eq!(fixed.minimum_interval(), std::time::Duration::from_secs(30));
        let adaptive = PollingStrategy::Adaptive(AdaptivePolicy {
            minimum_seconds: 45,
            maximum_seconds: 600,
            step_seconds: 30,
        });
        assert_eq!(
            adaptive.minimum_interval(),
            std::time::Duration::from_secs(45)
        );
    }
}
