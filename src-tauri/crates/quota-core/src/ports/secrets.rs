//! The credentials Quota owns itself.
//!
//! A provider that has no client of its own on the user's machine is signed in
//! by Quota: the user pastes an API key or completes a browser sign-in, and the
//! resulting credential belongs to Quota. Such a credential lives only in the
//! operating system's credential store, behind this port, and never in Quota's
//! database, preferences, logs, or snapshots.
//!
//! Each entry is keyed by the connection it signs in, so disconnecting an
//! account names exactly the entry to remove.

use std::fmt::{self, Debug, Formatter};

use quota_domain::ids::ConnectionId;

/// A secret value that never reveals itself through `Debug`.
///
/// It is deliberately not `Clone`, so no copy of it drifts into a second owner.
pub struct Secret(String);

impl Secret {
    /// Wraps a secret value.
    #[must_use]
    pub const fn new(value: String) -> Self {
        Self(value)
    }

    /// Borrows the value for the one place that sends or stores it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Debug for Secret {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Why the credential store refused an operation.
///
/// The variants carry no store text, which can name the account or the entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SecretStoreError {
    /// The system has no usable credential store, or it is locked.
    #[error("the system credential store is unavailable")]
    Unavailable,
    /// The store refused the operation.
    #[error("the system credential store refused the operation")]
    Refused,
}

/// The operating system's credential store.
///
/// Every call may block on the system, so an async caller runs it on a
/// blocking thread.
pub trait SecretStore: Debug + Send + Sync {
    /// The secret saved for one connection, or `None` when there is none.
    ///
    /// # Errors
    /// Returns [`SecretStoreError`] when the store cannot be read.
    fn read(&self, connection: &ConnectionId) -> Result<Option<Secret>, SecretStoreError>;

    /// Saves the secret for one connection, replacing any earlier one.
    ///
    /// # Errors
    /// Returns [`SecretStoreError`] when the store refuses the write.
    fn write(&self, connection: &ConnectionId, secret: &Secret) -> Result<(), SecretStoreError>;

    /// Removes the secret for one connection. Removing a missing entry succeeds.
    ///
    /// # Errors
    /// Returns [`SecretStoreError`] when the store refuses the removal.
    fn delete(&self, connection: &ConnectionId) -> Result<(), SecretStoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_prints_its_value() {
        let secret = Secret::new("sk-live-value".to_owned());
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert_eq!(secret.expose(), "sk-live-value");
    }
}
