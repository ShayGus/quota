//! The browser session against a stand-in `DevTools` endpoint, and the
//! stand-ins the website sign-in's own tests share.

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
                    (0..pages).map(|_| json!({ "type": "page", "url": "https://console.example.test/settings/billing" })).chain([json!({ "type": "service_worker" })]).collect::<Vec<_>>() }),
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
    let mut session =
        BrowserSession::connect(idle_child(), port, "/devtools/browser/test").expect("connected");
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
    let mut session =
        BrowserSession::connect(idle_child(), port, "/devtools/browser/test").expect("connected");
    assert!(matches!(
        session.cookies("console.example.test", "/"),
        Err(Ended::Closed)
    ));
    session.close();
    drop(server.join());
}
