//! The OAuth device authorization grant (RFC 8628), shared by every provider
//! Quota signs in to through the browser.
//!
//! The granted token is a credential Quota owns. It is kept as a small JSON
//! document, [`StoredToken`], in the system credential store, and refreshed by
//! Quota itself when the provider issues a refresh token. A verification page
//! is opened only when it is HTTPS on one of the provider's own hosts.

use chrono::{DateTime, Duration, Utc};
use quota_core::ports::{DeviceAuthorization, DevicePoll, ProviderError, Secret};
use serde::{Deserialize, Serialize};

use crate::http::{Answers, ProviderHttp};
use crate::post::{Body, PostRequest};

/// The longest a code may be waited for, whatever the provider says.
const MAX_WAIT_SECONDS: i64 = 900;

/// The shortest interval between polls, whatever the provider says.
const MIN_INTERVAL_SECONDS: u64 = 5;

/// One provider's device sign-in.
pub(crate) struct DeviceClient<'a> {
    /// Where a code is requested.
    pub(crate) device_url: &'a str,
    /// Where a code, or a refresh token, is exchanged for a token.
    pub(crate) token_url: &'a str,
    /// The public client the provider's own CLI signs in with.
    pub(crate) client_id: &'a str,
    /// The scopes requested, space-separated, when the provider wants any.
    pub(crate) scope: Option<&'a str>,
    /// Headers the provider requires on every sign-in request.
    pub(crate) headers: &'a [(&'a str, &'a str)],
    /// The hosts a verification page may be on, as suffixes.
    pub(crate) page_hosts: &'a [&'a str],
}

/// A granted token, as Quota keeps it.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct StoredToken {
    /// The bearer token.
    pub(crate) access_token: String,
    /// The refresh token, when the provider issued one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) refresh_token: Option<String>,
    /// When the access token expires, when the provider said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<DateTime<Utc>>,
}

impl StoredToken {
    /// Reads a stored token.
    pub(crate) fn parse(secret: &Secret) -> Result<Self, ProviderError> {
        serde_json::from_str(secret.expose()).map_err(|_| ProviderError::Authentication)
    }

    /// The token as a secret to store.
    pub(crate) fn to_secret(&self) -> Result<Secret, ProviderError> {
        serde_json::to_string(self)
            .map(Secret::new)
            .map_err(|_| ProviderError::InvalidData {
                detail: "the granted token could not be kept".to_owned(),
            })
    }

    /// Whether the access token expires within the next minute.
    pub(crate) fn expiring(&self) -> bool {
        self.expires_at
            .is_some_and(|expires_at| expires_at <= Utc::now() + Duration::seconds(60))
    }
}

/// What a device or token endpoint answers.
#[derive(Debug, Default, Deserialize)]
struct Answer {
    #[serde(default)]
    device_code: Option<String>,
    #[serde(default)]
    user_code: Option<String>,
    #[serde(default)]
    verification_uri: Option<String>,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    interval: Option<u64>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

impl DeviceClient<'_> {
    /// Requests a code for the person to enter.
    pub(crate) async fn begin(
        &self,
        http: &ProviderHttp,
    ) -> Result<DeviceAuthorization, ProviderError> {
        let mut form = vec![("client_id", self.client_id)];
        if let Some(scope) = self.scope {
            form.push(("scope", scope));
        }
        let answer = self
            .post(http, self.device_url, &form, Answers::Success)
            .await?;
        let refused = || ProviderError::UnsupportedSchema {
            detail: "the sign-in answer carried no code".to_owned(),
        };
        let page = answer.verification_uri.clone().ok_or_else(refused)?;
        if !self.trusted(&page) {
            return Err(refused());
        }
        let complete = answer
            .verification_uri_complete
            .filter(|complete| self.trusted(complete));
        Ok(DeviceAuthorization {
            user_code: answer.user_code.ok_or_else(refused)?,
            verification_uri: page,
            verification_uri_complete: complete,
            device_code: Secret::new(answer.device_code.ok_or_else(refused)?),
            interval_seconds: answer.interval.unwrap_or(5).max(MIN_INTERVAL_SECONDS),
            expires_at: Utc::now()
                + Duration::seconds(answer.expires_in.unwrap_or(600).clamp(60, MAX_WAIT_SECONDS)),
        })
    }

    /// Asks once whether the person has approved.
    pub(crate) async fn poll(
        &self,
        http: &ProviderHttp,
        authorization: &DeviceAuthorization,
    ) -> Result<DevicePoll, ProviderError> {
        if Utc::now() >= authorization.expires_at {
            return Ok(DevicePoll::Expired);
        }
        let form = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", authorization.device_code.expose()),
            ("client_id", self.client_id),
        ];
        let answer = self
            .post(http, self.token_url, &form, Answers::AnyJson)
            .await?;
        match answer.error.as_deref() {
            None => Ok(DevicePoll::Granted(granted(answer)?.to_secret()?)),
            Some("authorization_pending") => Ok(DevicePoll::Pending),
            Some("slow_down") => Ok(DevicePoll::SlowDown),
            Some("access_denied") => Ok(DevicePoll::Denied),
            Some("expired_token") => Ok(DevicePoll::Expired),
            Some(_) => Err(ProviderError::Authentication),
        }
    }

    /// Exchanges a refresh token for a fresh token. The provider may rotate
    /// the refresh token; the old one is kept only when it does not.
    pub(crate) async fn refresh(
        &self,
        http: &ProviderHttp,
        token: &StoredToken,
    ) -> Result<StoredToken, ProviderError> {
        let refresh = token
            .refresh_token
            .as_deref()
            .ok_or(ProviderError::Authentication)?;
        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", self.client_id),
        ];
        let answer = self
            .post(http, self.token_url, &form, Answers::AnyJson)
            .await?;
        if answer.error.is_some() {
            return Err(ProviderError::Authentication);
        }
        let mut fresh = granted(answer)?;
        if fresh.refresh_token.is_none() {
            fresh.refresh_token.clone_from(&token.refresh_token);
        }
        Ok(fresh)
    }

    async fn post(
        &self,
        http: &ProviderHttp,
        url: &str,
        form: &[(&str, &str)],
        answers: Answers,
    ) -> Result<Answer, ProviderError> {
        let mut headers = vec![("Accept", "application/json")];
        headers.extend_from_slice(self.headers);
        let reply = http
            .post(PostRequest {
                url,
                headers: &headers,
                body: Body::Form(form),
                deadline: None,
                answers,
            })
            .await?;
        serde_json::from_value(reply.body).map_err(|_| ProviderError::UnsupportedSchema {
            detail: "the sign-in answer did not match the OAuth shape".to_owned(),
        })
    }

    /// Whether a page is HTTPS on one of the provider's own hosts.
    fn trusted(&self, page: &str) -> bool {
        let Some(rest) = page.strip_prefix("https://") else {
            return false;
        };
        let host = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        !host.contains(['@', ':'])
            && self
                .page_hosts
                .iter()
                .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
    }
}

/// The token in a granting answer.
fn granted(answer: Answer) -> Result<StoredToken, ProviderError> {
    let access_token = answer
        .access_token
        .filter(|token| !token.trim().is_empty())
        .ok_or(ProviderError::Authentication)?;
    Ok(StoredToken {
        access_token,
        refresh_token: answer.refresh_token,
        expires_at: answer
            .expires_in
            .filter(|seconds| *seconds > 0)
            .map(|seconds| Utc::now() + Duration::seconds(seconds)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENT: DeviceClient<'static> = DeviceClient {
        device_url: "https://auth.example.test/device",
        token_url: "https://auth.example.test/token",
        client_id: "client",
        scope: None,
        headers: &[],
        page_hosts: &["example.test"],
    };

    #[test]
    fn only_https_pages_on_the_providers_own_hosts_are_opened() {
        assert!(CLIENT.trusted("https://example.test/device"));
        assert!(CLIENT.trusted("https://accounts.example.test/device?code=AB"));
        assert!(!CLIENT.trusted("http://example.test/device"));
        assert!(!CLIENT.trusted("https://example.test.evil.test/device"));
        assert!(!CLIENT.trusted("https://notexample.test/device"));
        assert!(!CLIENT.trusted("https://user@example.test/device"));
    }

    /// Answers each poll in turn from a local server, and reports what the
    /// client made of each answer.
    async fn polled(answers: &[(&'static str, &'static str)]) -> Vec<String> {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a local port");
        let token_url = format!(
            "http://{}/token",
            listener.local_addr().expect("an address")
        );
        let replies = answers.to_vec();
        let server = std::thread::spawn(move || {
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().expect("a poll");
                let mut request = [0; 4096];
                let _read = stream.read(&mut request);
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("an answer");
            }
        });
        let client = DeviceClient {
            token_url: &token_url,
            ..CLIENT
        };
        let http = ProviderHttp::new().expect("a client");
        let authorization = DeviceAuthorization {
            user_code: "CODE".to_owned(),
            verification_uri: "https://example.test/device".to_owned(),
            verification_uri_complete: None,
            device_code: Secret::new("device".to_owned()),
            interval_seconds: 5,
            expires_at: Utc::now() + Duration::seconds(600),
        };
        let mut seen = Vec::new();
        for _ in answers {
            seen.push(match client.poll(&http, &authorization).await {
                Ok(DevicePoll::Pending) => "pending".to_owned(),
                Ok(DevicePoll::SlowDown) => "slow down".to_owned(),
                Ok(DevicePoll::Denied) => "denied".to_owned(),
                Ok(DevicePoll::Expired) => "expired".to_owned(),
                Ok(DevicePoll::Granted(secret)) => StoredToken::parse(&secret)
                    .map_or_else(|_| "unreadable".to_owned(), |token| token.access_token),
                Err(error) => error.diagnostic_code().to_owned(),
            });
        }
        server.join().expect("the server finished");
        seen
    }

    #[tokio::test]
    async fn a_poll_reads_the_oauth_answer_including_a_refusal_body() {
        let seen = polled(&[
            ("400 Bad Request", r#"{"error":"authorization_pending"}"#),
            ("400 Bad Request", r#"{"error":"slow_down"}"#),
            ("400 Bad Request", r#"{"error":"access_denied"}"#),
            ("200 OK", r#"{"access_token":"granted","expires_in":3600}"#),
            (
                "503 Service Unavailable",
                r#"{"error":"temporarily_unavailable"}"#,
            ),
        ])
        .await;
        assert_eq!(
            seen,
            ["pending", "slow down", "denied", "granted", "transient"]
        );
    }

    #[test]
    fn a_stored_token_round_trips_and_knows_when_it_expires() {
        let token = StoredToken {
            access_token: "access".to_owned(),
            refresh_token: Some("refresh".to_owned()),
            expires_at: Some(Utc::now() + Duration::seconds(30)),
        };
        let kept = StoredToken::parse(&token.to_secret().expect("kept")).expect("read");
        assert_eq!(kept.access_token, "access");
        assert!(
            kept.expiring(),
            "thirty seconds is inside the refresh margin"
        );
    }
}
