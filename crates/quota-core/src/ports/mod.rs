//! The interfaces the application depends on.
//!
//! Everything the core needs from the outside world is declared here as a
//! trait. The core never imports a concrete provider, a storage plugin, or a
//! notification implementation.

pub mod provider;
pub mod publisher;
pub mod repository;

pub use provider::{
    ConnectionBinding, DiscoveredAccount, FetchOutcome, ProviderAdapter, ProviderError,
    ProviderFuture, QuotaRead, ReadContext,
};
pub use publisher::{NativeError, PublishError, SnapshotPublisher, WindowController};
pub use repository::{
    AccountRepository, AlertEpisode, AlertLevel, BackoffRepository, BackoffState,
    HistoryRepository, PreferenceRepository, RepositoryError, StoredAccount,
    default_history_retention,
};
