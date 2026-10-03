//! Reading a claim from a JSON Web Token's payload.
//!
//! The signature is not checked: the token is only ever sent back to the
//! provider that issued it, and a claim here only names or dates the person's
//! own sign-in, never grants anything.

use serde_json::Value;

/// One claim of the token's payload.
pub(crate) fn claim(token: &str, name: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let document: Value = serde_json::from_slice(&base64url(payload)?).ok()?;
    document.get(name).cloned()
}

/// Decodes base64url, with or without padding.
fn base64url(text: &str) -> Option<Vec<u8>> {
    let mut bits: u32 = 0;
    let mut count = 0;
    let mut bytes = Vec::with_capacity(text.len() * 3 / 4);
    for character in text.trim_end_matches('=').bytes() {
        let value = match character {
            b'A'..=b'Z' => character - b'A',
            b'a'..=b'z' => character - b'a' + 26,
            b'0'..=b'9' => character - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push(u8::try_from((bits >> count) & 0xff).ok()?);
        }
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claim_is_read_from_the_payload() {
        // {"sub":"auth0|user_42","exp":1700000000} in base64url.
        let token = "e30.eyJzdWIiOiJhdXRoMHx1c2VyXzQyIiwiZXhwIjoxNzAwMDAwMDAwfQ.sig";
        assert_eq!(
            claim(token, "sub").and_then(|value| value.as_str().map(str::to_owned)),
            Some("auth0|user_42".to_owned())
        );
        assert_eq!(
            claim(token, "exp").and_then(|value| value.as_i64()),
            Some(1_700_000_000)
        );
        assert_eq!(claim("not-a-token", "sub"), None);
    }
}
