//! Shared fixtures for the persistence integration tests.
//!
//! Every test file compiles this module, so some items are unused in some
//! targets. That is the point of a shared fixture module; the alternative is
//! five copies of the same builders.

#![allow(
    dead_code,
    reason = "a shared fixture module is compiled into every test target, and no target uses all of it"
)]

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, TimeZone, Utc};
use quota_domain::account::{AccountCardinality, CredentialOwnership};
use quota_domain::ids::{
    AccountId, ConnectionId, DefinitionVersion, QuotaPoolId, QuotaWindowId, ResourceId,
};
use quota_domain::polling::LimitScope;
use quota_domain::preferences::PresentationPreferences;
use quota_domain::provider::ProviderId;
use quota_domain::quota::measurement::{Measurement, PercentageMeasurement};
use quota_domain::quota::scope::QuotaScope;
use quota_domain::quota::units::DecimalPrecision;
use quota_domain::quota::window::{
    Boundary, BoundaryKind, Completeness, Enforcement, MetricRole, QuotaCategory, QuotaWindow,
    SourceKind, WindowSemantics,
};
use quota_persistence::sqlite::{NewAccount, NewConnection, SqlitePoolSettings};
use quota_persistence::sqlite::{open_pool, run_migrations, verify_pool_settings};
use serde_json::Value;
use sqlx::SqlitePool;

/// The key the preference codec uses for the live document.
pub(crate) const LIVE_KEY: &str = "quota.preferences.presentation.v1";
/// The key the preference codec uses to preserve a document.
pub(crate) const RECOVERY_KEY: &str = "quota.preferences.presentation.recovery.v1";

/// A temporary directory that removes itself when the test ends.
pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a fresh directory under the system temporary root.
    pub(crate) fn new(label: &str) -> Self {
        let unique = format!(
            "quota-persistence-{label}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    /// The directory holding the database file.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The database file inside this directory.
    pub(crate) fn database(&self) -> PathBuf {
        self.path.join("quota.sqlite")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _removed = std::fs::remove_dir_all(&self.path);
    }
}

/// Opens and migrates a database in a fresh temporary directory.
pub(crate) async fn migrated(directory: &TempDir) -> SqlitePool {
    let settings = SqlitePoolSettings::default();
    let pool = open_pool(&directory.database(), settings).await.unwrap();
    run_migrations(&pool).await.unwrap();
    verify_pool_settings(&pool, &settings).await.unwrap();
    pool
}

/// Builds a connection fixture.
pub(crate) fn connection(id: &str) -> NewConnection {
    NewConnection {
        id: ConnectionId::new(id).unwrap(),
        provider_id: ProviderId::Codex,
        credential_ownership: CredentialOwnership::AppOwned,
        profile_label: None,
        cardinality: AccountCardinality::Independent,
    }
}

/// Builds an account fixture.
pub(crate) fn account(id: &str, connection_id: &str, ordinal: u32, nickname: &str) -> NewAccount {
    NewAccount {
        id: AccountId::new(id).unwrap(),
        connection_id: ConnectionId::new(connection_id).unwrap(),
        provider_id: ProviderId::Codex,
        nickname: nickname.to_owned(),
        connection_ordinal: ordinal,
    }
}

/// Builds a window fixture with a remaining percentage.
pub(crate) fn window(
    id: &str,
    pool_id: &str,
    remaining: f64,
    observed_at: DateTime<Utc>,
) -> QuotaWindow {
    QuotaWindow {
        id: QuotaWindowId::new(id).unwrap(),
        provider_bucket_id: Some("bucket-1".to_owned()),
        pool_id: QuotaPoolId::new(pool_id).unwrap(),
        scope: QuotaScope::new(ResourceId::new("model_x").unwrap(), "Model X").unwrap(),
        category: QuotaCategory::Weekly,
        semantics: WindowSemantics::AnchoredPeriod,
        duration: Some(Duration::days(7)),
        metric_role: MetricRole::IncludedAllowance,
        enforcement: Enforcement::Hard,
        measurement: Measurement::Percentage(
            PercentageMeasurement::from_used_percent(
                100.0 - remaining,
                DecimalPrecision::new(1).unwrap(),
            )
            .unwrap(),
        ),
        period_started_at: Some(observed_at - Duration::days(2)),
        boundary: Some(Boundary {
            at: observed_at + Duration::days(5),
            kind: BoundaryKind::FullReset,
        }),
        observed_at: Some(observed_at),
        received_at: observed_at,
        valid_until: Some(observed_at + Duration::hours(1)),
        source: SourceKind::DocumentedApi,
        completeness: Completeness::Complete,
        definition_version: DefinitionVersion::INITIAL,
        issues: Vec::new(),
    }
}

/// A fixed instant, so a failure names a readable time.
pub(crate) fn at(hours: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap() + Duration::hours(hours)
}

/// A provider scope, for backoff fixtures.
pub(crate) fn provider_scope() -> LimitScope {
    LimitScope::Provider(ProviderId::Claude)
}

/// An in-memory stand-in for a document store.
#[derive(Default, Clone)]
pub(crate) struct MemoryStore(std::rc::Rc<std::cell::RefCell<MemoryState>>);

/// The mutable state behind a [`MemoryStore`].
#[derive(Default)]
pub(crate) struct MemoryState {
    entries: std::collections::BTreeMap<String, Value>,
    writes_allowed: Option<usize>,
}

impl MemoryStore {
    /// Wraps this store in the codec under test.
    pub(crate) fn codec(&self) -> quota_persistence::PresentationPreferencesCodec<Self> {
        quota_persistence::PresentationPreferencesCodec::new(self.clone())
    }

    /// Writes a value without going through the codec, as a foreign tool would.
    pub(crate) fn write_external(&self, key: &str, value: Value) {
        self.0.borrow_mut().entries.insert(key.to_owned(), value);
    }

    /// Reads a value without going through the codec.
    pub(crate) fn read_external(&self, key: &str) -> Option<Value> {
        self.0.borrow().entries.get(key).cloned()
    }

    /// Refuses every write after `allowed` writes have gone through.
    pub(crate) fn fail_writes_after(&self, allowed: usize) {
        self.0.borrow_mut().writes_allowed = Some(allowed);
    }

    /// The keys this store currently holds.
    pub(crate) fn keys(&self) -> Vec<String> {
        self.0.borrow().entries.keys().cloned().collect()
    }
}

impl quota_persistence::store::PreferenceDocumentStore for MemoryStore {
    fn read(&self, key: &str) -> Result<Option<Value>, quota_persistence::PersistenceError> {
        Ok(self.read_external(key))
    }

    fn write(&self, key: &str, value: &Value) -> Result<(), quota_persistence::PersistenceError> {
        let mut state = self.0.borrow_mut();
        if let Some(allowed) = state.writes_allowed {
            if allowed == 0 {
                return Err(quota_persistence::PersistenceError::StoreUnavailable);
            }
            state.writes_allowed = Some(allowed - 1);
        }
        state.entries.insert(key.to_owned(), value.clone());
        Ok(())
    }
}

/// A store that refuses every operation.
pub(crate) struct FailingStore;

impl quota_persistence::store::PreferenceDocumentStore for FailingStore {
    fn read(&self, _key: &str) -> Result<Option<Value>, quota_persistence::PersistenceError> {
        Err(quota_persistence::PersistenceError::StoreUnavailable)
    }

    fn write(&self, _key: &str, _value: &Value) -> Result<(), quota_persistence::PersistenceError> {
        Err(quota_persistence::PersistenceError::StoreUnavailable)
    }
}

/// The default preference document, serialized as it would be stored.
pub(crate) fn stored_defaults() -> Value {
    serde_json::to_value(PresentationPreferences::default()).unwrap()
}
