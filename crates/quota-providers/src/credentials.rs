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

use crate::platform_paths::{self, Lookup};

/// The spellings Claude Code has written its access token under: nested in a
/// `claudeAiOauth` object, or at the root of the file.
const CLAUDE_TOKEN_PATHS: &[&[&str]] = &[
    &["claudeAiOauth", "accessToken"],
    &["claudeAiOauth", "access_token"],
    &["accessToken"],
    &["access_token"],
];
/// The largest credential file this crate will read, in bytes.
///
/// A real credential file is a few kilobytes; the ceiling stops an accidental
/// or hostile giant file from being read into memory.
const MAX_CREDENTIAL_FILE_BYTES: u64 = 256 * 1024;

/// The key the Codex CLI writes the `ChatGPT` access token under.
const CODEX_TOKEN_PATH: &[&str] = &["tokens", "access_token"];

/// The `OpenCode` file entry that belongs to Go, and the only one used.
const OPENCODE_GO_ENTRY: &str = "opencode-go";

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
    /// The bearer key read from `auth.json`, or from the environment.
    pub(crate) key: SecretToken,
    /// A non-revealing label for the profile the credential came from.
    pub(crate) profile_label: String,
}

/// The process environment, with a blank value treated as an absent one.
fn process_lookup(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Reads the Codex credential, honouring `CODEX_HOME` before the default.
pub(crate) async fn codex_credential() -> Result<CodexCredential, ProviderError> {
    let lookup: Lookup<'_> = &process_lookup;
    let (path, profile_label) = codex_auth_file(lookup)?;
    let document = read_json(&path).await?;
    let token = string_at(&document, &[CODEX_TOKEN_PATH, &["access_token"]])
        .ok_or(ProviderError::Authentication)?;
    let account_id = string_at(&document, &[&["tokens", "account_id"], &["account_id"]]);
    Ok(CodexCredential {
        token: SecretToken::new(token),
        account_id,
        profile_label,
    })
}

/// Reads the Claude credential, honouring `CLAUDE_CONFIG_DIR` first.
///
/// Both spellings Claude Code has written are accepted deliberately: the nested
/// `claudeAiOauth` object, and the same fields at the root of the file. A file
/// that only carries a root token is a credential, not a missing one.
pub(crate) async fn claude_credential() -> Result<ClaudeCredential, ProviderError> {
    let lookup: Lookup<'_> = &process_lookup;
    let (path, profile_label) = claude_credentials_file(lookup)?;
    let document = read_json(&path).await?;
    let token = string_at(&document, CLAUDE_TOKEN_PATHS).ok_or(ProviderError::Authentication)?;
    Ok(ClaudeCredential {
        token: SecretToken::new(token),
        profile_label,
    })
}

/// Reads the `OpenCode` Go credential.
///
/// The environment key wins, because it is an explicit choice. Otherwise the
/// file is read, and only the `opencode-go` entry is used: the generic
/// `opencode` entry is a different product's credential, and silently sending
/// it to the Go usage endpoint would report the wrong account.
pub(crate) async fn opencode_go_credential() -> Result<OpenCodeGoCredential, ProviderError> {
    let lookup: Lookup<'_> = &process_lookup;
    if let Some(key) = lookup("OPENCODE_API_KEY") {
        return Ok(OpenCodeGoCredential {
            key: SecretToken::new(key),
            profile_label: "opencode-api-key-env".to_owned(),
        });
    }
    let (path, profile_label) = opencode_auth_file(lookup)?;
    let document = read_json(&path).await?;
    let key = document
        .get(OPENCODE_GO_ENTRY)
        .and_then(|entry| {
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
fn codex_auth_file(lookup: Lookup<'_>) -> Result<(PathBuf, String), ProviderError> {
    Ok(match lookup("CODEX_HOME").map(PathBuf::from) {
        Some(home) => (home.join("auth.json"), "codex-home-env".to_owned()),
        None => (
            profile_directory(lookup)?.join(".codex").join("auth.json"),
            "codex-profile-default".to_owned(),
        ),
    })
}

/// The Claude `.credentials.json` path and a non-revealing profile label.
fn claude_credentials_file(lookup: Lookup<'_>) -> Result<(PathBuf, String), ProviderError> {
    Ok(match lookup("CLAUDE_CONFIG_DIR").map(PathBuf::from) {
        Some(dir) => (
            dir.join(".credentials.json"),
            "claude-config-env".to_owned(),
        ),
        None => (
            profile_directory(lookup)?
                .join(".claude")
                .join(".credentials.json"),
            "claude-profile-default".to_owned(),
        ),
    })
}

/// The `OpenCode` `auth.json` path and a non-revealing profile label.
fn opencode_auth_file(lookup: Lookup<'_>) -> Result<(PathBuf, String), ProviderError> {
    Ok(match lookup("XDG_DATA_HOME").map(PathBuf::from) {
        Some(dir) => (
            dir.join("opencode").join("auth.json"),
            "opencode-xdg-env".to_owned(),
        ),
        None => (
            profile_directory(lookup)?
                .join(".local")
                .join("share")
                .join("opencode")
                .join("auth.json"),
            "opencode-profile-default".to_owned(),
        ),
    })
}

/// The user profile every default credential path is built from.
fn profile_directory(lookup: Lookup<'_>) -> Result<PathBuf, ProviderError> {
    platform_paths::user_profile(lookup).ok_or(ProviderError::Authentication)
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
    use std::collections::HashMap;

    use crate::platform_paths::tests::NATIVE_PROFILE_VARIABLE;
    /// An environment made only of the variables a test names.
    fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        move |name: &str| map.get(name).cloned()
    }

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
            string_at(&document, &[CODEX_TOKEN_PATH, &["access_token"]]),
            Some("second".to_owned())
        );
        assert_eq!(string_at(&document, &[&["missing"]]), None);
    }

    /// The Windows defect: a profile-only machine must still resolve every file.
    #[test]
    fn a_profile_only_machine_resolves_every_credential_file() {
        let profile = if cfg!(windows) {
            "C:\\Users\\someone"
        } else {
            "/home/someone"
        };
        let lookup = environment(&[(NATIVE_PROFILE_VARIABLE, profile)]);
        let (codex, codex_label) = codex_auth_file(&lookup).unwrap();
        assert_eq!(
            codex,
            PathBuf::from(profile).join(".codex").join("auth.json")
        );
        assert_eq!(codex_label, "codex-profile-default");
        let (claude, _) = claude_credentials_file(&lookup).unwrap();
        assert_eq!(
            claude,
            PathBuf::from(profile)
                .join(".claude")
                .join(".credentials.json")
        );
        let (go, go_label) = opencode_auth_file(&lookup).unwrap();
        assert_eq!(
            go,
            PathBuf::from(profile)
                .join(".local")
                .join("share")
                .join("opencode")
                .join("auth.json")
        );
        assert_eq!(go_label, "opencode-profile-default");
    }

    /// A Windows-only regression: `USERPROFILE` alone, with `HOME` absent.
    #[test]
    #[cfg(windows)]
    fn a_windows_profile_without_home_resolves_every_credential_file() {
        let lookup = environment(&[("USERPROFILE", "C:\\Users\\someone")]);
        for path in [
            codex_auth_file(&lookup).unwrap().0,
            claude_credentials_file(&lookup).unwrap().0,
            opencode_auth_file(&lookup).unwrap().0,
        ] {
            assert!(
                path.starts_with("C:\\Users\\someone"),
                "every default path lives under the profile: {}",
                path.display()
            );
        }
    }

    #[test]
    fn an_explicit_override_wins_over_the_profile() {
        let lookup = environment(&[
            (NATIVE_PROFILE_VARIABLE, "/profile"),
            ("CODEX_HOME", "/override/codex"),
            ("CLAUDE_CONFIG_DIR", "/override/claude"),
            ("XDG_DATA_HOME", "/override/xdg"),
        ]);
        let (codex, label) = codex_auth_file(&lookup).unwrap();
        assert_eq!(codex, PathBuf::from("/override/codex").join("auth.json"));
        assert_eq!(label, "codex-home-env");
        let (claude, _) = claude_credentials_file(&lookup).unwrap();
        assert_eq!(
            claude,
            PathBuf::from("/override/claude").join(".credentials.json")
        );
        let (go, _) = opencode_auth_file(&lookup).unwrap();
        assert_eq!(
            go,
            PathBuf::from("/override/xdg")
                .join("opencode")
                .join("auth.json")
        );
    }
    /// A root-only Claude file is a credential, not a missing one.
    #[test]
    fn a_root_only_claude_credential_is_still_read() {
        let nested = serde_json::json!({"claudeAiOauth": {"accessToken": "one"}});
        assert_eq!(
            string_at(&nested, CLAUDE_TOKEN_PATHS),
            Some("one".to_owned())
        );
        let root = serde_json::json!({"accessToken": "two", "expiresAt": 1});
        assert_eq!(string_at(&root, CLAUDE_TOKEN_PATHS), Some("two".to_owned()));
        let empty = serde_json::json!({"refreshToken": "only"});
        assert_eq!(string_at(&empty, CLAUDE_TOKEN_PATHS), None);
    }

    #[test]
    fn no_profile_and_no_override_is_an_authentication_state() {
        let lookup = environment(&[]);
        assert_eq!(codex_auth_file(&lookup), Err(ProviderError::Authentication));
        assert_eq!(
            claude_credentials_file(&lookup),
            Err(ProviderError::Authentication)
        );
        assert_eq!(
            opencode_auth_file(&lookup),
            Err(ProviderError::Authentication)
        );
    }
}
