/**
 * The settings surface.
 *
 * Every committed change is a typed command. The screen never writes a
 * preference itself, and it never shows a saved state before the backend
 * confirms it (spec 13.2).
 */
import { useSyncExternalStore, type JSX } from "react";

import type {
  AccountId,
  AccountSnapshot,
  BeginConnectionRequest,
  AttemptRef,
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
            accounts={accountsForManagement(state.snapshot?.accounts ?? [])}
            preferences={preferences}
            now={now}
            actions={actions}
            onAddAccount={() => navigate("connect")}
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
