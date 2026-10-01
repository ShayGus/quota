//! Application services for Quota: accounts, snapshots, scheduling, and alerts.
//!
//! This crate owns orchestration. It may use Tokio, but it must not depend on
//! Tauri, on a concrete provider adapter, on a storage plugin, or on any UI
//! transport type. Adapters implement the ports declared in [`ports`].

#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

pub mod accounts;
pub mod alerts;
pub mod clock;
pub mod error;
pub mod ports;
pub mod scheduler;
pub mod snapshots;

pub use accounts::{AccountRegistry, NewAccount, RegisteredAccount};
pub use alerts::AlertThresholds;
pub use clock::{Clock, SystemClock, TestClock};
pub use error::CoreError;
pub use ports::{
    ConnectionBinding, DiscoveredAccount, FetchOutcome, ProviderAdapter, ProviderError, QuotaRead,
    ReadContext, SnapshotPublisher, StoredAccount, WindowController,
};
pub use scheduler::{DueQueue, PollContext, ReadBudget, ReadExecutor, Shutdown, Visibility};
pub use snapshots::SnapshotBuilder;
