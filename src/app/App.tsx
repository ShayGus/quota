/**
 * Quota's application root.
 *
 * The renderer owns presentation state only: the current view. Every account
 * value comes from the snapshot store, and every mutation is a typed command
 * (spec 7.8.3).
 */
import { useState, type JSX } from "react";

import { AccountDetail } from "../features/accounts/AccountDetail";
import {
  Overview,
  overviewPlacements,
  overviewRows,
} from "../features/overview/Overview";
import type { OverviewFilter } from "../features/overview/OverviewToolbar";
import { Settings, type SettingsActions } from "../features/settings/Settings";
import type { AccountId } from "../generated/bindings";
import { launch } from "../shared/ipc/report";
import { useRendererState } from "../shared/state/useRendererState";
import { useNow } from "../shared/ui/useNow";
import { actions } from "./actions";
import { Icon } from "../shared/ui/Icon";
import { AppHeader } from "./AppHeader";
import { AppBoundary, FeatureBoundary } from "./ErrorBoundary";
import { useSnapshotSubscription } from "./useSnapshotSubscription";
import { useTheme } from "./useTheme";
import { displayName } from "../shared/format/alias";

/**
 * Which surface the overview window is showing.
 *
 * Settings has no entry here: it belongs to its own window, whose capability is
 * the only one that may save preferences. The overview asks the host to show it.
 */
type View =
  { readonly name: "overview" } | { readonly name: "detail"; readonly id: AccountId };

/** The settings actions, wired to the typed commands. */
const settingsActions: SettingsActions = {
  savePreferences: (next) => {
    launch(actions.savePreferences(next));
  },
  setAlwaysOnTop: (alwaysOnTop) => {
    launch(actions.setAlwaysOnTop(alwaysOnTop));
  },
  setOverviewMode: (mode) => {
    launch(actions.setOverviewMode(mode));
  },
  fitToAccounts: () => {
    launch(actions.fitToAccounts());
  },
  resetPosition: () => {
    launch(actions.resetPosition());
  },
  setAccountEnabled: (accountId, enabled) => {
    launch(actions.setAccountEnabled(accountId, enabled));
  },
  renameAccount: (accountId, nickname) => {
    launch(actions.renameAccount(accountId, nickname));
  },
  disconnectAccount: (accountId) => {
    launch(actions.disconnectAccount(accountId));
  },
  openUsagePage: (accountId) => {
    launch(actions.openUsagePage(accountId));
  },
  beginConnection: (request) => actions.beginConnection(request),
  clearConnectionAttempt: (attemptId) => {
    actions.clearConnectionAttempt(attemptId);
  },
  reconnectAccount: (accountId) => actions.reconnectAccount(accountId),
  clearHistory: (accountId) => {
    launch(actions.clearHistory(accountId));
  },
  exportDiagnostics: (destination) => {
    launch(actions.exportDiagnostics(destination));
  },
};

/** The window root, inside the application-level boundary. */
export function App(): JSX.Element {
  useSnapshotSubscription();
  return (
    <AppBoundary>
      <QuotaWindow />
    </AppBoundary>
  );
}

/** The window. */
function QuotaWindow(): JSX.Element {
  const state = useRendererState();
  // The settings window is opened with this hash and is the only window whose
  // capability may save preferences, so it is the only window that renders it.
  const isSettingsWindow = window.location.hash === "#/settings";
  const [view, setView] = useState<View>({ name: "overview" });
  // The filter and the search text belong to the window rather than the list,
  // because the footer states how many accounts the filter leaves visible.
  const [filter, setFilter] = useState<OverviewFilter>("all");
  const [search, setSearch] = useState("");
  const now = useNow();
  useTheme(state);

  const account =
    view.name === "detail" && state.snapshot !== null
      ? (state.snapshot.accounts.find((candidate) => candidate.account_id === view.id) ??
        null)
      : null;

  return (
    <div className="shell">
      <AppHeader
        state={state}
        view={view.name}
        onSettings={() => {
          launch(actions.openSettings());
        }}
        onOverview={() => {
          setView({ name: "overview" });
        }}
      />
      <main className="shell__main">
        {isSettingsWindow ? (
          <FeatureBoundary surface="settings">
            <Settings state={state} actions={settingsActions} />
          </FeatureBoundary>
        ) : account === null ? (
          <FeatureBoundary surface="overview">
            <Overview
              filter={filter}
              onFilter={setFilter}
              search={search}
              onSearch={setSearch}
              onFit={() => {
                launch(actions.fitToAccounts());
              }}
              onAddAccount={() => {
                launch(actions.openSettings());
              }}
              onSavePreferences={(next) => {
                launch(actions.savePreferences(next));
              }}
              state={state}
              onOpenAccount={(accountId) => {
                setView({ name: "detail", id: accountId });
              }}
              onReconnect={(accountId) => {
                // Reconnecting re-verifies the credential under a new generation.
                // Opening the detail view is a different thing and does not do it.
                launch(actions.reconnectAccount(accountId));
              }}
            />
          </FeatureBoundary>
        ) : (
          <FeatureBoundary surface="account details">
            <AccountDetail
              account={account}
              label={displayName(
                state.preferences,
                state.snapshot?.accounts ?? [],
                account,
              )}
              now={now}
              onBack={() => {
                setView({ name: "overview" });
              }}
            />
          </FeatureBoundary>
        )}
      </main>
      <WindowFooter
        counts={
          isSettingsWindow || account !== null
            ? null
            : {
                visible: overviewRows(state, filter, search).length,
                total: overviewPlacements(state).length,
              }
        }
      />
    </div>
  );
}

/** The window footer: what the backend is doing, and how much is on screen. */
function WindowFooter({
  counts,
}: {
  /** How many accounts the filter leaves visible, or `null` off the overview. */
  readonly counts: { readonly visible: number; readonly total: number } | null;
}): JSX.Element {
  const state = useRendererState();
  const link = state.link;
  const paused = state.monitoring?.kind === "paused";
  const label =
    link === "connecting"
      ? "Connecting to Quota"
      : link === "reconciling"
        ? "Reconciling with Quota"
        : link === "unavailable"
          ? "Quota is not reachable"
          : paused
            ? "Paused · last known values"
            : "Monitoring active";
  return (
    <footer className="shell__footer">
      <span className="shell__state">
        <Icon name={paused ? "pause" : "shield"} size={13} />
        {label}
      </span>
      {state.persistence !== null && state.persistence.kind !== "available" ? (
        <span className="shell__state">
          Local storage{" "}
          {state.persistence.kind === "degraded" ? "degraded" : "needs repair"}
        </span>
      ) : null}
      {counts !== null ? (
        <span className="shell__count" data-testid="visible-count">
          {counts.visible} / {counts.total} visible
        </span>
      ) : null}
    </footer>
  );
}
