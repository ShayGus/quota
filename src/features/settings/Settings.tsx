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
import { PrivacyPanel, DiagnosticExportRow } from "./panels/PrivacyPanel";
import { ConnectionWizard } from "./ConnectionWizard";
import { NotificationsPanel } from "./panels/NotificationsPanel";
import { WindowPanel } from "./panels/WindowPanel";

/** The settings sections. */
type SettingsTab =
  "general" | "accounts" | "appearance" | "notifications" | "privacy" | "diagnostics";

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
  readonly fitToAccounts: () => void;
  readonly resetPosition: () => void;
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
  readonly confirmConnection: (attempt: AttemptRef) => Promise<boolean>;
  /** Re-verifies one account under a new connection generation. */
  readonly reconnectAccount: (accountId: AccountId) => Promise<void>;
  /** Drops retained history for one account. The host has no all-accounts clear. */
  readonly clearHistory: (accountId: AccountId) => void;
  /** Writes a diagnostic export to the destination the host will validate. */
  readonly exportDiagnostics: (destination: string) => void;
}

/**
 * Gives Accounts and Connect requests new identities so retained details and
 * wizard state are reset even when the same section is requested again.
 */
let requestedRoutes = 0;

/** The settings surface. */
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
    if (next === "accounts" || next === "connect") {
      requestedRoutes += 1;
      window.location.hash = `#/settings/${next}/${requestedRoutes}`;
    } else {
      window.location.hash = `#/settings/${next}`;
    }
  }, []);
  const showAccounts = useCallback(() => {
    navigate("accounts");
  }, [navigate]);
  const now = useNow();
  const preferences = state.preferences;
  if (tab === "connect") {
    return (
      <div className="settings__content settings__connection">
        <ConnectionWizard
          key={route}
          state={state}
          actions={actions}
          onDone={showAccounts}
        />
      </div>
    );
  }
  return (
    <section className="settings" aria-label="Quota settings">
      <nav className="settings__nav" aria-label="Settings sections">
        {TABS.map(([id, label, icon]) => (
          <button
            key={id}
            type="button"
            aria-current={tab === id ? "page" : undefined}
            onClick={() => {
              navigate(id);
            }}
          >
            <Icon name={icon} size={15} />
            {label}
          </button>
        ))}
      </nav>
      <div className="settings__content">
        {preferences === null ? (
          <p className="note">
            Quota has not received the confirmed preferences yet. Settings appear as soon
            as the backend publishes them.
          </p>
        ) : tab === "general" ? (
          <WindowPanel
            preferences={preferences}
            nativeWindow={state.nativeWindow}
            monitoring={state.monitoring}
            actions={actions}
          />
        ) : tab === "appearance" ? (
          <AppearancePanel preferences={preferences} actions={actions} />
        ) : tab === "accounts" ? (
          <AccountsPanel
            key={route}
            accounts={accountsForManagement(state.snapshot?.accounts ?? [])}
            preferences={preferences}
            now={now}
            actions={actions}
            onAddAccount={() => {
              navigate("connect");
            }}
          />
        ) : tab === "notifications" ? (
          <NotificationsPanel preferences={preferences} actions={actions} />
        ) : tab === "diagnostics" ? (
          <>
            <h3 className="settings__title">Diagnostics</h3>
            <p className="settings__intro">
              Sanitized status and local diagnostic exports.
            </p>
            <dl className="detail__list">
              <div>
                <dt>Connection</dt>
                <dd>{state.link}</dd>
              </div>
              <div>
                <dt>Enabled accounts</dt>
                <dd>
                  {state.snapshot?.accounts.filter(
                    (account) => account.monitoring_enabled,
                  ).length ?? 0}{" "}
                  / {state.snapshot?.accounts.length ?? 0}
                </dd>
              </div>
            </dl>
            <DiagnosticExportRow actions={actions} />
          </>
        ) : tab === "privacy" ? (
          <PrivacyPanel preferences={preferences} actions={actions} />
        ) : null}
      </div>
    </section>
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
