//! POST requests at the same bounded HTTP boundary as every GET.
//!
//! A device sign-in posts a form, and Muse Code's usage is read with a JSON
//! post. Both bodies are encoded here rather than through optional `reqwest`
//! features, so the transport keeps the dependency set the release audit
//! approved.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;

use crate::http::{Answers, HttpReply, ProviderHttp, endpoint, send};

/// A request body.
#[derive(Clone, Copy)]
pub(crate) enum Body<'a> {
    /// `application/x-www-form-urlencoded` pairs.
    Form(&'a [(&'a str, &'a str)]),
    /// A JSON document.
    Json(&'a serde_json::Value),
}

/// One POST request.
#[derive(Clone, Copy)]
pub(crate) struct PostRequest<'a> {
    /// The absolute URL. Never logged.
    pub(crate) url: &'a str,
    /// Request headers, including the credential when the provider needs one.
    pub(crate) headers: &'a [(&'a str, &'a str)],
    /// The body.
    pub(crate) body: Body<'a>,
    /// The absolute deadline for the whole request, when one applies.
    pub(crate) deadline: Option<DateTime<Utc>>,
    /// Which answers are accepted.
    pub(crate) answers: Answers,
}

impl ProviderHttp {
    /// Performs one bounded POST and decodes the JSON body.
    pub(crate) async fn post(&self, request: PostRequest<'_>) -> Result<HttpReply, ProviderError> {
        let (content_type, bytes) = match request.body {
            Body::Form(pairs) => (
                "application/x-www-form-urlencoded",
                form(pairs).into_bytes(),
            ),
            Body::Json(document) => (
                "application/json",
                serde_json::to_vec(document).map_err(|_| ProviderError::InvalidData {
                    detail: "the request body could not be encoded".to_owned(),
                })?,
            ),
        };
        let builder = self
            .client()
            .post(endpoint(request.url))
            .header("Content-Type", content_type)
            .body(bytes);
        send(builder, request.headers, request.deadline, request.answers).await
    }
}

/// Form-encodes name and value pairs.
fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-encodes everything but the unreserved characters of RFC 3986.
fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            for nibble in [byte >> 4, byte & 0x0f] {
                if let Some(digit) = char::from_digit(u32::from(nibble), 16) {
                    encoded.push(digit.to_ascii_uppercase());
                }
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_encodes_spaces_colons_and_reserved_characters() {
        assert_eq!(
            form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("scope", "openid profile"),
                ("note", "a&b=c/é"),
            ]),
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code\
             &scope=openid%20profile&note=a%26b%3Dc%2F%C3%A9"
        );
    }
}
