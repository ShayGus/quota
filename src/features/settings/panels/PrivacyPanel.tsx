/**
 * Privacy settings.
 *
 * Aliases replace account labels on screen; they are stable and distinguishable,
 * never one repeated word (AC-72). Local history is a separate choice.
 */
import { useState, type JSX } from "react";

import type { AccountSnapshot, Preferences } from "../../../generated/bindings";
import { Dialog } from "../../../shared/ui/Dialog";
import { Icon } from "../../../shared/ui/Icon";
import { SettingRow, SettingsTitle, Select, Switch } from "../Primitives";
import type { SettingsActions } from "../Settings";
import { withAliasMode, withRetainHistory } from "../preferences";

/**
 * The retention periods, and which of them this build can actually apply.
 *
 * The host has no retention sweep, so only Disabled and keeping indefinitely are
 * real choices; the two durations the wireframe offers are shown but refused.
 */
const RETENTION: readonly (readonly [
  "0" | "7" | "30" | "indefinite",
  string,
  boolean?,
])[] = [
  ["0", "Disabled"],
  ["7", "7 days", true],
  ["30", "30 days", true],
  ["indefinite", "Keep indefinitely"],
];

/** The privacy settings panel. */
export function PrivacyPanel({
  preferences,
  accounts,
  actions,
}: {
  readonly preferences: Preferences;
  readonly accounts: readonly AccountSnapshot[];
  readonly actions: SettingsActions;
}): JSX.Element {
  const [confirming, setConfirming] = useState(false);
  return (
    <>
      <SettingsTitle
        title="Privacy"
        intro="A local view of your allowances, not your conversations."
      />
      <div className="privacy-card">
        <Icon name="shield" />
        <div>
          <strong>No credentials reach this window</strong>
          <p>
            Tokens and cookies stay in the operating system credential store. The window
            receives sanitized quota snapshots only.
          </p>
        </div>
      </div>
      <SettingRow
        label="Hide account labels"
        description="Replace nicknames, email identities, and workspaces on screen."
        control={
          <Switch
            checked={preferences.privacy.alias_mode === "stable_aliases"}
            label="Hide account labels"
            onChange={(hide) => {
              actions.savePreferences(
                withAliasMode(preferences, hide ? "stable_aliases" : "off"),
              );
            }}
          />
        }
      />
      <SettingRow
        label="Local history retention"
        description="Keeps normalized quota history on this device. Timed periods are not available yet."
        control={
          <Select
            label="Local history retention"
            value={preferences.privacy.retain_history ? "indefinite" : "0"}
            options={RETENTION}
            onChange={(value) => {
              actions.savePreferences(withRetainHistory(preferences, value !== "0"));
            }}
          />
        }
      />
      <SettingRow
        label="Clear local history"
        description="Remove stored quota history for every account. Current readings stay."
        control={
          <button
            type="button"
            className="button danger"
            disabled={accounts.length === 0}
            onClick={() => {
              setConfirming(true);
            }}
          >
            Clear history
          </button>
        }
      />
      <div className="note">
        Monitoring never reads conversation content or session titles, and Quota does not
        read or modify provider command-line sessions.
      </div>
      {confirming ? (
        <Dialog
          title="Clear local history?"
          confirmLabel="Clear history"
          onConfirm={() => {
            for (const account of accounts) {
              actions.clearHistory(account.account_id);
            }
          }}
          onClose={() => {
            setConfirming(false);
          }}
        >
          <p>
            This removes stored quota history for all accounts on this device. Accounts,
            connections, and current readings are not affected.
          </p>
        </Dialog>
      ) : null}
    </>
  );
}
