//! The one place this crate asks the operating system a question.
//!
//! Four implementations decide where the user profile and the application-data
//! directory are, and how this system's credential store is opened:
//! [`Windows`], [`Linux`], [`Macos`], and [`Unsupported`]. All four compile on
//! every host, so one contract test covers every system's paths on any runner.
//! [`system`] selects the one this build runs on.
//!
//! The only system-specific code left is each implementation's
//! [`open_credential_store`](Platform::open_credential_store): the path logic is
//! shared, and a system other than the one this build runs on reports the store
//! as unavailable instead of calling a store crate the host cannot link.
//!
//! One rule holds: a provider's own directory override always wins, then the
//! operating system's user directory, then a shell variable. A profile path is
//! never assembled from a user name, because a guessed path is worse than an
//! honest "not found".

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use keyring_core::CredentialStore;
use quota_core::ports::SecretStoreError;

mod linux;
mod macos;
mod unsupported;
mod windows;

pub use linux::Linux;
pub use macos::Macos;
pub use unsupported::Unsupported;
pub use windows::Windows;

/// Reads one environment variable, without treating a blank value as one.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The first non-blank value of a variable, as a path.
pub fn variable(lookup: Lookup<'_>, name: &str) -> Option<PathBuf> {
    let value = lookup(name)?;
    if value.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

/// What this system decides about the profile, application data, and the
/// credential store.
pub trait Platform: std::fmt::Debug + Send + Sync {
    /// The current user's profile directory, or `None` when nothing declares
    /// one.
    ///
    /// Every default credential this crate reads lives directly under this
    /// directory. The roaming and local application-data folders are not needed
    /// by any reader here, so they are not resolved.
    fn user_profile(&self, lookup: Lookup<'_>) -> Option<PathBuf>;

    /// The directory desktop applications keep their settings and state in, or
    /// `None` when nothing declares one.
    fn application_data(&self, lookup: Lookup<'_>) -> Option<PathBuf>;

    /// Opens this system's credential store for one application identifier.
    ///
    /// The identifier names the entries, not the store, so opening reads
    /// nothing.
    ///
    /// # Errors
    /// Returns [`SecretStoreError::Unavailable`] when this system has no usable
    /// store, and on every system other than the one this build runs on, so a
    /// store crate for a foreign system is never called.
    fn open_credential_store(
        &self,
        service: &str,
    ) -> Result<Arc<CredentialStore>, SecretStoreError>;
}

/// The implementation this build runs on, chosen once at compile time.
const SELECTED: fn() -> &'static dyn Platform = {
    #[cfg(windows)]
    {
        windows::selected
    }
    #[cfg(target_os = "linux")]
    {
        linux::selected
    }
    #[cfg(target_os = "macos")]
    {
        macos::selected
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        unsupported::selected
    }
};

/// The platform this build runs on.
#[must_use]
pub fn system() -> &'static dyn Platform {
    static SELECTED_PLATFORM: LazyLock<&'static dyn Platform> = LazyLock::new(|| (SELECTED)());
    *SELECTED_PLATFORM
}
