//! The refresh supervisor.
//!
//! Exactly one supervisor runs, on the runtime the host provides. It owns the
//! bounded due-work queue, the concurrency budget, the shutdown path, and the
//! rule that a result is accepted only when its binding still matches.

pub mod decision;
pub mod queue;

use std::sync::Arc;

use tokio::sync::{Semaphore, watch};

use quota_domain::ids::AccountId;

use crate::accounts::AccountRegistry;
use crate::error::CoreError;
use crate::ports::{AccountRepository, ConnectionBinding, ProviderAdapter, ReadContext};

pub use decision::{PollContext, Visibility, decide, stable_jitter};
pub use queue::{DueQueue, QueueError, Queued, ReadReason, ScheduledRead};

/// The shared concurrency budget for remote reads.
#[derive(Debug)]
pub struct ReadBudget {
    permits: Arc<Semaphore>,
    limit: u32,
}

impl ReadBudget {
    /// Creates a budget that admits at most `limit` simultaneous reads.
    #[must_use]
    pub fn new(limit: u32) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(limit.max(1) as usize)),
            limit: limit.max(1),
        }
    }

    /// The configured limit.
    #[must_use]
    pub const fn limit(&self) -> u32 {
        self.limit
    }

    /// How many slots are free right now.
    #[must_use]
    pub fn available(&self) -> usize {
        self.permits.available_permits()
    }

    /// Waits for a slot and returns a permit that releases it on drop.
    ///
    /// A timed-out wait never admits replacement work while a real read is still
    /// running, because the permit lives with the read, not with the wait.
    pub async fn acquire(&self) -> Result<tokio::sync::OwnedSemaphorePermit, CoreError> {
        self.permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| CoreError::Persistence { owner: "budget" })
    }
}

/// A cooperative shutdown signal shared by the supervisor and its workers.
#[derive(Clone, Debug)]
pub struct Shutdown {
    sender: watch::Sender<bool>,
}

impl Shutdown {
    /// Creates a shutdown signal that has not fired.
    #[must_use]
    pub fn new() -> Self {
        let (sender, _receiver) = watch::channel(false);
        Self { sender }
    }

    /// Asks every worker to stop and wakes the run loop.
    pub fn trigger(&self) {
        self.sender.send_replace(true);
    }

    /// Whether shutdown has been requested.
    #[must_use]
    pub fn is_triggered(&self) -> bool {
        *self.sender.borrow()
    }

    /// Waits until shutdown is requested.
    pub async fn wait(&self) {
        let mut receiver = self.sender.subscribe();
        while !*receiver.borrow() {
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}

/// What one supervised read produced.
#[derive(Clone, Debug, PartialEq)]
pub enum ReadResult {
    /// The reading was accepted and committed.
    Accepted {
        /// The account the reading belongs to.
        account_id: AccountId,
        /// How many windows arrived.
        window_count: usize,
    },
    /// The reading arrived after its binding was superseded.
    Rejected {
        /// The account the stale reading belonged to.
        account_id: AccountId,
    },
    /// The read failed.
    Failed {
        /// The account that failed.
        account_id: AccountId,
        /// A stable, non-identifying code.
        code: String,
    },
}

/// Runs one account's read and decides whether the result may be committed.
pub struct ReadExecutor<'a> {
    repository: &'a dyn AccountRepository,
    registry: &'a AccountRegistry,
}

impl<'a> ReadExecutor<'a> {
    /// Creates an executor over the injected ports.
    #[must_use]
    pub const fn new(repository: &'a dyn AccountRepository, registry: &'a AccountRegistry) -> Self {
        Self {
            repository,
            registry,
        }
    }

    /// Performs one read and commits it when it is still current.
    ///
    /// The durable write happens before the caller publishes, so an accepted
    /// reading is always recoverable.
    ///
    /// # Errors
    /// Returns [`CoreError::AccountNotFound`] when the registry no longer holds
    /// the account, and [`CoreError::StaleResult`] when the binding moved on.
    pub async fn run(
        &self,
        adapter: &Arc<dyn ProviderAdapter>,
        account_id: &AccountId,
        context: ReadContext,
    ) -> Result<ReadResult, CoreError> {
        let entry = self
            .registry
            .get(account_id)
            .ok_or_else(|| CoreError::AccountNotFound(account_id.clone()))?;
        let outcome = adapter.read_quota(&entry.binding, context).await;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                return Ok(ReadResult::Failed {
                    account_id: account_id.clone(),
                    code: error.diagnostic_code().to_owned(),
                });
            }
        };
        let Some(read) = outcome.read() else {
            return Ok(ReadResult::Failed {
                account_id: account_id.clone(),
                code: "no_reading".to_owned(),
            });
        };
        // The binding check is what rejects a late read after a reconnect,
        // disconnect, profile change, or account replacement.
        let current = self
            .registry
            .get(account_id)
            .ok_or(CoreError::StaleResult)?;
        if !binding_still_current(&current.binding, &entry.binding) {
            return Ok(ReadResult::Rejected {
                account_id: account_id.clone(),
            });
        }
        self.repository
            .persist_reading(
                account_id,
                &read.windows,
                read.windows.first().and_then(|w| w.observed_at),
            )
            .await
            .map_err(|error| CoreError::Persistence { owner: error.owner })?;
        Ok(ReadResult::Accepted {
            account_id: account_id.clone(),
            window_count: read.windows.len(),
        })
    }
}

fn binding_still_current(live: &ConnectionBinding, requested: &ConnectionBinding) -> bool {
    live.accepts(requested)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_budget_admits_only_its_configured_number_of_reads() {
        let budget = ReadBudget::new(2);
        assert_eq!(budget.limit(), 2);
        assert_eq!(budget.available(), 2);
        let first = budget.acquire().await.unwrap();
        let second = budget.acquire().await.unwrap();
        assert_eq!(budget.available(), 0);
        drop(first);
        assert_eq!(budget.available(), 1);
        drop(second);
        assert_eq!(budget.available(), 2);
    }

    #[tokio::test]
    async fn a_budget_never_admits_zero_reads() {
        assert_eq!(ReadBudget::new(0).limit(), 1);
    }

    #[tokio::test]
    async fn shutdown_starts_clear_and_then_reported() {
        let shutdown = Shutdown::new();
        assert!(!shutdown.is_triggered());
        shutdown.trigger();
        assert!(shutdown.is_triggered());
    }

    #[tokio::test]
    async fn a_waiter_wakes_when_shutdown_is_requested() {
        let shutdown = Shutdown::new();
        let waiter = shutdown.clone();
        let handle = tokio::spawn(async move {
            waiter.wait().await;
            true
        });
        tokio::task::yield_now().await;
        shutdown.trigger();
        assert!(handle.await.unwrap());
    }
}
