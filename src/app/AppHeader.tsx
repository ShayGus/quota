/**
 * The window headers.
 *
 * The popover header carries refresh, the pin, the switch to the mini widget,
 * Report a bug, settings, and hide. Refresh, the pin, settings and hide are as the wireframe
 * draws them. Pinning turns the tray popover into a floating window
 * that stays open and moves by its header; keeping it on top is a separate
 * setting, so pinning never changes topmost (spec 4.4). The pin's state is
 * also stated in words, so `aria-pressed` is never the only signal.
 */
import type { JSX } from "react";

import type { RendererState } from "../shared/state/types";
import { Icon, Logo } from "../shared/ui/Icon";
import { ReportBugMenu, type ReportBugActions } from "../shared/ui/ReportBug";
import { launch } from "../shared/ipc/report";
import { actions } from "./actions";

/**
 * Marks an element as a handle that moves the native window.
 *
 * Only the element under the pointer is checked, so each non-interactive part
 * of a header carries the mark rather than the header alone.
 */
function dragRegion(enabled: boolean): { "data-tauri-drag-region"?: true } {
  return enabled ? { "data-tauri-drag-region": true } : {};
}

/** The settings window's header. */
export function SettingsHeader(): JSX.Element {
  return (
    <header className="settings-head" {...dragRegion(true)}>
      <div {...dragRegion(true)}>
        <h2 {...dragRegion(true)}>Quota settings</h2>
        <p {...dragRegion(true)}>Saved on this device · applied immediately</p>
      </div>
      <button
        type="button"
        className="icon-btn"
        aria-label="Close settings"
        title="Close settings"
        onClick={() => {
          launch(actions.closeWindow());
        }}
      >
        <Icon name="close" />
      </button>
    </header>
  );
}

/** The popover's header. */
export function AppHeader({
  state,
  onSettings,
  onRefresh,
  report,
}: {
  readonly state: RendererState;
  readonly onSettings: () => void;
  readonly onRefresh: () => void;
  readonly report: ReportBugActions;
}): JSX.Element {
  const pinned = (state.preferences?.overview_mode ?? "tray") === "floating";
  const paused = state.monitoring?.kind === "paused";
  const refreshing =
    state.snapshot?.accounts.some((account) => account.fetch_state === "fetching") ??
    false;
  return (
    <header className="app-header" {...dragRegion(pinned)}>
      <div className="app-brand" {...dragRegion(pinned)}>
        <Logo />
        <div {...dragRegion(pinned)}>
          <h1 {...dragRegion(pinned)}>Quota</h1>
          <p {...dragRegion(pinned)}>Your AI subscriptions</p>
        </div>
      </div>
      <div className="app-actions">
        {pinned ? <span className="pin-label">Floating</span> : null}
        <button
          type="button"
          className="icon-btn"
          aria-label="Refresh readings"
          title="Refresh readings"
          disabled={paused || refreshing}
          onClick={onRefresh}
        >
          <Icon name="refresh" />
        </button>
        <button
          type="button"
          className={`icon-btn${pinned ? " active" : ""}`}
          aria-pressed={pinned}
          aria-label={pinned ? "Dock to the tray" : "Float as a separate window"}
          title={pinned ? "Dock to the tray" : "Float as a separate window"}
          onClick={() => {
            launch(actions.setOverviewMode(pinned ? "tray" : "floating"));
          }}
        >
          <Icon name="pin" />
        </button>
        <button
          type="button"
          className="icon-btn"
          aria-label="Switch to the mini widget"
          title="Switch to the mini widget"
          onClick={() => {
            launch(actions.setAppView("widget"));
          }}
        >
          <Icon name="shrink" />
        </button>
        <ReportBugMenu actions={report} variant="header" />
        <button
          type="button"
          className="icon-btn"
          aria-label="Settings"
          title="Settings"
          onClick={onSettings}
        >
          <Icon name="settings" />
        </button>
        <button
          type="button"
          className="icon-btn"
          aria-label="Hide popover"
          title="Hide popover"
          onClick={() => {
            launch(actions.closeWindow());
          }}
        >
          <Icon name="close" />
        </button>
      </div>
    </header>
  );
}
