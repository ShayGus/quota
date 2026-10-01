//! Read-only discovery of credentials that other clients own.
//!
//! Every credential in this module belongs to an external client: the Codex
//! CLI, Claude Code, or an `OpenCode` login. Quota only reads the file and never
//! writes to it, refreshes it, rotates it, or copies it anywhere. A missing or
//! unusable file is an authentication state for the user to fix in the owning
//! client, not something this crate repairs.

use std::fmt::{self, Debug, Formatter};
use std::path::{Path, PathBuf};

use quota_core::ports::ProviderError;
use serde_json::Value;

/// The largest credential file this crate will read, in bytes.
///
/// A real credential file is a few kilobytes; the ceiling stops an accidental
/// or hostile giant file from being read into memory.
const MAX_CREDENTIAL_FILE_BYTES: u64 = 256 * 1024;

/// A credential value that never reveals itself through `Debug`.
///
/// The value is deliberately not `Clone`, so no copy of it can drift into a
/// second owner.
pub(crate) struct SecretToken(String);

impl SecretToken {
    /// Wraps a discovered credential value.
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    /// Borrows the value for the one place that builds an authorization header.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl Debug for SecretToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("SecretToken(<redacted>)")
    }
}

/// The Codex credential, as the Codex CLI wrote it.
pub(crate) struct CodexCredential {
    /// The bearer token read from `auth.json`.
    pub(crate) token: SecretToken,
    /// The account identity the CLI signed in with, when the file carries one.
    pub(crate) account_id: Option<String>,
    /// A non-revealing label for the profile the credential came from.
    pub(crate) profile_label: String,
}

/// The Claude credential, as Claude Code wrote it.
pub(crate) struct ClaudeCredential {
    /// The OAuth access token read from `.credentials.json`.
    pub(crate) token: SecretToken,
    /// A non-revealing label for the profile the credential came from.
    pub(crate) profile_label: String,
}

/// The `OpenCode` Go credential, as an `OpenCode` login wrote it.
pub(crate) struct OpenCodeGoCredential {
    /// The bearer key read from `auth.json`.
    pub(crate) key: SecretToken,
    /// A non-revealing label for the profile the credential came from.
    pub(crate) profile_label: String,
}

/// Reads the Codex credential, honouring `CODEX_HOME` before the default.
pub(crate) async fn codex_credential() -> Result<CodexCredential, ProviderError> {
    let (path, profile_label) = codex_auth_file()?;
    let document = read_json(&path).await?;
    let token = string_at(&document, &[&["tokens", "access_token"], &["access_token"]])
        .ok_or(ProviderError::Authentication)?;
    let account_id = string_at(&document, &[&["account_id"], &["tokens", "account_id"]]);
    Ok(CodexCredential {
        token: SecretToken::new(token),
        account_id,
        profile_label,
    })
}

/// Reads the Claude credential, honouring `CLAUDE_CONFIG_DIR` before the default.
pub(crate) async fn claude_credential() -> Result<ClaudeCredential, ProviderError> {
    let (path, profile_label) = claude_credentials_file()?;
    let document = read_json(&path).await?;
    let token = string_at(
        &document,
        &[
            &["claudeAiOauth", "accessToken"],
            &["claudeAiOauth", "access_token"],
        ],
    )
    .ok_or(ProviderError::Authentication)?;
    Ok(ClaudeCredential {
        token: SecretToken::new(token),
        profile_label,
    })
}

/// Reads the `OpenCode` credential, honouring `XDG_DATA_HOME` before the default.
///
/// The `opencode-go` entry is preferred; the `opencode` entry is the fallback an
/// older or differently named login writes.
pub(crate) async fn opencode_go_credential() -> Result<OpenCodeGoCredential, ProviderError> {
    let (path, profile_label) = opencode_auth_file()?;
    let document = read_json(&path).await?;
    let key = ["opencode-go", "opencode"]
        .into_iter()
        .find_map(|entry| {
            let entry = document.get(entry)?;
            string_at(
                entry,
                &[&["key"], &["apiKey"], &["api_key"], &["access"], &["token"]],
            )
        })
        .ok_or(ProviderError::Authentication)?;
    Ok(OpenCodeGoCredential {
        key: SecretToken::new(key),
        profile_label,
    })
}

/// The Codex `auth.json` path and a non-revealing profile label.
fn codex_auth_file() -> Result<(PathBuf, String), ProviderError> {
    let override_home = std::env::var("CODEX_HOME")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let (path, label) = match override_home {
        Some(home) => (
            PathBuf::from(home).join("auth.json"),
            "codex-home-env".to_owned(),
        ),
        None => (
            home_directory()?.join(".codex").join("auth.json"),
            "codex-home-default".to_owned(),
        ),
    };
    Ok((path, label))
}

/// The Claude `.credentials.json` path and a non-revealing profile label.
fn claude_credentials_file() -> Result<(PathBuf, String), ProviderError> {
    let override_dir = std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let (path, label) = match override_dir {
        Some(dir) => (
            PathBuf::from(dir).join(".credentials.json"),
            "claude-config-env".to_owned(),
        ),
        None => (
            home_directory()?.join(".claude").join(".credentials.json"),
            "claude-config-default".to_owned(),
        ),
    };
    Ok((path, label))
}

/// The `OpenCode` `auth.json` path and a non-revealing profile label.
fn opencode_auth_file() -> Result<(PathBuf, String), ProviderError> {
    let override_dir = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let (path, label) = match override_dir {
        Some(dir) => (
            PathBuf::from(dir).join("opencode").join("auth.json"),
            "opencode-xdg-env".to_owned(),
        ),
        None => (
            home_directory()?
                .join(".local")
                .join("share")
                .join("opencode")
                .join("auth.json"),
            "opencode-xdg-default".to_owned(),
        ),
    };
    Ok((path, label))
}

/// The current user's home directory, or an authentication state when unknown.
fn home_directory() -> Result<PathBuf, ProviderError> {
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.trim().is_empty())
        .map(PathBuf::from)
        .ok_or(ProviderError::Authentication)
}

/// Reads and parses one credential file without ever writing to it.
///
/// A missing or unreadable file is an authentication state: the user fixes it in
/// the owning client. A file that is not JSON at all is invalid data.
async fn read_json(path: &Path) -> Result<Value, ProviderError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| ProviderError::Authentication)?;
    if !metadata.is_file() || metadata.len() > MAX_CREDENTIAL_FILE_BYTES {
        return Err(ProviderError::Authentication);
    }
    let text = tokio::fs::read_to_string(path)
        .await
        .map_err(|_| ProviderError::Authentication)?;
    serde_json::from_str(&text).map_err(|_| ProviderError::InvalidData {
        detail: "the credential file is not valid JSON".to_owned(),
    })
}

/// The first non-empty string found at one of the given key paths.
fn string_at(document: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|keys| {
        let mut cursor = document;
        for key in *keys {
            cursor = cursor.get(*key)?;
        }
        let text = cursor.as_str()?.trim();
        if text.is_empty() {
            return None;
        }
        Some(text.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_reveals_itself() {
        let token = SecretToken::new("canary-value".to_owned());
        let rendered = format!("{token:?}");
        assert!(!rendered.contains("canary-value"));
        assert_eq!(rendered, "SecretToken(<redacted>)");
        assert_eq!(token.expose(), "canary-value");
    }

    #[test]
    fn the_first_present_key_path_wins() {
        let document = serde_json::json!({
            "tokens": { "access_token": "  " },
            "access_token": "second"
        });
        assert_eq!(
            string_at(&document, &[&["tokens", "access_token"], &["access_token"]]),
            Some("second".to_owned())
        );
        assert_eq!(string_at(&document, &[&["missing"]]), None);
    }

    #[test]
    fn credential_paths_follow_the_documented_environment_overrides() {
        let (codex, _) = codex_auth_file().expect("a home directory exists");
        assert!(codex.ends_with("auth.json"));
        let (claude, _) = claude_credentials_file().expect("a home directory exists");
        assert!(claude.ends_with(".credentials.json"));
        let (opencode, _) = opencode_auth_file().expect("a home directory exists");
        assert!(opencode.ends_with("auth.json"));
    }
}
