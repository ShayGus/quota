//! One supervised read: eligibility, the fetch, and the two commit paths.
//!
//! It lives beside the coordinator rather than inside it, because deciding when
//! an account may be read is a separate question from keeping the queue fed.

use chrono::{DateTime, Utc};
use quota_core::clock::Clock;
use quota_core::ports::ProviderError;
use quota_domain::account::{ConnectionState, FetchState};
use quota_domain::ids::ConnectionAttemptId;
use quota_domain::polling::LimitScope;
use quota_domain::snapshot::MonitoringState;

use super::policy;
use super::worker::publish_snapshot;
use super::{REMOTE_TIMEOUT, RefreshReason, RefreshRequest, RuntimeState};

pub(super) async fn perform_read(
    state: &RuntimeState,
    request: &RefreshRequest,
) -> Result<(), String> {
    let Some(target) = resolve_read_target(state, request).await? else {
        return Ok(());
    };
    let read = fetch_reading(state, &target.entry, &target.adapter, &target.binding).await?;
    commit_reading(state, request, &target, &read).await
}

/// The verified account, adapter, and backoff scope one supervised read needs.
pub(super) struct ReadTarget {
    entry: quota_core::accounts::RegisteredAccount,
    binding: quota_core::ports::ConnectionBinding,
    adapter: std::sync::Arc<dyn quota_core::ports::ProviderAdapter>,
    scope: LimitScope,
}

/// The current time on the injected clock.
pub(super) fn now(state: &RuntimeState) -> DateTime<Utc> {
    state.clock.now()
}

/// When this account may next be read, or `None` when it has no policy.
///
/// An account whose provider declares no saved policy falls back to the
/// provider's own default, so a read is always schedulable.
pub(super) async fn next_read_at(
    state: &RuntimeState,
    entry: &quota_core::accounts::RegisteredAccount,
) -> Option<DateTime<Utc>> {
    let policy = state
        .policies
        .read()
        .await
        .iter()
        .find(|policy| policy.provider_id == entry.binding.provider_id)
        .cloned()?;
    Some(policy::next_read_at(&policy::ReadSchedule {
        policy: &policy,
        last_attempt: entry.stored.last_attempt_at,
        last_success: entry.stored.last_success_at,
        valid_until: entry
            .stored
            .windows
            .iter()
            .filter_map(|window| window.valid_until)
            .min(),
        now: state.clock.now(),
    }))
}

/// Resolves the account, adapter, and scope, or reports there is nothing to do.
pub(super) async fn resolve_read_target(
    state: &RuntimeState,
    request: &RefreshRequest,
) -> Result<Option<ReadTarget>, String> {
    if *state.monitoring.read().await == MonitoringState::Paused {
        return Ok(None);
    }
    let entry = state
        .registry
        .read()
        .await
        .get(&request.account_id)
        .cloned()
        .ok_or_else(|| "account_removed".to_owned())?;
    if !entry.stored.monitoring_enabled {
        return Ok(None);
    }
    let binding = entry.binding.clone();
    // A scheduled or lifecycle-driven request waits for the account's due time.
    // A refresh the person asked for is explicit, so it runs now; the policy
    // still bounds how often the account reads itself.
    if request.reason != RefreshReason::UserRequested {
        let Some(next_read_at) = next_read_at(state, &entry).await else {
            return Ok(None);
        };
        if next_read_at > now(state) {
            return Ok(None);
        }
    }
    let adapter = state
        .providers
        .provider(binding.provider_id)
        .cloned()
        .ok_or_else(|| "unsupported_provider".to_owned())?;
    let scope = LimitScope::Connection(binding.connection_id.clone());
    let now = state.clock.now();
    if state
        .backoff
        .load_backoff(&scope)
        .await
        .map_err(|error| error.reason)?
        .is_some_and(|backoff| backoff.is_waiting_at(now))
    {
        return Ok(None);
    }
    // Every eligibility check has passed, so this is a real read. The floor is
    // stamped now, after the decision, so a check that decided not to read
    // leaves the account's schedule untouched.
    let entry = stamp_dispatch_floor(state, request).await?;
    Ok(Some(ReadTarget {
        entry,
        binding,
        adapter,
        scope,
    }))
}

/// Records when this account may next be read, and returns its stamped entry.
///
/// It is a floor, not the final schedule: an accepted commit recomputes it from
/// the policy and the reading it just took.
async fn stamp_dispatch_floor(
    state: &RuntimeState,
    request: &RefreshRequest,
) -> Result<quota_core::accounts::RegisteredAccount, String> {
    let now = state.clock.now();
    let _commit = state.commit.lock().await;
    let mut registry = state.registry.write().await;
    let Some(entry) = registry.get(&request.account_id).cloned() else {
        return Err("account_removed_before_dispatch".to_owned());
    };
    let minimum = {
        let policies = state.policies.read().await;
        policies
            .iter()
            .find(|policy| policy.provider_id == entry.binding.provider_id)
            .map_or_else(
                || chrono::Duration::seconds(300),
                |policy| {
                    chrono::Duration::from_std(policy.strategy.minimum_interval())
                        .unwrap_or_default()
                },
            )
    };
    registry
        .record_dispatch(&request.account_id, now, Some(now + minimum))
        .map_err(|error| error.to_string())?;
    registry
        .get(&request.account_id)
        .cloned()
        .ok_or_else(|| "account_removed_before_dispatch".to_owned())
}

/// Reads one binding with a timeout, recording a failure when none arrives.
pub(super) async fn fetch_reading(
    state: &RuntimeState,
    entry: &quota_core::accounts::RegisteredAccount,
    adapter: &std::sync::Arc<dyn quota_core::ports::ProviderAdapter>,
    binding: &quota_core::ports::ConnectionBinding,
) -> Result<quota_core::ports::QuotaRead, String> {
    let now = state.clock.now();
    let context = quota_core::ports::ReadContext {
        attempt_id: ConnectionAttemptId::generate(),
        deadline: Some(now + chrono::Duration::seconds(10)),
    };
    let response =
        match tokio::time::timeout(REMOTE_TIMEOUT, adapter.read_quota(binding, context)).await {
            Err(_) => {
                record_failure(
                    state,
                    entry,
                    ProviderError::Transient {
                        detail: "request timeout".to_owned(),
                    },
                )
                .await?;
                return Err("request_timeout".to_owned());
            }
            Ok(Err(error)) => {
                let code = error.diagnostic_code().to_owned();
                record_failure(state, entry, error).await?;
                return Err(code);
            }
            Ok(Ok(response)) => response,
        };
    let Some(read) = response.read() else {
        if let quota_core::ports::FetchOutcome::Failed(error) = response {
            let code = error.diagnostic_code().to_owned();
            record_failure(state, entry, error).await?;
            return Err(code);
        }
        return Err("provider_returned_no_reading".to_owned());
    };
    Ok(read.clone())
}

/// Commits one reading after re-checking the connection generation.
pub(super) async fn commit_reading(
    state: &RuntimeState,
    request: &RefreshRequest,
    target: &ReadTarget,
    read: &quota_core::ports::QuotaRead,
) -> Result<(), String> {
    let windows = read.windows.clone();
    let expected_missing = read.expected_but_missing.clone();
    let identity = read.identity.clone();
    let now = state.clock.now();
    // The boundary covers the registry mutation, the durable write and the
    // publication, so no other writer can interleave between them.
    let _commit = state.commit.lock().await;
    let stored = {
        let mut registry = state.registry.write().await;
        let live = registry
            .get(&request.account_id)
            .ok_or_else(|| "account_removed_before_commit".to_owned())?;
        if !live.binding.accepts(&target.binding) {
            return Err("stale_connection_generation".to_owned());
        }
        registry
            .apply_reading(&request.account_id, windows, expected_missing)
            .map_err(|error| error.to_string())?;
        registry
            .set_identity(&request.account_id, identity)
            .map_err(|error| error.to_string())?;
        // A reconnect is only finished when a read is accepted, so a successful
        // read is what returns the connection to Connected.
        registry
            .set_connection_state(&target.binding.connection_id, ConnectionState::Connected)
            .map_err(|error| error.to_string())?;
        let next = next_read_at(state, &target.entry)
            .await
            .unwrap_or(now + chrono::Duration::seconds(300));
        registry
            .record_attempt(&request.account_id, FetchState::Idle, now, Some(next))
            .map_err(|error| error.to_string())?;
        registry
            .get(&request.account_id)
            .map(|account| account.stored.clone())
            .ok_or_else(|| "account_removed_before_persist".to_owned())?
    };

    // The repository commits all current measurement values before the
    // renderer can observe the new revision.
    state
        .accounts
        .upsert_account(stored)
        .await
        .map_err(|error| error.reason)?;
    state
        .backoff
        .clear_backoff(&target.scope)
        .await
        .map_err(|error| error.reason)?;
    publish_snapshot(state).await
}

pub(super) async fn record_failure(
    state: &RuntimeState,
    entry: &quota_core::accounts::RegisteredAccount,
    error: ProviderError,
) -> Result<(), String> {
    let now = state.clock.now();
    let scope = LimitScope::Connection(entry.binding.connection_id.clone());
    // The connection may have been reconnected while this read was in flight. A
    // late failure must not write backoff, fetch state or an authentication
    // demand onto the generation that replaced it.
    {
        let registry = state.registry.read().await;
        let live = registry
            .get(entry.account_id())
            .ok_or_else(|| "account_removed_during_failure".to_owned())?;
        if !live.binding.accepts(&entry.binding) {
            return Ok(());
        }
    }
    let previous = state
        .backoff
        .load_backoff(&scope)
        .await
        .map_err(|e| e.reason)?;
    let attempts = previous.map_or(1, |record| record.attempts.saturating_add(1));
    let delay = failure_delay(state, entry, attempts, &error).await;
    let provider_retry_after = match &error {
        ProviderError::RateLimited { retry_after } => *retry_after,
        _ => None,
    };
    let next_eligible_at = provider_retry_after.unwrap_or(now + delay);
    state
        .backoff
        .persist_backoff(
            &scope,
            quota_core::ports::BackoffState {
                attempts,
                next_eligible_at,
                provider_retry_after,
            },
        )
        .await
        .map_err(|e| e.reason)?;

    let _commit = state.commit.lock().await;
    let updated = {
        let mut registry = state.registry.write().await;
        // Re-checked under the boundary, because a reconnect can land between the
        // guard above and this write.
        let live = registry
            .get(entry.account_id())
            .ok_or_else(|| "account_removed_after_failure".to_owned())?;
        if !live.binding.accepts(&entry.binding) {
            return Ok(());
        }
        let fetch_state = if matches!(&error, ProviderError::RateLimited { .. }) {
            FetchState::Backoff
        } else {
            FetchState::Error
        };
        registry
            .record_attempt(entry.account_id(), fetch_state, now, Some(next_eligible_at))
            .map_err(|error| error.to_string())?;
        if matches!(&error, ProviderError::Authentication) {
            registry
                .set_connection_state(
                    &entry.binding.connection_id,
                    ConnectionState::ReauthenticationRequired,
                )
                .map_err(|error| error.to_string())?;
        }
        registry
            .get(entry.account_id())
            .map(|value| value.stored.clone())
            .ok_or_else(|| "account_removed_after_failure".to_owned())?
    };
    state
        .accounts
        .upsert_account(updated)
        .await
        .map_err(|e| e.reason)?;
    publish_snapshot(state).await
}

/// How long to wait before retrying, given the saved policy and this failure.
///
/// A rate limit the provider named wins; otherwise the policy's own backoff
/// steps apply.
pub(super) async fn failure_delay(
    state: &RuntimeState,
    entry: &quota_core::accounts::RegisteredAccount,
    attempts: u32,
    error: &ProviderError,
) -> chrono::Duration {
    if let ProviderError::RateLimited {
        retry_after: Some(deadline),
    } = error
    {
        return (*deadline - state.clock.now()).max(chrono::Duration::zero());
    }
    let policy = state
        .policies
        .read()
        .await
        .iter()
        .find(|policy| policy.provider_id == entry.binding.provider_id)
        .cloned();
    policy.map_or_else(
        || chrono::Duration::minutes(5),
        |policy| policy.backoff_for_attempt(attempts),
    )
}
