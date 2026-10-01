//! Scratch debug harness.
use chrono::{Duration, TimeZone, Utc};
use quota_domain::polling::LimitScope;
use quota_domain::provider::ProviderId;
use quota_persistence::sqlite::{BackoffRecord, SqliteRepositories};

fn at(hours: i64) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap() + Duration::hours(hours)
}

#[tokio::test]
async fn dbg_backoff_read() {
    let dir = std::env::temp_dir().join(format!("dbg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = quota_persistence::SqlitePoolSettings::default();
    let pool = quota_persistence::sqlite::open_pool(&dir.join("q.sqlite"), settings)
        .await
        .unwrap();
    quota_persistence::sqlite::run_migrations(&pool).await.unwrap();
    let repos = SqliteRepositories::new(pool.clone());
    let scope = LimitScope::Provider(ProviderId::Claude);
    repos
        .backoff()
        .persist(&BackoffRecord {
            scope: scope.clone(),
            attempts: 1,
            next_eligible_at: at(2),
            provider_retry_after: Some(at(3)),
        })
        .await
        .unwrap();
    let rows: Result<Vec<(String, String, i64, String, Option<String>)>, sqlx::Error> =
        sqlx::query_as("SELECT scope_kind, scope_id, attempts, next_eligible_at, provider_retry_after FROM refresh_backoff")
            .fetch_all(&pool)
            .await;
    println!("ROWS: {rows:?}");
    println!("READ: {:?}", repos.backoff().read(&scope).await);
}
