/**
 * The window header.
 *
 * Three separate controls, per spec 4.4: the window mode, the independent
 * always-on-top pin, and the search/settings actions. The pin's selected state
 * is visible and textual, so `aria-pressed` is never the only signal.
 */
import type { JSX } from "react";

import type { RendererState } from "../shared/state/types";
import { Icon, Logo } from "../shared/ui/Icon";
import { launch } from "../shared/ipc/report";
import { actions } from "./actions";

/** The header, including the independent always-on-top control. */
export function AppHeader({
  state,
  view,
  onSettings,
  onOverview,
}: {
  readonly state: RendererState;
  readonly view: "overview" | "detail" | "settings";
  readonly onSettings: () => void;
  readonly onOverview: () => void;
}): JSX.Element {
  const accountCount = state.snapshot?.accounts.length ?? 0;
  const providerCount =
    state.snapshot === null
      ? 0
      : new Set(state.snapshot.accounts.map((account) => account.provider_id)).size;
  const alwaysOnTop = state.preferences?.always_on_top ?? false;
  const mode = state.preferences?.overview_mode ?? "floating";
  const paused = state.monitoring?.kind === "paused";
  return (
    <header className="shell__header">
      <div className="shell__brand">
        <Logo size={27} />
        <div>
          <h1>Quota</h1>
          <p>
            {String(accountCount)} accounts · {String(providerCount)} providers
          </p>
        </div>
      </div>
      <div className="shell__actions">
        {alwaysOnTop ? <span className="shell__top-label">Always on top</span> : null}
        <span className="mode-control">
          <Icon name={mode === "floating" ? "layers" : "external"} size={14} />
          {mode === "floating" ? "Floating" : "Tray"}
        </span>
        <button
          type="button"
          className="icon-button"
          aria-pressed={alwaysOnTop}
          aria-label={alwaysOnTop ? "Turn off always on top" : "Keep the window on top"}
          title={alwaysOnTop ? "Always on top — on" : "Always on top — off"}
          onClick={() => {
            launch(actions.setAlwaysOnTop(!alwaysOnTop));
          }}
        >
          <Icon name="pin" size={17} />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label="Refresh the readings now"
          title="Refresh the readings now"
          disabled={paused}
          onClick={() => {
            launch(actions.refresh("user_requested"));
          }}
        >
          <Icon name="refresh" size={17} />
        </button>
        {view === "overview" ? (
          <button
            type="button"
            className="icon-button"
            aria-label="Open settings"
            title="Settings"
            onClick={onSettings}
          >
            <Icon name="settings" size={17} />
          </button>
        ) : (
          <button
            type="button"
            className="icon-button"
            aria-label="Back to the overview"
            title="Back to the overview"
            onClick={onOverview}
          >
            <Icon name="donut" size={17} />
          </button>
        )}
      </div>
    </header>
  );
}
