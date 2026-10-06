//! A website sign-in in the person's own browser, in a profile Quota owns.
//!
//! Google refuses its sign-in inside an app's embedded window, so for a
//! console sign-in Quota opens the installed Chrome or Edge instead, with a
//! profile folder of Quota's own, never the person's usual one. The browser is
//! started with its `DevTools` connection on the loopback address only, on a
//! port the browser picks and writes into the profile (`DevToolsActivePort`).
//! Through that connection Quota does two things only: asks for the profile's
//! cookies for the provider's site, and closes the browser once they work.
//! Quota never runs anything in the page, clicks, or types, and reads no file
//! the browser writes other than that port file. The profile stays, so the
//! next sign-in usually needs one click; disconnecting the account deletes it.

use std::io;
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

/// The browser Quota started and the connection to it.
pub(super) struct BrowserSession {
    child: Child,
    socket: WebSocket<TcpStream>,
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
        match open_socket(port, path) {
            Ok(socket) => Ok(Self {
                child,
                socket,
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
        self.socket
            .send(Message::text(request.to_string()))
            .map_err(|_| Ended::Closed)?;
        let started = Instant::now();
        while started.elapsed() < ANSWER_LIMIT {
            let message = self.socket.read().map_err(|_| Ended::Closed)?;
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

    /// The profile's cookies for `host` and the page path `path`, as name and
    /// value pairs; `Ended::Closed` once no window of the profile is open.
    pub(super) fn cookies(
        &mut self,
        host: &str,
        path: &str,
    ) -> Result<Vec<(String, String)>, Ended> {
        let targets = self.call("Target.getTargets", &json!({}))?;
        if open_pages(&targets) == 0 {
            return Err(Ended::Closed);
        }
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

/// How many pages the browser has open.
fn open_pages(targets: &Value) -> usize {
    targets
        .get("targetInfos")
        .and_then(Value::as_array)
        .map_or(0, |targets| {
            targets
                .iter()
                .filter(|target| target.get("type").and_then(Value::as_str) == Some("page"))
                .count()
        })
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
pub(super) mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn the_port_file_names_a_port_and_a_browser_path() {
        assert_eq!(
            parse_active_port("51234\n/devtools/browser/0b8c-41a9_x\n"),
            Some((51234, "/devtools/browser/0b8c-41a9_x".to_owned()))
        );
        for bad in [
            "",
            "0\n/devtools/browser/a",
            "70000\n/devtools/browser/a",
            "123",
            "123\n/elsewhere",
            "123\n/devtools/browser/a?b",
        ] {
            assert_eq!(parse_active_port(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_browser_gets_its_own_profile_and_a_loopback_connection() {
        let args = arguments(Path::new("/q/profile"), "https://example.test/login");
        assert!(args.contains(&"--user-data-dir=/q/profile".to_owned()));
        assert!(args.contains(&"--remote-debugging-address=127.0.0.1".to_owned()));
        assert!(args.contains(&"--remote-debugging-port=0".to_owned()));
        assert_eq!(
            args.last().map(String::as_str),
            Some("https://example.test/login")
        );
    }

    #[test]
    fn only_the_sites_cookies_for_the_page_are_kept() {
        let answer = json!({ "cookies": [
            { "name": "session", "value": "s1", "domain": "console.example.test", "path": "/" },
            { "name": "shared", "value": "s2", "domain": ".example.test", "path": "/" },
            { "name": "deep", "value": "s3", "domain": "console.example.test", "path": "/settings" },
            { "name": "other_path", "value": "x", "domain": "console.example.test", "path": "/api" },
            { "name": "elsewhere", "value": "x", "domain": "accounts.google.test", "path": "/" },
            { "name": "lookalike", "value": "x", "domain": "notexample.test", "path": "/" }
        ]});
        let kept = cookies_for(&answer, "console.example.test", "/settings/billing");
        let names: Vec<&str> = kept.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["session", "shared", "deep"]);
    }

    /// A stand-in browser: answers `DevTools` commands over a local websocket
    /// the way the real one does, with an event in between.
    pub(in crate::monitoring) fn fake_browser(
        pages: usize,
    ) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("a connection");
            let mut socket = tungstenite::accept(stream).expect("a websocket");
            let mut methods = Vec::new();
            while let Ok(message) = socket.read() {
                let Message::Text(text) = message else {
                    continue;
                };
                let request: Value = serde_json::from_str(text.as_str()).expect("JSON");
                let method = request["method"].as_str().unwrap_or_default().to_owned();
                let result = match method.as_str() {
                    "Target.getTargets" => json!({ "targetInfos":
                        (0..pages).map(|_| json!({ "type": "page" })).chain([json!({ "type": "service_worker" })]).collect::<Vec<_>>() }),
                    "Storage.getCookies" => json!({ "cookies": [
                        { "name": "session", "value": "fictional", "domain": "console.example.test", "path": "/" }
                    ]}),
                    _ => json!({}),
                };
                drop(socket.send(Message::text(json!({ "method": "Page.event" }).to_string())));
                drop(socket.send(Message::text(
                    json!({ "id": request["id"], "result": result }).to_string(),
                )));
                methods.push(method.clone());
                if method == "Browser.close" {
                    break;
                }
            }
            methods
        });
        (port, server)
    }

    /// A process that runs until it is stopped, standing in for the browser.
    pub(in crate::monitoring) fn idle_child() -> Child {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("ping");
            command.args(["-n", "30", "127.0.0.1"]);
            command
        } else {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        };
        command
            .stdout(Stdio::null())
            .spawn()
            .expect("a stand-in process")
    }

    #[test]
    fn the_session_reads_the_sites_cookies_then_closes_the_browser() {
        let (port, server) = fake_browser(1);
        let mut session = BrowserSession::connect(idle_child(), port, "/devtools/browser/test")
            .expect("connected");
        let cookies = session
            .cookies("console.example.test", "/settings/billing")
            .expect("cookies");
        assert_eq!(cookies, [("session".to_owned(), "fictional".to_owned())]);
        session.close();
        assert_eq!(
            server.join().expect("the stand-in"),
            ["Target.getTargets", "Storage.getCookies", "Browser.close"]
        );
    }

    #[test]
    fn no_open_window_means_the_person_closed_it() {
        let (port, server) = fake_browser(0);
        let mut session = BrowserSession::connect(idle_child(), port, "/devtools/browser/test")
            .expect("connected");
        assert!(matches!(
            session.cookies("console.example.test", "/"),
            Err(Ended::Closed)
        ));
        session.close();
        drop(server.join());
    }
}
