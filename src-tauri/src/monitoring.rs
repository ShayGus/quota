//! One shared polling supervisor for all provider accounts.
//!
//! The supervisor runs on Tauri's async runtime. Every automatic and manual
//! read enters one bounded queue, uses one global two-read budget, checks its
//! connection generation before commit, and commits durable state before it
//! publishes a snapshot event.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use quota_contracts::commands::{BeginConnectionRequest, ConnectionAttemptAccepted};
use quota_contracts::events::ConnectionProgress;
use quota_contracts::refs::AttemptRef;
use quota_core::accounts::AccountRegistry;
use quota_core::clock::SystemClock;
use quota_core::ports::{AccountRepository, BackoffRepository};
use quota_core::snapshots::SnapshotBuilder;
use quota_domain::account::ConnectionState;
use quota_domain::ids::{AccountId, ConnectionAttemptId};
use quota_domain::polling::ProviderPollingPolicy;
use quota_domain::snapshot::MonitoringState;
use tokio::sync::{Semaphore, mpsc, watch};

use connection::emit_connection_progress;

mod connection;
mod worker;

const QUEUE_CAPACITY: usize = 64;
const MAX_REMOTE_READS: usize = 2;
const REMOTE_TIMEOUT: Duration = Duration::from_secs(10);
const PERIODIC_REFRESH: Duration = Duration::from_secs(300);

/// One account refresh request. All request sources use this same channel.
#[derive(Clone, Debug)]
pub struct RefreshRequest {
    /// The immutable account identity.
    pub account_id: AccountId,
    /// The reason is logged as a closed vocabulary, never parsed from text.
    pub reason: RefreshReason,
}

/// Why the shared supervisor was asked to read an account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshReason {
    /// The user asked for a refresh.
    UserRequested,
    /// The shared provider-specific schedule became due.
    Scheduled,
    /// A reported quota boundary needs verification.
    BoundaryVerification,
    /// The renderer opened and its cache was old.
    OverviewOpened,
    /// The operating system resumed from sleep.
    Resumed,
}

/// The handles shared by the supervisor and each bounded worker.
#[derive(Clone)]
struct RuntimeState {
    app: tauri::AppHandle,
    registry: Arc<tokio::sync::RwLock<AccountRegistry>>,
    snapshots: Arc<tokio::sync::Mutex<SnapshotBuilder>>,
    accounts: Arc<dyn AccountRepository>,
    backoff: Arc<dyn BackoffRepository>,
    monitoring: Arc<tokio::sync::RwLock<MonitoringState>>,
    policies: Arc<tokio::sync::RwLock<Vec<ProviderPollingPolicy>>>,
    providers: Arc<quota_providers::ProviderRegistry>,
    clock: Arc<SystemClock>,
    permits: Arc<Semaphore>,
    pending: Arc<tokio::sync::Mutex<HashSet<AccountId>>>,
}

/// A handle for user refresh and orderly shutdown.
#[derive(Clone)]
pub struct MonitoringRuntime {
    sender: mpsc::Sender<RefreshRequest>,
    shutdown: watch::Sender<bool>,
    attempts: Arc<tokio::sync::Mutex<HashMap<ConnectionAttemptId, watch::Sender<bool>>>>,
    connection_gate: Arc<tokio::sync::Mutex<()>>,
    state: RuntimeState,
}

/// The owned handles the shared polling supervisor starts with.
pub struct MonitoringStartup {
    /// The Tauri handle the supervisor emits events through.
    pub app: tauri::AppHandle,
    /// The live account registry the supervisor reads and commits.
    pub registry: Arc<tokio::sync::RwLock<AccountRegistry>>,
    /// The snapshot builder the supervisor renders from.
    pub snapshots: Arc<tokio::sync::Mutex<SnapshotBuilder>>,
    /// Durable account and binding storage.
    pub accounts: Arc<dyn AccountRepository>,
    /// Durable backoff storage.
    pub backoff: Arc<dyn BackoffRepository>,
    /// The live monitoring switch the supervisor obeys.
    pub monitoring: Arc<tokio::sync::RwLock<MonitoringState>>,
    /// The live polling policies the supervisor enforces.
    pub policies: Arc<tokio::sync::RwLock<Vec<ProviderPollingPolicy>>>,
    /// The compiled provider adapters the supervisor reads through.
    pub providers: Arc<quota_providers::ProviderRegistry>,
}

impl std::fmt::Debug for MonitoringRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonitoringRuntime")
            .field("queue_capacity", &QUEUE_CAPACITY)
            .field("max_remote_reads", &MAX_REMOTE_READS)
            .finish_non_exhaustive()
    }
}

impl MonitoringRuntime {
    /// Starts one supervised coordinator on Tauri's shared async runtime.
    #[must_use]
    pub fn start(startup: MonitoringStartup) -> Self {
        let (sender, receiver) = mpsc::channel(QUEUE_CAPACITY);
        let (shutdown, shutdown_receiver) = watch::channel(false);
        let state = RuntimeState {
            app: startup.app,
            registry: startup.registry,
            snapshots: startup.snapshots,
            accounts: startup.accounts,
            backoff: startup.backoff,
            monitoring: startup.monitoring,
            policies: startup.policies,
            providers: startup.providers,
            clock: Arc::new(SystemClock),
            permits: Arc::new(Semaphore::new(MAX_REMOTE_READS)),
            pending: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
        };
        let runtime = Self {
            sender,
            shutdown,
            attempts: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            connection_gate: Arc::new(tokio::sync::Mutex::new(())),
            state,
        };
        tauri::async_runtime::spawn(worker::run_coordinator(
            runtime.clone(),
            receiver,
            shutdown_receiver,
        ));
        runtime
    }

    /// Adds one user or lifecycle refresh to the common bounded queue.
    ///
    /// Repeated requests for the same account coalesce while a request is queued
    /// or in flight. The provider budget remains global across every account.
    pub async fn refresh(
        &self,
        account_ids: impl IntoIterator<Item = AccountId>,
        reason: RefreshReason,
    ) -> Result<Vec<AccountId>, quota_contracts::CommandError> {
        let mut accepted = Vec::new();
        for account_id in account_ids {
            let mut pending = self.state.pending.lock().await;
            if !pending.insert(account_id.clone()) {
                continue;
            }
            match self.sender.try_send(RefreshRequest {
                account_id: account_id.clone(),
                reason,
            }) {
                Ok(()) => accepted.push(account_id),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    pending.remove(&account_id);
                    return Err(quota_contracts::CommandError::ValidationFailed {
                        field: "refresh_queue".into(),
                        reason: "the shared refresh queue is full; try again shortly".into(),
                    });
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    pending.remove(&account_id);
                    return Err(quota_contracts::CommandError::Cancelled);
                }
            }
        }
        Ok(accepted)
    }

    /// Starts a cancellable local-credential connection attempt.
    ///
    /// The attempt identifier is returned before provider I/O starts, so a
    /// timeout or lost renderer reply can be reconciled without duplicating it.
    pub async fn begin_connection(
        &self,
        request: BeginConnectionRequest,
    ) -> Result<ConnectionAttemptAccepted, quota_contracts::CommandError> {
        if request.nickname.trim().is_empty()
            || request.nickname.chars().count() > quota_domain::account::MAX_NICKNAME_LEN
        {
            return Err(quota_contracts::CommandError::ValidationFailed {
                field: "nickname".into(),
                reason: "the nickname is blank or too long".into(),
            });
        }
        let adapter = self
            .state
            .providers
            .provider(request.provider_id)
            .cloned()
            .ok_or(quota_contracts::CommandError::UnsupportedProvider {
                provider_id: request.provider_id,
            })?;
        let attempt_id = ConnectionAttemptId::generate();
        let (cancel, cancel_receiver) = watch::channel(false);
        self.attempts
            .lock()
            .await
            .insert(attempt_id.clone(), cancel);
        let runtime = self.clone();
        let owned_attempt = attempt_id.clone();
        tauri::async_runtime::spawn(async move {
            let result = connection::run_connection_attempt(
                runtime.clone(),
                adapter,
                request,
                owned_attempt.clone(),
                cancel_receiver,
            )
            .await;
            runtime.attempts.lock().await.remove(&owned_attempt);
            if let Err(error) = result {
                emit_connection_progress(
                    &runtime.state,
                    &owned_attempt,
                    2,
                    ConnectionProgress::Failed { error },
                )
                .await;
            }
        });
        Ok(ConnectionAttemptAccepted {
            attempt_ref: AttemptRef::new(attempt_id.clone()),
            attempt_id,
        })
    }

    /// Cancels one active connection attempt.
    pub async fn cancel_connection(
        &self,
        attempt_id: &ConnectionAttemptId,
    ) -> Result<(), quota_contracts::CommandError> {
        let cancel = self.attempts.lock().await.remove(attempt_id);
        let Some(cancel) = cancel else {
            return Err(quota_contracts::CommandError::Cancelled);
        };
        cancel.send_replace(true);
        emit_connection_progress(&self.state, attempt_id, 2, ConnectionProgress::Cancelled).await;
        Ok(())
    }

    /// Bumps one connection generation, then queues a verified refresh.
    pub async fn reconnect_account(
        &self,
        account_id: &AccountId,
    ) -> Result<u64, quota_contracts::CommandError> {
        let connection_id = self
            .state
            .registry
            .read()
            .await
            .get(account_id)
            .map(|entry| entry.stored.connection.id.clone())
            .ok_or(quota_contracts::CommandError::AccountNotFound)?;
        let generation = self
            .state
            .accounts
            .bump_generation(&connection_id)
            .await
            .map_err(
                |error| quota_contracts::CommandError::PersistenceUnavailable {
                    owner: error.owner.to_owned(),
                },
            )?;
        let stored = {
            let mut registry = self.state.registry.write().await;
            registry
                .set_generation(&connection_id, generation)
                .map_err(|_| quota_contracts::CommandError::AccountNotFound)?;
            registry
                .set_connection_state(&connection_id, ConnectionState::Connecting)
                .map_err(|_| quota_contracts::CommandError::AccountNotFound)?;
            registry
                .get(account_id)
                .map(|entry| entry.stored.clone())
                .ok_or(quota_contracts::CommandError::AccountNotFound)?
        };
        self.state
            .accounts
            .upsert_account(stored)
            .await
            .map_err(
                |error| quota_contracts::CommandError::PersistenceUnavailable {
                    owner: error.owner.to_owned(),
                },
            )?;
        self.refresh([account_id.clone()], RefreshReason::UserRequested)
            .await?;
        Ok(generation)
    }

    /// Queues every currently enabled account through the same supervisor.
    pub async fn request_all(
        &self,
        reason: RefreshReason,
    ) -> Result<Vec<AccountId>, quota_contracts::CommandError> {
        let ids: Vec<AccountId> = self
            .state
            .registry
            .read()
            .await
            .iter()
            .filter(|entry| entry.stored.monitoring_enabled)
            .map(|entry| entry.account_id().clone())
            .collect();
        self.refresh(ids, reason).await
    }

    /// Stops new work and asks the coordinator to cancel its bounded workers.
    pub fn shutdown(&self) {
        self.shutdown.send_replace(true);
    }
}
