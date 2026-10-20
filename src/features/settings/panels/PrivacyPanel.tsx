/**
 * Privacy settings.
 *
 * The panel first says, in plain words, what Quota sends and keeps, so a
 * person can trust it without reading the code: it asks each provider only for
 * usage, keeps keys and browser sign-ins in the system's credential store,
 * reads other apps' sign-ins without changing them, and never reads
 * conversations. Every statement here is true of the host today.
 *
 * Then the choices: hiding account names on screen, which uses stable,
 * distinguishable aliases rather than one repeated word (AC-72), whether past
 * readings are kept on this device, and, once a Claude account exists, whether
 * Quota may start Claude Code to renew its expired sign-in. That one is off
 * until the person turns it on, because it runs another app.
 */
import { useState, type JSX } from "react";

import type { AccountSnapshot, Preferences } from "../../../generated/bindings";
import { Dialog } from "../../../shared/ui/Dialog";
import { Icon } from "../../../shared/ui/Icon";
import { SettingRow, SettingsTitle, Switch } from "../Primitives";
import type { SettingsActions } from "../Settings";
import { withAliasMode, withRenewClaudeSignIn, withRetainHistory } from "../preferences";
import { credentialStoreName } from "../providers";

/** What Quota does with data, each as a short heading and one sentence. */
function facts(): readonly (readonly [string, string])[] {
  return [
    [
      "No Quota server, no tracking",
      "Quota has no server or account of its own and collects nothing. Everything it keeps stays on this computer.",
    ],
    [
      "It only talks to your AI providers",
      "To see how much of your plan is left, Quota asks each provider you added, such as Anthropic or OpenAI, the same way their own apps do. It contacts no one else.",
    ],
    [
      "Your sign-ins stay protected",
      `API keys and browser sign-ins are kept in ${credentialStoreName()}. Sign-ins from apps you already use, such as Codex, Claude Code or Cursor, are only read, never changed, and removing an account here does not sign you out of them.`,
    ],
    ["Your work stays yours", "Quota never reads your conversations, prompts, or files."],
  ];
}

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
        intro="Quota runs on this computer. Here is exactly what it does with your data."
      />
      <ul className="privacy-facts">
        {facts().map(([heading, text]) => (
          <li key={heading}>
            <Icon name="check" />
            <div>
              <strong>{heading}</strong>
              <p>{text}</p>
            </div>
          </li>
        ))}
      </ul>
      <SettingRow
        label="Hide account names"
        description="Show “Account 1”, “Account 2” instead of names and emails. Handy when you share your screen."
        control={
          <Switch
            checked={preferences.privacy.alias_mode === "stable_aliases"}
            label="Hide account names"
            onChange={(hide) => {
              actions.savePreferences(
                withAliasMode(preferences, hide ? "stable_aliases" : "off"),
              );
            }}
          />
        }
      />
      <SettingRow
        label="Keep reading history"
        description="Saves past readings on this device, for trends in a later version. Turning it off stops saving new ones."
        control={
          <Switch
            checked={preferences.privacy.retain_history}
            label="Keep reading history"
            onChange={(keep) => {
              actions.savePreferences(withRetainHistory(preferences, keep));
            }}
          />
        }
      />
      <SettingRow
        label="Clear reading history"
        description="Deletes the saved past readings. Your accounts and current readings stay."
        control={
          <button
            type="button"
            className="button"
            disabled={accounts.length === 0}
            onClick={() => {
              setConfirming(true);
            }}
          >
            Clear…
          </button>
        }
      />
      {accounts.some((account) => account.provider_id === "claude") ? (
        <SettingRow
          label="Renew the Claude Code sign-in"
          description="Claude Code's sign-in runs out after 8 hours without use. When it does, Quota starts Claude Code briefly in the background to renew it. No prompt is sent and none of your plan is used."
          control={
            <Switch
              checked={preferences.privacy.renew_claude_sign_in}
              label="Renew the Claude Code sign-in"
              onChange={(renew) => {
                actions.savePreferences(withRenewClaudeSignIn(preferences, renew));
              }}
            />
          }
        />
      ) : null}
      {confirming ? (
        <Dialog
          title="Clear reading history?"
          confirmLabel="Clear"
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
            This deletes the saved past readings for every account on this device. Your
            accounts and their current readings stay.
          </p>
        </Dialog>
      ) : null}
    </>
  );
}
