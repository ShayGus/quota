/**
 * Quota's application root.
 *
 * The renderer owns presentation state only: the current view. Every account
 * value comes from the snapshot store, and every mutation is a typed command
 * (spec 7.8.3).
 */
import { useEffect, useLayoutEffect, useRef, useState, type JSX } from "react";

import { AccountDetail } from "../features/accounts/AccountDetail";
import { Overview } from "../features/overview/Overview";
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
  setMonitoring: (paused) => {
    launch(actions.setMonitoring(paused));
  },
  savePollingPreferences: (policy) => {
    launch(actions.savePollingPreferences(policy));
  },
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
  beginConnection: async (request) => {
    const accepted = await actions.beginConnection(request);
    return accepted === null ? null : { id: accepted.attempt_id };
  },
  cancelConnection: (attempt) => actions.cancelConnection(attempt),
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
  const isSettingsWindow = window.location.hash.startsWith("#/settings");
  const [view, setView] = useState<View>({ name: "overview" });
  // The filter and the search text belong to the window rather than the list,
  // because the footer states how many accounts the filter leaves visible.
  const [filter, setFilter] = useState<OverviewFilter>("all");
  const [search, setSearch] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const mainRef = useRef<HTMLElement | null>(null);
  const [counts, setCounts] = useState({ visible: 0, total: 0 });
  const toggleSearch = (open: boolean): void => {
    setSearchOpen(open);
    if (!open) setSearch("");
  };
  const now = useNow();
  useTheme(state);

  const account =
    view.name === "detail" && state.snapshot !== null
      ? (state.snapshot.accounts.find((candidate) => candidate.account_id === view.id) ??
        null)
      : null;

  useEffect(() => {
    if (isSettingsWindow) return;
    const fit = (): void => {
      setFilter("all");
      setSearch("");
      setSearchOpen(false);
      setView({ name: "overview" });
    };
    window.addEventListener("quota-fit-overview", fit);
    return () => {
      window.removeEventListener("quota-fit-overview", fit);
    };
  }, [isSettingsWindow]);

  useLayoutEffect(() => {
    const main = mainRef.current;
    if (main === null || isSettingsWindow || account !== null) return;
    const measure = (): void => {
      const bounds = main.getBoundingClientRect();
      const heading = main.querySelector<HTMLElement>(".table__columns");
      const top =
        heading !== null && getComputedStyle(heading).display !== "none"
          ? Math.max(bounds.top, heading.getBoundingClientRect().bottom)
          : bounds.top;
      const rows = [...main.querySelectorAll<HTMLElement>(".account-row")];
      const visible = rows.filter((row) => {
        const rect = row.getBoundingClientRect();
        return (
          rect.height > 0 &&
          rect.top >= top - 1 &&
          rect.bottom <= bounds.bottom + 1 &&
          rect.left >= bounds.left - 1 &&
          rect.right <= bounds.right + 1
        );
      }).length;
      setCounts((previous) =>
        previous.visible === visible && previous.total === rows.length
          ? previous
          : { visible, total: rows.length },
      );
    };
    measure();
    main.addEventListener("scroll", measure, true);
    window.addEventListener("resize", measure);
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(main);
    const content = main.querySelector(".overview");
    if (content !== null) observer?.observe(content);
    for (const row of main.querySelectorAll(".account-row")) observer?.observe(row);
    return () => {
      main.removeEventListener("scroll", measure, true);
      window.removeEventListener("resize", measure);
      observer?.disconnect();
    };
  }, [state, filter, search, searchOpen, account, isSettingsWindow]);

  return (
    <div className="shell">
      <AppHeader
        state={state}
        view={isSettingsWindow ? "settings" : view.name}
        onSettings={() => {
          launch(actions.openSettings());
        }}
      />
      <main className="shell__main" ref={mainRef}>
        {isSettingsWindow ? (
          <FeatureBoundary surface="settings">
            <Settings state={state} actions={settingsActions} />
          </FeatureBoundary>
        ) : account === null ? (
          <FeatureBoundary surface="overview">
            <Overview
              filter={filter}
              onFilter={setFilter}
              searchOpen={searchOpen}
              onSearchOpen={toggleSearch}
              search={search}
              onSearch={setSearch}
              onFit={() => {
                launch(actions.fitToAccounts());
              }}
              onAddAccount={() => {
                launch(actions.openSettings("connect"));
              }}
              onIndicatorStyle={(next) => {
                launch(actions.setIndicatorStyle(next));
              }}
              state={state}
              onOpenAccount={(accountId) => {
                setView({ name: "detail", id: accountId });
              }}
              onResume={() => {
                launch(actions.setMonitoring(false));
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
              preferences={state.preferences}
              accounts={state.snapshot?.accounts ?? []}
              now={now}
              onUsagePage={() => {
                launch(actions.openUsagePage(account.account_id));
              }}
              onManageAccounts={() => {
                launch(actions.openSettings("accounts"));
              }}
              onBack={() => {
                setView({ name: "overview" });
              }}
            />
          </FeatureBoundary>
        )}
      </main>
      {isSettingsWindow ? null : (
        <WindowFooter
          counts={account !== null ? null : counts}
          onAddAccount={() => {
            launch(actions.openSettings("connect"));
          }}
        />
      )}
    </div>
  );
}

/** The window footer: what the backend is doing, and how much is on screen. */
function WindowFooter({
  counts,
  onAddAccount,
}: {
  /** Fully visible rows and total filtered rows, or `null` off the overview. */
  readonly counts: { readonly visible: number; readonly total: number } | null;
  readonly onAddAccount: () => void;
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
      <button type="button" className="text-button" onClick={onAddAccount}>
        <Icon name="plus" size={13} />
        Add account
      </button>
    </footer>
  );
}
