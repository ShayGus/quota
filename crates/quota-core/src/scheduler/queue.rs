//! The bounded due-work queue.
//!
//! Two different kinds of traffic pass through the supervisor and they need
//! different guarantees. Latest-snapshot notifications may coalesce; account
//! mutations and worker completions may not disappear. This queue therefore
//! coalesces by account, applies explicit backpressure when it is full, and
//! never blocks shutdown.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tokio::sync::mpsc;

use quota_domain::ids::AccountId;

/// One unit of scheduled work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledRead {
    /// The account to read.
    pub account_id: AccountId,
    /// The binding generation the read belongs to.
    pub generation: u32,
    /// The earliest instant the read may start.
    pub not_before: DateTime<Utc>,
    /// Why the read was scheduled, for diagnostics.
    pub reason: ReadReason,
}

/// Why a read was scheduled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadReason {
    /// The policy's ordinary interval elapsed.
    Interval,
    /// A reported boundary needs verification.
    BoundaryVerification,
    /// The user asked for a refresh.
    UserRequested,
}

/// Why an enqueue did not happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueError {
    /// The queue is full and the caller must decide whether to drop or wait.
    Full,
    /// The supervisor is shutting down.
    ShuttingDown,
}

/// A bounded queue that coalesces repeated work for the same account.
///
/// Capacity is a hard bound: memory cannot grow without limit when a provider
/// is slow or a user clicks refresh repeatedly.
#[derive(Debug)]
pub struct DueQueue {
    sender: mpsc::Sender<ScheduledRead>,
    receiver: tokio::sync::Mutex<mpsc::Receiver<ScheduledRead>>,
    capacity: usize,
    coalesced: HashMap<AccountId, ScheduledRead>,
}

impl DueQueue {
    /// Creates a queue with a fixed capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity.max(1));
        Self {
            sender,
            receiver: tokio::sync::Mutex::new(receiver),
            capacity: capacity.max(1),
            coalesced: HashMap::new(),
        }
    }

    /// The hard capacity.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Queues one read, coalescing with any earlier pending read for the same
    /// account by keeping the earlier due time.
    ///
    /// # Errors
    /// Returns [`QueueError::Full`] when the queue is at capacity, and
    /// [`QueueError::ShuttingDown`] once the receiver is gone.
    pub fn enqueue(&mut self, read: ScheduledRead) -> Result<Queued, QueueError> {
        if let Some(pending) = self.coalesced.get_mut(&read.account_id) {
            pending.not_before = pending.not_before.min(read.not_before);
            pending.generation = read.generation;
            return Ok(Queued::Coalesced);
        }
        match self.sender.try_send(read.clone()) {
            Ok(()) => {
                self.coalesced.insert(read.account_id.clone(), read);
                Ok(Queued::Accepted)
            }
            Err(mpsc::error::TrySendError::Full(_)) => Err(QueueError::Full),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(QueueError::ShuttingDown),
        }
    }
    /// Takes the next due read, removing it from the coalescing table.
    pub async fn take(&mut self) -> Option<ScheduledRead> {
        let mut read = self.receiver.lock().await.recv().await?;
        if let Some(pending) = self.coalesced.remove(&read.account_id) {
            read.not_before = read.not_before.min(pending.not_before);
            read.generation = pending.generation;
        }
        Some(read)
    }
}

/// What happened to an enqueue attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Queued {
    /// The read was added to the queue.
    Accepted,
    /// The read merged with one already pending for the same account.
    Coalesced,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn read(account: &str, not_before: DateTime<Utc>) -> ScheduledRead {
        ScheduledRead {
            account_id: AccountId::new(account).unwrap(),
            generation: 1,
            not_before,
            reason: ReadReason::Interval,
        }
    }

    #[tokio::test]
    async fn repeated_work_for_one_account_coalesces_into_one_read() {
        let mut queue = DueQueue::new(4);
        let now = DateTime::UNIX_EPOCH;
        assert_eq!(
            queue
                .enqueue(read("a", now + Duration::minutes(5)))
                .unwrap(),
            Queued::Accepted
        );
        assert_eq!(
            queue
                .enqueue(read("a", now + Duration::minutes(1)))
                .unwrap(),
            Queued::Coalesced
        );
        let taken = queue.take().await.unwrap();
        assert_eq!(taken.not_before, now + Duration::minutes(1));
    }

    #[tokio::test]
    async fn a_full_queue_reports_backpressure_instead_of_growing() {
        let mut queue = DueQueue::new(1);
        let now = DateTime::UNIX_EPOCH;
        assert_eq!(queue.enqueue(read("a", now)).unwrap(), Queued::Accepted);
        assert_eq!(queue.enqueue(read("b", now)).unwrap_err(), QueueError::Full);
    }

    #[tokio::test]
    async fn distinct_accounts_are_kept_separate() {
        let mut queue = DueQueue::new(4);
        let now = DateTime::UNIX_EPOCH;
        queue.enqueue(read("a", now)).unwrap();
        queue
            .enqueue(read("b", now + Duration::minutes(1)))
            .unwrap();
        let first = queue.take().await.unwrap();
        let second = queue.take().await.unwrap();
        assert_eq!(first.account_id.as_str(), "a");
        assert_eq!(second.account_id.as_str(), "b");
    }
}
