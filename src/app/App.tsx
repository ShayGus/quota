/**
 * Quota's application root.
 *
 * The renderer owns presentation state only: the current view. Every account
 * value comes from the snapshot store, and every mutation is a typed command
 * (spec 7.8.3).
 */
import { useEffect, useRef, useState, type JSX } from "react";

import { AccountDetail } from "../features/accounts/AccountDetail";
import { Overview } from "../features/overview/Overview";
import type { OverviewFilter } from "../features/overview/OverviewToolbar";
import { Settings, type SettingsActions } from "../features/settings/Settings";
import { UpdateWindow } from "../features/update/UpdatePrompt";
import { useUpdatePrompt } from "../features/update/useUpdatePrompt";
import { Widget } from "../features/widget/Widget";
import type { AccountId, QuotaWindowId } from "../generated/bindings";
import {
  describeCommandError,
  describeTransportFailure,
  launch,
} from "../shared/ipc/report";
import type { RendererFailure, RendererState } from "../shared/state/types";
import { getRendererState } from "../shared/state/store";
import { useRendererState } from "../shared/state/useRendererState";
import { Icon } from "../shared/ui/Icon";
import { refreshMessage, Toast, type ToastMessage } from "../shared/ui/RefreshNotice";
import { PROMPT_COPIED } from "../shared/ui/ReportBug";
import { useNow } from "../shared/ui/useNow";
import { actions } from "./actions";
import { AppHeader, SettingsHeader } from "./AppHeader";
import { AppBoundary, FeatureBoundary } from "./ErrorBoundary";
import { listenForNavigation, showInPopover } from "../shared/ipc/navigation";
import { useFitContentHeight } from "./useFitContentHeight";
import { useSnapshotSubscription } from "./useSnapshotSubscription";
import { useTheme } from "./useTheme";

/**
 * Which surface the popover is showing.
 *
 * Settings has no entry here: it belongs to its own window, whose capability is
 * the only one that may save preferences. The popover asks the host to show it.
 */
type View =
  | { readonly name: "overview" }
  | {
      readonly name: "detail";
      readonly id: AccountId;
      readonly windowId: QuotaWindowId | null;
    };

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
  setAppView: (view) => {
    launch(actions.setAppView(view));
  },
  setOverviewMode: (mode) => {
    launch(actions.setOverviewMode(mode));
  },
  setAccountEnabled: (accountId, enabled) => {
    launch(actions.setAccountEnabled(accountId, enabled));
  },
  renameAccount: (accountId, nickname) => {
    launch(actions.renameAccount(accountId, nickname));
  },
  setKeyLimitShown: (accountId, shown) => {
    launch(actions.setKeyLimitShown(accountId, shown));
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
  confirmConnection: (attempt, nickname) => actions.confirmConnection(attempt, nickname),
  reconnectAccount: (accountId) => actions.reconnectAccount(accountId),
  clearHistory: (accountId) => {
    launch(actions.clearHistory(accountId));
  },
  exportDiagnostics: (label) => actions.exportDiagnostics(label),
  openBugReportIssue: () => actions.openBugReportIssue(),
  copyBugReportPrompt: () => actions.copyBugReportPrompt(),
  launchAtLogin: () => actions.launchAtLogin(),
  setLaunchAtLogin: (launch) => actions.setLaunchAtLogin(launch),
  showOverview: () => {
    launch(showInPopover({ view: "overview" }));
  },
  showAccountDetail: (accountId) => {
    launch(showInPopover({ view: "detail", accountId, windowId: null }));
  },
};

/** The words for a failed command, as a toast states them. */
function failureText(failure: RendererFailure): string {
  return failure.kind === "domain"
    ? describeCommandError(failure.error)
    : describeTransportFailure(failure.failure);
}

/**
 * The toast for the latest failure. A command that the host refuses is stated
 * where the person acted, instead of failing silently.
 */
function useFailureToast(
  failure: RendererFailure | null,
  show: (message: ToastMessage) => void,
): void {
  useEffect(() => {
    if (failure !== null) {
      show({ text: failureText(failure) });
    }
  }, [failure, show]);
}

/** The window root, inside the application-level boundary. */
export function App(): JSX.Element {
  useSnapshotSubscription();
  return (
    <AppBoundary>
      <QuotaWindow />
    </AppBoundary>
  );
}

/** The update pop-up's window, including the installing and failure states. */
function UpdatePopup(): JSX.Element {
  const prompt = useUpdatePrompt();
  return (
    <FeatureBoundary surface="update">
      <UpdateWindow
        prompt={prompt}
        onRespond={(response) => {
          launch(actions.respondToUpdate(response));
        }}
      />
    </FeatureBoundary>
  );
}

/** The window: the update pop-up, the settings window, the mini widget, or the popover. */
function QuotaWindow(): JSX.Element {
  const state = useRendererState();
  useTheme(state);
  // The settings window is opened with this hash and is the only window whose
  // capability may save preferences, so it is the only window that renders it.
  const isSettingsWindow = window.location.hash.startsWith("#/settings");
  const [toast, setToast] = useState<ToastMessage | null>(null);
  useFailureToast(state.failure, setToast);
  // The update pop-up is opened by the host with its own hash.
  if (window.location.hash.startsWith("#/update")) {
    return <UpdatePopup />;
  }
  // The widget window is opened with its own hash. It is the compact view of
  // the app, and its only action is switching back to the full window.
  if (window.location.hash.startsWith("#/widget")) {
    return (
      <FeatureBoundary surface="widget">
        <Widget
          state={state}
          report={{
            openIssue: () => actions.openBugReportIssue(),
            copyPrompt: () => actions.copyBugReportPrompt(),
          }}
          onExpand={() => {
            launch(actions.setAppView("overview"));
          }}
        />
      </FeatureBoundary>
    );
  }
  if (isSettingsWindow) {
    return (
      <div className="window settings-window">
        <SettingsHeader />
        <FeatureBoundary surface="settings">
          <Settings state={state} actions={settingsActions} />
        </FeatureBoundary>
        <Toast message={toast} />
      </div>
    );
  }
  return <Popover state={state} toast={toast} onToast={setToast} />;
}

/** The popover: header, the current surface, and footer. */
function Popover({
  state,
  toast,
  onToast,
}: {
  readonly state: RendererState;
  readonly toast: ToastMessage | null;
  readonly onToast: (message: ToastMessage) => void;
}): JSX.Element {
  const [view, setView] = useState<View>({ name: "overview" });
  // The filter belongs to the window rather than the list, so returning from
  // details keeps it.
  const [filter, setFilter] = useState<OverviewFilter>("all");
  const now = useNow();

  const account =
    view.name === "detail" && state.snapshot !== null
      ? (state.snapshot.accounts.find((candidate) => candidate.account_id === view.id) ??
        null)
      : null;
  const surface = view.name === "detail" && account === null ? "overview" : view.name;

  // Adding an account happens in the settings window, on its add-account page.
  const openConnect = (): void => {
    launch(actions.openSettings("connect"));
  };

  useEffect(() => {
    // Fit returns the overview to its default state: all accounts, no detail.
    const fit = (): void => {
      setFilter("all");
      setView({ name: "overview" });
    };
    window.addEventListener("quota-fit-overview", fit);
    return () => {
      window.removeEventListener("quota-fit-overview", fit);
    };
  }, []);

  useEffect(() => {
    let stop: (() => void) | null = null;
    let stopped = false;
    launch(
      listenForNavigation((target) => {
        // Settings asked for a surface of the full window. While the widget is
        // the view, the full window takes its place rather than joining it.
        if (getRendererState().preferences?.view === "widget") {
          launch(actions.setAppView("overview"));
        }
        if (target.view === "detail") {
          setView({ name: "detail", id: target.accountId, windowId: target.windowId });
        } else {
          setView({ name: "overview" });
        }
      }).then((detach) => {
        if (stopped) {
          detach();
        } else {
          stop = detach;
        }
      }),
    );
    return () => {
      stopped = true;
      stop?.();
    };
  }, []);

  useEffect(() => {
    // Escape steps back to the overview; from the overview it hides the popover.
    const onKey = (event: KeyboardEvent): void => {
      if (event.key !== "Escape" || event.defaultPrevented) {
        return;
      }
      if (document.querySelector("dialog[open]") !== null) {
        return;
      }
      event.preventDefault();
      if (surface === "overview") {
        launch(actions.closeWindow());
      } else {
        setView({ name: "overview" });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [surface]);

  const pinned = state.preferences?.overview_mode === "floating";
  const root = useRef<HTMLDivElement | null>(null);
  useFitContentHeight(root);

  return (
    <div className={`window popover${pinned ? " pinned" : ""}`} ref={root}>
      <AppHeader
        state={state}
        onSettings={() => {
          launch(actions.openSettings());
        }}
        report={{
          openIssue: () => actions.openBugReportIssue(),
          copyPrompt: () => actions.copyBugReportPrompt(),
          onCopied: () => {
            onToast({ text: PROMPT_COPIED });
          },
        }}
        onRefresh={() => {
          onToast({
            text: refreshMessage(
              state.snapshot?.accounts ?? [],
              state.preferences,
              Date.now(),
            ),
          });
          launch(actions.refresh("user_requested"));
        }}
      />
      <main className="app-main">
        {account !== null && view.name === "detail" ? (
          <FeatureBoundary surface="quota detail">
            <AccountDetail
              key={`${account.account_id}:${view.windowId ?? ""}`}
              account={account}
              windowId={view.windowId}
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
        ) : (
          <FeatureBoundary surface="overview">
            <Overview
              filter={filter}
              onFilter={setFilter}
              onAddAccount={openConnect}
              onIndicatorStyle={(next) => {
                launch(actions.setIndicatorStyle(next));
              }}
              state={state}
              onOpenAccount={(accountId) => {
                setView({ name: "detail", id: accountId, windowId: null });
              }}
              onOpenWindow={(accountId, windowId) => {
                setView({ name: "detail", id: accountId, windowId });
              }}
              onResume={() => {
                launch(actions.setMonitoring(false));
              }}
              onReconnect={(accountId) => {
                // Reconnecting re-verifies the credential under a new generation.
                // Opening the detail view is a different thing and does not do it.
                launch(actions.reconnectAccount(accountId));
              }}
              onEnable={(accountId) => {
                launch(actions.setAccountEnabled(accountId, true));
              }}
            />
          </FeatureBoundary>
        )}
      </main>
      <PopoverFooter state={state} onAddAccount={openConnect} />
      <Toast message={toast} />
    </div>
  );
}

/** The popover footer: what the backend is doing, and Add account. */
function PopoverFooter({
  state,
  onAddAccount,
}: {
  readonly state: RendererState;
  readonly onAddAccount: () => void;
}): JSX.Element {
  const link = state.link;
  const paused = state.monitoring?.kind === "paused";
  const refreshing =
    state.snapshot?.accounts.some((account) => account.fetch_state === "fetching") ??
    false;
  const storage =
    state.persistence === null || state.persistence.kind === "available"
      ? ""
      : state.persistence.kind === "degraded"
        ? " · storage degraded"
        : " · storage needs repair";
  const label =
    link === "connecting"
      ? "Connecting to Quota…"
      : link === "reconciling"
        ? "Reconciling with Quota…"
        : link === "unavailable"
          ? "Quota is not reachable"
          : refreshing
            ? "Refreshing…"
            : paused
              ? "Monitoring paused"
              : "Local only · monitoring active";
  return (
    <footer className="app-footer">
      <span className="footer-state">
        <Icon name={paused ? "pause" : "shield"} />
        {label}
        {storage}
      </span>
      <button type="button" className="text-btn" onClick={onAddAccount}>
        <Icon name="plus" />
        Add account
      </button>
    </footer>
  );
}
