//! Typed application failures.

use quota_domain::ids::{AccountId, ConnectionId};

use crate::ports::ProviderError;

/// A failure inside the application layer.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// The caller referenced an account this instance does not know.
    #[error("account {0} is not known")]
    AccountNotFound(AccountId),
    /// The caller referenced a connection this instance does not know.
    #[error("connection {0} is not known")]
    ConnectionNotFound(ConnectionId),
    /// The operation needs a credential the user must renew first.
    #[error("the connection must be reauthorized before this operation")]
    ReconnectRequired,
    /// The read completed, but it belongs to a superseded binding.
    #[error("the read result belongs to a superseded binding")]
    StaleResult,
    /// The argument failed validation before any work started.
    #[error("`{field}` is not valid: {reason}")]
    Validation {
        /// The rejected field.
        field: &'static str,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// A provider read failed.
    #[error("provider read failed: {0}")]
    Provider(#[from] ProviderError),
    /// A repository call failed.
    #[error("durable state failed: {owner}")]
    Persistence {
        /// Which durable owner failed.
        owner: &'static str,
    },
}

impl CoreError {
    /// Whether the same call could plausibly succeed unchanged later.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        match self {
            Self::Provider(value) => value.is_retryable(),
            Self::Persistence { .. } => true,
            Self::AccountNotFound(_)
            | Self::ConnectionNotFound(_)
            | Self::ReconnectRequired
            | Self::StaleResult
            | Self::Validation { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transient_provider_failure_is_retryable() {
        let error = CoreError::Provider(ProviderError::Transient {
            detail: "reset".into(),
        });
        assert!(error.is_retryable());
    }

    #[test]
    fn a_reconnect_requirement_is_not_a_retry_loop() {
        assert!(!CoreError::ReconnectRequired.is_retryable());
        assert!(!CoreError::StaleResult.is_retryable());
    }
}
