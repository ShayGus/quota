//! What the update pop-up shows, and who is waiting for its answer.
//!
//! The flow presents an offer and waits; the pop-up window reads the current
//! prompt and sends an answer back through the two commands in
//! `ipc::commands_update`. Only an answer that makes sense for what is on screen
//! is accepted: an offer takes OK or Cancel, the failure message takes Close, and
//! nothing is accepted while an install is running.

use std::sync::{Mutex, PoisonError};

use quota_contracts::{UpdatePrompt, UpdateResponse};
use tokio::sync::oneshot;

/// The pop-up's state, shared by the flow and the commands.
#[derive(Default)]
pub struct PromptSlot {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    prompt: Option<UpdatePrompt>,
    waiting: Option<oneshot::Sender<UpdateResponse>>,
}

impl PromptSlot {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // The state is two plain values, so a panic elsewhere cannot leave it
        // half-written.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What the pop-up shows now, or nothing when it is closed.
    #[must_use]
    pub fn current(&self) -> Option<UpdatePrompt> {
        self.lock().prompt.clone()
    }

    /// Shows `prompt` and returns where its answer will arrive. Anyone waiting
    /// on an earlier prompt is released without an answer.
    pub(crate) fn present(&self, prompt: UpdatePrompt) -> oneshot::Receiver<UpdateResponse> {
        let (sender, receiver) = oneshot::channel();
        let mut inner = self.lock();
        inner.prompt = Some(prompt);
        inner.waiting = Some(sender);
        receiver
    }

    /// Shows `prompt`, which takes no answer.
    pub(crate) fn show(&self, prompt: UpdatePrompt) {
        let mut inner = self.lock();
        inner.prompt = Some(prompt);
        inner.waiting = None;
    }

    /// Closes the pop-up's state.
    pub(crate) fn clear(&self) {
        let mut inner = self.lock();
        inner.prompt = None;
        inner.waiting = None;
    }

    /// Delivers an answer from the pop-up. `false` means it was not accepted:
    /// nobody is waiting, or the answer does not belong to what is shown.
    pub fn respond(&self, response: UpdateResponse) -> bool {
        let mut inner = self.lock();
        let fits = matches!(
            (&inner.prompt, response),
            (
                Some(UpdatePrompt::Offer { .. }),
                UpdateResponse::Install | UpdateResponse::Decline
            ) | (Some(UpdatePrompt::Failed), UpdateResponse::Dismiss)
        );
        if !fits {
            return false;
        }
        inner
            .waiting
            .take()
            .is_some_and(|sender| sender.send(response).is_ok())
    }

    /// The window was closed some other way than by a button: that is Cancel on
    /// an offer and Close on the failure message.
    pub(crate) fn window_closed(&self) {
        let response = match self.current() {
            Some(UpdatePrompt::Offer { .. }) => UpdateResponse::Decline,
            Some(UpdatePrompt::Failed) => UpdateResponse::Dismiss,
            Some(UpdatePrompt::Installing { .. }) | None => return,
        };
        self.respond(response);
    }

    /// Whether the window must stay open: an install is running in it.
    pub(crate) fn is_busy(&self) -> bool {
        matches!(self.current(), Some(UpdatePrompt::Installing { .. }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer() -> UpdatePrompt {
        UpdatePrompt::Offer {
            version: "0.2.0".to_owned(),
            current: "0.1.0".to_owned(),
        }
    }

    #[test]
    fn a_new_slot_shows_nothing() {
        assert_eq!(PromptSlot::default().current(), None);
    }

    #[test]
    fn an_offer_is_shown_and_ok_reaches_the_waiting_flow() {
        let slot = PromptSlot::default();
        let mut answer = slot.present(offer());
        assert_eq!(slot.current(), Some(offer()));
        assert!(slot.respond(UpdateResponse::Install));
        assert_eq!(answer.try_recv().unwrap(), UpdateResponse::Install);
    }

    #[test]
    fn cancel_reaches_the_waiting_flow() {
        let slot = PromptSlot::default();
        let mut answer = slot.present(offer());
        assert!(slot.respond(UpdateResponse::Decline));
        assert_eq!(answer.try_recv().unwrap(), UpdateResponse::Decline);
    }

    #[test]
    fn an_offer_does_not_take_the_failure_answer() {
        let slot = PromptSlot::default();
        let mut answer = slot.present(offer());
        assert!(!slot.respond(UpdateResponse::Dismiss));
        assert_eq!(answer.try_recv(), Err(oneshot::error::TryRecvError::Empty));
    }

    #[test]
    fn the_failure_message_takes_only_close() {
        let slot = PromptSlot::default();
        let mut answer = slot.present(UpdatePrompt::Failed);
        assert!(!slot.respond(UpdateResponse::Install));
        assert!(!slot.respond(UpdateResponse::Decline));
        assert!(slot.respond(UpdateResponse::Dismiss));
        assert_eq!(answer.try_recv().unwrap(), UpdateResponse::Dismiss);
    }

    #[test]
    fn nothing_is_accepted_while_an_install_runs() {
        let slot = PromptSlot::default();
        slot.show(UpdatePrompt::Installing {
            version: "0.2.0".to_owned(),
        });
        for response in [
            UpdateResponse::Install,
            UpdateResponse::Decline,
            UpdateResponse::Dismiss,
        ] {
            assert!(!slot.respond(response));
        }
        assert!(slot.is_busy());
    }

    #[test]
    fn an_answer_is_accepted_once() {
        let slot = PromptSlot::default();
        let _answer = slot.present(offer());
        assert!(slot.respond(UpdateResponse::Decline));
        assert!(!slot.respond(UpdateResponse::Decline));
    }

    #[test]
    fn closing_the_window_is_cancel_on_an_offer_and_close_on_a_failure() {
        let slot = PromptSlot::default();
        let mut answer = slot.present(offer());
        slot.window_closed();
        assert_eq!(answer.try_recv().unwrap(), UpdateResponse::Decline);

        let mut answer = slot.present(UpdatePrompt::Failed);
        slot.window_closed();
        assert_eq!(answer.try_recv().unwrap(), UpdateResponse::Dismiss);
    }

    #[test]
    fn closing_the_window_during_an_install_changes_nothing() {
        let slot = PromptSlot::default();
        slot.show(UpdatePrompt::Installing {
            version: "0.2.0".to_owned(),
        });
        slot.window_closed();
        assert!(slot.is_busy());
    }

    #[test]
    fn a_cleared_slot_shows_nothing_and_takes_no_answer() {
        let slot = PromptSlot::default();
        let _answer = slot.present(offer());
        slot.clear();
        assert_eq!(slot.current(), None);
        assert!(!slot.respond(UpdateResponse::Decline));
        assert!(!slot.is_busy());
    }

    #[test]
    fn a_second_offer_releases_the_first_waiter_without_an_answer() {
        let slot = PromptSlot::default();
        let mut first = slot.present(offer());
        let _second = slot.present(offer());
        assert!(matches!(
            first.try_recv(),
            Err(oneshot::error::TryRecvError::Closed)
        ));
    }
}
