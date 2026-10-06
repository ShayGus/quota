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
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::watch;

use super::browser_session::{self, Opened, Running};
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

/// How often the session is tried.
const POLL_INTERVAL: Duration = Duration::from_secs(3);

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
    let source = match browser::find() {
        Some(browser) => {
            let profile = profile_folder(app, adapter.provider_id())?;
            match browser_session::start(&browser, &profile, console.sign_in_url, cancelled).await {
                Opened::Session(session) => Source::Browser(Running::new(*session)),
                Opened::Cancelled => return Ok(None),
                Opened::Failed => open_window(app, key, console)?,
            }
        }
        None => open_window(app, key, console)?,
    };
    let outcome = wait(adapter, &source, &cookie_url, cancelled).await;
    source.finish().await;
    outcome
}

/// Where the session is read from while the person signs in.
enum Source {
    Window {
        window: Box<WebviewWindow>,
        folder: PathBuf,
    },
    Browser(Running),
}

/// The person closed the browser or the window before the sign-in finished.
struct Closed;

impl Source {
    /// The provider's cookies as one `Cookie` header, `None` while there are
    /// none.
    async fn header(&self, url: &tauri::Url) -> Result<Option<String>, Closed> {
        match self {
            Self::Window { window, .. } => {
                if window
                    .app_handle()
                    .get_webview_window(window.label())
                    .is_none()
                {
                    return Err(Closed);
                }
                Ok(session_header(window, url).await)
            }
            Self::Browser(running) => running
                .cookies(url.host_str().unwrap_or_default(), url.path())
                .await
                .map(|cookies| cookie_header(cookies.into_iter()))
                .map_err(|_| Closed),
        }
    }

    /// Closes the browser or the window, and deletes a window's storage.
    async fn finish(self) {
        match self {
            Self::Window { window, folder } => {
                if window.close().is_err() {
                    warn("the console sign-in window did not close");
                }
                forget_later(folder);
            }
            Self::Browser(running) => running.close().await,
        }
    }
}

/// Opens the provider's sign-in page in a window with its own storage.
fn open_window(
    app: &tauri::AppHandle,
    key: &str,
    console: &ConsoleSignIn,
) -> Result<Source, CommandError> {
    let storage = storage_root(app)?;
    forget_leftovers(&storage);
    let folder = storage.join(key);
    let url = tauri::Url::parse(console.sign_in_url).map_err(|_| refused())?;
    let window = WebviewWindowBuilder::new(
        app,
        format!("{LABEL_PREFIX}{key}"),
        WebviewUrl::External(url),
    )
    .title("Sign in to TypeSafe · Quota")
    .inner_size(1000.0, 820.0)
    .center()
    .focused(true)
    .data_directory(folder.clone())
    .build()
    .map_err(|error| {
        tracing::warn!(%error, "the console sign-in window did not open");
        refused()
    })?;
    Ok(Source::Window {
        window: Box::new(window),
        folder,
    })
}

/// Waits until the source holds a working session, the person closes it, the
/// attempt is cancelled, or time runs out.
async fn wait(
    adapter: &Arc<dyn ProviderAdapter>,
    source: &Source,
    cookie_url: &tauri::Url,
    cancelled: &mut watch::Receiver<bool>,
) -> Result<Option<String>, CommandError> {
    let started = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = cancelled.changed() => return Ok(None),
            () = tokio::time::sleep(POLL_INTERVAL) => {}
        }
        if started.elapsed() > SIGN_IN_LIMIT {
            return Err(internal("browser_sign_in_timeout"));
        }
        let Some(header) = source
            .header(cookie_url)
            .await
            .map_err(|Closed| internal("console_sign_in_closed"))?
        else {
            continue;
        };
        let session = Secret::new(header.clone());
        let tried = tokio::select! {
            _ = cancelled.changed() => return Ok(None),
            tried = tokio::time::timeout(REMOTE_TIMEOUT, adapter.discover_with(&session)) => tried,
        };
        match tried {
            Ok(Ok(accounts)) if !accounts.is_empty() => return Ok(Some(header)),
            Ok(Err(
                error @ (ProviderError::Blocked { .. } | ProviderError::UnsupportedSchema { .. }),
            )) => {
                return Err(attempt_error(&error));
            }
            // Not signed in yet, or a passing failure: keep waiting.
            _ => {}
        }
    }
}

/// The window's cookies for the provider as one `Cookie` header, or `None`
/// while it has none.
///
/// Reading cookies blocks on the webview, and on Windows it deadlocks on the
/// main thread, so it runs on a blocking thread.
async fn session_header(window: &WebviewWindow, url: &tauri::Url) -> Option<String> {
    let window = window.clone();
    let url = url.clone();
    let cookies = tokio::task::spawn_blocking(move || window.cookies_for_url(url))
        .await
        .ok()?
        .ok()?;
    cookie_header(
        cookies
            .iter()
            .map(|cookie| (cookie.name().to_owned(), cookie.value().to_owned())),
    )
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
