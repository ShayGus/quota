//! What the core needs from durable state.

use async_trait::async_trait;

use chrono::{DateTime, Duration, Utc};

use quota_domain::account::{ConnectionState, ConnectionSummary, FetchState, VerifiedIdentity};
use quota_domain::ids::{AccountId, ConnectionId};
use quota_domain::polling::LimitScope;
use quota_domain::preferences::{OperationalPreferences, PresentationPreferences};
use quota_domain::quota::window::QuotaWindow;
use quota_domain::snapshot::AccountSnapshot;

/// An account and its binding, as the registry holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredAccount {
    /// The immutable local identity used by every row action.
    pub account_id: AccountId,
    /// The connection summary, without any secret material.
    pub connection: ConnectionSummary,
    /// The user-chosen display name.
    pub nickname: String,
    /// The stable tie-break order.
    pub connection_ordinal: u32,
    /// Whether the supervisor schedules reads for this account.
    pub monitoring_enabled: bool,
    /// Where the connection stands.
    pub connection_state: ConnectionState,
    /// How the last read attempt went.
    pub fetch_state: FetchState,
    /// When the last read attempt ran.
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// When a reading was last accepted.
    pub last_success_at: Option<DateTime<Utc>>,
    /// When the next read becomes eligible.
    pub next_attempt_at: Option<DateTime<Utc>>,
    /// The verified identity, when one was confirmed.
    pub identity: Option<VerifiedIdentity>,
    /// The account's windows, newest reading per window.
    pub windows: Vec<QuotaWindow>,
    /// Windows the provider expected to report but did not.
    pub expected_but_missing_window_ids: Vec<quota_domain::ids::QuotaWindowId>,
}

/// A persisted rate-limit or backoff deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackoffState {
    /// How many consecutive transient failures happened.
    pub attempts: u32,
    /// The earliest instant another read may start.
    pub next_eligible_at: DateTime<Utc>,
    /// The provider's own `Retry-After`, when it sent one.
    pub provider_retry_after: Option<DateTime<Utc>>,
}

impl BackoffState {
    /// Whether the deadline has passed at `now`.
    #[must_use]
    pub fn is_waiting_at(&self, now: DateTime<Utc>) -> bool {
        self.next_eligible_at > now
    }
}

/// An open alert episode, keyed by pool and window rather than by account row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertEpisode {
    /// The account the row shows.
    pub account_id: AccountId,
    /// The window that crossed.
    pub window_id: quota_domain::ids::QuotaWindowId,
    /// The definition version, so a changed definition starts a new episode.
    pub definition_version: quota_domain::ids::DefinitionVersion,
    /// The crossed level.
    pub level: AlertLevel,
    /// When the episode opened.
    pub opened_at: DateTime<Utc>,
}

/// How severe a crossing was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertLevel {
    /// The allowance is low.
    Low,
    /// The allowance is critical.
    Critical,
    /// The allowance is confirmed exhausted.
    Exhausted,
}

impl AlertLevel {
    /// The name used as the episode key.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Critical => "critical",
            Self::Exhausted => "exhausted",
        }
    }
}

/// Durable account and binding state.
#[async_trait]
pub trait AccountRepository: Send + Sync {
    /// Loads every account, with its newest windows.
    async fn load_accounts(&self) -> Result<Vec<StoredAccount>, RepositoryError>;

    /// Inserts or replaces one account and its binding.
    async fn upsert_account(&self, account: StoredAccount) -> Result<(), RepositoryError>;

    /// Sets whether one account is monitored.
    async fn set_enabled(
        &self,
        account_id: &AccountId,
        enabled: bool,
    ) -> Result<(), RepositoryError>;

    /// Increments a connection generation and returns the new value.
    async fn bump_generation(&self, connection_id: &ConnectionId) -> Result<u32, RepositoryError>;

    /// Removes one account and only its own rows.
    async fn remove_account(&self, account_id: &AccountId) -> Result<(), RepositoryError>;

    /// Writes an accepted reading and its history in one transaction.
    async fn persist_reading(
        &self,
        account_id: &AccountId,
        windows: &[QuotaWindow],
        observed_at: Option<DateTime<Utc>>,
    ) -> Result<(), RepositoryError>;

    /// Builds the transport projection for one stored account.
    async fn snapshot_of(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<AccountSnapshot>, RepositoryError>;
}

/// Durable rate-limit and backoff state, scoped to the real limit owner.
#[async_trait]
pub trait BackoffRepository: Send + Sync {
    /// Persists a scoped backoff deadline before dependent work resumes.
    async fn persist_backoff(
        &self,
        scope: &LimitScope,
        state: BackoffState,
    ) -> Result<(), RepositoryError>;

    /// Reads the scoped backoff, when one exists.
    async fn load_backoff(
        &self,
        scope: &LimitScope,
    ) -> Result<Option<BackoffState>, RepositoryError>;

    /// Clears a scoped backoff after a verified success.
    async fn clear_backoff(&self, scope: &LimitScope) -> Result<(), RepositoryError>;
}

/// Local reading history, with explicit retention.
#[async_trait]
pub trait HistoryRepository: Send + Sync {
    /// Deletes one account's history without touching its siblings.
    async fn clear_history(&self, account_id: &AccountId) -> Result<(), RepositoryError>;
}

/// Non-transactional presentation preferences.
#[async_trait]
pub trait PreferenceRepository: Send + Sync {
    /// Loads the confirmed preferences.
    async fn load(&self) -> Result<PresentationPreferences, RepositoryError>;

    /// Saves committed preferences and returns the confirmed revision.
    async fn save(
        &self,
        preferences: &PresentationPreferences,
    ) -> Result<PresentationPreferences, RepositoryError>;
}

/// Monitoring, notification, and retained-data preferences owned by `SQLite`.
#[async_trait]
pub trait OperationalPreferencesRepository: Send + Sync {
    /// Loads the confirmed operational settings.
    async fn load(&self) -> Result<OperationalPreferences, RepositoryError>;

    /// Saves all operational settings in one durable update.
    async fn save(
        &self,
        preferences: &OperationalPreferences,
    ) -> Result<OperationalPreferences, RepositoryError>;
}

/// Durable application-wide monitoring state.
#[async_trait]
pub trait MonitoringRepository: Send + Sync {
    /// Loads the durable state, using `Running` only when no row exists.
    async fn load_monitoring_state(
        &self,
    ) -> Result<quota_domain::snapshot::MonitoringState, RepositoryError>;

    /// Saves the user's confirmed pause or resume choice.
    async fn save_monitoring_state(
        &self,
        state: &quota_domain::snapshot::MonitoringState,
    ) -> Result<(), RepositoryError>;
}

/// A durable owner could not be reached.
#[derive(Debug, thiserror::Error)]
#[error("durable owner `{owner}` failed: {reason}")]
pub struct RepositoryError {
    /// Which owner failed, such as `sqlite` or `store`.
    pub owner: &'static str,
    /// A sanitized, non-identifying reason.
    pub reason: String,
}

impl RepositoryError {
    /// Builds an error for one owner.
    #[must_use]
    pub fn new(owner: &'static str, reason: impl Into<String>) -> Self {
        Self {
            owner,
            reason: reason.into(),
        }
    }
}

/// The number of retries a durable owner should tolerate before degrading.
#[must_use]
pub fn default_history_retention() -> Duration {
    Duration::days(30)
}
