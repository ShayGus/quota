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
            let _read = stream.read(&mut request);
            write!(stream, "HTTP/1.1 {status}\r\nRetry-After: 60\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
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

/// The `wham/usage` body as the endpoint returned it in October 2026, with
/// synthetic values: several fields arrive as an explicit `null`.
const CURRENT_USAGE_BODY: &str = r#"{
    "user_id": "user-synthetic",
    "account_id": "account-synthetic",
    "email": "someone@example.test",
    "plan_type": "plus",
    "rate_limit": {
        "allowed": true,
        "limit_reached": false,
        "primary_window": {
            "used_percent": 37,
            "limit_window_seconds": 604800,
            "reset_after_seconds": 3600,
            "reset_at": 1790000000
        },
        "secondary_window": null
    },
    "code_review_rate_limit": null,
    "additional_rate_limits": null,
    "model_usage": {"model-x": {"available": true, "available_at": null, "credits_would_enable": false}},
    "credits": {
        "has_credits": false,
        "unlimited": false,
        "overage_limit_reached": false,
        "balance": "0",
        "approx_local_messages": [0, 0],
        "approx_cloud_messages": [0, 0]
    },
    "spend_control": {"reached": false, "individual_limit": null},
    "rate_limit_reached_type": null,
    "promo": null,
    "rate_limit_reset_credits": {"available_count": 0, "applicable_available_count": 0}
}"#;

#[tokio::test]
async fn the_current_usage_body_is_read() {
    // Only the first endpoint is asked: a readable body needs no fallback.
    let usage = read_endpoints(&[("200 OK", CURRENT_USAGE_BODY)])
        .await
        .unwrap_or_else(|error| panic!("the read failed: {error:?}"));
    assert!(usage.is_complete());
    let weekly = usage
        .windows
        .iter()
        .find(|window| window.category == quota_domain::quota::window::QuotaCategory::Weekly)
        .expect("the weekly window is read");
    assert_eq!(
        weekly
            .measurement
            .remaining_percent()
            .map(|p| p.value().round()),
        Some(63.0)
    );
    assert_eq!(usage.plan_label.as_deref(), Some("plus"));
}

#[tokio::test]
async fn a_blocked_fallback_does_not_hide_why_an_accepted_body_failed() {
    // The first endpoint accepted the credential but sent an unreadable body;
    // the fallback's 403 page must not turn that into an authorization failure.
    let error = read_endpoints(&[
        ("200 OK", r#"{"rate_limit": "not an object"}"#),
        ("403 Forbidden", "<html>blocked</html>"),
    ])
    .await
    .unwrap_err();
    assert!(
        matches!(error, ProviderError::UnsupportedSchema { .. }),
        "{error:?}"
    );
}
