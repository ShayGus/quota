//! A website sign-in in the person's own browser, in a profile Quota owns.
//!
//! Google refuses its sign-in inside an app's embedded window, so for a
//! console sign-in Quota opens the installed Chrome or Edge instead, with a
//! profile folder of Quota's own, never the person's usual one. The browser is
//! started with its `DevTools` connection on the loopback address only, on a
//! port the browser picks and writes into the profile (`DevToolsActivePort`).
//! While the person signs in, nothing is connected to the browser: Quota only
//! reads the list of open pages from the browser's local `/json/list` address,
//! which does not attach to any page. A connected `DevTools` client during the
//! sign-in made `TypeSafe`'s human check refuse it again and again, while the
//! same browser with nothing connected passed. Only once a page has left the
//! sign-in does Quota connect, to ask for the profile's cookies for the
//! provider's site and to close the browser once they work.
//! Quota never runs anything in the page, clicks, or types, and reads no file
//! the browser writes other than that port file. The profile stays, so the
//! next sign-in usually needs one click; disconnecting the account deletes it.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

use tokio::sync::watch;

use crate::platform::browser::Browser;

/// The file the browser writes its chosen port and browser path into.
const ACTIVE_PORT_FILE: &str = "DevToolsActivePort";

/// How long one `DevTools` answer may take.
const ANSWER_LIMIT: Duration = Duration::from_secs(5);

/// How long the browser has to start its `DevTools` connection.
const START_LIMIT: Duration = Duration::from_secs(20);

/// How long the browser has to close before Quota stops the process it started.
const CLOSE_LIMIT: Duration = Duration::from_secs(5);

/// The browser Quota started, and the connection to it once there is one.
pub(super) struct BrowserSession {
    child: Child,
    port: u16,
    path: String,
    socket: Option<WebSocket<TcpStream>>,
    next_id: u64,
}

/// Why the browser stopped answering.
#[derive(Debug)]
pub(super) enum Ended {
    /// The person closed the window, or the browser exited.
    Closed,
}

/// Starts the browser on `url` with the profile in `profile`.
///
/// # Errors
/// Returns the reason the folder could not be made or the program not run.
pub(super) fn launch(browser: &Browser, profile: &Path, url: &str) -> io::Result<Child> {
    std::fs::create_dir_all(profile)?;
    // A port file left by an earlier run would name a port nobody listens on.
    drop(std::fs::remove_file(profile.join(ACTIVE_PORT_FILE)));
    Command::new(&browser.program)
        .args(arguments(profile, url))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

/// What starting the browser came to.
pub(super) enum Opened {
    Session(Box<BrowserSession>),
    Cancelled,
    /// The browser did not start its connection; Quota's own window is used.
    Failed,
}

/// Starts `browser` on `url` with the profile in `profile`, and connects to it.
pub(super) async fn start(
    browser: &Browser,
    profile: &Path,
    url: &str,
    cancelled: &mut watch::Receiver<bool>,
) -> Opened {
    let mut child = match launch(browser, profile, url) {
        Ok(child) => child,
        Err(error) => {
            warn(browser, &format!("the browser did not start: {error}"));
            return Opened::Failed;
        }
    };
    let Some(found) = wait_for_port(profile, cancelled).await else {
        stop(&mut child);
        return Opened::Cancelled;
    };
    let Some((port, path)) = found else {
        warn(browser, "the browser did not open its sign-in connection");
        stop(&mut child);
        return Opened::Failed;
    };
    let connected =
        tokio::task::spawn_blocking(move || BrowserSession::connect(child, port, &path)).await;
    if let Ok(Ok(session)) = connected {
        Opened::Session(Box::new(session))
    } else {
        warn(browser, "the browser refused the sign-in connection");
        Opened::Failed
    }
}

/// Waits for the browser to write its `DevTools` port: `None` when the attempt
/// is cancelled, `Some(None)` when the browser never writes it.
async fn wait_for_port(
    profile: &Path,
    cancelled: &mut watch::Receiver<bool>,
) -> Option<Option<(u16, String)>> {
    let started = tokio::time::Instant::now();
    while started.elapsed() < START_LIMIT {
        if let Some(found) = active_port(profile) {
            return Some(Some(found));
        }
        tokio::select! {
            _ = cancelled.changed() => return None,
            () = tokio::time::sleep(Duration::from_millis(250)) => {}
        }
    }
    Some(None)
}

/// Logs a problem with the sign-in browser, naming which browser it was.
fn warn(browser: &Browser, message: &str) {
    tracing::warn!(browser = browser.name, "{message}");
}

/// A connected browser the sign-in reads from, on blocking threads.
pub(super) struct Running(Arc<Mutex<Option<BrowserSession>>>);

impl Running {
    pub(super) fn new(session: BrowserSession) -> Self {
        Self(Arc::new(Mutex::new(Some(session))))
    }

    /// The browser's cookies for `host` at `path`.
    pub(super) async fn cookies(
        &self,
        host: &str,
        path: &str,
    ) -> Result<Vec<(String, String)>, Ended> {
        let session = Arc::clone(&self.0);
        let host = host.to_owned();
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut guard = session.lock().map_err(|_| Ended::Closed)?;
            guard.as_mut().ok_or(Ended::Closed)?.cookies(&host, &path)
        })
        .await
        .map_err(|_| Ended::Closed)?
    }

    /// The addresses of the browser's open pages; `Ended::Closed` once none
    /// is open.
    pub(super) async fn pages(&self) -> Result<Vec<String>, Ended> {
        let session = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            let mut guard = session.lock().map_err(|_| Ended::Closed)?;
            guard.as_mut().ok_or(Ended::Closed)?.pages()
        })
        .await
        .map_err(|_| Ended::Closed)?
    }

    /// Closes the browser.
    pub(super) async fn close(self) {
        let session = self.0;
        let closed = tokio::task::spawn_blocking(move || {
            if let Some(session) = session.lock().ok().and_then(|mut guard| guard.take()) {
                session.close();
            }
        })
        .await;
        if closed.is_err() {
            tracing::warn!("the sign-in browser could not be closed");
        }
    }
}

/// What the browser is started with: Quota's profile, a `DevTools` connection
/// on the loopback address only, and none of the first-run prompts.
fn arguments(profile: &Path, url: &str) -> Vec<String> {
    vec![
        format!("--user-data-dir={}", profile.display()),
        "--remote-debugging-port=0".to_owned(),
        "--remote-debugging-address=127.0.0.1".to_owned(),
        "--no-first-run".to_owned(),
        "--no-default-browser-check".to_owned(),
        "--new-window".to_owned(),
        url.to_owned(),
    ]
}

/// The port and browser path the browser wrote, once it has.
pub(super) fn active_port(profile: &Path) -> Option<(u16, String)> {
    parse_active_port(&std::fs::read_to_string(profile.join(ACTIVE_PORT_FILE)).ok()?)
}

/// Reads `DevToolsActivePort`: the port on the first line, the browser's
/// `DevTools` path on the second.
fn parse_active_port(text: &str) -> Option<(u16, String)> {
    let mut lines = text.lines().map(str::trim);
    let port = lines
        .next()?
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)?;
    let path = lines.next()?;
    let valid = path.starts_with("/devtools/browser/")
        && path
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/-_".contains(character));
    valid.then(|| (port, path.to_owned()))
}

impl BrowserSession {
    /// Connects to the browser Quota started.
    ///
    /// # Errors
    /// Returns the reason the loopback connection could not be made.
    /// A connection that cannot be made stops the browser Quota started.
    pub(super) fn connect(mut child: Child, port: u16, path: &str) -> io::Result<Self> {
        // The page list proves the browser answers on the loopback address,
        // without connecting a `DevTools` client to it yet.
        match page_list(port) {
            Ok(_) => Ok(Self {
                child,
                port,
                path: path.to_owned(),
                socket: None,
                next_id: 0,
            }),
            Err(error) => {
                stop(&mut child);
                Err(error)
            }
        }
    }

    /// Sends one `DevTools` command and waits for its answer, skipping events.
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, Ended> {
        self.next_id += 1;
        let id = self.next_id;
        let request = json!({ "id": id, "method": method, "params": params });
        if self.socket.is_none() {
            self.socket = Some(open_socket(self.port, &self.path).map_err(|_| Ended::Closed)?);
        }
        let socket = self.socket.as_mut().ok_or(Ended::Closed)?;
        socket
            .send(Message::text(request.to_string()))
            .map_err(|_| Ended::Closed)?;
        let started = Instant::now();
        while started.elapsed() < ANSWER_LIMIT {
            let message = socket.read().map_err(|_| Ended::Closed)?;
            let Message::Text(text) = message else {
                continue;
            };
            let Ok(answer) = serde_json::from_str::<Value>(text.as_str()) else {
                continue;
            };
            if answer.get("id").and_then(Value::as_u64) == Some(id) {
                return answer.get("result").cloned().ok_or(Ended::Closed);
            }
        }
        Err(Ended::Closed)
    }

    /// The addresses of the profile's open pages; `Ended::Closed` once no
    /// window of the profile is open. Reading them does not touch the pages.
    pub(super) fn pages(&mut self) -> Result<Vec<String>, Ended> {
        let pages = page_urls(&page_list(self.port).map_err(|_| Ended::Closed)?);
        if pages.is_empty() {
            return Err(Ended::Closed);
        }
        Ok(pages)
    }

    /// The profile's cookies for `host` and the page path `path`, as name and
    /// value pairs; `Ended::Closed` once no window of the profile is open.
    pub(super) fn cookies(
        &mut self,
        host: &str,
        path: &str,
    ) -> Result<Vec<(String, String)>, Ended> {
        self.pages()?;
        let answer = self.call("Storage.getCookies", &json!({}))?;
        Ok(cookies_for(&answer, host, path))
    }

    /// Closes the browser, then stops the process Quota started if it is
    /// still running. Only that process is ever stopped.
    pub(super) fn close(mut self) {
        drop(self.call("Browser.close", &json!({})));
        let started = Instant::now();
        while started.elapsed() < CLOSE_LIMIT {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        stop(&mut self.child);
    }
}

/// Stops the browser process Quota started, and only that one.
pub(super) fn stop(child: &mut Child) {
    drop(child.kill());
    drop(child.wait());
}

/// The `DevTools` websocket to the browser on the loopback address.
fn open_socket(port: u16, path: &str) -> io::Result<WebSocket<TcpStream>> {
    let stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(ANSWER_LIMIT))?;
    stream.set_write_timeout(Some(ANSWER_LIMIT))?;
    tungstenite::client::client(format!("ws://127.0.0.1:{port}{path}"), stream)
        .map(|(socket, _)| socket)
        .map_err(|_| io::Error::other("the browser refused the DevTools connection"))
}

/// The browser's open targets from its local `/json/list` address, which lists
/// them without attaching to any.
fn page_list(port: u16) -> io::Result<Value> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(ANSWER_LIMIT))?;
    stream.set_write_timeout(Some(ANSWER_LIMIT))?;
    write!(
        stream,
        "GET /json/list HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer)?;
    let body = answer
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .ok_or_else(|| io::Error::other("the browser's page list had no body"))?;
    serde_json::from_str(body)
        .map_err(|_| io::Error::other("the browser's page list was unreadable"))
}

/// The addresses of the pages in a `/json/list` answer.
fn page_urls(targets: &Value) -> Vec<String> {
    targets
        .as_array()
        .map(|targets| {
            targets
                .iter()
                .filter(|target| target.get("type").and_then(Value::as_str) == Some("page"))
                .map(|target| {
                    target
                        .get("url")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The cookies in a `Storage.getCookies` answer a request to `host` at `path`
/// would carry.
fn cookies_for(answer: &Value, host: &str, path: &str) -> Vec<(String, String)> {
    let host = host.to_ascii_lowercase();
    answer
        .get("cookies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|cookie| {
            let domain = cookie
                .get("domain")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim_start_matches('.')
                .to_ascii_lowercase();
            let cookie_path = cookie.get("path").and_then(Value::as_str).unwrap_or("/");
            !domain.is_empty()
                && (host == domain || host.ends_with(&format!(".{domain}")))
                && path.starts_with(cookie_path)
        })
        .filter_map(|cookie| {
            Some((
                cookie.get("name")?.as_str()?.to_owned(),
                cookie.get("value")?.as_str()?.to_owned(),
            ))
        })
        .collect()
}

#[cfg(test)]
#[path = "browser_session_tests.rs"]
pub(super) mod tests;
