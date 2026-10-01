//! The resource scope a quota window meters.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::DomainError;
use crate::ids::ResourceId;

/// Maximum accepted length of a scope display label, in characters.
pub const MAX_SCOPE_LABEL_LEN: usize = 80;

/// A metered resource plus the label shown beside it.
///
/// The scope must survive every mapping from provider payload to renderer, so it
/// travels with the window rather than being recovered from a label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct QuotaScope {
    resource: ResourceId,
    label: String,
}

impl QuotaScope {
    /// Builds a scope from a validated resource identifier and a display label.
    ///
    /// # Errors
    /// Returns [`DomainError::TooLong`] when the label exceeds
    /// [`MAX_SCOPE_LABEL_LEN`].
    pub fn new(resource: ResourceId, label: impl Into<String>) -> Result<Self, DomainError> {
        let label = label.into();
        if label.chars().count() > MAX_SCOPE_LABEL_LEN {
            return Err(DomainError::TooLong {
                field: "QuotaScope.label",
                max: MAX_SCOPE_LABEL_LEN,
            });
        }
        Ok(Self { resource, label })
    }

    /// The validated resource identifier.
    #[must_use]
    pub const fn resource(&self) -> &ResourceId {
        &self.resource
    }

    /// The display label, possibly empty.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_an_empty_label() {
        let scope = QuotaScope::new(ResourceId::new("model_x").unwrap(), "").unwrap();
        assert_eq!(scope.label(), "");
    }

    #[test]
    fn rejects_an_oversized_label() {
        let long = "x".repeat(MAX_SCOPE_LABEL_LEN + 1);
        assert_eq!(
            QuotaScope::new(ResourceId::new("m").unwrap(), long).unwrap_err(),
            DomainError::TooLong {
                field: "QuotaScope.label",
                max: MAX_SCOPE_LABEL_LEN
            }
        );
    }
}
