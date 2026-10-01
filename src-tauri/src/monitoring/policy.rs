//! When one account may next be read.
//!
//! The strategy is pure data owned by the provider; this turns it into the one
//! instant the supervisor schedules against. Every source of a refresh, whether
//! a timer tick or a user pressing refresh, goes through here, so a saved
//! interval, the provider's floor, a reported boundary and a stale reading all
//! constrain the next read the same way.

use chrono::{DateTime, Duration, Utc};
use quota_domain::polling::{PollingStrategy, ProviderPollingPolicy};

/// What the supervisor knows when it decides.
pub(crate) struct ReadSchedule<'a> {
    /// The policy the provider or the user saved.
    pub policy: &'a ProviderPollingPolicy,
    /// When the last accepted reading arrived, if there was one.
    pub last_success: Option<DateTime<Utc>>,
    /// When the current reading stops being current, if it expires.
    pub valid_until: Option<DateTime<Utc>>,
    /// Now.
    pub now: DateTime<Utc>,
}

/// The earliest instant this account may be read again.
///
/// Three floors apply at once, and the latest one wins: the strategy's own
/// spacing, the provider's minimum measured from the last accepted reading, and
/// the moment the current reading goes stale. A boundary-aware strategy moves a
/// read towards a reported reset but never past any of those floors.
pub(crate) fn next_read_at(schedule: &ReadSchedule<'_>) -> DateTime<Utc> {
    let ReadSchedule {
        policy,
        last_success,
        valid_until,
        now,
    } = schedule;
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

    let strategy_floor = now + strategy_delay.max(Duration::zero());

    // The provider's minimum is measured from the last accepted reading, so a
    // repeated manual refresh cannot outrun it.
    let provider_floor = last_success.map_or(now, |last| {
        last + chrono::Duration::from_std(policy.strategy.minimum_interval()).unwrap_or_default()
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

    next
}
