//! The adapter against a local stand-in for the console.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use quota_core::ports::{ProviderAdapter, ProviderError, Secret};

use super::{TypesafeAdapter, wire};

/// The path the stand-in serves the overview at.
const OVERVIEW_PATH: &str = "/api/billing/overview";

/// A billing overview carrying everything the console sends, personal
/// details included, all fictional.
const ANSWER: &str = r#"{"billing":{"balance":37.2,"spent":12.8,"purchased":40,"freeCreditsRemaining":4.2,"resetsInDays":25,"cycleLabel":"October 2026","plan":"free_plan","autoPay":null,"billingAddress":{"city":null,"country":"ZZ","line1":"1 Private Lane","line2":null,"name":"Private Person","postal_code":null,"state":null},"billingAddressValid":true,"invoiceEmail":"private@example.test","paymentMethod":{"kind":"card","brand":"visa","expMonth":1,"expYear":2030,"last4":"4242"},"taxId":null,"credits":[{"id":"cr_1","amount":40,"remaining":33,"createdAt":"2026-09-01T00:00:00Z","expiresAt":"2099-09-01T00:00:00Z","reason":"purchased_credits"},{"id":"cr_2","amount":10,"remaining":4.2,"createdAt":"2026-10-01T00:00:00Z","expiresAt":"2099-10-31T00:00:00Z","reason":"free_tier_credit"}]},"payments":[{"id":"pay_1","amount":40,"createdAt":"2026-09-01T00:00:00Z","description":"Private invoice text","reason":"invoice_payment","status":"Pending","invoiceUrl":null,"metronomeInvoiceId":"inv_1"}],"hasMore":false,"credits":[]}"#;

/// How the stand-in answers.
#[derive(Clone, Copy)]
enum Console {
    /// Signed in.
    SignedIn,
    /// The session ended: the overview refuses it.
    Expired,
    /// A bot check answers every request.
    Challenge,
    /// The overview answers something that is not its JSON.
    Redesigned,
}

/// Serves the console until the test ends, and answers the overview's address.
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
            let mut cookie = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("cookie:") {
                    value.trim().clone_into(&mut cookie);
                }
            }
            let (status, headers, text) = answer(console, &request_line, &cookie, &renew);
            let _written = write!(
                stream,
                "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
        }
    });
    format!("{base}{OVERVIEW_PATH}")
}

fn answer(
    console: Console,
    request_line: &str,
    cookie: &str,
    renew: &AtomicBool,
) -> (&'static str, String, String) {
    let parts: Vec<&str> = request_line.split_whitespace().take(2).collect();
    if parts != ["GET", OVERVIEW_PATH] {
        return ("404 Not Found", String::new(), String::new());
    }
    match console {
        Console::Challenge => (
            "403 Forbidden",
            String::new(),
            "<html><title>Attention Required! | Cloudflare</title></html>".to_owned(),
        ),
        Console::Expired => (
            "401 Unauthorized",
            String::new(),
            r#"{"error":"Unauthorized","code":"unauthorized"}"#.to_owned(),
        ),
        Console::Redesigned => ("200 OK", String::new(), "<html></html>".to_owned()),
        Console::SignedIn if !cookie.contains("session=original") => (
            "401 Unauthorized",
            String::new(),
            r#"{"error":"Unauthorized","code":"unauthorized"}"#.to_owned(),
        ),
        Console::SignedIn => {
            let renewed = if renew.swap(false, Ordering::SeqCst) {
                "Set-Cookie: session=renewed; Path=/; HttpOnly\r\n".to_owned()
            } else {
                String::new()
            };
            ("200 OK", renewed, ANSWER.to_owned())
        }
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
async fn a_signed_in_session_reads_the_balance() {
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
async fn a_refused_session_is_an_ended_session() {
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

#[tokio::test]
async fn an_overview_quota_does_not_know_is_reported_not_guessed() {
    let adapter = adapter(Console::Redesigned, false);
    assert!(matches!(
        adapter.discover_with(&session()).await,
        Err(ProviderError::UnsupportedSchema { .. })
    ));
}

#[test]
fn nothing_personal_is_read_from_the_answer() {
    let overview: wire::Overview = serde_json::from_str(ANSWER).expect("decodes");
    let debug = format!("{overview:?}");
    for private in [
        "Private Person",
        "1 Private Lane",
        "private@example.test",
        "4242",
        "visa",
        "Private invoice text",
        "pay_1",
        "inv_1",
    ] {
        assert!(!debug.contains(private), "{private} was read");
    }
    let billing = overview.billing.expect("billing");
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
