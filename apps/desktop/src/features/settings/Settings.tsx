/**
 * The settings surface.
 *
 * Every committed change is a typed command. The screen never writes a
 * preference itself, and it never shows a saved state before the backend
 * confirms it (spec 13.2).
 */
import { useState, type JSX } from "react";

import type { AccountId, AccountSnapshot, Preferences } from "../../generated/bindings";
import type { RendererState } from "../../shared/state/types";
import { Icon, type IconName } from "../../shared/ui/Icon";
import { useNow } from "../../shared/ui/useNow";
import { AccountsPanel } from "./panels/AccountsPanel";
import { AppearancePanel } from "./panels/AppearancePanel";
import { PrivacyPanel } from "./panels/PrivacyPanel";
import { WindowPanel } from "./panels/WindowPanel";

/** The settings sections. */
type SettingsTab = "window" | "appearance" | "accounts" | "privacy";

/** One navigation entry. */
const TABS: readonly (readonly [SettingsTab, string, IconName])[] = [
  ["window", "Window", "layers"],
  ["appearance", "Appearance", "palette"],
  ["accounts", "Accounts", "user"],
  ["privacy", "Privacy", "shield"],
];

/** The actions a settings panel may ask the application to perform. */
export interface SettingsActions {
  readonly savePreferences: (next: Preferences) => void;
  readonly setAlwaysOnTop: (alwaysOnTop: boolean) => void;
  readonly setOverviewMode: (mode: Preferences["overview_mode"]) => void;
  readonly fitToAccounts: () => void;
  readonly resetPosition: () => void;
  readonly setAccountEnabled: (accountId: AccountId, enabled: boolean) => void;
  readonly renameAccount: (accountId: AccountId, nickname: string) => void;
  readonly disconnectAccount: (accountId: AccountId) => void;
  readonly openUsagePage: (accountId: AccountId) => void;
  readonly clearHistory: (accountId: AccountId | null) => void;
  readonly exportDiagnostics: () => void;
}

/** The settings surface. */
export function Settings({
  state,
  actions,
}: {
  readonly state: RendererState;
  readonly actions: SettingsActions;
}): JSX.Element {
  const [tab, setTab] = useState<SettingsTab>("window");
  const now = useNow();
  const preferences = state.preferences;
  return (
    <section className="settings" aria-label="Quota settings">
      <nav className="settings__nav" aria-label="Settings sections">
        {TABS.map(([id, label, icon]) => (
          <button
            key={id}
            type="button"
            aria-current={tab === id ? "page" : undefined}
            onClick={() => {
              setTab(id);
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
        ) : tab === "window" ? (
          <WindowPanel
            preferences={preferences}
            nativeWindow={state.nativeWindow}
            actions={actions}
          />
        ) : tab === "appearance" ? (
          <AppearancePanel preferences={preferences} actions={actions} />
        ) : tab === "accounts" ? (
          <AccountsPanel
            accounts={accountsForManagement(state.snapshot?.accounts ?? [])}
            now={now}
            actions={actions}
          />
        ) : (
          <PrivacyPanel preferences={preferences} actions={actions} />
        )}
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
