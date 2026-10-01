//! Integration tests against a real migrated SQLite file.
//!
//! Fixtures use `unwrap` for readability; this file is not `#[cfg(test)]`, so
//! the workspace's test-mode allowance does not reach it.

#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert the setup they build, so a broken fixture must fail loudly"
)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, TimeZone, Utc};
use quota_domain::account::{AccountCardinality, ConnectionState, CredentialOwnership, FetchState};
use quota_domain::account::VerifiedIdentity;
use quota_domain::ids::{
    AccountId, ConnectionId, DefinitionVersion, QuotaPoolId, QuotaWindowId, ResourceId,
};
use quota_domain::polling::LimitScope;
use quota_domain::preferences::{PREFERENCES_SCHEMA_VERSION, PresentationPreferences, Theme};
use quota_domain::provider::ProviderId;
use quota_domain::quota::measurement::{Measurement, PercentageMeasurement, UnavailableReason};
use quota_domain::quota::scope::QuotaScope;
use quota_domain::quota::units::DecimalPrecision;
use quota_domain::quota::window::{
    Boundary, BoundaryKind, Completeness, Enforcement, MetricRole, QuotaCategory, QuotaWindow,
    SourceKind, WindowSemantics,
};
use quota_persistence::store::PreferenceDocumentStore;
use quota_persistence::sqlite::{
    AlertLevel, BackoffRecord, EpisodeKey, NewAccount, NewConnection,
    SqlitePoolSettings, SqliteRepositories, open_pool, run_migrations, verify_pool_settings,
};
use quota_persistence::{PersistenceError, PresentationPreferencesCodec};
use serde_json::Value;
use sqlx::SqlitePool;

/// The key the preference codec uses for the live document.
const LIVE_KEY: &str = "quota.preferences.presentation.v1";
/// The key the preference codec uses to preserve a document.
const RECOVERY_KEY: &str = "quota.preferences.presentation.recovery.v1";

/// A temporary directory that removes itself when the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a fresh directory under the system temporary root.
    fn new(label: &str) -> Self {
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
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Opens and migrates a database in a fresh temporary directory.
async fn migrated(directory: &TempDir) -> SqlitePool {
    let settings = SqlitePoolSettings::default();
    let pool = open_pool(&directory.path().join("quota.sqlite"), settings)
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    verify_pool_settings(&pool, &settings).await.unwrap();
    pool
}

/// Builds a connection fixture.
fn connection(id: &str) -> NewConnection {
    NewConnection {
        id: ConnectionId::new(id).unwrap(),
        provider_id: ProviderId::Codex,
        credential_ownership: CredentialOwnership::AppOwned,
        profile_label: None,
        cardinality: AccountCardinality::Independent,
    }
}

/// Builds an account fixture.
fn account(id: &str, connection_id: &str, ordinal: u32, nickname: &str) -> NewAccount {
    NewAccount {
        id: AccountId::new(id).unwrap(),
        connection_id: ConnectionId::new(connection_id).unwrap(),
        provider_id: ProviderId::Codex,
        nickname: nickname.to_owned(),
        connection_ordinal: ordinal,
    }
}

/// Builds a window fixture with a remaining percentage.
fn window(id: &str, pool_id: &str, remaining: f64, observed_at: DateTime<Utc>) -> QuotaWindow {
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
fn at(hours: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap() + Duration::hours(hours)
}

#[tokio::test]
async fn migrations_are_idempotent_on_reopen() {
    let directory = TempDir::new("migrate");
    let path = directory.path().join("quota.sqlite");

    let first = open_pool(&path, SqlitePoolSettings::default()).await.unwrap();
    run_migrations(&first).await.unwrap();
    let versions: Vec<u32> =
        sqlx::query_scalar("SELECT version FROM schema_migrations ORDER BY version")
            .fetch_all(&first)
            .await
            .unwrap();
    first.close().await;

    let second = open_pool(&path, SqlitePoolSettings::default()).await.unwrap();
    run_migrations(&second).await.unwrap();
    let again: Vec<u32> =
        sqlx::query_scalar("SELECT version FROM schema_migrations ORDER BY version")
            .fetch_all(&second)
            .await
            .unwrap();

    assert_eq!(versions, vec![1]);
    assert_eq!(versions, again, "a reopen must not re-record a version");
    second.close().await;
}

#[tokio::test]
async fn every_declared_table_exists_after_migration() {
    let directory = TempDir::new("tables");
    let pool = migrated(&directory).await;

    let names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .fetch_all(&pool)
            .await
            .unwrap();

    for required in [
        "schema_migrations",
        "connections",
        "accounts",
        "quota_pools",
        "account_pool_bindings",
        "quota_windows",
        "latest_measurements",
        "measurement_history",
        "alert_episodes",
        "notification_outbox",
        "refresh_backoff",
        "monitoring_preferences",
    ] {
        assert!(
            names.iter().any(|name| name == required),
            "{required} is missing"
        );
    }

    pool.close().await;
}

#[tokio::test]
async fn no_table_or_column_names_a_credential() {
    let directory = TempDir::new("no-secrets");
    let pool = migrated(&directory).await;

    let schema: String = sqlx::query_scalar(
        "SELECT group_concat(COALESCE(sql, name), ' ') FROM sqlite_master",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let lowered = schema.to_lowercase();
    for forbidden in ["token", "cookie", "authorization", "secret", "password"] {
        assert!(
            !lowered.contains(forbidden),
            "`{forbidden}` must not appear in the schema"
        );
    }

    pool.close().await;
}

#[tokio::test]
async fn foreign_keys_are_enforced_and_reject_a_bad_insert() {
    let directory = TempDir::new("fk");
    let pool = migrated(&directory).await;

    let rejected = sqlx::query(
        "INSERT INTO account_pool_bindings (account_id, pool_id)
         VALUES ('missing-account', 'missing-pool')",
    )
    .execute(&pool)
    .await;
    assert!(rejected.is_err(), "a dangling binding must be refused");

    let missing_reference = sqlx::query(
        "INSERT INTO quota_windows (
             id, pool_id, scope_resource, scope_label, category, semantics,
             metric_role, enforcement, source_kind, completeness, definition_version
         ) VALUES ('win-x', 'absent-pool', 'r', 'R', 'weekly', 'unknown',
                   'included_allowance', 'unknown', 'documented_api', 'complete', 1)",
    )
    .execute(&pool)
    .await;
    assert!(
        missing_reference.is_err(),
        "a window must reference a real pool"
    );

    let orphan_history = sqlx::query(
        "INSERT INTO measurement_history (account_id, window_id, observed_at, received_at)
         VALUES ('absent', 'absent', '2026-03-01T00:00:00.000Z', '2026-03-01T00:00:00.000Z')",
    )
    .execute(&pool)
    .await;
    assert!(
        orphan_history.is_ok(),
        "prunable history carries no foreign key, so it can never cascade"
    );

    pool.close().await;
}

#[tokio::test]
async fn deleting_a_connection_cascades_to_its_accounts() {
    let directory = TempDir::new("cascade");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());

    repositories
        .accounts()
        .upsert_connection(&connection("conn-1"))
        .await
        .unwrap();
    repositories
        .accounts()
        .upsert_account(&account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();

    sqlx::query("DELETE FROM connections WHERE id = 'conn-1'")
        .execute(&pool)
        .await
        .unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        remaining, 0,
        "ON DELETE CASCADE must remove the account rows"
    );
    pool.close().await;
}

#[tokio::test]
async fn pool_settings_are_verified_on_every_connection() {
    let directory = TempDir::new("settings");
    let settings = SqlitePoolSettings::default();
    let pool = open_pool(&directory.path().join("quota.sqlite"), settings)
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    verify_pool_settings(&pool, &settings).await.unwrap();

    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .unwrap();
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&pool)
        .await
        .unwrap();
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .unwrap();
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(journal_mode, SqlitePoolSettings::required_journal_mode());
    assert_eq!(synchronous, SqlitePoolSettings::required_synchronous());
    assert_eq!(foreign_keys, 1);
    assert_eq!(
        busy_timeout,
        i64::try_from(settings.busy_timeout().as_millis()).unwrap()
    );

    // A pool whose live setting disagrees with the expected settings is refused.
    let disagreeing = SqlitePoolSettings::new(4, StdDuration::from_secs(9)).unwrap();
    assert_eq!(
        verify_pool_settings(&pool, &disagreeing).await.unwrap_err(),
        PersistenceError::PoolUnavailable
    );

    pool.close().await;
}

#[tokio::test]
async fn pool_settings_reject_values_outside_their_bounds() {
    assert_eq!(
        SqlitePoolSettings::new(0, SqlitePoolSettings::DEFAULT_BUSY_TIMEOUT).unwrap_err(),
        PersistenceError::PoolUnavailable
    );
    assert_eq!(
        SqlitePoolSettings::new(
            SqlitePoolSettings::MAX_MAX_CONNECTIONS + 1,
            SqlitePoolSettings::DEFAULT_BUSY_TIMEOUT
        )
        .unwrap_err(),
        PersistenceError::PoolUnavailable
    );
    assert_eq!(
        SqlitePoolSettings::new(
            SqlitePoolSettings::DEFAULT_MAX_CONNECTIONS,
            StdDuration::from_millis(1)
        )
        .unwrap_err(),
        PersistenceError::PoolUnavailable
    );
    assert!(SqlitePoolSettings::new(2, StdDuration::from_secs(2)).is_ok());
}

#[tokio::test]
async fn deleting_one_account_leaves_its_same_provider_sibling_intact() {
    let directory = TempDir::new("isolation");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let accounts = repositories.accounts();

    accounts.upsert_connection(&connection("conn-1")).await.unwrap();
    accounts
        .upsert_account(&account("acct-a", "conn-1", 0, "Alpha"))
        .await
        .unwrap();
    accounts
        .upsert_account(&account("acct-b", "conn-1", 1, "Beta"))
        .await
        .unwrap();

    let repo = repositories.measurements();
    let window_id = QuotaWindowId::new("win-1").unwrap();
    repo.persist_reading(
        &AccountId::new("acct-a").unwrap(),
        &window("win-1", "pool-1", 40.0, at(0)),
    )
    .await
    .unwrap();
    repo.persist_reading(
        &AccountId::new("acct-b").unwrap(),
        &window("win-1", "pool-1", 70.0, at(0)),
    )
    .await
    .unwrap();

    assert!(
        accounts
            .delete_account(&AccountId::new("acct-a").unwrap())
            .await
            .unwrap()
    );
    assert!(
        !accounts
            .delete_account(&AccountId::new("acct-a").unwrap())
            .await
            .unwrap()
    );

    let survivors = accounts
        .list_for_connection(&ConnectionId::new("conn-1").unwrap())
        .await
        .unwrap();
    assert_eq!(survivors.len(), 1);
    assert_eq!(survivors[0].id.as_str(), "acct-b", "the sibling must survive");

    let readings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM latest_measurements")
        .fetch_one(&pool)
        .await
        .unwrap();
    let history: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM measurement_history")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(readings, 1, "only the deleted account's reading may go");
    assert_eq!(history, 1, "only the deleted account's history may go");

    let surviving = repo
        .latest(&AccountId::new("acct-b").unwrap(), &window_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        surviving.measurement.remaining_percent().unwrap().value(),
        70.0
    );

    pool.close().await;
}

#[tokio::test]
async fn clearing_one_account_history_leaves_the_sibling_history() {
    let directory = TempDir::new("history-isolation");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());

    repositories
        .accounts()
        .upsert_connection(&connection("conn-1"))
        .await
        .unwrap();
    for (id, ordinal) in [("acct-a", 0_u32), ("acct-b", 1_u32)] {
        repositories
            .accounts()
            .upsert_account(&account(id, "conn-1", ordinal, id))
            .await
            .unwrap();
    }

    let repo = repositories.measurements();
    repo.persist_reading(
        &AccountId::new("acct-a").unwrap(),
        &window("win-1", "pool-1", 10.0, at(0)),
    )
    .await
    .unwrap();
    repo.persist_reading(
        &AccountId::new("acct-b").unwrap(),
        &window("win-1", "pool-1", 20.0, at(0)),
    )
    .await
    .unwrap();

    assert_eq!(
        repo.clear_history_for_account(&AccountId::new("acct-a").unwrap())
            .await
            .unwrap(),
        1
    );
    assert!(
        repo.history_for_account(&AccountId::new("acct-a").unwrap())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repo.history_for_account(&AccountId::new("acct-b").unwrap())
            .await
            .unwrap()
            .len(),
        1,
        "the sibling's history must survive"
    );

    pool.close().await;
}

#[tokio::test]
async fn a_reading_and_its_history_are_written_in_one_transaction() {
    let directory = TempDir::new("reading");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());

    repositories
        .accounts()
        .upsert_connection(&connection("conn-1"))
        .await
        .unwrap();
    repositories
        .accounts()
        .upsert_account(&account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();

    let repo = repositories.measurements();
    let account_id = AccountId::new("acct-1").unwrap();
    let window_id = QuotaWindowId::new("win-1").unwrap();

    assert!(
        repo.persist_reading(&account_id, &window("win-1", "pool-1", 55.0, at(0)))
            .await
            .unwrap()
    );

    let stored = repo.latest(&account_id, &window_id).await.unwrap().unwrap();
    assert_eq!(stored.measurement.remaining_percent().unwrap().value(), 55.0);
    assert_eq!(stored.definition_version, DefinitionVersion::INITIAL.0);
    assert_eq!(stored.observed_at, Some(at(0)));

    let history = repo.history_for_account(&account_id).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].remaining_percent, Some(55.0));

    // An account that does not exist makes the reading write fail. The window
    // row is written first in the same transaction, so the failure must roll it
    // back as well; that is what makes this one transaction and not two.
    let refused = repo
        .persist_reading(
            &AccountId::new("acct-absent").unwrap(),
            &window("win-rolled-back", "pool-1", 30.0, at(1)),
        )
        .await;
    assert!(refused.is_err(), "the reading must be refused");
    let windows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quota_windows")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(windows, 1, "the rolled-back window row must not survive");
    assert_eq!(
        repo.history_for_account(&account_id).await.unwrap().len(),
        1,
        "the failed write must not append history"
    );

    pool.close().await;
}

#[tokio::test]
async fn an_unavailable_measurement_stores_no_invented_percentage() {
    let directory = TempDir::new("unavailable");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());

    repositories
        .accounts()
        .upsert_connection(&connection("conn-1"))
        .await
        .unwrap();
    repositories
        .accounts()
        .upsert_account(&account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();

    let mut value = window("win-1", "pool-1", 50.0, at(0));
    value.measurement = Measurement::Unavailable(UnavailableReason::NotReported);

    let repo = repositories.measurements();
    repo.persist_reading(&AccountId::new("acct-1").unwrap(), &value)
        .await
        .unwrap();

    let stored = repo
        .latest(
            &AccountId::new("acct-1").unwrap(),
            &QuotaWindowId::new("win-1").unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.measurement.has_number());

    let recorded: Option<f64> = sqlx::query_scalar("SELECT remaining_percent FROM measurement_history")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        recorded, None,
        "no percentage may be invented for this reading"
    );

    pool.close().await;
}

#[tokio::test]
async fn an_unchanged_observation_is_coalesced() {
    let directory = TempDir::new("coalesce");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());

    repositories
        .accounts()
        .upsert_connection(&connection("conn-1"))
        .await
        .unwrap();
    repositories
        .accounts()
        .upsert_account(&account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();

    let repo = repositories.measurements();
    let account_id = AccountId::new("acct-1").unwrap();

    let first = window("win-1", "pool-1", 42.0, at(0));
    assert!(repo.persist_reading(&account_id, &first).await.unwrap());
    assert!(repo.coalesce_unchanged(&account_id, &first).await.unwrap());

    // The same value at the same observation time appends nothing.
    assert!(!repo.persist_reading(&account_id, &first).await.unwrap());
    assert_eq!(repo.history_for_account(&account_id).await.unwrap().len(), 1);

    // A value observed later appends exactly one row.
    let moved = window("win-1", "pool-1", 42.0, at(1));
    assert!(!repo.coalesce_unchanged(&account_id, &moved).await.unwrap());
    assert!(repo.persist_reading(&account_id, &moved).await.unwrap());
    assert_eq!(repo.history_for_account(&account_id).await.unwrap().len(), 2);

    pool.close().await;
}

#[tokio::test]
async fn backoff_persists_and_answers_is_rate_limited_now() {
    let directory = TempDir::new("backoff");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let repo = repositories.backoff();

    let scope = LimitScope::Provider(ProviderId::Claude);
    assert!(!repo.is_rate_limited_now(&scope, at(0)).await.unwrap());

    let record = BackoffRecord {
        scope: scope.clone(),
        attempts: 1,
        next_eligible_at: at(2),
        provider_retry_after: Some(at(3)),
    };
    repo.persist(&record).await.unwrap();

    assert!(repo.is_rate_limited_now(&scope, at(1)).await.unwrap());
    assert!(!repo.is_rate_limited_now(&scope, at(2)).await.unwrap());
    assert!(!repo.is_rate_limited_now(&scope, at(5)).await.unwrap());

    let read = repo.read(&scope).await.unwrap().unwrap();
    assert_eq!(read, record);

    let second = repo.record_failure(&scope, at(4), None).await.unwrap();
    assert_eq!(second.attempts, 2);
    assert_eq!(second.provider_retry_after, None);

    assert!(repo.clear(&scope).await.unwrap());
    assert!(repo.read(&scope).await.unwrap().is_none());
    assert!(!repo.clear(&scope).await.unwrap());

    // A different scope keeps its own deadline.
    let account_scope = LimitScope::Account(AccountId::new("acct-1").unwrap());
    repo.persist(&BackoffRecord {
        scope: account_scope.clone(),
        attempts: 1,
        next_eligible_at: at(6),
        provider_retry_after: None,
    })
    .await
    .unwrap();
    assert!(repo.is_rate_limited_now(&account_scope, at(1)).await.unwrap());
    assert_eq!(
        repo.read(&account_scope).await.unwrap().unwrap().attempts,
        1
    );

    pool.close().await;
}

#[tokio::test]
async fn backoff_round_trips_every_scope_kind() {
    let directory = TempDir::new("backoff-scopes");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let repo = repositories.backoff();

    let scopes = [
        LimitScope::Account(AccountId::new("acct-1").unwrap()),
        LimitScope::Connection(ConnectionId::new("conn-1").unwrap()),
        LimitScope::Provider(ProviderId::OpenCodeGo),
        LimitScope::QuotaPool(QuotaPoolId::new("pool-1").unwrap()),
        LimitScope::SourceAddress,
    ];

    for scope in &scopes {
        repo.persist(&BackoffRecord {
            scope: scope.clone(),
            attempts: 2,
            next_eligible_at: at(12),
            provider_retry_after: None,
        })
        .await
        .unwrap();
    }

    for (index, scope) in scopes.iter().enumerate() {
        let read = repo.read(scope).await.unwrap().unwrap();
        assert_eq!(&read.scope, scope, "scope {index} did not round trip");
    }

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_backoff")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows as usize, scopes.len(), "each scope owns exactly one row");

    pool.close().await;
}

#[tokio::test]
async fn an_episode_cannot_arm_twice_while_it_stays_open() {
    let directory = TempDir::new("episodes");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let repo = repositories.alerts();

    let key = EpisodeKey::new(
        AccountId::new("acct-1").unwrap(),
        QuotaWindowId::new("win-1").unwrap(),
        1,
        AlertLevel::Low,
    );

    assert!(repo.open_episode(&key, at(0)).await.unwrap());
    assert!(repo.is_open(&key).await.unwrap());
    assert!(!repo.is_armed(&key).await.unwrap());

    // A second open for the same key leaves the live episode alone.
    assert!(!repo.open_episode(&key, at(1)).await.unwrap());
    assert_eq!(repo.episode(&key).await.unwrap().opened_at, at(0));

    repo.mark_armed(&key, at(1)).await.unwrap();
    assert!(repo.is_armed(&key).await.unwrap());

    // The armed flag is what stops a second notification for this episode.
    assert!(repo.close_episode(&key, at(2)).await.unwrap());
    assert!(!repo.is_open(&key).await.unwrap());
    assert!(!repo.close_episode(&key, at(3)).await.unwrap());

    // A verified recovery plus a new crossing re-opens the same key.
    assert!(repo.open_episode(&key, at(4)).await.unwrap());
    let episode = repo.episode(&key).await.unwrap();
    assert_eq!(episode.opened_at, at(4));
    assert_eq!(episode.armed_at, None, "the re-opened episode must re-arm");
    assert_eq!(episode.closed_at, None);

    let unknown = EpisodeKey::new(
        AccountId::new("acct-absent").unwrap(),
        QuotaWindowId::new("win-absent").unwrap(),
        1,
        AlertLevel::Low,
    );
    assert!(repo.mark_armed(&unknown, at(0)).await.is_err());
    assert!(repo.episode(&unknown).await.is_err());

    pool.close().await;
}

#[tokio::test]
async fn a_duplicate_notification_cannot_be_enqueued_for_one_episode() {
    let directory = TempDir::new("outbox");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let repo = repositories.alerts();

    let key = EpisodeKey::new(
        AccountId::new("acct-1").unwrap(),
        QuotaWindowId::new("win-1").unwrap(),
        1,
        AlertLevel::Critical,
    );
    let other = EpisodeKey::new(
        AccountId::new("acct-2").unwrap(),
        QuotaWindowId::new("win-1").unwrap(),
        1,
        AlertLevel::Critical,
    );

    assert!(repo.enqueue_notification(&key, at(0)).await.unwrap());
    assert!(!repo.enqueue_notification(&key, at(1)).await.unwrap());
    assert!(
        repo.enqueue_notification(&other, at(0)).await.unwrap(),
        "a sibling account is a different episode"
    );

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification_outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 2, "one entry per episode, and never two for one");
    assert_eq!(repo.undelivered_keys().await.unwrap().len(), 2);

    pool.close().await;
}

#[tokio::test]
async fn a_new_definition_version_starts_its_own_episode() {
    let directory = TempDir::new("episode-version");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let repo = repositories.alerts();

    let account_id = AccountId::new("acct-1").unwrap();
    let window_id = QuotaWindowId::new("win-1").unwrap();
    let first = EpisodeKey::new(account_id.clone(), window_id.clone(), 1, AlertLevel::Low);
    let second = EpisodeKey::new(account_id, window_id, 2, AlertLevel::Low);

    repo.open_episode(&first, at(0)).await.unwrap();
    repo.mark_armed(&first, at(0)).await.unwrap();

    assert!(repo.open_episode(&second, at(1)).await.unwrap());
    assert!(repo.is_armed(&first).await.unwrap());
    assert!(
        !repo.is_armed(&second).await.unwrap(),
        "a new definition starts unarmed"
    );

    pool.close().await;
}

#[tokio::test]
async fn a_connection_generation_advances_and_survives_a_restart() {
    let directory = TempDir::new("generation");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let accounts = repositories.accounts();

    accounts.upsert_connection(&connection("conn-1")).await.unwrap();
    accounts
        .upsert_account(&account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();
    assert_eq!(
        accounts
            .list_for_connection(&ConnectionId::new("conn-1").unwrap())
            .await
            .unwrap()[0]
            .generation,
        0
    );

    assert_eq!(
        accounts
            .bump_generation(&ConnectionId::new("conn-1").unwrap())
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        accounts
            .list_for_connection(&ConnectionId::new("conn-1").unwrap())
            .await
            .unwrap()[0]
            .generation,
        1,
        "the account reads the connection's current generation"
    );

    let reopened = SqliteRepositories::new(pool.clone());
    assert_eq!(
        reopened
            .accounts()
            .connection(&ConnectionId::new("conn-1").unwrap())
            .await
            .unwrap()
            .generation,
        1,
        "the generation must survive a reopen"
    );

    pool.close().await;
}

#[tokio::test]
async fn an_account_records_its_verified_identity_and_fetch_state() {
    let directory = TempDir::new("identity");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let accounts = repositories.accounts();

    accounts.upsert_connection(&connection("conn-1")).await.unwrap();
    accounts
        .upsert_account(&account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();

    let identity = VerifiedIdentity {
        principal_label: "user@example.test".to_owned(),
        workspace_label: Some("Work".to_owned()),
        plan_label: Some("Pro".to_owned()),
        source: SourceKind::DocumentedApi,
    };
    accounts
        .record_verified_identity(
            &AccountId::new("acct-1").unwrap(),
            ConnectionState::Connected,
            &identity,
        )
        .await
        .unwrap();
    accounts
        .record_attempt(
            &AccountId::new("acct-1").unwrap(),
            FetchState::Idle,
            at(3),
            Some(at(4)),
        )
        .await
        .unwrap();

    let listed = accounts
        .list_for_connection(&ConnectionId::new("conn-1").unwrap())
        .await
        .unwrap();
    let stored = &listed[0];

    assert_eq!(stored.verified_identity, Some(identity));
    assert_eq!(stored.connection_state, ConnectionState::Connected);
    assert_eq!(stored.fetch_state, FetchState::Idle);
    assert_eq!(stored.last_attempt_at, Some(at(3)));
    assert_eq!(stored.last_success_at, Some(at(3)));
    assert_eq!(stored.next_attempt_at, Some(at(4)));
    assert!(stored.monitoring_enabled);

    accounts
        .set_monitoring_enabled(&AccountId::new("acct-1").unwrap(), false)
        .await
        .unwrap();
    assert!(
        !accounts
            .list_for_connection(&ConnectionId::new("conn-1").unwrap())
            .await
            .unwrap()[0]
            .monitoring_enabled
    );

    pool.close().await;
}

#[tokio::test]
async fn two_accounts_may_not_share_one_connection_ordinal() {
    let directory = TempDir::new("ordinal");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let accounts = repositories.accounts();

    accounts.upsert_connection(&connection("conn-1")).await.unwrap();
    accounts
        .upsert_account(&account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();

    let refused = accounts
        .upsert_account(&account("acct-2", "conn-1", 0, "Second"))
        .await;
    assert_eq!(
        refused.unwrap_err(),
        PersistenceError::IntegrityViolation {
            constraint: "accounts"
        },
        "the unique ordinal is the connection's identity order"
    );

    pool.close().await;
}

#[tokio::test]
async fn an_account_may_not_name_a_connection_that_does_not_exist() {
    let directory = TempDir::new("orphan-account");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());

    let refused = repositories
        .accounts()
        .upsert_account(&account("acct-1", "conn-absent", 0, "First"))
        .await;

    assert_eq!(
        refused.unwrap_err(),
        PersistenceError::RowRejected {
            table: "connections",
            reason: "the account names a connection that does not exist"
        }
    );
    pool.close().await;
}

#[tokio::test]
async fn a_missing_identity_is_reported_instead_of_silently_ignored() {
    let directory = TempDir::new("missing");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let accounts = repositories.accounts();

    assert_eq!(
        accounts
            .bump_generation(&ConnectionId::new("conn-absent").unwrap())
            .await
            .unwrap_err(),
        PersistenceError::RowRejected {
            table: "connections",
            reason: "no row matched the requested identity"
        }
    );
    assert_eq!(
        accounts
            .connection(&ConnectionId::new("conn-absent").unwrap())
            .await
            .unwrap_err(),
        PersistenceError::RowRejected {
            table: "connections",
            reason: "no connection matched the requested identity"
        }
    );

    pool.close().await;
}

#[test]
fn the_store_round_trips_a_preferences_document() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    assert_eq!(codec.validate().unwrap(), None);
    let defaults = codec.load().unwrap();
    assert_eq!(defaults, PresentationPreferences::default());
    assert!(
        !defaults.always_on_top,
        "an absent document must not opt the user in"
    );

    let mut updated = defaults.clone();
    updated.theme = Theme::Dark;
    assert_eq!(codec.save(&mut updated).unwrap(), 1);
    assert_eq!(updated.revision, 1);
    assert_eq!(codec.load().unwrap().theme, Theme::Dark);

    assert_eq!(codec.save(&mut updated).unwrap(), 2);
    assert_eq!(codec.load().unwrap().revision, 2);

    let keys = store.keys();
    assert_eq!(keys.len(), 2, "the codec owns exactly its own two keys");
    assert!(
        keys.iter()
            .all(|key| key.starts_with("quota.preferences.presentation."))
    );
}

#[test]
fn a_corrupt_document_is_preserved_and_never_opted_in() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    // A document that carries a topmost preference but is not a preferences
    // document this build can read.
    let corrupt = serde_json::json!({ "theme": "dark", "always_on_top": true });
    store.write_external(LIVE_KEY, corrupt.clone());

    assert_eq!(
        codec.load().unwrap_err(),
        PersistenceError::StoreDocumentCorrupt
    );

    // The rejected document is kept, not replaced by defaults.
    assert_eq!(store.read_external(LIVE_KEY), Some(corrupt.clone()));
    assert_eq!(
        store.read_external(RECOVERY_KEY),
        Some(corrupt),
        "the corrupt document must be preserved for recovery"
    );
}

#[test]
fn a_document_with_a_misshaped_field_never_produces_a_topmost_preference() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    store.write_external(
        LIVE_KEY,
        serde_json::json!({
            "schema_version": PREFERENCES_SCHEMA_VERSION,
            "revision": 4,
            "theme": "dark",
            "density": "compact",
            "indicator_style": "ring",
            "overview_mode": "floating",
            "always_on_top": true,
            "launch_behavior": 7,
            "privacy_alias_mode": "off",
            "reduce_motion": false
        }),
    );

    assert_eq!(
        codec.load().unwrap_err(),
        PersistenceError::StoreDocumentCorrupt
    );
}

#[test]
fn a_document_from_a_newer_build_is_refused_and_preserved() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    let mut newer = serde_json::to_value(PresentationPreferences::default()).unwrap();
    newer["schema_version"] = serde_json::json!(PREFERENCES_SCHEMA_VERSION + 1);
    store.write_external(LIVE_KEY, newer.clone());

    assert_eq!(
        codec.load().unwrap_err(),
        PersistenceError::StoreDocumentUnsupported {
            found_version: PREFERENCES_SCHEMA_VERSION + 1,
            supported_version: PREFERENCES_SCHEMA_VERSION
        }
    );
    assert_eq!(
        store.read_external(LIVE_KEY),
        Some(newer.clone()),
        "a newer document is never silently overwritten"
    );
    assert_eq!(store.read_external(RECOVERY_KEY), Some(newer));
}

#[test]
fn an_unavailable_store_is_reported_as_such() {
    let codec = PresentationPreferencesCodec::new(FailingStore);
    assert_eq!(codec.load().unwrap_err(), PersistenceError::StoreUnavailable);
}

#[test]
fn a_failed_backup_stops_the_save_and_keeps_the_previous_document() {
    let store = MemoryStore::default();
    let previous = serde_json::to_value(PresentationPreferences::default()).unwrap();
    store.write_external(LIVE_KEY, previous.clone());

    // The first save preserves the existing document, then writes the new one.
    // Refusing the very first write therefore refuses the save.
    store.fail_writes_after(0);
    let mut updated = PresentationPreferences::default();
    assert_eq!(
        store.codec().save(&mut updated).unwrap_err(),
        PersistenceError::BackupFailed
    );
    assert_eq!(
        store.read_external(LIVE_KEY),
        Some(previous),
        "the previous document must be untouched"
    );
}

/// A store that refuses every operation.
struct FailingStore;

impl PreferenceDocumentStore for FailingStore {
    fn read(&self, _key: &str) -> Result<Option<Value>, PersistenceError> {
        Err(PersistenceError::StoreUnavailable)
    }

    fn write(&self, _key: &str, _value: &Value) -> Result<(), PersistenceError> {
        Err(PersistenceError::StoreUnavailable)
    }
}

/// An in-memory stand-in for a document store.
#[derive(Default, Clone)]
struct MemoryStore(Rc<RefCell<MemoryState>>);

#[derive(Default)]
struct MemoryState {
    entries: BTreeMap<String, Value>,
    writes_allowed: Option<usize>,
}

impl MemoryStore {
    /// Wraps this store in the codec under test.
    fn codec(&self) -> PresentationPreferencesCodec<Self> {
        PresentationPreferencesCodec::new(self.clone())
    }

    /// Writes a value without going through the codec, as a foreign tool would.
    fn write_external(&self, key: &str, value: Value) {
        self.0.borrow_mut().entries.insert(key.to_owned(), value);
    }

    /// Reads a value without going through the codec.
    fn read_external(&self, key: &str) -> Option<Value> {
        self.0.borrow().entries.get(key).cloned()
    }

    /// Refuses every write after `allowed` writes have gone through.
    fn fail_writes_after(&self, allowed: usize) {
        self.0.borrow_mut().writes_allowed = Some(allowed);
    }

    /// The keys this store currently holds.
    fn keys(&self) -> Vec<String> {
        self.0.borrow().entries.keys().cloned().collect()
    }
}

impl PreferenceDocumentStore for MemoryStore {
    fn read(&self, key: &str) -> Result<Option<Value>, PersistenceError> {
        Ok(self.read_external(key))
    }

    fn write(&self, key: &str, value: &Value) -> Result<(), PersistenceError> {
        let mut state = self.0.borrow_mut();
        if let Some(allowed) = state.writes_allowed {
            if allowed == 0 {
                return Err(PersistenceError::StoreUnavailable);
            }
            state.writes_allowed = Some(allowed - 1);
        }
        state.entries.insert(key.to_owned(), value.clone());
        Ok(())
    }
}
