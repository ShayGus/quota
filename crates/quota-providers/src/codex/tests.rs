use std::io::{Read, Write};

use super::*;
use crate::credentials::SecretToken;

async fn read_endpoints(statuses: &[(&str, &str)]) -> Result<DecodedUsage, ProviderError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let replies: Vec<_> = statuses
        .iter()
        .map(|(status, body)| ((*status).to_owned(), (*body).to_owned()))
        .collect();
    let server = std::thread::spawn(move || {
        for (status, body) in replies {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            write!(stream, "HTTP/1.1 {status}\r\nRetry-After: 60\r\nx-codex-primary-used-percent: 99\r\nx-codex-secondary-used-percent: 99\r\nx-codex-credits-balance: 99\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    let credential = CodexCredential {
        token: SecretToken::new("synthetic-token".into()),
        account_id: None,
        profile_label: "codex-home-default".into(),
    };
    let primary = format!("{origin}/primary");
    let fallback = format!("{origin}/fallback");
    let result = CodexAdapter::new()
        .unwrap()
        .read_usage(
            &credential,
            &decode::pool_id(ProviderId::Codex, &credential.profile_label),
            ReadContext {
                attempt_id: quota_domain::ids::ConnectionAttemptId::generate(),
                deadline: None,
            },
            [&primary, &fallback],
        )
        .await;
    server.join().unwrap();
    result
}

#[tokio::test]
async fn decisive_errors_win_from_either_endpoint() {
    for status in ["401 Unauthorized", "403 Forbidden", "429 Too Many Requests"] {
        for fallback in [false, true] {
            let mut statuses = Vec::new();
            if fallback {
                statuses.push(("404 Not Found", "{}"));
            }
            statuses.push((status, "refused"));
            let before = Utc::now();
            let error = read_endpoints(&statuses).await.unwrap_err();
            match status {
                "401 Unauthorized" => assert_eq!(error, ProviderError::Authentication),
                "403 Forbidden" => assert_eq!(error, ProviderError::Authorization),
                _ => {
                    let ProviderError::RateLimited {
                        retry_after: Some(deadline),
                    } = error
                    else {
                        panic!("expected a rate limit with its deadline");
                    };
                    assert!(deadline >= before + chrono::Duration::seconds(60));
                }
            }
        }
    }
}

#[tokio::test]
async fn non_decisive_failures_keep_the_first_error() {
    let error = read_endpoints(&[("404 Not Found", "{}"), ("500 Internal Server Error", "{}")])
        .await
        .unwrap_err();
    assert!(matches!(error, ProviderError::UnsupportedSchema { .. }));
}

#[tokio::test]
async fn fallback_can_still_return_a_reading() {
    let usage = read_endpoints(&[
        ("404 Not Found", "{}"),
        ("200 OK", r#"{"credits":{"balance":12.5}}"#),
    ])
    .await
    .unwrap();
    assert!(usage.is_complete());
    assert_eq!(usage.windows.len(), 1);
}

#[tokio::test]
async fn live_reading_uses_only_body_measurements() {
    let usage = read_endpoints(&[(
        "200 OK",
        r#"{
        "rate_limit": {
            "primary_window": {"used_percent": 28, "limit_window_seconds": 18000},
            "secondary_window": {"used_percent": 40, "limit_window_seconds": 604800}
        },
        "credits": {"balance": 12.5}
    }"#,
    )])
    .await
    .unwrap();
    assert_eq!(usage.windows.len(), 3);
    assert_eq!(
        usage.windows[0]
            .measurement
            .remaining_percent()
            .unwrap()
            .value(),
        72.0
    );
    assert_eq!(
        usage.windows[1]
            .measurement
            .remaining_percent()
            .unwrap()
            .value(),
        60.0
    );
    let quota_domain::quota::measurement::Measurement::Quantity(balance) =
        &usage.windows[2].measurement
    else {
        panic!("expected a credit balance");
    };
    assert_eq!(balance.remaining, Some(12.5));
}
