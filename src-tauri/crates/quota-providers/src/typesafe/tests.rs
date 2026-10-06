//! The adapter against a local stand-in for the console.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use quota_core::ports::{ProviderAdapter, ProviderError, Secret};

use super::{TypesafeAdapter, wire};

/// The action identifier the stand-in's current deployment names.
const CURRENT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// An identifier from an earlier deployment.
const OLD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// A billing answer carrying everything the console sends, personal details
/// included, all fictional.
const ANSWER: &str = r#"0:{"a":"$@1","f":"","b":"build"}
1:{"ok":true,"data":{"billing":{"balance":37.2,"spent":12.8,"purchased":40,"freeCreditsRemaining":4.2,"resetsInDays":25,"cycleLabel":"October 2026","plan":"free_plan","autoPay":null,"billingAddress":{"city":null,"country":"ZZ","line1":"1 Private Lane","line2":null,"name":"Private Person","postal_code":null,"state":null},"billingAddressValid":true,"invoiceEmail":"private@example.test","paymentMethod":{"brand":"visa","expMonth":1,"expYear":2030,"last4":"4242"},"taxId":null,"credits":[{"id":"cr_1","amount":40,"remaining":33,"createdAt":"2026-09-01T00:00:00Z","expiresAt":"2099-09-01T00:00:00Z","reason":"purchased_credits"},{"id":"cr_2","amount":10,"remaining":4.2,"createdAt":"2026-10-01T00:00:00Z","expiresAt":"2099-10-31T00:00:00Z","reason":"free_tier_credit"}]},"payments":[{"id":"pay_1","amount":40,"createdAt":"2026-09-01T00:00:00Z","description":"Private invoice text","reason":"invoice_payment","status":"Pending","invoiceUrl":null}],"credits":"$undefined","hasMore":false}}
"#;

/// How the stand-in answers.
#[derive(Clone, Copy)]
enum Console {
    /// Signed in, on the current deployment.
    SignedIn,
    /// The session ended: the billing page redirects to the sign-in.
    Expired,
    /// A bot check answers every request.
    Challenge,
}

/// Serves the console until the test ends, and answers its address.
fn serve(console: Console, renew: Arc<AtomicBool>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let base = format!("http://{}", listener.local_addr().expect("an address"));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().expect("a stream"));
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let mut length = 0;
            let mut action = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
                if let Some(value) = lower.strip_prefix("next-action:") {
                    value.trim().clone_into(&mut action);
                }
            }
            let mut body = vec![0; length];
            let _read = reader.read_exact(&mut body);
            let (status, headers, text) = answer(console, &request_line, &action, &renew);
            let _written = write!(
                stream,
                "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
        }
    });
    format!("{base}/settings/billing")
}

fn answer(
    console: Console,
    request_line: &str,
    action: &str,
    renew: &AtomicBool,
) -> (&'static str, String, String) {
    if matches!(console, Console::Challenge) {
        return (
            "403 Forbidden",
            String::new(),
            "<html><title>Just a moment...</title></html>".to_owned(),
        );
    }
    let cookie = if renew.swap(false, Ordering::SeqCst) {
        "Set-Cookie: session=renewed; Path=/; HttpOnly\r\n".to_owned()
    } else {
        String::new()
    };
    let parts: Vec<&str> = request_line.split_whitespace().take(2).collect();
    match (console, parts.as_slice()) {
        (Console::Expired, _) => (
            "307 Temporary Redirect",
            "Location: /login\r\n".to_owned(),
            String::new(),
        ),
        (_, ["GET", "/settings/billing"]) => (
            "200 OK",
            cookie,
            r#"<html><script src="/_next/a.js"></script><script src="/_next/b.js"></script></html>"#
                .to_owned(),
        ),
        (_, ["GET", "/_next/a.js"]) => ("200 OK", String::new(), "console.log(1)".to_owned()),
        (_, ["GET", "/_next/b.js"]) => (
            "200 OK",
            String::new(),
            format!(r#"(0,s.createServerReference)("{CURRENT}",s.callServer,void 0,s.findSourceMapURL,"getBillingOverviewResult")"#),
        ),
        (_, ["POST", "/settings/billing"]) if action == CURRENT => {
            ("200 OK", cookie, ANSWER.to_owned())
        }
        (_, ["POST", "/settings/billing"]) => (
            "404 Not Found",
            "x-nextjs-action-not-found: 1\r\n".to_owned(),
            String::new(),
        ),
        _ => ("404 Not Found", String::new(), String::new()),
    }
}

fn adapter(console: Console, renew: bool) -> TypesafeAdapter {
    let url = serve(console, Arc::new(AtomicBool::new(renew)));
    TypesafeAdapter::at(crate::secrets::unavailable(), url).expect("an adapter")
}

fn session() -> Secret {
    Secret::new("session=original; theme=dark".to_owned())
}

#[tokio::test]
async fn a_signed_in_session_finds_the_action_and_reads_the_balance() {
    let adapter = adapter(Console::SignedIn, true);
    let (billing, renewed) = adapter.billing(&session()).await.expect("billing");
    assert_eq!(billing.credits.len(), 2);
    assert_eq!(
        renewed.as_deref(),
        Some("session=renewed; theme=dark"),
        "a renewed cookie replaces the old one"
    );
    let accounts = adapter.discover_with(&session()).await.expect("an account");
    assert_eq!(accounts[0].identity.principal_label, "TypeSafe console");
    assert_eq!(accounts[0].identity.plan_label.as_deref(), Some("Free"));
}

#[tokio::test]
async fn an_action_from_an_earlier_deployment_is_looked_up_again_once() {
    let adapter = adapter(Console::SignedIn, false);
    adapter.remember_action(Some(OLD.to_owned()));
    adapter
        .billing(&session())
        .await
        .expect("billing after rediscovery");
    assert_eq!(adapter.cached_action().as_deref(), Some(CURRENT));
}

#[tokio::test]
async fn a_redirect_to_the_sign_in_is_an_ended_session() {
    let adapter = adapter(Console::Expired, false);
    assert_eq!(
        adapter.discover_with(&session()).await.err(),
        Some(ProviderError::Authentication)
    );
}

#[tokio::test]
async fn a_bot_check_is_reported_and_never_passed() {
    let adapter = adapter(Console::Challenge, false);
    assert!(matches!(
        adapter.discover_with(&session()).await,
        Err(ProviderError::Blocked { .. })
    ));
}

#[test]
fn nothing_personal_is_read_from_the_answer() {
    let value = super::page::action_result(ANSWER).expect("a result");
    let result: wire::BillingResult = serde_json::from_value(value).expect("decodes");
    let debug = format!("{result:?}");
    for private in [
        "Private Person",
        "1 Private Lane",
        "private@example.test",
        "4242",
        "visa",
        "Private invoice text",
        "pay_1",
    ] {
        assert!(!debug.contains(private), "{private} was read");
    }
    let billing = result.data.and_then(|data| data.billing).expect("billing");
    assert!(billing.balance.is_some());
}

#[test]
fn the_adapter_offers_its_console_sign_in() {
    let adapter = TypesafeAdapter::new(crate::secrets::unavailable()).expect("an adapter");
    let console = adapter.console_sign_in().expect("a console sign-in");
    assert!(
        console
            .sign_in_url
            .starts_with("https://console.typesafe.ai/login")
    );
    assert_eq!(console.cookie_url, "https://console.typesafe.ai/");
}
