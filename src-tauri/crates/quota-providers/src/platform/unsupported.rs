//! What a system this build does not know decides about paths.

use std::path::PathBuf;
use std::sync::Arc;

use keyring_core::CredentialStore;
use quota_core::ports::SecretStoreError;

use super::{Lookup, Platform};

/// A system this build has no special knowledge of.
///
/// Linux behaviour is the honest default for an unknown system: `HOME` names
/// the profile, `XDG_CONFIG_HOME` names the application-data directory, and
/// `.config` in the profile is the fallback. There is no credential store to
/// open, so every operation on it reports itself unavailable rather than
/// guessing at one.
///
/// This file is compiled on every host, so the path contract is covered by one
/// test whatever the runner is.
#[derive(Debug)]
pub struct Unsupported;

impl Platform for Unsupported {
    fn user_profile(&self, lookup: Lookup<'_>) -> Option<PathBuf> {
        super::variable(lookup, "HOME")
    }

    fn application_data(&self, lookup: Lookup<'_>) -> Option<PathBuf> {
        super::variable(lookup, "XDG_CONFIG_HOME")
            .or_else(|| self.user_profile(lookup).map(|home| home.join(".config")))
    }

    fn open_credential_store(
        &self,
        _service: &str,
    ) -> Result<Arc<CredentialStore>, SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }
}

/// The one unfamiliar-system instance this build carries.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
const INSTANCE: Unsupported = Unsupported;

/// The unfamiliar system, as the platform this build runs on.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub(crate) fn selected() -> &'static dyn Platform {
    &INSTANCE
}
