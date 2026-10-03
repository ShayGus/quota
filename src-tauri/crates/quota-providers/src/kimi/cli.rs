//! The sign-in the Kimi CLI keeps on this computer.
//!
//! `kimi` stores its sign-in in `credentials/kimi-code.json` under its share
//! directory: `KIMI_SHARE_DIR` when set, otherwise `.kimi` in the user
//! profile. Quota only reads the file. It never refreshes the token, whose
//! refresh the CLI owns, so an expired token asks the person to sign in with
//! the CLI again.

use chrono::Utc;
use quota_core::ports::{ProviderError, Secret};

use crate::credentials::{process_lookup, profile_directory, read_json, string_at};
use crate::decode::Numberish;
use crate::platform_paths::Lookup;

/// The profile label a connection that reads the CLI's sign-in carries.
pub(crate) const PROFILE: &str = "kimi-cli";

/// The CLI's access token, while it is still valid.
pub(crate) async fn token() -> Result<Secret, ProviderError> {
    let lookup: Lookup<'_> = &process_lookup;
    let directory = match lookup("KIMI_SHARE_DIR") {
        Some(directory) => std::path::PathBuf::from(directory),
        None => profile_directory(lookup)?.join(".kimi"),
    };
    let document = read_json(&directory.join("credentials").join("kimi-code.json")).await?;
    let token = string_at(&document, &[&["access_token"], &["accessToken"]])
        .ok_or(ProviderError::Authentication)?;
    let expires_at = document
        .get("expires_at")
        .cloned()
        .and_then(|value| serde_json::from_value::<Numberish>(value).ok())
        .and_then(|value| value.whole());
    if expires_at.is_some_and(|expires_at| expires_at <= Utc::now().timestamp()) {
        return Err(ProviderError::Authentication);
    }
    Ok(Secret::new(token))
}
