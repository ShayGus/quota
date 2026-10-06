//! The website sign-in in a window of Quota's own: used when no supported
//! browser is installed. Its browser storage is a folder of its own and its
//! label is in no capability file, so the page cannot reach the app. Google
//! refuses its sign-in here; other sign-ins work.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use quota_contracts::CommandError;
use quota_core::ports::{ConsoleSignIn, ProviderAdapter};
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::watch;

use super::{
    LABEL_PREFIX, SIGN_IN_LIMIT, Tried, cookie_header, forget_later, forget_leftovers, internal,
    refused, storage_root, try_session, warn,
};

/// How long a page that did not give a working session waits before the
/// same page is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// How often the session is tried.
const POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Where the session is read from while the person signs in.
pub(super) enum Source {
    Window {
        window: Box<WebviewWindow>,
        folder: PathBuf,
    },
}

/// The person closed the browser or the window before the sign-in finished.
struct Closed;

impl Source {
    /// The addresses of the source's open pages.
    fn pages(&self) -> Result<Vec<String>, Closed> {
        match self {
            Self::Window { window, .. } => window
                .app_handle()
                .get_webview_window(window.label())
                .and_then(|open| open.url().ok())
                .map(|url| vec![url.to_string()])
                .ok_or(Closed),
        }
    }

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
        }
    }

    /// Closes the browser or the window, and deletes a window's storage.
    pub(super) fn finish(self) {
        match self {
            Self::Window { window, folder } => {
                if window.close().is_err() {
                    warn("the console sign-in window did not close");
                }
                forget_later(folder);
            }
        }
    }
}

/// Opens the provider's sign-in page in a window with its own storage.
pub(super) fn open_window(
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
pub(super) async fn wait(
    adapter: &Arc<dyn ProviderAdapter>,
    source: &Source,
    cookie_url: &tauri::Url,
    cancelled: &mut watch::Receiver<bool>,
) -> Result<Option<String>, CommandError> {
    let started = tokio::time::Instant::now();
    let mut last_try: Option<(String, tokio::time::Instant)> = None;
    loop {
        tokio::select! {
            _ = cancelled.changed() => return Ok(None),
            () = tokio::time::sleep(POLL_INTERVAL) => {}
        }
        if started.elapsed() > SIGN_IN_LIMIT {
            return Err(internal("browser_sign_in_timeout"));
        }
        // Nothing is sent to the provider until the page itself has left
        // sign-in: a request beside the person's sign-in, with the cookies a
        // human check just handed out, makes the check refuse them.
        let pages = source
            .pages()
            .map_err(|Closed| internal("console_sign_in_closed"))?;
        let Some(page) = pages
            .into_iter()
            .find(|page| past_sign_in(page, cookie_url))
        else {
            continue;
        };
        if last_try
            .as_ref()
            .is_some_and(|(tried, at)| *tried == page && at.elapsed() < RETRY_AFTER)
        {
            continue;
        }
        last_try = Some((page, tokio::time::Instant::now()));
        let Some(header) = source
            .header(cookie_url)
            .await
            .map_err(|Closed| internal("console_sign_in_closed"))?
        else {
            continue;
        };
        match try_session(adapter, &header, cancelled).await? {
            Tried::Accepted => return Ok(Some(header)),
            Tried::Cancelled => return Ok(None),
            Tried::NotYet => {}
        }
    }
}

/// Whether a page is on the provider's console and past its sign-in.
pub(super) fn past_sign_in(page: &str, console: &tauri::Url) -> bool {
    let Ok(page) = tauri::Url::parse(page) else {
        return false;
    };
    let path = page.path().to_ascii_lowercase();
    page.scheme() == "https"
        && page.host_str() == console.host_str()
        && ![
            "/login",
            "/signin",
            "/sign-in",
            "/signup",
            "/sign-up",
            "/auth",
            "/callback",
            "/sso",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
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
