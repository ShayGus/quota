//! Asking Claude Code to renew its own expired sign-in.
//!
//! Claude Code's access token lasts eight hours and only Claude Code renews it,
//! while it runs. Quota never renews it itself: the refresh token is single
//! use, so a renewal Quota made would leave Claude Code holding a spent token
//! and sign it out. Instead this starts Claude Code once, without a window, on
//! a command that renews an expired sign-in through Claude Code's own locked
//! refresh and needs nothing else: `mcp get` on a server name nobody uses
//! starts no MCP server, sends no prompt and changes no setting.
//!
//! Two rules from the people who found the failure modes are kept. The command
//! runs only once the token has really expired, and the process is never
//! killed: a Claude Code that exits halfway through saving a renewal is what
//! spends the refresh token without keeping its replacement.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use chrono::Utc;
use quota_core::ports::{ProviderError, SignInRenewal as Renewal};

use crate::credentials::{self, process_lookup};
use crate::platform::{self, Lookup};

/// The server name asked about. Nobody configures one by this name, so the
/// command finds nothing to start once the sign-in is renewed.
const PROBE_SERVER: &str = "quota-sign-in-renewal-probe";

/// How long the renewal is waited for before Quota stops waiting. Claude Code
/// is left to finish on its own even then.
const WAIT: Duration = Duration::from_secs(60);

/// The folder Claude Code runs in: an empty one Quota owns, so no project's
/// settings, hooks or MCP servers apply.
const WORK_FOLDER: &str = "quota-claude-renewal";

/// Starts Claude Code once to renew an expired sign-in, and reports whether
/// the expiry in the credential file moved.
pub(crate) async fn renew() -> Result<Renewal, ProviderError> {
    let before = credentials::claude_credential().await?;
    if !before.expired(Utc::now()) {
        return Ok(Renewal::NotNeeded);
    }
    let lookup: Lookup<'_> = &process_lookup;
    let Some(program) = claude_program(platform::system(), lookup, Path::is_file) else {
        return Ok(Renewal::Unavailable);
    };
    let folder = std::env::temp_dir().join(WORK_FOLDER);
    tokio::fs::create_dir_all(&folder)
        .await
        .map_err(|_| ProviderError::Transient {
            detail: "the folder Claude Code renews in could not be made".to_owned(),
        })?;
    let mut command = tokio::process::Command::new(&program);
    command
        .args(["mcp", "get", PROBE_SERVER])
        .current_dir(&folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Killing it mid-save is the failure this module exists to avoid.
        .kill_on_drop(false);
    #[cfg(windows)]
    {
        /// Starts a console program without opening a console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|_| ProviderError::Transient {
        detail: "Claude Code could not be started".to_owned(),
    })?;
    if tokio::time::timeout(WAIT, child.wait()).await.is_err() {
        tracing::warn!("Claude Code is still renewing; it is left to finish");
    }
    let after = credentials::claude_credential().await?;
    Ok(match (before.expires_at, after.expires_at) {
        (Some(old), Some(new)) if new > old => Renewal::Renewed,
        _ => Renewal::Unchanged,
    })
}

/// The Claude Code program, from `PATH` first and then the native installer's
/// folder.
///
/// On Windows an npm install leaves a `claude.cmd` shim; it is accepted after a
/// real `claude.exe`, and the arguments are fixed words, so nothing the person
/// typed reaches the shell.
pub(crate) fn claude_program(
    platform: &dyn platform::Platform,
    lookup: Lookup<'_>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    let mut folders: Vec<PathBuf> = lookup("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if let Some(profile) = platform.user_profile(lookup) {
        folders.push(profile.join(".local").join("bin"));
    }
    names.iter().find_map(|name| {
        folders
            .iter()
            .map(|folder| folder.join(name))
            .find(|candidate| candidate.is_absolute() && exists(candidate))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A system whose profile is fixed, so the fallback folder is known.
    #[derive(Debug)]
    struct Profile(PathBuf);

    impl platform::Platform for Profile {
        fn user_profile(&self, _lookup: Lookup<'_>) -> Option<PathBuf> {
            Some(self.0.clone())
        }

        fn application_data(&self, _lookup: Lookup<'_>) -> Option<PathBuf> {
            None
        }

        fn open_credential_store(
            &self,
            _service: &str,
        ) -> Result<
            std::sync::Arc<keyring_core::CredentialStore>,
            quota_core::ports::SecretStoreError,
        > {
            Err(quota_core::ports::SecretStoreError::Unavailable)
        }
    }

    fn root() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\home")
        } else {
            PathBuf::from("/home/person")
        }
    }

    fn name() -> &'static str {
        if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        }
    }

    #[test]
    fn a_program_on_the_path_wins_over_the_installer_folder() {
        let on_path = root().join("tools");
        let path = std::env::join_paths([&on_path]).unwrap();
        let path = path.to_string_lossy().into_owned();
        let lookup = move |key: &str| (key == "PATH").then(|| path.clone());
        let found = claude_program(&Profile(root()), &lookup, |_| true);
        assert_eq!(found, Some(on_path.join(name())));
    }

    #[test]
    fn the_installer_folder_is_the_fallback() {
        let lookup = |_: &str| None;
        let expected = root().join(".local").join("bin").join(name());
        let found = claude_program(&Profile(root()), &lookup, |candidate| {
            candidate == expected.as_path()
        });
        assert_eq!(found, Some(expected));
    }

    #[test]
    fn a_relative_path_entry_is_never_run() {
        let lookup = |key: &str| (key == "PATH").then(|| "relative".to_owned());
        let found = claude_program(&Profile(root()), &lookup, |candidate| {
            candidate.starts_with("relative")
        });
        assert_eq!(found, None);
    }

    #[test]
    fn nothing_found_is_reported_as_nothing() {
        let lookup = |_: &str| None;
        assert_eq!(claude_program(&Profile(root()), &lookup, |_| false), None);
    }
}
