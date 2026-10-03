//! One verified connection candidate, held until the person decides.
//!
//! A candidate has been verified against the provider and read once, but
//! nothing about it is durable until it is committed. Candidates live only in
//! memory, so cancelling the review or restarting the application leaves no
//! account and no monitoring behind. Closing the native settings window only
//! hides it; leaving or replacing the wizard cancels its attempt on unmount.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use quota_contracts::CommandError;
use quota_contracts::commands::{BeginConnectionRequest, VerifiedCandidate};
use quota_contracts::events::ConnectionProgress;
use quota_core::ports::{ConnectionBinding, DiscoveredAccount, ProviderAdapter, QuotaRead};
use quota_domain::account::ConnectionState;
use quota_domain::ids::{AccountId, ConnectionAttemptId, ConnectionId};
use tokio::sync::{Mutex, watch};

use super::MonitoringRuntime;
use super::connection::AttemptReporter;
use super::worker::publish_snapshot;

/// The fresh identities one verified candidate is committed under.
pub(super) struct CandidateIds {
    connection_id: ConnectionId,
    account_id: AccountId,
    pub(super) binding: ConnectionBinding,
}

/// When the verifying read was sent and when its answer arrived, so the saved
/// account records that read rather than the moment it was confirmed.
#[derive(Clone, Copy)]
pub(super) struct ReadTimestamps {
    pub(super) dispatched_at: DateTime<Utc>,
    pub(super) completed_at: DateTime<Utc>,
}

/// One candidate waiting for the person's decision.
pub(super) struct PendingConnection {
    candidate: DiscoveredAccount,
    request: BeginConnectionRequest,
    ids: CandidateIds,
    read: QuotaRead,
    timestamps: ReadTimestamps,
    /// The attempt's reporter, so a later result keeps that attempt's revisions.
    pub(super) reporter: Arc<AttemptReporter>,
}

impl PendingConnection {
    /// Collects everything one verified attempt produced.
    pub(super) fn new(
        candidate: DiscoveredAccount,
        request: BeginConnectionRequest,
        ids: CandidateIds,
        read: QuotaRead,
        timestamps: ReadTimestamps,
        reporter: Arc<AttemptReporter>,
    ) -> Self {
        Self {
            candidate,
            request,
            ids,
            read,
            timestamps,
            reporter,
        }
    }

    /// The identity the wizard shows, before anything is saved.
    fn verified(&self) -> VerifiedCandidate {
        VerifiedCandidate {
            provider_id: self.ids.binding.provider_id,
            nickname: self.request.nickname.clone(),
            identity: self.read.identity.clone(),
            windows: self.read.windows.clone(),
        }
    }
}

/// The candidates waiting for a decision, keyed by the attempt that found them.
///
/// Keying on the attempt lets a second attempt hold its own candidate while an
/// earlier one is still on screen, and keeps a restart from resurrecting one.
#[derive(Clone, Default)]
pub(super) struct PendingConnections(Arc<Mutex<HashMap<ConnectionAttemptId, PendingConnection>>>);

impl PendingConnections {
    /// Holds one candidate under the attempt that verified it.
    async fn hold(&self, attempt_id: &ConnectionAttemptId, pending: PendingConnection) {
        self.0.lock().await.insert(attempt_id.clone(), pending);
    }

    /// Removes one candidate, or answers `None` when none is waiting.
    ///
    /// The caller holds the connection gate, so a candidate cannot be held
    /// between this removal and the caller's own check.
    pub(super) async fn take(&self, attempt_id: &ConnectionAttemptId) -> Option<PendingConnection> {
        self.0.lock().await.remove(attempt_id)
    }
}

/// Mints the connection, account, and binding for one candidate.
pub(super) fn candidate_binding(
    adapter: &Arc<dyn ProviderAdapter>,
    candidate: &DiscoveredAccount,
) -> CandidateIds {
    let connection_id = ConnectionId::generate();
    let account_id = AccountId::generate();
    let binding = ConnectionBinding {
        connection_id: connection_id.clone(),
        generation: 0,
        provider_id: adapter.provider_id(),
        principal_id: candidate.principal_id.clone(),
        workspace_id: candidate.workspace_id.clone(),
        entitlement_id: candidate.entitlement_id.clone(),
        profile_label: candidate.profile_label.clone(),
    };
    CandidateIds {
        connection_id,
        account_id,
        binding,
    }
}

/// Holds one verified candidate for the person to confirm.
///
/// The gate is taken here and the cancellation flag is re-read under it, so a
/// cancellation that already ran cannot leave behind a candidate that the
/// settings window has stopped showing.
pub(super) async fn hold_candidate(
    runtime: &MonitoringRuntime,
    attempt_id: &ConnectionAttemptId,
    pending: PendingConnection,
    cancelled: &watch::Receiver<bool>,
) -> Result<VerifiedCandidate, CommandError> {
    let _connection_gate = runtime.connection_gate.lock().await;
    if *cancelled.borrow() {
        return Err(CommandError::Cancelled);
    }
    let verified = pending.verified();
    runtime.pending.hold(attempt_id, pending).await;
    Ok(verified)
}

/// Saves the candidate the person confirmed under the nickname they confirmed,
/// then starts monitoring it.
pub(super) async fn commit_pending(
    runtime: &MonitoringRuntime,
    attempt_id: &ConnectionAttemptId,
    nickname: String,
) -> Result<(), CommandError> {
    let _connection_gate = runtime.connection_gate.lock().await;
    let Some(pending) = runtime.pending.take(attempt_id).await else {
        return Err(CommandError::AccountNotFound);
    };
    let _commit = runtime.state.commit.lock().await;
    let PendingConnection {
        candidate,
        mut request,
        ids,
        read,
        timestamps,
        reporter,
    } = pending;
    request.nickname = nickname;
    if let Err(error) =
        commit_with_credential(runtime, candidate, &request, ids, read, timestamps).await
    {
        // The candidate is spent either way, so the wizard returns to the
        // connect step with the typed reason rather than a dead Add button.
        reporter
            .emit(
                &runtime.state,
                attempt_id,
                ConnectionProgress::Failed {
                    error: error.clone(),
                },
            )
            .await;
        return Err(error);
    }
    reporter
        .emit(
            &runtime.state,
            attempt_id,
            ConnectionProgress::Verified {
                state: ConnectionState::Connected,
            },
        )
        .await;
    if let Err(code) = publish_snapshot(&runtime.state).await {
        tracing::warn!(%code, "the saved connection snapshot was not delivered");
    }
    Ok(())
}

/// Stores the credential the person supplied, under the connection it signs
/// in, then saves the account that uses it. When the account cannot be saved,
/// its credential is removed again, so nothing is left behind.
async fn commit_with_credential(
    runtime: &MonitoringRuntime,
    candidate: DiscoveredAccount,
    request: &BeginConnectionRequest,
    ids: CandidateIds,
    read: QuotaRead,
    timestamps: ReadTimestamps,
) -> Result<(), CommandError> {
    let Some(credential) = request.credential.as_ref() else {
        return commit_candidate(runtime, candidate, request, ids, read, timestamps).await;
    };
    let secrets = runtime.state.providers.secrets();
    let connection_id = ids.connection_id.clone();
    let secret = quota_core::ports::Secret::new(credential.expose().to_owned());
    super::credentials::store(secrets, &connection_id, secret).await?;
    let committed = commit_candidate(runtime, candidate, request, ids, read, timestamps).await;
    if committed.is_err() {
        super::credentials::forget(secrets, connection_id).await;
    }
    committed
}

/// Persists one confirmed candidate and registers it with the supervisor.
async fn commit_candidate(
    runtime: &MonitoringRuntime,
    candidate: DiscoveredAccount,
    request: &BeginConnectionRequest,
    ids: CandidateIds,
    read: QuotaRead,
    timestamps: ReadTimestamps,
) -> Result<(), CommandError> {
    let stored = quota_core::ports::StoredAccount {
        account_id: ids.account_id.clone(),
        connection: quota_domain::account::ConnectionSummary {
            id: ids.connection_id,
            provider_id: ids.binding.provider_id,
            credential_ownership: candidate.credential_ownership,
            generation: 0,
            profile_label: candidate.profile_label.clone(),
            cardinality: candidate.cardinality,
            state: ConnectionState::Connected,
            principal_id: candidate.principal_id,
            workspace_id: candidate.workspace_id,
            entitlement_id: candidate.entitlement_id,
        },
        nickname: request.nickname.clone(),
        connection_ordinal: 0,
        monitoring_enabled: true,
        connection_state: ConnectionState::Connected,
        fetch_state: quota_domain::account::FetchState::Idle,
        last_attempt_at: Some(timestamps.dispatched_at),
        last_success_at: Some(timestamps.completed_at),
        next_attempt_at: Some(timestamps.completed_at + chrono::Duration::seconds(300)),
        identity: Some(read.identity),
        windows: read.windows,
        expected_but_missing_window_ids: read.expected_but_missing,
    };
    let new_account = quota_core::accounts::NewAccount {
        account_id: ids.account_id.clone(),
        stored: stored.clone(),
        profile_label: candidate.profile_label,
        monitoring_enabled: true,
    };

    if runtime
        .state
        .registry
        .read()
        .await
        .find_duplicate(&new_account)
        .is_some()
    {
        return Err(CommandError::ValidationFailed {
            field: "binding".into(),
            reason: "this verified account and quota pool are already connected".into(),
        });
    }
    // The registry assigns the authoritative ordinal, so the account is
    // registered before it is written: persisting first stored ordinal zero and
    // left the stable tie-break order to chance on the next restart.
    let mut registry = runtime.state.registry.write().await;
    let registered = match registry.register(new_account) {
        Ok(entry) => entry.stored.clone(),
        Err(error) => return Err(core_command_error(error)),
    };
    drop(registry);
    if let Err(error) = runtime.state.accounts.upsert_account(registered).await {
        // The account is only durable if this succeeded, so the registration it
        // made is withdrawn rather than left in memory alone.
        let mut registry = runtime.state.registry.write().await;
        if let Err(error) = registry.remove(&ids.account_id) {
            tracing::warn!(code = %error, "the rolled-back account was still registered in memory");
        }
        return Err(CommandError::PersistenceUnavailable {
            owner: error.owner.to_owned(),
        });
    }
    Ok(())
}

fn core_command_error(error: quota_core::CoreError) -> CommandError {
    match error {
        quota_core::CoreError::AccountNotFound(_)
        | quota_core::CoreError::ConnectionNotFound(_) => CommandError::AccountNotFound,
        quota_core::CoreError::ReconnectRequired => CommandError::ReconnectRequired,
        quota_core::CoreError::StaleResult => CommandError::RevisionConflict {
            expected: 0,
            actual: 0,
        },
        quota_core::CoreError::Validation { field, reason } => CommandError::ValidationFailed {
            field: field.into(),
            reason: reason.into(),
        },
        quota_core::CoreError::Provider(error) => super::connection::provider_command_error(error),
        quota_core::CoreError::Persistence { owner } => CommandError::PersistenceUnavailable {
            owner: owner.into(),
        },
    }
}
