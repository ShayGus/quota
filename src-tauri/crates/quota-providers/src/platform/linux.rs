//! What Linux decides about the user profile and application data.

use std::path::PathBuf;
use std::sync::Arc;

use keyring_core::CredentialStore;
use quota_core::ports::SecretStoreError;

use super::{Lookup, Platform};

/// Linux.
///
/// This file is compiled on every host, so the path contract is covered by one
/// test whatever the runner is. Only [`open_credential_store`] is specific to
/// the system the build runs on.
///
/// [`open_credential_store`]: Platform::open_credential_store
#[derive(Debug)]
pub struct Linux;

impl Platform for Linux {
    fn user_profile(&self, lookup: Lookup<'_>) -> Option<PathBuf> {
        super::variable(lookup, "HOME")
    }

    /// Desktop applications keep their settings in `XDG_CONFIG_HOME`, otherwise
    /// in `.config` in the profile.
    fn application_data(&self, lookup: Lookup<'_>) -> Option<PathBuf> {
        super::variable(lookup, "XDG_CONFIG_HOME")
            .or_else(|| self.user_profile(lookup).map(|home| home.join(".config")))
    }

    fn open_credential_store(
        &self,
        _service: &str,
    ) -> Result<Arc<CredentialStore>, SecretStoreError> {
        #[cfg(target_os = "linux")]
        {
            let store: Arc<CredentialStore> = zbus_secret_service_keyring_store::Store::new()
                .map_err(|_| SecretStoreError::Unavailable)?;
            Ok(store)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(SecretStoreError::Unavailable)
        }
    }
}

/// The one Linux instance this build carries.
#[cfg(target_os = "linux")]
const INSTANCE: Linux = Linux;

/// Linux, as the platform this build runs on.
#[cfg(target_os = "linux")]
pub(crate) fn selected() -> &'static dyn Platform {
    &INSTANCE
}
