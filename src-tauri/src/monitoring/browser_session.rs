//! A website sign-in in the person's own browser, in a profile Quota owns.
//!
//! Google refuses its sign-in inside an app's embedded window, so for a
//! console sign-in Quota opens the installed Chrome or Edge instead, with a
//! profile folder of Quota's own, never the person's usual one.
//!
//! While the person signs in, it is an ordinary browser: no `DevTools`
//! connection, and nothing reads it. Cloudflare's human check on the console
//! refused the sign-in again and again whenever anything was attached, even a
//! poller of the browser's page list, and let the same browser through when
//! nothing was. The person signs in and closes the window. Only then does
//! Quota open the same profile again without a window, with a `DevTools`
//! connection on the loopback address only, on a port the browser picks and
//! writes into the profile (`DevToolsActivePort`); it asks for the profile's
//! cookies for the provider's site and closes the browser.
//!
//! Quota never runs anything in the page, clicks, or types, and reads no file
//! the browser writes other than that port file and the profile's lock. The
//! profile stays, so the next sign-in usually needs one click; disconnecting
//! the account deletes it.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
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

/// Starts `browser` with `arguments` in the profile in `profile`.
fn spawn(browser: &Browser, profile: &Path, arguments: Vec<String>) -> io::Result<Child> {
    std::fs::create_dir_all(profile)?;
    // A port file left by an earlier run would name a port nobody listens on.
    drop(std::fs::remove_file(profile.join(ACTIVE_PORT_FILE)));
    Command::new(&browser.program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

/// How a sign-in in the person's browser ended.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Plain {
    /// The person closed the browser; its profile is free to read.
    Closed,
    Cancelled,
    TimedOut,
    /// The browser could not be started; Quota's own window is used.
    Failed,
}

/// Opens the sign-in in the person's browser as an ordinary browser, and
/// waits for the person to close it.
///
/// Nothing is connected to it and nothing reads it while the person signs
/// in: Cloudflare's human check refused the sign-in whenever a `DevTools`
/// client, or even a poller of the browser's page list, was attached, and let
/// the same browser through when nothing was.
pub(super) async fn sign_in(
    browser: &Browser,
    profile: &Path,
    url: &str,
    limit: Duration,
    cancelled: &mut watch::Receiver<bool>,
) -> Plain {
    let mut child = match spawn(browser, profile, plain_arguments(profile, url)) {
        Ok(child) => child,
        Err(error) => {
            warn(browser, &format!("the browser did not start: {error}"));
            return Plain::Failed;
        }
    };
    let started = tokio::time::Instant::now();
    loop {
        // The process can hand over to a browser already running in this
        // profile and end at once; the profile is free only once its lock is.
        if matches!(child.try_wait(), Ok(Some(_))) && !profile_in_use(profile) {
            return Plain::Closed;
        }
        if started.elapsed() > limit {
            stop(&mut child);
            return Plain::TimedOut;
        }
        tokio::select! {
            _ = cancelled.changed() => {
                stop(&mut child);
                return Plain::Cancelled;
            }
            () = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
    }
}

/// Whether a browser still holds the profile: Chrome keeps a `lockfile`
/// (Windows) or `SingletonLock` (Linux and macOS) in it while it runs.
fn profile_in_use(profile: &Path) -> bool {
    let lock = profile.join("lockfile");
    // On Windows the lock cannot be deleted while the browser holds it.
    if lock.exists() && std::fs::remove_file(&lock).is_err() {
        return true;
    }
    profile.join("SingletonLock").symlink_metadata().is_ok()
}

/// Reads the profile's cookies for `host` at `path`, once the person has
/// closed the browser: the profile is opened again without a window, read
/// through its `DevTools` connection, and closed.
pub(super) async fn read_cookies(
    browser: &Browser,
    profile: &Path,
    host: &str,
    path: &str,
) -> Option<Vec<(String, String)>> {
    let mut child = match spawn(browser, profile, headless_arguments(profile)) {
        Ok(child) => child,
        Err(error) => {
            warn(
                browser,
                &format!("the browser did not start to read the sign-in: {error}"),
            );
            return None;
        }
    };
    let (_keep, mut never) = watch::channel(false);
    let Some(Some((port, devtools))) = wait_for_port(profile, &mut never).await else {
        warn(
            browser,
            "the browser did not open its connection to read the sign-in",
        );
        stop(&mut child);
        return None;
    };
    let host = host.to_owned();
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut session = BrowserSession::connect(child, port, &devtools).ok()?;
        let cookies = session.cookies(&host, &path).ok();
        session.close();
        cookies
    })
    .await
    .ok()
    .flatten()
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

/// The ordinary browser the person signs in with: Quota's profile, none of
/// the first-run prompts, and no `DevTools` connection.
fn plain_arguments(profile: &Path, url: &str) -> Vec<String> {
    vec![
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".to_owned(),
        "--no-default-browser-check".to_owned(),
        "--new-window".to_owned(),
        url.to_owned(),
    ]
}

/// The windowless browser that reads the profile afterwards: a `DevTools`
/// connection on the loopback address only, on a port the browser picks.
fn headless_arguments(profile: &Path) -> Vec<String> {
    vec![
        format!("--user-data-dir={}", profile.display()),
        "--headless=new".to_owned(),
        "--remote-debugging-port=0".to_owned(),
        "--remote-debugging-address=127.0.0.1".to_owned(),
        "--no-first-run".to_owned(),
        "--no-default-browser-check".to_owned(),
        "about:blank".to_owned(),
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
    // Chrome keeps the connection open whatever the request says, so the
    // answer ends where its `Content-Length` says, not where the stream does.
    let mut answer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        if let Some(end) = answer.windows(4).position(|window| window == b"\r\n\r\n") {
            let head =
                String::from_utf8_lossy(answer.get(..end).unwrap_or_default()).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok());
            let body = length.and_then(|length| answer.get(end + 4..end + 4 + length));
            if let Some(body) = body {
                return serde_json::from_slice(body)
                    .map_err(|_| io::Error::other("the browser's page list was unreadable"));
            }
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(io::Error::other("the browser's page list ended early"));
        }
        answer.extend_from_slice(chunk.get(..read).unwrap_or_default());
    }
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
