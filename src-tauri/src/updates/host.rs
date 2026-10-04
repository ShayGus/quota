//! The real [`UpdateHost`]: the updater plugin, the pop-up window, and a restart.
//!
//! The updater reads its address and public key from the `plugins.updater`
//! table of `tauri.conf.json` and from nothing else. This file never names an
//! address, a key, a target, or a header, and `cargo xtask check-release`
//! fails if it starts to.
//!
//! The pop-up is the application's own interface, not an operating-system
//! dialog: a small frameless window, created when an update is found, that loads
//! the same renderer as every other window and draws what the shared
//! [`PromptSlot`] says.

use std::time::Duration;

use quota_contracts::{UpdatePrompt, UpdatePromptChangedPayload, UpdateResponse};
use semver::Version;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder, Wry};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event as _;

use super::flow::{Found, UpdateHost};
use super::prompt::PromptSlot;
use crate::ipc::events::UpdatePromptChanged;

/// The pop-up window's label. Its capability names the same word.
const LABEL: &str = "update";

/// The pop-up window's size, in logical pixels.
const WIDTH: f64 = 440.0;
const HEIGHT: f64 = 250.0;

/// Talks to the updater plugin and the pop-up window of the running application.
pub(crate) struct TauriHost {
    app: AppHandle<Wry>,
}

impl TauriHost {
    /// A host for the running application.
    pub(crate) const fn new(app: AppHandle<Wry>) -> Self {
        Self { app }
    }

    fn slot(&self) -> State<'_, PromptSlot> {
        self.app.state::<PromptSlot>()
    }

    /// Tells the pop-up window what it shows now.
    fn publish(&self, prompt: UpdatePrompt) {
        let event = UpdatePromptChanged(UpdatePromptChangedPayload { prompt });
        if event.emit_to(&self.app, LABEL).is_err() {
            tracing::warn!("the update pop-up was not told what to show");
        }
    }

    /// Opens the pop-up window, or brings the open one to the front.
    ///
    /// The window is frameless like the settings window, centred, and on top, so
    /// a tray application that has no visible window can still be heard.
    fn open_window(&self) -> Result<(), String> {
        if let Some(window) = self.app.get_webview_window(LABEL) {
            window.show().map_err(|error| error.to_string())?;
            return window.set_focus().map_err(|error| error.to_string());
        }
        let window = WebviewWindowBuilder::new(
            &self.app,
            LABEL,
            WebviewUrl::App("index.html#/update".into()),
        )
        .title("Quota update")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .center()
        .focused(true)
        .build()
        .map_err(|error| error.to_string())?;
        let app = self.app.clone();
        window.on_window_event(move |event| match event {
            // An install that has started cannot be cancelled by closing its window.
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if app.state::<PromptSlot>().is_busy() {
                    api.prevent_close();
                }
            }
            // Any other way of closing it is Cancel, or Close on the failure.
            tauri::WindowEvent::Destroyed => app.state::<PromptSlot>().window_closed(),
            _ => {}
        });
        Ok(())
    }

    /// Empties the pop-up's state, then closes its window.
    fn close_window(&self) {
        self.slot().clear();
        if let Some(window) = self.app.get_webview_window(LABEL)
            && window.destroy().is_err()
        {
            tracing::warn!("the update pop-up could not be closed");
        }
    }

    /// Shows `prompt` in the window and waits for the person's answer.
    async fn wait_for(&self, prompt: UpdatePrompt) -> Option<UpdateResponse> {
        let answer = self.slot().present(prompt.clone());
        if let Err(reason) = self.open_window() {
            tracing::warn!(%reason, "the update pop-up could not be opened");
            self.slot().clear();
            return None;
        }
        self.publish(prompt);
        answer.await.ok()
    }
}

impl UpdateHost for TauriHost {
    type Pending = Update;

    async fn check(&self) -> Result<Option<Found<Update>>, String> {
        let builder = self.app.updater_builder().timeout(Duration::from_secs(60));
        #[cfg(feature = "sample-data")]
        let builder = super::test_endpoint::apply(builder)?;
        let updater = builder.build().map_err(|error| error.to_string())?;
        let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        let version = Version::parse(&update.version)
            .map_err(|error| format!("the published version is not valid: {error}"))?;
        let current = Version::parse(&update.current_version)
            .map_err(|error| format!("the running version is not valid: {error}"))?;
        Ok(Some(Found {
            version,
            current,
            pending: update,
        }))
    }

    async fn ask(&self, version: &Version, current: &Version) -> bool {
        let offer = UpdatePrompt::Offer {
            version: version.to_string(),
            current: current.to_string(),
        };
        if self.wait_for(offer).await == Some(UpdateResponse::Install) {
            // The window stays open and shows the install until it ends.
            let installing = UpdatePrompt::Installing {
                version: version.to_string(),
            };
            self.slot().show(installing.clone());
            self.publish(installing);
            true
        } else {
            self.close_window();
            false
        }
    }

    async fn install(&self, mut update: Update) -> Result<(), String> {
        // The pop-up shows the install as busy, with no progress. On Windows this
        // call does not return: the installer takes over and restarts Quota.
        update.timeout = Some(Duration::from_secs(15 * 60));
        update
            .download_and_install(|_chunk, _total| {}, || {})
            .await
            .map_err(|error| error.to_string())
    }

    fn relaunch(&self) {
        self.app.restart();
    }

    async fn tell_install_failed(&self) {
        // Whatever the person presses, or if the window cannot show, it ends here.
        self.wait_for(UpdatePrompt::Failed).await;
        self.close_window();
    }
}
