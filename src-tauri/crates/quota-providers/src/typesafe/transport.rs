//! The requests the `TypeSafe` adapter sends to the console.
//!
//! The console is a website, not an API, so these differ from the shared JSON
//! transport: answers are read as text (HTML, scripts, and React Server
//! Components rows), and no redirect is ever followed, because the console
//! answers an expired session by redirecting to its sign-in page. No cookie
//! jar is kept: the only cookies sent are the session header Quota holds.
//! Nothing here logs a header, a body, or an address.

use std::time::Duration;

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use reqwest::redirect::Policy;

use crate::typesafe::page;

/// Identifies the client honestly. Quota never presents itself as a browser.
const USER_AGENT: &str = concat!("Quota/", env!("CARGO_PKG_VERSION"));

/// The longest one request may take.
const REQUEST_SECONDS: u64 = 8;

/// The longest establishing a connection may take.
const CONNECT_SECONDS: u64 = 5;

/// The largest answer read, in bytes: the console's scripts are large.
const MAX_TEXT_BYTES: usize = 6 * 1024 * 1024;

/// What one request returned, before any interpretation.
#[derive(Debug)]
pub(crate) struct TextReply {
    /// The status.
    pub(crate) status: u16,
    /// Whether the answer said the server action is unknown to this deployment.
    pub(crate) action_not_found: bool,
    /// Whether a bot-protection layer said it challenged the request.
    pub(crate) mitigated: bool,
    /// The provider's own retry deadline, when it sent one.
    pub(crate) retry_after: Option<DateTime<Utc>>,
    /// The cookies the answer set or cleared, as `Set-Cookie` values.
    pub(crate) set_cookies: Vec<String>,
    /// The body, as text.
    pub(crate) text: String,
}

/// Builds the console client: no redirects, no cookie jar, bounded.
pub(crate) fn client() -> Result<reqwest::Client, ProviderError> {
    crate::http::install_crypto_provider();
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(CONNECT_SECONDS))
        .timeout(Duration::from_secs(REQUEST_SECONDS))
        .build()
        .map_err(|_| ProviderError::Transient {
            detail: "the HTTP client could not be constructed".to_owned(),
        })
}

/// Sends one built request and reads its answer as text.
pub(crate) async fn send(builder: reqwest::RequestBuilder) -> Result<TextReply, ProviderError> {
    let mut response = builder.send().await.map_err(|error| {
        if error.is_timeout() || error.is_connect() || error.is_request() {
            ProviderError::Transient {
                detail: "the console request did not complete".to_owned(),
            }
        } else {
            ProviderError::Transient {
                detail: "the console request failed".to_owned(),
            }
        }
    })?;
    let status = response.status().as_u16();
    let headers = response.headers();
    let flag = |name: &str, value: &str| {
        headers
            .get(name)
            .and_then(|header| header.to_str().ok())
            .is_some_and(|header| header.trim().eq_ignore_ascii_case(value))
    };
    let action_not_found = flag("x-nextjs-action-not-found", "1");
    let mitigated = flag("cf-mitigated", "challenge") || flag("x-vercel-mitigated", "challenge");
    let retry_after = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .and_then(|seconds| chrono::Duration::try_seconds(seconds.max(0)))
        .and_then(|delay| Utc::now().checked_add_signed(delay));
    let set_cookies = headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok().map(str::to_owned))
        .collect();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_TEXT_BYTES as u64)
    {
        return Err(too_large());
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ProviderError::Transient {
            detail: "the console answer could not be read".to_owned(),
        })?
    {
        if body.len() + chunk.len() > MAX_TEXT_BYTES {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(TextReply {
        status,
        action_not_found,
        mitigated,
        retry_after,
        set_cookies,
        text: String::from_utf8_lossy(&body).into_owned(),
    })
}

/// What an answer means for a read: nothing, or the failure it is.
///
/// A challenge is told apart first, because a bot check also answers 403. A
/// redirect, a refusal, or the console's sign-in page means the session has
/// ended. The billing action may also be unknown to a new deployment, which
/// the caller handles before this.
///
/// # Errors
/// Returns the typed failure the answer stands for.
pub(crate) fn check(reply: &TextReply) -> Result<(), ProviderError> {
    if page::is_challenge(reply.status, reply.mitigated, &reply.text) {
        return Err(ProviderError::Blocked {
            detail: "the console answered with a bot check".to_owned(),
        });
    }
    match reply.status {
        200..=299 if page::is_login_landing(&reply.text) => Err(ProviderError::Authentication),
        200..=299 => Ok(()),
        300..=399 | 401 | 403 => Err(ProviderError::Authentication),
        429 => Err(ProviderError::RateLimited {
            retry_after: reply.retry_after,
        }),
        408 | 500..=599 => Err(ProviderError::Transient {
            detail: "the console reported a temporary failure".to_owned(),
        }),
        404 => Err(ProviderError::UnsupportedSchema {
            detail: "the console's billing page is not where it was".to_owned(),
        }),
        _ => Err(ProviderError::InvalidData {
            detail: "the console refused the request".to_owned(),
        }),
    }
}

fn too_large() -> ProviderError {
    ProviderError::InvalidData {
        detail: "the console answer exceeded the size limit".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(status: u16, text: &str) -> TextReply {
        TextReply {
            status,
            action_not_found: false,
            mitigated: false,
            retry_after: None,
            set_cookies: Vec::new(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn an_ended_session_is_authentication_whichever_way_it_shows() {
        for status in [302, 303, 307, 401, 403] {
            assert_eq!(
                check(&reply(status, "")),
                Err(ProviderError::Authentication)
            );
        }
        assert_eq!(
            check(&reply(200, r#"["(auth)",{"children":["login",{}]}]"#)),
            Err(ProviderError::Authentication)
        );
        assert_eq!(check(&reply(200, "1:{\"ok\":true}")), Ok(()));
    }

    #[test]
    fn a_bot_check_is_blocked_not_an_ended_session() {
        assert!(matches!(
            check(&reply(403, "<title>Just a moment...</title>")),
            Err(ProviderError::Blocked { .. })
        ));
        let mut mitigated = reply(200, "");
        mitigated.mitigated = true;
        assert!(matches!(
            check(&mitigated),
            Err(ProviderError::Blocked { .. })
        ));
    }

    #[test]
    fn server_faults_and_rate_limits_are_retried() {
        assert!(check(&reply(503, "busy")).is_err_and(|error| error.is_retryable()));
        assert!(matches!(
            check(&reply(429, "")),
            Err(ProviderError::RateLimited { .. })
        ));
    }

    #[test]
    fn the_client_builds_without_network_access() {
        client().expect("a client");
    }
}
