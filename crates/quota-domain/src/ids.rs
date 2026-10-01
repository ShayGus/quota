//! Opaque identifier newtypes.
//!
//! Every identifier is validated on construction. Deserialization goes through
//! the same constructor, so a decoded value cannot bypass the invariant.

use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::DomainError;

/// Maximum accepted length of an opaque identifier, in characters.
pub const MAX_ID_LEN: usize = 128;

macro_rules! validated_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(Box<str>);

        impl $name {
            /// The field name reported when validation fails.
            pub const FIELD: &'static str = stringify!($name);

            /// Validates and wraps an identifier.
            ///
            /// # Errors
            /// Returns [`DomainError::InvalidIdentifier`] for empty or blank text, and
            /// [`DomainError::TooLong`] beyond `MAX_ID_LEN`.
            pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
                let raw = raw.into();
                if raw.trim().is_empty() {
                    return Err(DomainError::InvalidIdentifier { field: Self::FIELD });
                }
                if raw.chars().count() > $crate::ids::MAX_ID_LEN {
                    return Err(DomainError::TooLong {
                        field: Self::FIELD,
                        max: $crate::ids::MAX_ID_LEN,
                    });
                }
                Ok(Self(raw.into_boxed_str()))
            }

            /// Generates a fresh random identifier.
            #[must_use]
            pub fn generate() -> Self {
                Self(uuid::Uuid::new_v4().to_string().into_boxed_str())
            }

            /// Borrows the underlying text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = DomainError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0.into_string()
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), &*self.0)
            }
        }
    };
}

pub(crate) use validated_id;

validated_id!(
    /// A monitored subscription account.
    AccountId
);
validated_id!(
    /// An authorization connection owned by, or referenced by, the application.
    ConnectionId
);
validated_id!(
    /// One attempt to establish a connection.
    ConnectionAttemptId
);
validated_id!(
    /// A provider-confirmed shared allowance owner, or an opaque isolated pool.
    QuotaPoolId
);
validated_id!(
    /// A stable pool, scope, metric and period definition.
    QuotaWindowId
);
validated_id!(
    /// Identifies one running application instance, so restarts never reuse revisions.
    AppInstanceId
);
validated_id!(
    /// A provider-verified principal, such as the signed-in user.
    ProviderPrincipalId
);
validated_id!(
    /// A provider workspace or team within a principal.
    WorkspaceId
);
validated_id!(
    /// A provider-reported plan or entitlement context.
    EntitlementId
);
validated_id!(
    /// A metered resource such as a model or product surface.
    ResourceId
);

/// A version of a window or account definition, as reported by the provider.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
pub struct DefinitionVersion(pub u32);

impl DefinitionVersion {
    /// The initial version, used when a provider reports no version of its own.
    pub const INITIAL: Self = Self(1);
}

impl Default for DefinitionVersion {
    fn default() -> Self {
        Self::INITIAL
    }
}

impl Display for DefinitionVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_identifier() {
        assert_eq!(
            AccountId::new("   ").unwrap_err(),
            DomainError::InvalidIdentifier { field: "AccountId" }
        );
    }

    #[test]
    fn rejects_oversized_identifier() {
        let long = "a".repeat(MAX_ID_LEN + 1);
        assert_eq!(
            AccountId::new(long).unwrap_err(),
            DomainError::TooLong {
                field: "AccountId",
                max: MAX_ID_LEN
            }
        );
    }

    #[test]
    fn accepts_maximum_length() {
        assert!(AccountId::new("a".repeat(MAX_ID_LEN)).is_ok());
    }

    #[test]
    fn deserialization_enforces_the_same_invariant() {
        let ok: Result<AccountId, _> = serde_json::from_str("\"acct-1\"");
        assert_eq!(ok.unwrap().as_str(), "acct-1");
        assert!(serde_json::from_str::<AccountId>("\"\"").is_err());
        let long = format!("\"{}\"", "a".repeat(MAX_ID_LEN + 1));
        assert!(serde_json::from_str::<AccountId>(&long).is_err());
    }

    #[test]
    fn generated_identifiers_are_valid() {
        assert!(ConnectionId::generate().as_str().contains('-'));
    }
}
