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
fn the_person_signs_in_with_an_ordinary_browser_and_nothing_attached() {
    let args = plain_arguments(Path::new("/q/profile"), "https://example.test/login");
    assert!(args.contains(&"--user-data-dir=/q/profile".to_owned()));
    assert!(
        args.iter()
            .all(|arg| !arg.contains("remote-debugging") && !arg.contains("headless")),
        "the sign-in browser must have no DevTools connection: {args:?}"
    );
    assert_eq!(
        args.last().map(String::as_str),
        Some("https://example.test/login")
    );
}

#[test]
fn the_profile_is_read_afterwards_without_a_window_on_a_loopback_connection() {
    let args = headless_arguments(Path::new("/q/profile"));
    assert!(args.contains(&"--user-data-dir=/q/profile".to_owned()));
    assert!(args.contains(&"--headless=new".to_owned()));
    assert!(args.contains(&"--remote-debugging-address=127.0.0.1".to_owned()));
    assert!(args.contains(&"--remote-debugging-port=0".to_owned()));
}

#[test]
fn a_profile_without_a_browser_lock_is_free() {
    let profile = std::env::temp_dir().join(format!("quota-profile-lock-{}", std::process::id()));
    std::fs::create_dir_all(&profile).expect("a folder");
    assert!(!profile_in_use(&profile));
    // A lock nobody holds is a leftover, not a running browser.
    std::fs::write(profile.join("lockfile"), b"").expect("a lock");
    assert!(!profile_in_use(&profile));
    std::fs::remove_dir_all(&profile).expect("removed");
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
        let mut methods = Vec::new();
        // Like Chrome, the stand-in keeps a page-list connection open after
        // answering, so a reader that waits for the stream to end hangs.
        let mut kept_open = Vec::new();
        // Page lists arrive as plain requests; the one `DevTools` client, if
        // any, arrives as a websocket. The stand-in records both, in order.
        loop {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            // Wait until the request's first line has arrived before deciding.
            let mut head = [0_u8; 16];
            let mut seen = 0;
            for _ in 0..200 {
                seen = stream.peek(&mut head).unwrap_or(0);
                if seen == head.len() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if head[..seen].starts_with(b"GET /json/list") {
                // Read the whole request before answering, so closing the
                // connection never discards unread bytes.
                let mut request = Vec::new();
                let mut byte = [0_u8; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    match std::io::Read::read(&mut stream, &mut byte) {
                        Ok(1) => request.push(byte[0]),
                        _ => break,
                    }
                }
                let body = Value::Array(
                    (0..pages)
                        .map(|_| json!({ "type": "page", "url": "https://console.example.test/settings/billing" }))
                        .chain([json!({ "type": "service_worker", "url": "" })])
                        .collect(),
                )
                .to_string();
                drop(std::io::Write::write_all(
                    &mut stream,
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                ));
                methods.push("/json/list".to_owned());
                kept_open.push(stream);
                if pages == 0 && methods.len() > 1 {
                    // The window is closed; nothing more will be asked.
                    return methods;
                }
                continue;
            }
            let mut socket = tungstenite::accept(stream).expect("a websocket");
            while let Ok(message) = socket.read() {
                let Message::Text(text) = message else {
                    continue;
                };
                let request: Value = serde_json::from_str(text.as_str()).expect("JSON");
                let method = request["method"].as_str().unwrap_or_default().to_owned();
                let result = match method.as_str() {
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
                    return methods;
                }
            }
            break;
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
        [
            "/json/list",
            "/json/list",
            "Storage.getCookies",
            "Browser.close"
        ]
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
