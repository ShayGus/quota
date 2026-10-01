//! Durable state for Quota.
//!
//! This crate owns two durable owners and nothing else:
//!
//! - [`sqlite`] holds typed repositories over one SQLite pool. The pool is
//!   opened and migrated once by the caller; cloning the handle never opens a
//!   second pool and never claims a second migration owner.
//! - [`store`] holds the versioned presentation-preferences document. It is
//!   non-transactional by nature, so no setting that must commit together with
//!   monitoring or alert state lives there.
//!
//! `tauri-plugin-sql` hosts the database, so the desktop host resolves the
//! plugin's pool and injects it with [`sqlite::SqliteRepositories::new`]. The
//! preference store is reached through [`store::PreferenceDocumentStore`], and
//! the plugin-backed implementation of that trait lives behind the
//! `tauri-plugins` feature.
//!
//! No token, cookie, authorization header, or credential locator belongs in
//! either owner.

#![forbid(unsafe_code)]

pub mod error;
pub mod sqlite;
pub mod store;

pub use error::{PersistenceError, PersistenceResult};
pub use sqlite::{SqlitePoolSettings, SqliteRepositories};
pub use store::PresentationPreferencesCodec;
