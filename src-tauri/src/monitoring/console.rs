//! A sign-in on the provider's own website, which Quota opens for it.
//!
//! For a provider whose usage only its website shows (`TypeSafe`), Quota opens
//! the provider's sign-in page itself. It prefers the person's own browser,
//! Chrome or Edge, in a profile folder of Quota's own (`browser_session`):
//! Google refuses its sign-in inside an app's embedded window, and a real
//! browser is what the provider's sign-in expects. With no such browser, Quota
//! opens a window of its own instead, which keeps its browser storage in a
//! folder of its own and whose label no capability names, so the page in it
//! cannot reach the app. Either way, while the person signs in the host reads
//! the cookies for the provider and asks the adapter whether they are a
//! working session; once they are, that `Cookie` header is the credential,
//! continuing exactly like a pasted key, and the browser or window closes. A
//! bot check the provider puts in front of Quota's requests ends the attempt
//! with that reason; Quota never tries to pass one. Reconnect runs the same
//! sign-in again and replaces the session the account keeps.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use quota_contracts::CommandError;
use quota_contracts::commands::{BeginConnectionRequest, PastedCredential};
use quota_contracts::events::ConnectionProgress;
use quota_core::ports::{ConsoleSignIn, ProviderAdapter, ProviderError, Secret};
use quota_domain::ids::{AccountId, ConnectionAttemptId, ConnectionId};
use quota_domain::provider::ProviderId;
use tauri::Manager;
use tokio::sync::watch;

use super::browser_session::{self, Plain};
use super::connection::{AttemptReporter, attempt_error};
use super::{MonitoringRuntime, REMOTE_TIMEOUT};
use crate::platform::browser;

/// Every console sign-in window's label starts with this. No capability file
/// names it, so the provider's page has no access to the app.
pub(crate) const LABEL_PREFIX: &str = "console-sign-in-";

/// The folder, under the app's local data folder, the windows' browser
/// storage is kept in, one subfolder per attempt.
const STORAGE_FOLDER: &str = "console-sign-in";

/// The folder, under the app's local data folder, of the browser profiles
/// Quota owns, one per provider.
const PROFILE_FOLDER: &str = "browser-sign-in";

/// How long a person has to sign in.
const SIGN_IN_LIMIT: Duration = Duration::from_secs(15 * 60);

/// Runs the console sign-in an attempt asked for, for an adapter that offers
/// one, and answers the request carrying the session. Any other request comes
/// back unchanged; `None` means the attempt was cancelled.
pub(super) async fn sign_in(
    runtime: &MonitoringRuntime,
    adapter: &Arc<dyn ProviderAdapter>,
    mut request: BeginConnectionRequest,
    attempt_id: &ConnectionAttemptId,
    cancelled: &mut watch::Receiver<bool>,
    reporter: &AttemptReporter,
) -> Result<Option<BeginConnectionRequest>, CommandError> {
    let Some(console) = adapter
        .console_sign_in()
        .filter(|_| request.browser_sign_in)
    else {
        return Ok(Some(request));
    };
    reporter
        .emit(&runtime.state, attempt_id, ConnectionProgress::Started)
        .await;
    reporter
        .emit(
            &runtime.state,
            attempt_id,
            ConnectionProgress::AwaitingUser { sign_in: None },
        )
        .await;
    let app = runtime.state.app.clone();
    let Some(session) = capture(&app, adapter, &console, attempt_id.as_str(), cancelled).await?
    else {
        return Ok(None);
    };
    request.credential = Some(PastedCredential::new(session));
    request.browser_sign_in = false;
    Ok(Some(request))
}

/// Runs the website sign-in again for an account whose session ended, in the
/// background, and on success replaces the session it keeps and verifies the
/// account. A sign-in the person abandons leaves the account as it was.
pub(super) fn reconnect(
    runtime: MonitoringRuntime,
    adapter: Arc<dyn ProviderAdapter>,
    console: ConsoleSignIn,
    account_id: AccountId,
    connection_id: ConnectionId,
) {
    tauri::async_runtime::spawn(async move {
        // Nothing cancels a reconnect but the person closing the browser.
        let (_keep, mut cancelled) = watch::channel(false);
        let app = runtime.state.app.clone();
        let key = format!("reconnect-{}", connection_id.as_str());
        match capture(&app, &adapter, &console, &key, &mut cancelled).await {
            Ok(Some(session)) => {
                if runtime
                    .secrets()
                    .write(&connection_id, &Secret::new(session))
                    .await
                    .is_err()
                {
                    tracing::warn!("a renewed website session could not be stored");
                    return;
                }
                if let Err(error) = runtime.verify_again(&account_id).await {
                    tracing::warn!(?error, "a renewed website session could not be verified");
                }
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(?error, "a website sign-in to reconnect did not finish"),
        }
    });
}

/// Deletes the browser profile Quota keeps for `provider`, once its account
/// is disconnected.
pub(crate) fn forget_profile(app: &tauri::AppHandle, provider: ProviderId) {
    if let Ok(folder) = profile_folder(app, provider) {
        forget_later(folder);
    }
}

/// Opens the sign-in, in the person's browser when one is installed and
/// otherwise in a window of Quota's own, and waits for a working session.
/// `key` names this attempt's window and storage.
async fn capture(
    app: &tauri::AppHandle,
    adapter: &Arc<dyn ProviderAdapter>,
    console: &ConsoleSignIn,
    key: &str,
    cancelled: &mut watch::Receiver<bool>,
) -> Result<Option<String>, CommandError> {
    let cookie_url = tauri::Url::parse(console.cookie_url).map_err(|_| refused())?;
    if let Some(browser) = browser::find() {
        let profile = profile_folder(app, adapter.provider_id())?;
        match browser_session::sign_in(
            &browser,
            &profile,
            console.sign_in_url,
            SIGN_IN_LIMIT,
            cancelled,
        )
        .await
        {
            Plain::Closed => {
                let cookies = browser_session::read_cookies(
                    &browser,
                    &profile,
                    cookie_url.host_str().unwrap_or_default(),
                    cookie_url.path(),
                )
                .await;
                let Some(header) = cookies.and_then(|cookies| cookie_header(cookies.into_iter()))
                else {
                    return Err(internal("console_sign_in_closed"));
                };
                return match try_session(adapter, &header, cancelled).await? {
                    Tried::Accepted => Ok(Some(header)),
                    Tried::Cancelled => Ok(None),
                    Tried::NotYet => Err(internal("console_sign_in_closed")),
                };
            }
            Plain::Cancelled => return Ok(None),
            Plain::TimedOut => return Err(internal("browser_sign_in_timeout")),
            Plain::Failed => {}
        }
    }
    let source = window::open_window(app, key, console)?;
    let outcome = window::wait(adapter, &source, &cookie_url, cancelled).await;
    source.finish();
    outcome
}

/// What one try of a session came to.
enum Tried {
    Accepted,
    /// Not signed in yet, or a passing failure.
    NotYet,
    Cancelled,
}

/// Tries `header` on the provider once.
async fn try_session(
    adapter: &Arc<dyn ProviderAdapter>,
    header: &str,
    cancelled: &mut watch::Receiver<bool>,
) -> Result<Tried, CommandError> {
    let session = Secret::new(header.to_owned());
    let tried = tokio::select! {
        _ = cancelled.changed() => return Ok(Tried::Cancelled),
        tried = tokio::time::timeout(REMOTE_TIMEOUT, adapter.discover_with(&session)) => tried,
    };
    match tried {
        Ok(Ok(accounts)) if !accounts.is_empty() => Ok(Tried::Accepted),
        Ok(Err(
            error @ (ProviderError::Blocked { .. } | ProviderError::UnsupportedSchema { .. }),
        )) => Err(attempt_error(&error)),
        _ => Ok(Tried::NotYet),
    }
}

/// Joins cookies into one header, leaving out any with an empty name or a
/// character a header cannot carry.
fn cookie_header(cookies: impl Iterator<Item = (String, String)>) -> Option<String> {
    let pairs: Vec<String> = cookies
        .filter(|(name, value)| {
            !name.is_empty()
                && !name.contains(['=', ';', '\r', '\n'])
                && !value.contains([';', '\r', '\n'])
        })
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    (!pairs.is_empty()).then(|| pairs.join("; "))
}

/// The folder every console window's storage lives under.
fn storage_root(app: &tauri::AppHandle) -> Result<PathBuf, CommandError> {
    app.path()
        .app_local_data_dir()
        .map(|folder| folder.join(STORAGE_FOLDER))
        .map_err(|_| refused())
}

/// The browser profile Quota keeps for `provider`.
fn profile_folder(app: &tauri::AppHandle, provider: ProviderId) -> Result<PathBuf, CommandError> {
    app.path()
        .app_local_data_dir()
        .map(|folder| folder.join(PROFILE_FOLDER).join(provider.as_str()))
        .map_err(|_| refused())
}

/// Deletes storage an earlier attempt could not delete, such as one that was
/// still locked when its window closed, or one left by a crash.
fn forget_leftovers(storage: &Path) {
    let Ok(entries) = std::fs::read_dir(storage) else {
        return;
    };
    for entry in entries.flatten() {
        if std::fs::remove_dir_all(entry.path()).is_err() {
            tracing::debug!("an earlier console sign-in's storage is still in use");
        }
    }
}

/// Deletes a window's storage or a browser profile once the browser has let
/// go of it.
fn forget_later(folder: PathBuf) {
    tauri::async_runtime::spawn(async move {
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if !folder.exists() || std::fs::remove_dir_all(&folder).is_ok() {
                return;
            }
        }
        tracing::warn!("a website sign-in's storage is kept until the next sign-in");
    });
}

/// Logs a problem with a website sign-in.
fn warn(message: &str) {
    tracing::warn!("{message}");
}

fn refused() -> CommandError {
    CommandError::NativeOperationFailed {
        operation: "console_sign_in".into(),
        reason: "Quota could not open the sign-in".into(),
    }
}

fn internal(code: &str) -> CommandError {
    CommandError::Internal { code: code.into() }
}

#[path = "console_window.rs"]
mod window;

#[cfg(test)]
#[path = "console_tests.rs"]
mod wait_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_join_into_one_header_without_unsafe_ones() {
        let cookies = [
            ("session", "abc"),
            ("", "nameless"),
            ("bad;name", "x"),
            ("split", "a;b"),
            ("theme", "dark"),
        ]
        .map(|(name, value)| (name.to_owned(), value.to_owned()));
        assert_eq!(
            cookie_header(cookies.into_iter()).as_deref(),
            Some("session=abc; theme=dark")
        );
        assert_eq!(cookie_header(std::iter::empty()), None);
    }

    /// The provider's page runs in a window whose label no capability names,
    /// so it can reach no command. Every capability names its windows exactly.
    #[test]
    fn no_capability_reaches_a_console_sign_in_window() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
        let mut checked = 0;
        for entry in std::fs::read_dir(folder).expect("the capability folder") {
            let path = entry.expect("an entry").path();
            let text = std::fs::read_to_string(&path).expect("a capability file");
            let capability: serde_json::Value =
                serde_json::from_str(&text).expect("a capability is JSON");
            assert!(
                capability.get("remote").is_none(),
                "{} grants a remote page",
                path.display()
            );
            for key in ["windows", "webviews"] {
                for label in capability[key].as_array().into_iter().flatten() {
                    let label = label.as_str().expect("a label");
                    assert!(
                        !label.contains('*') && !label.starts_with(LABEL_PREFIX),
                        "{} reaches {label}",
                        path.display()
                    );
                }
            }
            checked += 1;
        }
        assert!(checked > 0);
    }
}
