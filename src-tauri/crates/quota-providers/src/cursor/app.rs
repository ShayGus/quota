//! The sign-in the Cursor app keeps on this computer.
//!
//! Cursor keeps its sign-in in its state store, `User/globalStorage/state.vscdb`
//! under its application-data directory: a `SQLite` table `ItemTable` of keys and
//! values, with the access token under `cursorAuth/accessToken`. Quota opens the
//! store read-only, reads that one value, and closes it. It never writes to the
//! store and never refreshes the token, whose refresh the app owns; an expired
//! token asks the person to open Cursor, which renews it.

use std::path::PathBuf;
use std::str::FromStr;

use chrono::Utc;
use quota_core::ports::{ProviderError, Secret};
use sqlx::ConnectOptions;
use sqlx::sqlite::SqliteConnectOptions;

use crate::credentials::process_lookup;
use crate::decode::jwt;
use crate::platform_paths::{self, Lookup};

/// The profile label a Cursor connection carries.
pub(crate) const PROFILE: &str = "cursor-app";

/// The key the access token is stored under.
const TOKEN_KEY: &str = "cursorAuth/accessToken";

/// The signed-in token and the account it belongs to.
pub(crate) struct AppSignIn {
    /// The access token.
    pub(crate) token: Secret,
    /// The account, from the token's subject after its `provider|` prefix.
    pub(crate) user_id: String,
}

/// The app's current sign-in, while its token is valid.
pub(crate) async fn sign_in() -> Result<AppSignIn, ProviderError> {
    let lookup: Lookup<'_> = &process_lookup;
    let path = store_path(lookup).ok_or(ProviderError::Authentication)?;
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Err(ProviderError::Authentication);
    }
    let token = read_token(&path).await?;
    let expired = jwt::claim(&token, "exp")
        .and_then(|value| value.as_i64())
        .is_some_and(|expires| expires <= Utc::now().timestamp());
    if expired {
        return Err(ProviderError::Authentication);
    }
    let user_id = jwt::claim(&token, "sub")
        .and_then(|value| value.as_str().map(str::to_owned))
        .map(|subject| {
            subject
                .rsplit_once('|')
                .map_or_else(|| subject.clone(), |(_, id)| id.to_owned())
        })
        .filter(|id| !id.is_empty())
        .ok_or(ProviderError::Authentication)?;
    Ok(AppSignIn {
        token: Secret::new(token),
        user_id,
    })
}

/// Where the Cursor app keeps its state store.
fn store_path(lookup: Lookup<'_>) -> Option<PathBuf> {
    Some(
        platform_paths::application_data(lookup)?
            .join("Cursor")
            .join("User")
            .join("globalStorage")
            .join("state.vscdb"),
    )
}

/// Reads the token from the store, read-only.
///
/// The app may hold the store open; a read-only connection shares it without
/// taking a write lock, and is closed as soon as the value is read.
async fn read_token(path: &std::path::Path) -> Result<String, ProviderError> {
    let unreadable = |_| ProviderError::Transient {
        detail: "the Cursor app's state store could not be read".to_owned(),
    };
    let options = SqliteConnectOptions::from_str("sqlite://")
        .map_err(unreadable)?
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    let mut connection = options.connect().await.map_err(unreadable)?;
    let value: Option<String> =
        sqlx::query_scalar("SELECT CAST(value AS TEXT) FROM ItemTable WHERE key = ?")
            .bind(TOKEN_KEY)
            .fetch_optional(&mut connection)
            .await
            .map_err(unreadable)?;
    if let Err(error) = sqlx::Connection::close(connection).await {
        tracing::debug!(error = %error, "the Cursor state store did not close cleanly");
    }
    value
        .map(|token| token.trim().trim_matches('"').to_owned())
        .filter(|token| !token.is_empty())
        .ok_or(ProviderError::Authentication)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_token_is_read_from_a_state_store_without_changing_it() {
        let directory = std::env::temp_dir().join(format!("quota-cursor-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("a directory");
        let path = directory.join("state.vscdb");
        {
            let mut writer = SqliteConnectOptions::from_str("sqlite://")
                .expect("options")
                .filename(&path)
                .create_if_missing(true)
                .connect()
                .await
                .expect("a store");
            sqlx::query("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)")
                .execute(&mut writer)
                .await
                .expect("a table");
            sqlx::query("INSERT INTO ItemTable VALUES (?, ?)")
                .bind(TOKEN_KEY)
                .bind("header.payload.signature")
                .execute(&mut writer)
                .await
                .expect("a row");
            sqlx::Connection::close(writer).await.expect("closed");
        }
        let before = std::fs::read(&path).expect("the store");
        assert_eq!(
            read_token(&path).await.expect("a token"),
            "header.payload.signature"
        );
        assert_eq!(
            std::fs::read(&path).expect("the store"),
            before,
            "the store is unchanged"
        );
        std::fs::remove_dir_all(&directory).expect("cleaned up");
    }
}
