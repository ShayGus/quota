//! Tagged identifier wrappers used in command arguments.
//!
//! A generated string alias alone would let a connection identifier be passed
//! where an account identifier is expected. Each wrapper carries its own tag, so
//! the exported TypeScript types are distinct and the mistake fails to compile.

use serde::{Deserialize, Serialize};
use specta::Type;

use quota_domain::ids::{AccountId, ConnectionAttemptId, ConnectionId};

macro_rules! tagged_ref {
    ($(#[$meta:meta])* $name:ident, $inner:ty) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
        pub struct $name {
            id: $inner,
        }

        impl $name {
            /// Wraps a validated identifier.
            #[must_use]
            pub const fn new(id: $inner) -> Self {
                Self { id }
            }

            /// Borrows the wrapped identifier.
            #[must_use]
            pub const fn id(&self) -> &$inner {
                &self.id
            }

            /// Unwraps the identifier.
            #[must_use]
            pub fn into_id(self) -> $inner {
                self.id
            }
        }

        impl From<$inner> for $name {
            fn from(id: $inner) -> Self {
                Self::new(id)
            }
        }
    };
}

tagged_ref!(
    /// A monitored account, tagged so it cannot be confused with a connection.
    AccountRef,
    AccountId
);
tagged_ref!(
    /// An authorization connection, tagged so it cannot be confused with an account.
    ConnectionRef,
    ConnectionId
);
tagged_ref!(
    /// One connection attempt.
    AttemptRef,
    ConnectionAttemptId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_round_trips_through_its_tag() {
        let reference = AccountRef::new(AccountId::new("acct-1").unwrap());
        let json = serde_json::to_string(&reference).unwrap();
        assert_eq!(json, "{\"id\":\"acct-1\"}");
        assert_eq!(
            serde_json::from_str::<AccountRef>(&json).unwrap(),
            reference
        );
    }

    #[test]
    fn each_wrapper_unwraps_to_its_own_identifier_type() {
        let connection = ConnectionRef::new(ConnectionId::new("conn-1").unwrap());
        assert_eq!(connection.id().as_str(), "conn-1");

        let account = AccountRef::new(AccountId::new("acct-1").unwrap());
        assert_eq!(account.into_id().as_str(), "acct-1");
    }
}
