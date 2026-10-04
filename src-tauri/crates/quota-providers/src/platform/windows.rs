//! What Windows decides about the user profile and application data.

use std::path::PathBuf;
use std::sync::Arc;

use keyring_core::CredentialStore;
use quota_core::ports::SecretStoreError;

use super::{Lookup, Platform};

/// Windows.
///
/// This file is compiled on every host, so the path contract is covered by one
/// test whatever the runner is. Only [`open_credential_store`] is specific to
/// the system the build runs on.
///
/// [`open_credential_store`]: Platform::open_credential_store
#[derive(Debug)]
pub struct Windows;

impl Platform for Windows {
    fn profile_variable(&self) -> &'static str {
        "USERPROFILE"
    }

    /// Windows exports the profile to every process from the same
    /// known-folder API, with `HOME` as a fallback if it is absent or blank.
    fn user_profile(&self, lookup: Lookup<'_>) -> Option<PathBuf> {
        super::variable(lookup, "USERPROFILE").or_else(|| super::variable(lookup, "HOME"))
    }

    fn application_data(&self, lookup: Lookup<'_>) -> Option<PathBuf> {
        super::variable(lookup, "APPDATA")
    }

    fn open_credential_store(
        &self,
        _service: &str,
    ) -> Result<Arc<CredentialStore>, SecretStoreError> {
        #[cfg(windows)]
        {
            let store: Arc<CredentialStore> = windows_native_keyring_store::Store::new()
                .map_err(|_| SecretStoreError::Unavailable)?;
            Ok(store)
        }
        #[cfg(not(windows))]
        {
            Err(SecretStoreError::Unavailable)
        }
    }
}

/// The one Windows instance this build carries.
#[cfg(windows)]
const INSTANCE: Windows = Windows;

/// Windows, as the platform this build runs on.
#[cfg(windows)]
pub(crate) fn selected() -> &'static dyn Platform {
    &INSTANCE
}
