//! One shared polling supervisor for all provider accounts.
//!
//! The supervisor runs on Tauri's async runtime. Every automatic and manual
//! read enters one bounded queue, uses one global two-read budget, checks its
//! connection generation before commit, and commits durable state before it
//! publishes a snapshot event.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use quota_contracts::CommandError;
use quota_contracts::commands::{BeginConnectionRequest, ConnectionAttemptAccepted};
use quota_contracts::events::ConnectionProgress;
use quota_contracts::refs::AttemptRef;
use quota_core::accounts::AccountRegistry;
use quota_core::clock::SystemClock;
use quota_core::ports::{AccountRepository, BackoffRepository};
use quota_core::snapshots::SnapshotBuilder;
use quota_domain::account::{ConnectionState, CredentialOwnership};
use quota_domain::ids::{AccountId, ConnectionAttemptId};
use quota_domain::polling::ProviderPollingPolicy;
use quota_domain::snapshot::MonitoringState;
use tokio::sync::{Semaphore, mpsc, watch};

use confirm::PendingConnections;
use connection::AttemptReporter;

/// The live handle to one connection attempt.
///
/// The attempt stays here until it reaches a terminal result, so a cancellation
/// can reach it and a terminal event can be emitted under the same revision
/// counter that emitted its progress.
struct AttemptHandle {
    /// The flag the running attempt watches.
    cancel: watch::Sender<bool>,
    /// The counter that numbers this attempt's progress.
    reporter: Arc<AttemptReporter>,
}

mod browser_session;
mod confirm;
mod connection;
pub(crate) mod console;
pub(crate) mod credentials;
mod device;
mod policy;
mod queue;
mod read_path;
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
    /// A later refresh for the same account may arrive with this generation.
    pub generation: u32,
}

/// Why the shared supervisor was asked to read an account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshReason {
    /// The user asked for a refresh.
    UserRequested,
    /// The person asked to reconnect the account.
    Reconnect,
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
    pending: Arc<tokio::sync::Mutex<HashSet<(AccountId, u32)>>>,
    /// Serialises every change that reads the registry, mutates it, writes it
    /// durably and publishes it. Without it a delayed write can resurrect an
    /// account a disconnect removed, or overwrite a concurrent rename.
    commit: Arc<tokio::sync::Mutex<()>>,
}

/// A handle for user refresh and orderly shutdown.
#[derive(Clone)]
pub struct MonitoringRuntime {
    sender: mpsc::Sender<RefreshRequest>,
    shutdown: watch::Sender<bool>,
    attempts: Arc<tokio::sync::Mutex<HashMap<ConnectionAttemptId, AttemptHandle>>>,
    connection_gate: Arc<tokio::sync::Mutex<()>>,
    /// Verified candidates waiting for the person's decision, not yet saved.
    pending: PendingConnections,
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
            commit: Arc::new(tokio::sync::Mutex::new(())),
        };
        let runtime = Self {
            sender,
            shutdown,
            attempts: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            connection_gate: Arc::new(tokio::sync::Mutex::new(())),
            pending: PendingConnections::default(),
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
            let generation = self
                .state
                .registry
                .read()
                .await
                .get(&account_id)
                .map(|entry| entry.binding.generation)
                .ok_or(CommandError::AccountNotFound)?;
            if queue::enqueue(
                &self.sender,
                &self.state.pending,
                RefreshRequest {
                    account_id: account_id.clone(),
                    reason,
                    generation,
                },
            )
            .await?
            {
                accepted.push(account_id);
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
        credentials::supplied(&adapter, &request)?;
        let attempt_id = ConnectionAttemptId::generate();
        let (cancel, cancel_receiver) = watch::channel(false);
        let reporter = Arc::new(AttemptReporter::new());
        self.attempts.lock().await.insert(
            attempt_id.clone(),
            AttemptHandle {
                cancel,
                reporter: Arc::clone(&reporter),
            },
        );
        let runtime = self.clone();
        let owned_attempt = attempt_id.clone();
        tauri::async_runtime::spawn(async move {
            // The body runs as its own task so a panic comes back as a join
            // error instead of unwinding past the cleanup below and leaving the
            // attempt stuck as Started forever.
            let body = tokio::spawn({
                let runtime = runtime.clone();
                let owned_attempt = owned_attempt.clone();
                let inner_reporter = Arc::clone(&reporter);
                async move {
                    connection::run_connection_attempt(
                        runtime,
                        adapter,
                        request,
                        owned_attempt,
                        cancel_receiver,
                        inner_reporter,
                    )
                    .await
                }
            });
            let failure = match body.await {
                Err(joined) if joined.is_panic() => Some(CommandError::Internal {
                    code: "connection_attempt_stopped_unexpectedly".into(),
                }),
                // Successful verification already reported its pending candidate,
                // cancellation reported `Cancelled`, and an aborted task has
                // nobody left to report to.
                Ok(Ok(()) | Err(CommandError::Cancelled)) | Err(_) => None,
                Ok(Err(error)) => Some(error),
            };
            if let Some(error) = failure {
                reporter
                    .emit(
                        &runtime.state,
                        &owned_attempt,
                        ConnectionProgress::Failed { error },
                    )
                    .await;
            }
            runtime.attempts.lock().await.remove(&owned_attempt);
        });
        Ok(ConnectionAttemptAccepted {
            attempt_ref: AttemptRef::new(attempt_id.clone()),
            attempt_id,
        })
    }

    /// Cancels one active attempt, or discards one verified candidate.
    ///
    /// A candidate the person has not confirmed is dropped with nothing stored,
    /// so declining a verified connection leaves no account behind.
    pub async fn cancel_connection(
        &self,
        attempt_id: &ConnectionAttemptId,
    ) -> Result<(), quota_contracts::CommandError> {
        // Taken before the flag is set and before Cancelled is reported, so an
        // attempt that is already committing finishes first and is reported as
        // verified rather than cancelled, one that has not reached the gate
        // sees the flag and stops, and no candidate can be held between this
        // removal and the removal below.
        let _connection_gate = self.connection_gate.lock().await;
        let handle = self.attempts.lock().await.remove(attempt_id);
        let candidate = self.pending.take(attempt_id).await;
        let reporter = match handle {
            Some(handle) => {
                handle.cancel.send_replace(true);
                handle.reporter
            }
            None => match candidate {
                Some(candidate) => candidate.reporter,
                None => return Err(CommandError::Cancelled),
            },
        };
        reporter
            .emit(&self.state, attempt_id, ConnectionProgress::Cancelled)
            .await;
        Ok(())
    }

    /// Saves the verified candidate one attempt is holding, then starts it.
    ///
    /// This is the only step that persists a new account, so a candidate that
    /// is never confirmed leaves storage untouched.
    pub async fn confirm_connection(
        &self,
        attempt_id: &ConnectionAttemptId,
        nickname: String,
    ) -> Result<(), quota_contracts::CommandError> {
        // Checked before the candidate is taken, so a rejected name leaves the
        // candidate waiting for a corrected one.
        let nickname = nickname.trim().to_owned();
        if nickname.is_empty() || nickname.chars().count() > quota_domain::account::MAX_NICKNAME_LEN
        {
            return Err(quota_contracts::CommandError::ValidationFailed {
                field: "nickname".into(),
                reason: "the nickname is blank or too long".into(),
            });
        }
        confirm::commit_pending(self, attempt_id, nickname).await
    }

    /// Deletes the browser profile a website sign-in keeps for `provider`,
    /// once its account is disconnected. Other providers keep none.
    pub fn forget_website_sign_in(&self, provider: quota_domain::provider::ProviderId) {
        let website = self
            .state
            .providers
            .provider(provider)
            .is_some_and(|adapter| adapter.console_sign_in().is_some());
        if website {
            console::forget_profile(&self.state.app, provider);
        }
    }

    /// Where the credentials Quota owns itself are kept.
    #[must_use]
    pub fn secrets(&self) -> &Arc<dyn quota_core::ports::SecretStore> {
        self.state.providers.secrets()
    }

    /// Takes the one boundary every durable account change shares.
    ///
    /// A command that changes an account uses this so its read of the registry,
    /// its write, and its publication cannot interleave with a supervised read
    /// doing the same.
    pub async fn commit(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.state.commit.lock().await
    }

    /// Publishes the projection the renderer shows, after a committed change.
    ///
    /// A mutation that stops scheduled work, such as pausing or disconnecting the
    /// last account, produces no snapshot of its own, so without this the view
    /// keeps showing the state before it.
    ///
    /// # Errors
    /// Returns a typed persistence error when the publication fails.
    pub async fn publish(&self) -> Result<(), quota_contracts::CommandError> {
        worker::publish_snapshot(&self.state)
            .await
            .map_err(|code| quota_contracts::CommandError::Internal { code })
    }

    /// Reconnects one account. A session Quota signs in to on the provider's
    /// website is signed in to again first, in the background; any other
    /// account is verified again at once.
    pub async fn reconnect_account(
        &self,
        account_id: &AccountId,
    ) -> Result<u32, quota_contracts::CommandError> {
        let website = {
            let registry = self.state.registry.read().await;
            let entry = registry
                .get(account_id)
                .ok_or(quota_contracts::CommandError::AccountNotFound)?;
            let connection = &entry.stored.connection;
            self.state
                .providers
                .provider(connection.provider_id)
                .filter(|_| connection.credential_ownership == CredentialOwnership::AppOwned)
                .and_then(|adapter| Some((adapter.clone(), adapter.console_sign_in()?)))
                .map(|found| (found, connection.id.clone(), entry.binding.generation))
        };
        if let Some(((adapter, console), connection_id, generation)) = website {
            console::reconnect(
                self.clone(),
                adapter,
                console,
                account_id.clone(),
                connection_id,
            );
            return Ok(generation);
        }
        self.verify_again(account_id).await
    }

    /// Bumps one connection generation, then queues a verified refresh.
    pub(crate) async fn verify_again(
        &self,
        account_id: &AccountId,
    ) -> Result<u32, quota_contracts::CommandError> {
        let commit = self.state.commit.lock().await;
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
        let mut registry = self.state.registry.write().await;
        registry
            .set_generation(&connection_id, generation)
            .map_err(|_| quota_contracts::CommandError::AccountNotFound)?;
        registry
            .set_connection_state(&connection_id, ConnectionState::Connecting)
            .map_err(|_| quota_contracts::CommandError::AccountNotFound)?;
        let stored = registry
            .get(account_id)
            .map(|entry| entry.stored.clone())
            .ok_or(quota_contracts::CommandError::AccountNotFound)?;
        self.state
            .accounts
            .upsert_account(stored)
            .await
            .map_err(
                |error| quota_contracts::CommandError::PersistenceUnavailable {
                    owner: error.owner.to_owned(),
                },
            )?;
        let queued = queue::enqueue(
            &self.sender,
            &self.state.pending,
            RefreshRequest {
                account_id: account_id.clone(),
                generation,
                reason: RefreshReason::Reconnect,
            },
        )
        .await;
        drop(registry);
        drop(commit);
        queued?;
        // The reconnect itself changed the connection state the renderer shows.
        self.publish().await?;
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
