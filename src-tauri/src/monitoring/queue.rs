//! The shared bounded refresh queue.

use std::collections::HashSet;

use quota_contracts::CommandError;
use quota_domain::ids::AccountId;
use tokio::sync::{Mutex, mpsc};

use super::RefreshRequest;

pub(super) async fn enqueue(
    sender: &mpsc::Sender<RefreshRequest>,
    pending: &Mutex<HashSet<(AccountId, u32)>>,
    request: RefreshRequest,
) -> Result<bool, CommandError> {
    let key = (request.account_id.clone(), request.generation);
    let mut pending = pending.lock().await;
    if !pending.insert(key.clone()) {
        return Ok(false);
    }
    match sender.try_send(request) {
        Ok(()) => Ok(true),
        Err(mpsc::error::TrySendError::Full(_)) => {
            pending.remove(&key);
            Err(CommandError::ValidationFailed {
                field: "refresh_queue".into(),
                reason: "the shared refresh queue is full; try again shortly".into(),
            })
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            pending.remove(&key);
            Err(CommandError::Cancelled)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::RefreshReason;

    fn request(generation: u32, reason: RefreshReason) -> RefreshRequest {
        RefreshRequest {
            account_id: AccountId::new("synthetic-account").unwrap(),
            generation,
            reason,
        }
    }

    #[tokio::test]
    async fn a_reconnect_coalesces_new_generation_requests_but_not_later_retries() {
        let (sender, mut receiver) = mpsc::channel(4);
        let pending = Mutex::new(HashSet::new());
        assert!(
            enqueue(&sender, &pending, request(0, RefreshReason::Scheduled))
                .await
                .unwrap()
        );
        assert!(
            enqueue(&sender, &pending, request(1, RefreshReason::Reconnect))
                .await
                .unwrap()
        );
        assert!(
            !enqueue(&sender, &pending, request(1, RefreshReason::Scheduled))
                .await
                .unwrap()
        );
        assert!(
            !enqueue(&sender, &pending, request(1, RefreshReason::Reconnect))
                .await
                .unwrap()
        );
        assert_eq!(receiver.recv().await.unwrap().generation, 0);
        let verification = receiver.recv().await.unwrap();
        assert_eq!(verification.reason, RefreshReason::Reconnect);
        pending
            .lock()
            .await
            .remove(&(verification.account_id, verification.generation));
        assert!(
            enqueue(&sender, &pending, request(1, RefreshReason::Scheduled))
                .await
                .unwrap()
        );
        assert_eq!(
            receiver.recv().await.unwrap().reason,
            RefreshReason::Scheduled
        );
        receiver.try_recv().unwrap_err();
    }

    #[tokio::test]
    async fn queue_rejections_release_the_pending_key() {
        let (sender, receiver) = mpsc::channel(1);
        let pending = Mutex::new(HashSet::new());
        enqueue(&sender, &pending, request(0, RefreshReason::Scheduled))
            .await
            .unwrap();
        assert!(matches!(
            enqueue(&sender, &pending, request(1, RefreshReason::Reconnect)).await,
            Err(CommandError::ValidationFailed { .. })
        ));
        assert!(
            !pending
                .lock()
                .await
                .contains(&(AccountId::new("synthetic-account").unwrap(), 1))
        );
        drop(receiver);
        assert_eq!(
            enqueue(&sender, &pending, request(1, RefreshReason::Reconnect)).await,
            Err(CommandError::Cancelled)
        );
        assert!(
            !pending
                .lock()
                .await
                .contains(&(AccountId::new("synthetic-account").unwrap(), 1))
        );
    }
}
