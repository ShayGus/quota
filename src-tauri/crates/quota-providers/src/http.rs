//! The bounded HTTP boundary every real provider adapter shares.
//!
//! One client per adapter, so connection pooling works and policy is uniform:
//! per-request credentials, a connect/read/overall deadline, a response-size
//! limit, and a redirect rule that refuses to carry a credential to another
//! origin. Nothing here logs a header, a body, a token, or a full URL.

use std::sync::LazyLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use reqwest::StatusCode;
use reqwest::redirect::Policy;

/// The largest response body this crate will read, in bytes.
pub(crate) const MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// The longest a single remote request may take.
const REQUEST_DEADLINE_SECONDS: u64 = 10;

/// How long establishing the connection may take.
const CONNECT_DEADLINE_SECONDS: u64 = 5;

/// The largest redirect chain a request may follow.
const MAX_REDIRECTS: usize = 5;

/// One GET request at the transport boundary.
#[derive(Clone, Copy)]
pub(crate) struct GetRequest<'a> {
    /// The absolute URL. Never logged.
    pub(crate) url: &'a str,
    /// Request headers, including the credential when the provider needs one.
    pub(crate) headers: &'a [(&'a str, &'a str)],
    /// The absolute deadline for the whole read, when the scheduler gave one.
    pub(crate) deadline: Option<DateTime<Utc>>,
}

/// What a completed request returned, before any provider interpretation.
pub(crate) struct HttpReply {
    /// The response status.
    pub(crate) status: StatusCode,
    /// The provider's own `Retry-After` deadline, when it sent one.
    pub(crate) retry_after: Option<DateTime<Utc>>,
    /// The response body, decoded as JSON.
    pub(crate) body: serde_json::Value,
}

/// A client configured for one provider family.
///
/// The client holds no credential, so its `Debug` output names only the type.
#[derive(Debug)]
pub(crate) struct ProviderHttp {
    client: reqwest::Client,
}

impl ProviderHttp {
    /// Builds the client, installing the process-wide crypto provider once.
    ///
    /// The build performs no network access.
    pub(crate) fn new() -> Result<Self, ProviderError> {
        LazyLock::force(&CRYPTO_PROVIDER);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_DEADLINE_SECONDS))
            .timeout(Duration::from_secs(REQUEST_DEADLINE_SECONDS))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    return attempt.stop();
                }
                // A credential never travels to a new origin: only a redirect
                // that stays on the origin of the first request is followed.
                let origin = attempt.previous().first().map(reqwest::Url::origin);
                match origin {
                    Some(origin) if origin == attempt.url().origin() => attempt.follow(),
                    _ => attempt.stop(),
                }
            }))
            .build()
            .map_err(|_| ProviderError::Transient {
                detail: "the HTTP client could not be constructed".to_owned(),
            })?;
        Ok(Self { client })
    }

    /// Performs one bounded GET and decodes the JSON body.
    pub(crate) async fn get(&self, request: GetRequest<'_>) -> Result<HttpReply, ProviderError> {
        let builder = self.client.get(request.url);
        send(builder, request.headers, request.deadline, Answers::Success).await
    }

    /// The pooled client, for the other request shapes.
    pub(crate) const fn client(&self) -> &reqwest::Client {
        &self.client
    }
}

/// Which answers a request accepts.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answers {
    /// Only a success; any other status becomes its typed failure.
    Success,
    /// Any JSON answer, including a refusal's, for a protocol such as OAuth
    /// that explains a refusal in the body. Only a server fault or a rate
    /// limit still becomes a failure.
    AnyJson,
}

/// Sends one built request within its deadline and decodes the JSON body.
pub(crate) async fn send(
    mut builder: reqwest::RequestBuilder,
    headers: &[(&str, &str)],
    deadline: Option<DateTime<Utc>>,
    answers: Answers,
) -> Result<HttpReply, ProviderError> {
    if let Some(remaining) = remaining_timeout(deadline) {
        builder = builder.timeout(remaining);
    }
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = builder
        .send()
        .await
        .map_err(|error| transport_error(&error))?;
    let status = response.status();
    let retry_after = retry_after(&response);
    if let Some(error) = classify_status(status, retry_after.as_ref().ok().copied().flatten()) {
        let explained = answers == Answers::AnyJson
            && !matches!(
                error,
                ProviderError::Transient { .. } | ProviderError::RateLimited { .. }
            );
        if !explained {
            return Err(error);
        }
    }
    let retry_after = retry_after?;
    let body = read_body(response).await?;
    let text = String::from_utf8(body).map_err(|_| ProviderError::InvalidData {
        detail: "the provider body was not text".to_owned(),
    })?;
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if !trimmed.starts_with('{') {
        // An HTML error page, a plain-text refusal, or a truncated body.
        return Err(ProviderError::InvalidData {
            detail: "the provider body was not a JSON object".to_owned(),
        });
    }
    let body = serde_json::from_str(trimmed).map_err(|_| ProviderError::InvalidData {
        detail: "the provider body was not valid JSON".to_owned(),
    })?;
    Ok(HttpReply {
        status,
        retry_after,
        body,
    })
}

/// Installs the process-wide rustls crypto provider once, on first use.
///
/// A second install of a different provider fails harmlessly and leaves the
/// first one in place.
static CRYPTO_PROVIDER: LazyLock<()> = LazyLock::new(|| {
    // Installing a second provider fails by design and leaves the first one in
    // place, so the outcome carries no information for the caller.
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a second provider install is the documented no-op"
    )]
    let _ = rustls::crypto::ring::default_provider().install_default();
});

/// The deadline remaining before the read must stop, or the default.
fn remaining_timeout(deadline: Option<DateTime<Utc>>) -> Option<Duration> {
    let deadline = deadline?;
    let remaining = deadline.signed_duration_since(Utc::now());
    if remaining <= chrono::Duration::zero() {
        return Some(Duration::from_millis(1));
    }
    let millis = u64::try_from(remaining.num_milliseconds()).ok()?;
    Some(Duration::from_millis(millis))
}

/// Reads the body with a hard size ceiling.
async fn read_body(mut response: reqwest::Response) -> Result<Vec<u8>, ProviderError> {
    if let Some(length) = response.content_length()
        && length > MAX_RESPONSE_BYTES as u64
    {
        return Err(too_large());
    }
    let mut collected: Vec<u8> = Vec::new();
    loop {
        let chunk = response
            .chunk()
            .await
            .map_err(|error| transport_error(&error))?;
        let Some(chunk) = chunk else {
            return Ok(collected);
        };
        if collected.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(too_large());
        }
        collected.extend_from_slice(&chunk);
    }
}

/// The typed failure for an oversized response.
fn too_large() -> ProviderError {
    ProviderError::InvalidData {
        detail: "the provider response exceeded the size limit".to_owned(),
    }
}

/// The provider's own retry deadline, when it sent one.
fn retry_after(response: &reqwest::Response) -> Result<Option<DateTime<Utc>>, ProviderError> {
    let invalid = || ProviderError::InvalidData {
        detail: "the provider retry deadline was outside the supported range".to_owned(),
    };
    let Some(value) = response.headers().get(reqwest::header::RETRY_AFTER) else {
        return Ok(None);
    };
    let text = value.to_str().map_err(|_| invalid())?.trim();
    if let Ok(seconds) = text.parse::<i64>() {
        return chrono::Duration::try_seconds(seconds.max(0))
            .and_then(|delay| Utc::now().checked_add_signed(delay))
            .map(Some)
            .ok_or_else(invalid);
    }
    DateTime::parse_from_rfc2822(text)
        .map(|parsed| Some(parsed.with_timezone(&Utc)))
        .map_err(|_| invalid())
}

/// Classifies a transport failure without inspecting message text.
fn transport_error(error: &reqwest::Error) -> ProviderError {
    if error.is_timeout() || error.is_connect() || error.is_request() {
        return ProviderError::Transient {
            detail: "the provider request did not complete".to_owned(),
        };
    }
    if error.is_decode() || error.is_body() {
        return ProviderError::InvalidData {
            detail: "the provider response could not be read".to_owned(),
        };
    }
    ProviderError::Transient {
        detail: "the provider request failed".to_owned(),
    }
}

/// Maps a non-success status onto the retry classification the scheduler uses.
///
/// Every real provider in this crate classifies the same way: a credential
/// problem is never retried, a refusal is not retried, and only a rate limit or
/// a server-side fault is.
pub(crate) fn classify_status(
    status: StatusCode,
    retry_after: Option<DateTime<Utc>>,
) -> Option<ProviderError> {
    if status.is_success() {
        return None;
    }
    Some(match status.as_u16() {
        401 => ProviderError::Authentication,
        403 => ProviderError::Authorization,
        429 => ProviderError::RateLimited { retry_after },
        408 | 500..=599 => ProviderError::Transient {
            detail: "the provider reported a temporary failure".to_owned(),
        },
        404 => ProviderError::UnsupportedSchema {
            detail: "the endpoint is not available".to_owned(),
        },
        _ => ProviderError::InvalidData {
            detail: "the provider refused the request".to_owned(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_map_to_retry_classification() {
        assert!(classify_status(StatusCode::OK, None).is_none());
        assert_eq!(
            classify_status(StatusCode::UNAUTHORIZED, None),
            Some(ProviderError::Authentication)
        );
        assert_eq!(
            classify_status(StatusCode::FORBIDDEN, None),
            Some(ProviderError::Authorization)
        );
        let limited = classify_status(StatusCode::TOO_MANY_REQUESTS, None);
        assert!(matches!(limited, Some(ProviderError::RateLimited { .. })));
        assert!(
            classify_status(StatusCode::INTERNAL_SERVER_ERROR, None)
                .is_some_and(|error| error.is_retryable())
        );
    }

    #[test]
    fn a_rate_limit_keeps_the_provider_deadline() {
        let deadline = Utc::now() + chrono::Duration::seconds(30);
        let classified = classify_status(StatusCode::TOO_MANY_REQUESTS, Some(deadline));
        assert_eq!(
            classified,
            Some(ProviderError::RateLimited {
                retry_after: Some(deadline)
            })
        );
    }

    #[test]
    fn a_client_builds_without_network_access() {
        ProviderHttp::new().unwrap();
    }
    #[tokio::test]
    async fn overflowing_retry_headers_return_typed_failures_without_losing_refusals() {
        use std::io::{Read, Write};
        for value in ["9223372036854775807", "9007199254740991"] {
            for status in ["200 OK", "401 Unauthorized", "429 Too Many Requests"] {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let url = format!("http://{}/usage", listener.local_addr().unwrap());
                let server = std::thread::spawn(move || {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut request = [0; 4096];
                    let _read = stream.read(&mut request);
                    write!(stream, "HTTP/1.1 {status}\r\nRetry-After: {value}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
                });
                let result = ProviderHttp::new()
                    .unwrap()
                    .get(GetRequest {
                        url: &url,
                        headers: &[],
                        deadline: None,
                    })
                    .await;
                match status {
                    "200 OK" => assert!(matches!(result, Err(ProviderError::InvalidData { .. }))),
                    "401 Unauthorized" => {
                        assert!(matches!(result, Err(ProviderError::Authentication)));
                    }
                    _ => assert!(matches!(
                        result,
                        Err(ProviderError::RateLimited { retry_after: None })
                    )),
                }
                server.join().unwrap();
            }
        }
    }

    #[tokio::test]
    async fn non_json_refusals_keep_their_status_and_retry_deadline() {
        use std::io::{Read, Write};
        for (status, body) in [
            ("429 Too Many Requests", "<html>wait</html>"),
            ("401 Unauthorized", ""),
            ("403 Forbidden", "refused"),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/usage", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let _read = stream.read(&mut request);
                write!(stream, "HTTP/1.1 {status}\r\nRetry-After: 60\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            let before = Utc::now();
            let error = ProviderHttp::new()
                .unwrap()
                .get(GetRequest {
                    url: &url,
                    headers: &[],
                    deadline: None,
                })
                .await
                .err()
                .unwrap();
            match status {
                "429 Too Many Requests" => {
                    let ProviderError::RateLimited {
                        retry_after: Some(at),
                    } = error
                    else {
                        panic!("expected rate limit");
                    };
                    assert!(at >= before + chrono::Duration::seconds(60));
                }
                "401 Unauthorized" => assert_eq!(error, ProviderError::Authentication),
                _ => assert_eq!(error, ProviderError::Authorization),
            }
            server.join().unwrap();
        }
    }
}
