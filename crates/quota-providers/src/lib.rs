//! Provider adapters for Quota.
//!
//! Each module here is one connector: it discovers the credential another client
//! owns, reads that provider's quota source within a bounded HTTP boundary, and
//! normalises the payload into [`quota_domain::quota::window::QuotaWindow`]
//! values. Nothing in this crate starts a conversation, a turn, or a tool call,
//! and nothing writes to, refreshes, or rotates an externally owned credential.
//!
//! # Compiled-in providers
//!
//! [`ProviderRegistry`] holds only the adapters this build contains. A provider
//! with no compiled adapter has no entry, so a caller receives `None` and the
//! application reports an explicit unsupported-provider state. There is no
//! dynamic adapter lookup and no downloaded parser code.
//!
//! # Schema risk
//!
//! Every endpoint these adapters read is undocumented. The field spellings,
//! endpoint paths, and payload shapes are recorded in `README.md` together with
//! whether each reading is verified against a live login or only assumed. The
//! decoders tolerate unknown fields and accept every documented spelling of every
//! field, so an added provider field cannot fail a parse or silently change what
//! a number means.
//!
//! # Credentials
//!
//! Credential files belong to the Codex CLI, Claude Code, and the local
//! `OpenCode` login. This crate reads them, never writes them, and never logs a
//! token, a cookie, an address, a profile path, a request body, or a full URL.

#![forbid(unsafe_code)]

pub mod credentials;
pub(crate) mod decode;
pub(crate) mod http;
pub mod offline;
pub mod registry;

pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod opencode_go;
pub(crate) mod platform_paths;

#[cfg(feature = "test-fixtures")]
pub mod fixture;

pub use offline::{OfflineReading, decode_offline};
pub use registry::ProviderRegistry;

#[cfg(feature = "test-fixtures")]
pub use fixture::{FixtureAdapter, FixtureProfile};
