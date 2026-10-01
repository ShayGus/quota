/**
 * Quota's application root.
 *
 * The renderer owns presentation state only: the current view. Every account
 * value comes from the snapshot store, and every mutation is a typed command
 * (spec 7.8.3).
 */
import { useState, type JSX } from "react";

import { AccountDetail } from "../features/accounts/AccountDetail";
import { Overview } from "../features/overview/Overview";
import { Settings, type SettingsActions } from "../features/settings/Settings";
import type { AccountId } from "../generated/bindings";
import { useRendererState } from "../shared/state/useRendererState";
import { useNow } from "../shared/ui/useNow";
import { actions } from "./actions";
import { AppHeader } from "./AppHeader";
import { AppBoundary, FeatureBoundary } from "./ErrorBoundary";
import { useSnapshotSubscription } from "./useSnapshotSubscription";
import { useTheme } from "./useTheme";

/** Which surface the window is showing. */
type View =
  | { readonly name: "overview" }
  | { readonly name: "detail"; readonly id: AccountId }
  | { readonly name: "settings" };

/** The settings actions, wired to the typed commands. */
const settingsActions: SettingsActions = {
  savePreferences: (next) => void actions.savePreferences(next),
  setAlwaysOnTop: (alwaysOnTop) => void actions.setAlwaysOnTop(alwaysOnTop),
  setOverviewMode: (mode) => void actions.setOverviewMode(mode),
  fitToAccounts: () => void actions.fitToAccounts(),
  resetPosition: () => void actions.resetPosition(),
  setAccountEnabled: (accountId, enabled) =>
    void actions.setAccountEnabled(accountId, enabled),
  renameAccount: (accountId, nickname) => void actions.renameAccount(accountId, nickname),
  disconnectAccount: (accountId) => void actions.disconnectAccount(accountId),
  openUsagePage: (accountId) => void actions.openUsagePage(accountId),
  clearHistory: (accountId) => void actions.clearHistory(accountId),
  exportDiagnostics: () => void actions.exportDiagnostics(),
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
  const [view, setView] = useState<View>({ name: "overview" });
  const now = useNow();
  useTheme(state);

  const account =
    view.name === "detail" && state.snapshot !== null
      ? (state.snapshot.accounts.find((candidate) => candidate.account_id === view.id) ?? null)
      : null;

  return (
    <div className="shell">
      <AppHeader
        state={state}
        view={view.name}
        onSettings={() => {
          setView({ name: "settings" });
        }}
        onOverview={() => {
          setView({ name: "overview" });
        }}
      />
      <main className="shell__main">
        {view.name === "settings" ? (
          <FeatureBoundary surface="settings">
            <Settings state={state} actions={settingsActions} />
          </FeatureBoundary>
        ) : account === null ? (
          <FeatureBoundary surface="overview">
            <Overview
              state={state}
              onOpenAccount={(accountId) => {
                setView({ name: "detail", id: accountId });
              }}
              onReconnect={(accountId) => {
                setView({ name: "detail", id: accountId });
              }}
            />
          </FeatureBoundary>
        ) : (
          <FeatureBoundary surface="account details">
            <AccountDetail
              account={account}
              now={now}
              onBack={() => {
                setView({ name: "overview" });
              }}
            />
          </FeatureBoundary>
        )}
      </main>
      <WindowFooter />
    </div>
  );
}

/** The window footer: what the backend is doing, and what it is not. */
function WindowFooter(): JSX.Element {
  const state = useRendererState();
  const link = state.link;
  const label =
    link === "connecting"
      ? "Connecting to Quota"
      : link === "reconciling"
        ? "Reconciling with Quota"
        : link === "unavailable"
          ? "Quota is not reachable"
          : state.monitoring?.kind === "paused"
            ? "Paused · last known values"
            : "Monitoring active";
  return (
    <footer className="shell__footer">
      <span className="shell__state">{label}</span>
      {state.persistence !== null && state.persistence.kind !== "available" ? (
        <span className="shell__state">
          Local storage {state.persistence.kind === "degraded" ? "degraded" : "needs repair"}
        </span>
      ) : null}
    </footer>
  );
}
