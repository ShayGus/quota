//! What macOS decides about the user profile and application data.

use std::path::PathBuf;
use std::sync::Arc;

use keyring_core::CredentialStore;
use quota_core::ports::SecretStoreError;

use super::{Lookup, Platform};

/// macOS.
///
/// This file is compiled on every host, so the path contract is covered by one
/// test whatever the runner is. Only [`open_credential_store`] is specific to
/// the system the build runs on.
///
/// [`open_credential_store`]: Platform::open_credential_store
#[derive(Debug)]
pub struct Macos;

impl Platform for Macos {
    fn profile_variable(&self) -> &'static str {
        "HOME"
    }

    fn user_profile(&self, lookup: Lookup<'_>) -> Option<PathBuf> {
        super::variable(lookup, "HOME")
    }

    /// macOS keeps application data in `Library/Application Support` in the
    /// profile.
    fn application_data(&self, lookup: Lookup<'_>) -> Option<PathBuf> {
        self.user_profile(lookup)
            .map(|home| home.join("Library").join("Application Support"))
    }

    fn open_credential_store(
        &self,
        _service: &str,
    ) -> Result<Arc<CredentialStore>, SecretStoreError> {
        #[cfg(target_os = "macos")]
        {
            let store: Arc<CredentialStore> = apple_native_keyring_store::keychain::Store::new()
                .map_err(|_| SecretStoreError::Unavailable)?;
            Ok(store)
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(SecretStoreError::Unavailable)
        }
    }
}

/// The one macOS instance this build carries.
#[cfg(target_os = "macos")]
const INSTANCE: Macos = Macos;

/// macOS, as the platform this build runs on.
#[cfg(target_os = "macos")]
pub(crate) fn selected() -> &'static dyn Platform {
    &INSTANCE
}
