/**
 * The settings surface.
 *
 * Every committed change is a typed command. The screen never writes a
 * preference itself, and it never shows a saved state before the backend
 * confirms it (spec 13.2).
 */
import { useCallback, useSyncExternalStore, type JSX } from "react";

import type {
  AccountId,
  AccountSnapshot,
  AttemptRef,
  BeginConnectionRequest,
  Preferences,
  ProviderPollingPolicy,
} from "../../generated/bindings";
import type { RendererState } from "../../shared/state/types";
import { Icon, type IconName } from "../../shared/ui/Icon";
import { useNow } from "../../shared/ui/useNow";
import { AccountsPanel } from "./panels/AccountsPanel";
import { AppearancePanel } from "./panels/AppearancePanel";
import { DiagnosticsPanel } from "./panels/DiagnosticsPanel";
import { PrivacyPanel } from "./panels/PrivacyPanel";
import { ConnectionWizard } from "./ConnectionWizard";
import { NotificationsPanel } from "./panels/NotificationsPanel";
import { WindowPanel } from "./panels/WindowPanel";

/** The settings sections. */
type SettingsTab =
  "general" | "accounts" | "appearance" | "notifications" | "privacy" | "diagnostics";

/** The application version the navigation rail states. */
const APP_VERSION = "0.1.0";

/** One navigation entry. */
const TABS: readonly (readonly [SettingsTab, string, IconName])[] = [
  ["general", "General", "settings"],
  ["accounts", "Accounts", "user"],
  ["appearance", "Appearance", "palette"],
  ["notifications", "Notifications", "bell"],
  ["privacy", "Privacy", "shield"],
  ["diagnostics", "Diagnostics", "terminal"],
];

/** The actions a settings panel may ask the application to perform. */
export interface SettingsActions {
  readonly setMonitoring: (paused: boolean) => void;
  readonly savePollingPreferences: (policy: ProviderPollingPolicy) => void;
  readonly savePreferences: (next: Preferences) => void;
  readonly setAlwaysOnTop: (alwaysOnTop: boolean) => void;
  readonly setOverviewMode: (mode: Preferences["overview_mode"]) => void;
  readonly setAccountEnabled: (accountId: AccountId, enabled: boolean) => void;
  readonly renameAccount: (accountId: AccountId, nickname: string) => void;
  readonly disconnectAccount: (accountId: AccountId) => void;
  readonly openUsagePage: (accountId: AccountId) => void;
  /**
   * Starts a connection attempt for one provider and returns the backend's
   * attempt identity, or `null` when the attempt was refused. A refusal is shown
   * rather than swallowed, because a duplicate credential profile is the most
   * common one.
   */
  readonly beginConnection: (
    request: BeginConnectionRequest,
  ) => Promise<AttemptRef | null>;
  readonly cancelConnection: (attempt: AttemptRef) => Promise<void>;
  /** See `actions.confirmConnection` in `src/app/actions.ts` for result semantics. */
  readonly confirmConnection: (attempt: AttemptRef, nickname: string) => Promise<boolean>;
  /** Re-verifies one account under a new connection generation. */
  readonly reconnectAccount: (accountId: AccountId) => Promise<void>;
  /** Drops retained history for one account. The host has no all-accounts clear. */
  readonly clearHistory: (accountId: AccountId) => void;
  /** Writes a diagnostic export to the destination the host will validate. */
  /** Writes a sanitized report and answers where, or `null` when it failed. */
  readonly exportDiagnostics: (label: string) => Promise<string | null>;
  /** Whether Quota starts at login, as the system confirms it; `null` when unknown. */
  readonly launchAtLogin: () => Promise<boolean | null>;
  /** Registers or removes the login item and returns the confirmed state. */
  readonly setLaunchAtLogin: (launch: boolean) => Promise<boolean | null>;
  /** Shows the overview in the popover. */
  readonly showOverview: () => void;
  /** Shows one account's quota detail in the popover. */
  readonly showAccountDetail: (accountId: AccountId) => void;
}

/**
 * Counts route requests, so asking for a section the window already shows is a
 * new route rather than the unchanged hash.
 */
let requestedRoutes = 0;

/** The settings surface: the section rail and the selected panel. */
export function Settings({
  state,
  actions,
}: {
  readonly state: RendererState;
  readonly actions: SettingsActions;
}): JSX.Element {
  const route = useSyncExternalStore(subscribeRoute, () => window.location.hash);
  const tab = route.split("/")[2] ?? "general";
  const navigate = useCallback((next: SettingsTab | "connect"): void => {
    requestedRoutes += 1;
    window.location.hash = `#/settings/${next}/${String(requestedRoutes)}`;
  }, []);
  const showAccounts = useCallback(() => {
    navigate("accounts");
  }, [navigate]);
  const now = useNow();
  const preferences = state.preferences;
  const selected: SettingsTab | "connect" = TABS.some(([id]) => id === tab)
    ? (tab as SettingsTab)
    : tab === "connect"
      ? "connect"
      : "general";
  // Adding an account is part of Accounts, so the rail shows Accounts there.
  const current: SettingsTab = selected === "connect" ? "accounts" : selected;
  return (
    <div className="settings-layout">
      <nav className="settings-nav" aria-label="Settings sections">
        {TABS.map(([id, label, icon]) => (
          <button
            key={id}
            type="button"
            className={current === id ? "selected" : ""}
            aria-current={current === id ? "page" : undefined}
            onClick={() => {
              navigate(id);
            }}
          >
            <Icon name={icon} />
            {label}
          </button>
        ))}
        <div className="nav-foot">
          Quota / {APP_VERSION}
          <br />
          Local quota monitor
        </div>
      </nav>
      <div className="settings-content">
        {selected === "connect" ? (
          // Every Add account, in the popover or here, opens this route.
          <ConnectionWizard
            key={route}
            state={state}
            actions={actions}
            onDone={showAccounts}
          />
        ) : preferences === null ? (
          <p className="note">
            Quota has not received the confirmed preferences yet. Settings appear as soon
            as the backend publishes them.
          </p>
        ) : selected === "general" ? (
          <WindowPanel
            preferences={preferences}
            monitoring={state.monitoring}
            actions={actions}
          />
        ) : selected === "appearance" ? (
          <AppearancePanel preferences={preferences} actions={actions} />
        ) : selected === "accounts" ? (
          <AccountsPanel
            key={route}
            accounts={accountsForManagement(state.snapshot?.accounts ?? [])}
            preferences={preferences}
            actions={actions}
            onAddAccount={() => {
              navigate("connect");
            }}
          />
        ) : selected === "notifications" ? (
          <NotificationsPanel
            preferences={preferences}
            accounts={state.snapshot?.accounts ?? []}
            now={now}
            actions={actions}
          />
        ) : selected === "diagnostics" ? (
          <DiagnosticsPanel state={state} now={now} actions={actions} />
        ) : (
          <PrivacyPanel
            preferences={preferences}
            accounts={state.snapshot?.accounts ?? []}
            actions={actions}
          />
        )}
      </div>
    </div>
  );
}

/** The accounts a management panel lists, in a stable order. */
export function accountsForManagement(
  accounts: readonly AccountSnapshot[],
): readonly AccountSnapshot[] {
  return [...accounts].sort((a, b) =>
    a.connection_ordinal === b.connection_ordinal
      ? a.account_id < b.account_id
        ? -1
        : 1
      : a.connection_ordinal - b.connection_ordinal,
  );
}

function subscribeRoute(notify: () => void): () => void {
  window.addEventListener("hashchange", notify);
  return () => {
    window.removeEventListener("hashchange", notify);
  };
}
