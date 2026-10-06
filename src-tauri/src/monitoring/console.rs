//! A sign-in on the provider's own website, in a window Quota opens for it.
//!
//! For a provider whose usage only its website shows (`TypeSafe`), Quota opens
//! the provider's sign-in page in a window of its own. The window keeps its
//! browser storage in a folder of its own, apart from every other browser and
//! from Quota's windows, and its label is named by no capability, so the page
//! in it cannot reach the app. While the person signs in, the host reads the
//! window's cookies for the provider and asks the adapter whether they are a
//! working session; once they are, that `Cookie` header is the credential,
//! continuing exactly like a pasted key. The window then closes and its folder
//! is deleted. A bot check the provider puts in front of Quota's requests ends
//! the attempt with that reason; Quota never tries to pass one.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use quota_contracts::CommandError;
use quota_contracts::commands::{BeginConnectionRequest, PastedCredential};
use quota_contracts::events::ConnectionProgress;
use quota_core::ports::{ConsoleSignIn, ProviderAdapter, ProviderError, Secret};
use quota_domain::ids::ConnectionAttemptId;
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::watch;

use super::connection::{AttemptReporter, attempt_error};
use super::{MonitoringRuntime, REMOTE_TIMEOUT};

/// Every console sign-in window's label starts with this. No capability file
/// names it, so the provider's page has no access to the app.
pub(crate) const LABEL_PREFIX: &str = "console-sign-in-";

/// The folder, under the app's local data folder, the windows' browser
/// storage is kept in, one subfolder per attempt.
const STORAGE_FOLDER: &str = "console-sign-in";

/// How often the window's cookies are tried.
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
    let app = runtime.state.app.clone();
    let storage = storage_root(&app)?;
    forget_leftovers(&storage);
    let folder = storage.join(attempt_id.as_str());
    let window = open(&app, attempt_id, &console, &folder)?;
    reporter
        .emit(
            &runtime.state,
            attempt_id,
            ConnectionProgress::AwaitingUser { sign_in: None },
        )
        .await;
    let outcome = wait(adapter, &window, &console, cancelled).await;
    if let Err(error) = window.close() {
        tracing::warn!(%error, "the console sign-in window did not close");
    }
    forget_later(folder);
    let Some(session) = outcome? else {
        return Ok(None);
    };
    request.credential = Some(PastedCredential::new(session));
    request.browser_sign_in = false;
    Ok(Some(request))
}

/// Opens the provider's sign-in page in a window with its own storage.
fn open(
    app: &tauri::AppHandle,
    attempt_id: &ConnectionAttemptId,
    console: &ConsoleSignIn,
    folder: &Path,
) -> Result<WebviewWindow, CommandError> {
    let url = tauri::Url::parse(console.sign_in_url).map_err(|_| refused())?;
    WebviewWindowBuilder::new(
        app,
        format!("{LABEL_PREFIX}{}", attempt_id.as_str()),
        WebviewUrl::External(url),
    )
    .title("Sign in to TypeSafe · Quota")
    .inner_size(1000.0, 820.0)
    .center()
    .focused(true)
    .data_directory(folder.to_path_buf())
    .build()
    .map_err(|error| {
        tracing::warn!(%error, "the console sign-in window did not open");
        refused()
    })
}

/// Waits until the window holds a working session, the person closes it, the
/// attempt is cancelled, or time runs out.
async fn wait(
    adapter: &Arc<dyn ProviderAdapter>,
    window: &WebviewWindow,
    console: &ConsoleSignIn,
    cancelled: &mut watch::Receiver<bool>,
) -> Result<Option<String>, CommandError> {
    let cookie_url = tauri::Url::parse(console.cookie_url).map_err(|_| refused())?;
    let started = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = cancelled.changed() => return Ok(None),
            () = tokio::time::sleep(POLL_INTERVAL) => {}
        }
        if started.elapsed() > SIGN_IN_LIMIT {
            return Err(internal("browser_sign_in_timeout"));
        }
        if window
            .app_handle()
            .get_webview_window(window.label())
            .is_none()
        {
            return Err(internal("console_sign_in_closed"));
        }
        let Some(header) = session_header(window, &cookie_url).await else {
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

/// Deletes one window's storage once the browser has let go of it.
fn forget_later(folder: PathBuf) {
    tauri::async_runtime::spawn(async move {
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if !folder.exists() || std::fs::remove_dir_all(&folder).is_ok() {
                return;
            }
        }
        tracing::warn!("a console sign-in's storage is kept until the next sign-in");
    });
}

fn refused() -> CommandError {
    CommandError::NativeOperationFailed {
        operation: "console_sign_in".into(),
        reason: "Quota could not open the sign-in window".into(),
    }
}

fn internal(code: &str) -> CommandError {
    CommandError::Internal { code: code.into() }
}

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
