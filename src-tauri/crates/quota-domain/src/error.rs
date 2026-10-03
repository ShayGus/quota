//! Typed failures for domain construction and validation.

use thiserror::Error;

/// Why a value could not become a valid domain value.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    /// An identifier was empty, blank, or longer than [`crate::ids::MAX_ID_LEN`].
    #[error("`{field}` must be 1..={max} characters of non-blank text", max = crate::ids::MAX_ID_LEN)]
    InvalidIdentifier {
        /// Name of the rejected field.
        field: &'static str,
    },
    /// A numeric value was NaN, infinite, or outside its permitted range.
    #[error("`{field}` is not a usable value: {reason}")]
    InvalidNumber {
        /// Name of the rejected field.
        field: &'static str,
        /// Why the value was rejected.
        reason: &'static str,
    },
    /// A unit, category, or scope value was not part of its closed vocabulary.
    #[error("`{field}` is not a supported value")]
    UnsupportedValue {
        /// Name of the rejected field.
        field: &'static str,
    },
    /// A string exceeded the documented length budget for its field.
    #[error("`{field}` exceeds its maximum length of {max} characters")]
    TooLong {
        /// Name of the rejected field.
        field: &'static str,
        /// Maximum permitted length.
        max: usize,
    },
}
