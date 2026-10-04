//! The sign-in the Grok CLI keeps on this computer.
//!
//! `grok` stores its sign-in in `auth.json` under `GROK_HOME`, otherwise
//! `.grok` in the user profile: a map keyed by the issuer it signed in with,
//! each entry carrying the access token as `key` and its `expires_at`. Quota
//! only reads the file. It never refreshes the token, whose refresh the CLI
//! owns, so an expired token asks the person to sign in with the CLI again.

use chrono::{DateTime, Utc};
use quota_core::ports::{ProviderError, Secret};
use serde_json::Value;

use crate::credentials::{process_lookup, profile_directory, read_json};
use crate::platform::Lookup;

/// The profile label a connection that reads the CLI's sign-in carries.
pub(crate) const PROFILE: &str = "grok-cli";

/// The issuer prefixes, current first, that a sign-in is filed under.
const ISSUERS: [&str; 2] = ["https://auth.x.ai", "https://accounts.x.ai"];

/// The CLI's access token, while it is still valid.
pub(crate) async fn token() -> Result<Secret, ProviderError> {
    let lookup: Lookup<'_> = &process_lookup;
    let directory = match lookup("GROK_HOME") {
        Some(directory) => std::path::PathBuf::from(directory),
        None => profile_directory(crate::platform::system(), lookup)?.join(".grok"),
    };
    let document = read_json(&directory.join("auth.json")).await?;
    let entry = entry(&document).ok_or(ProviderError::Authentication)?;
    let token = entry
        .get("key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(ProviderError::Authentication)?;
    let expired = entry
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .is_some_and(|expires_at| expires_at.with_timezone(&Utc) <= Utc::now());
    if expired {
        return Err(ProviderError::Authentication);
    }
    Ok(Secret::new(token.to_owned()))
}

/// The sign-in entry, preferring the current issuer.
fn entry(document: &Value) -> Option<&serde_json::Map<String, Value>> {
    let entries = document.as_object()?;
    ISSUERS.iter().find_map(|issuer| {
        entries
            .iter()
            .find(|(name, value)| name.starts_with(issuer) && value.get("key").is_some())
            .and_then(|(_, value)| value.as_object())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_issuers_entry_wins_over_the_legacy_one() {
        let document = serde_json::json!({
            "https://accounts.x.ai/sign-in": { "key": "legacy" },
            "https://auth.x.ai::b1a00492": { "key": "current" }
        });
        let found = entry(&document).and_then(|entry| entry.get("key")?.as_str());
        assert_eq!(found, Some("current"));
    }
}
