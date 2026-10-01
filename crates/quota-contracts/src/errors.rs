//! The typed error union every command may return.

use serde::{Deserialize, Serialize};
use specta::Type;

use quota_domain::provider::ProviderId;

/// A command failure with structured, non-prose context.
///
/// Recovery switches on the variant. The optional sanitized message supplements
/// the variant for display and is never parsed for control flow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "context")]
pub enum CommandError {
    /// Backend bootstrap has not finished, so no command may act yet.
    InitializationPending,
    /// An argument failed validation.
    ValidationFailed {
        /// Which field was rejected.
        field: String,
        /// A sanitized, human-readable reason.
        reason: String,
    },
    /// The account is not known to this instance.
    AccountNotFound,
    /// The provider identifier is outside the compiled registry.
    UnsupportedProvider {
        /// The provider this build does not implement.
        provider_id: ProviderId,
    },
    /// The access method the user asked for is not supported by the adapter.
    UnsupportedMethod {
        /// A sanitized description of the requested method.
        requested: String,
    },
    /// The credential must be renewed by the user before any read can succeed.
    ReconnectRequired,
    /// The window label is not permitted to call this command.
    PermissionDenied {
        /// The window that attempted the call.
        window_label: String,
    },
    /// The OS secure store is locked or unavailable. There is no plaintext fallback.
    SecureStoreUnavailable,
    /// The caller's expected revision does not match the stored one.
    RevisionConflict {
        /// The revision the caller believed was current.
        expected: u32,
        /// The revision the backend actually holds.
        actual: u32,
    },
    /// A durable owner could not be reached.
    PersistenceUnavailable {
        /// Which owner failed, such as `sqlite` or `store`.
        owner: String,
    },
    /// The native window operation is not supported on this platform.
    NativeOperationUnsupported {
        /// The native operation that was refused.
        operation: String,
    },
    /// The native window operation failed.
    NativeOperationFailed {
        /// The native operation that failed.
        operation: String,
        /// A sanitized reason.
        reason: String,
    },
    /// The work was deliberately cancelled.
    Cancelled,
    /// An unexpected internal failure, with no internal detail exposed.
    Internal {
        /// A stable, non-identifying failure code for diagnostics.
        code: String,
    },
}

impl CommandError {
    /// Whether retrying the same call could plausibly succeed unchanged.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::InitializationPending | Self::PersistenceUnavailable { .. }
        )
    }

    /// A stable, non-identifying code for logs and diagnostics.
    #[must_use]
    pub fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::InitializationPending => "initialization_pending",
            Self::ValidationFailed { .. } => "validation_failed",
            Self::AccountNotFound => "account_not_found",
            Self::UnsupportedProvider { .. } => "unsupported_provider",
            Self::UnsupportedMethod { .. } => "unsupported_method",
            Self::ReconnectRequired => "reconnect_required",
            Self::PermissionDenied { .. } => "permission_denied",
            Self::SecureStoreUnavailable => "secure_store_unavailable",
            Self::RevisionConflict { .. } => "revision_conflict",
            Self::PersistenceUnavailable { .. } => "persistence_unavailable",
            Self::NativeOperationUnsupported { .. } => "native_operation_unsupported",
            Self::NativeOperationFailed { .. } => "native_operation_failed",
            Self::Cancelled => "cancelled",
            Self::Internal { .. } => "internal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable_and_distinct() {
        let errors = [
            CommandError::InitializationPending,
            CommandError::ValidationFailed {
                field: "n".into(),
                reason: "r".into(),
            },
            CommandError::AccountNotFound,
            CommandError::UnsupportedProvider {
                provider_id: ProviderId::Codex,
            },
            CommandError::UnsupportedMethod {
                requested: "m".into(),
            },
            CommandError::ReconnectRequired,
            CommandError::PermissionDenied {
                window_label: "w".into(),
            },
            CommandError::SecureStoreUnavailable,
            CommandError::RevisionConflict {
                expected: 1,
                actual: 2,
            },
            CommandError::PersistenceUnavailable {
                owner: "sqlite".into(),
            },
            CommandError::NativeOperationUnsupported {
                operation: "o".into(),
            },
            CommandError::NativeOperationFailed {
                operation: "o".into(),
                reason: "r".into(),
            },
            CommandError::Cancelled,
            CommandError::Internal { code: "c".into() },
        ];
        let mut codes: Vec<_> = errors.iter().map(CommandError::diagnostic_code).collect();
        let total = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), total);
    }

    #[test]
    fn only_transport_level_failures_are_retryable() {
        assert!(CommandError::InitializationPending.is_retryable());
        assert!(!CommandError::ReconnectRequired.is_retryable());
        assert!(!CommandError::Cancelled.is_retryable());
    }

    #[test]
    fn the_wire_form_is_tagged_and_parseable() {
        let error = CommandError::RevisionConflict {
            expected: 3,
            actual: 4,
        };
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(serde_json::from_str::<CommandError>(&json).unwrap(), error);
        assert!(json.contains("revision_conflict"));
    }
}
