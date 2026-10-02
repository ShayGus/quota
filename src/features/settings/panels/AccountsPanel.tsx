/**
 * Account management settings.
 *
 * Separate identities, including several subscriptions from one provider. The
 * provider mark is text, so a provider label can never inject markup (AC-37).
 * Disconnecting one account leaves its same-provider siblings untouched
 * (spec 4.6, AC-40).
 */
import { useState, type JSX } from "react";

import type {
  AccountId,
  AccountSnapshot,
  Preferences,
} from "../../../generated/bindings";
import { accountLabel } from "../../../shared/format/alias";
import { formatAge, instantOf } from "../../../shared/format/duration";
import { providerLabel } from "../../../shared/format/provider";
import { ProviderMark } from "../../../shared/ui/ProviderMark";
import { launch } from "../../../shared/ipc/report";
import { Icon } from "../../../shared/ui/Icon";
import { RefreshNotice } from "../../../shared/ui/RefreshNotice";
import type { SettingsActions } from "../Settings";
import { AccountDetail } from "../../accounts/AccountDetail";
import { statusOf } from "../../overview/status";

/** One managed account. */
function ManagedAccount({
  account,
  now,
  actions,
  onDetails,
  accounts,
  preferences,
}: {
  readonly accounts: readonly AccountSnapshot[];
  readonly preferences: Preferences | null;
  readonly onDetails: () => void;
  readonly account: AccountSnapshot;
  readonly now: number;
  readonly actions: SettingsActions;
}): JSX.Element {
  const alias = accountLabel(preferences, accounts, account.account_id);
  const label = alias || account.nickname;
  const [renaming, setRenaming] = useState(false);
  const [nickname, setNickname] = useState(account.nickname);
  const status = statusOf(account);
  const lastSuccess = instantOf(account.last_success_at);
  const dirty = nickname.trim().length > 0 && nickname.trim() !== account.nickname;
  return (
    <article className="account-manage-card" aria-label={`Manage ${label}`}>
      <div className="account-manage-card__head">
        <div className="identity">
          <ProviderMark providerId={account.provider_id} />
          <div>
            <span className="account-manage-card__name">{label}</span>
            <span className="account-manage-card__meta">
              {providerLabel(account.provider_id)}
              {!alias && account.identity?.workspace_label != null
                ? ` · ${account.identity.workspace_label}`
                : ""}
              {` · ${String(account.windows.length)} limits`}
            </span>
          </div>
        </div>
        <button
          type="button"
          role="switch"
          className="switch"
          aria-checked={account.monitoring_enabled}
          aria-label={`Monitor ${label}`}
          onClick={() => {
            actions.setAccountEnabled(account.account_id, !account.monitoring_enabled);
          }}
        />
      </div>
      <p className="account-manage-card__meta">
        {alias || account.identity?.principal_label || account.nickname}
      </p>
      <p className="account-manage-card__meta">
        {status.text} · {lastSuccess === null ? "None yet" : formatAge(lastSuccess, now)}{" "}
        · {account.account_id}
      </p>
      <div className="account-manage-card__actions">
        <button type="button" className="text-button" onClick={onDetails}>
          Details
        </button>
        <button
          type="button"
          className="text-button"
          disabled={alias !== ""}
          onClick={() => {
            setNickname(account.nickname);
            setRenaming(!renaming);
          }}
        >
          Rename
        </button>
        {renaming && !alias ? (
          <>
            <label className="account-manage-card__rename">
              <span>Nickname</span>
              <input
                type="text"
                value={nickname}
                maxLength={64}
                onChange={(event) => {
                  setNickname(event.currentTarget.value);
                }}
              />
            </label>
            <button
              type="button"
              className="text-button"
              disabled={!dirty}
              onClick={() => {
                actions.renameAccount(account.account_id, nickname.trim());
                setRenaming(false);
              }}
            >
              Save name
            </button>
          </>
        ) : null}
        <button
          type="button"
          className="text-button"
          onClick={() => {
            actions.openUsagePage(account.account_id);
          }}
        >
          <Icon name="external" size={13} />
          Usage page
        </button>
        <button
          type="button"
          className="text-button"
          onClick={() => {
            actions.clearHistory(account.account_id);
          }}
        >
          Clear history
        </button>
        <button
          type="button"
          className="text-button"
          onClick={() => {
            launch(actions.reconnectAccount(account.account_id));
          }}
        >
          Reconnect
        </button>
        <button
          type="button"
          className="text-button text-button--danger"
          onClick={() => {
            actions.disconnectAccount(account.account_id);
          }}
        >
          Disconnect
        </button>
      </div>
    </article>
  );
}

/** The account management panel. */
export function AccountsPanel({
  accounts,
  preferences,
  now,
  actions,
  onAddAccount,
}: {
  readonly onAddAccount: () => void;
  readonly accounts: readonly AccountSnapshot[];
  readonly preferences: Preferences | null;
  readonly now: number;
  readonly actions: SettingsActions;
}): JSX.Element {
  const [selectedId, setSelectedId] = useState<AccountId | null>(null);
  const selected = accounts.find((account) => account.account_id === selectedId);
  if (selected !== undefined) {
    return (
      <AccountDetail
        account={selected}
        preferences={preferences}
        accounts={accounts}
        now={now}
        onBack={() => {
          setSelectedId(null);
        }}
        onUsagePage={() => {
          actions.openUsagePage(selected.account_id);
        }}
        onManageAccounts={() => {
          setSelectedId(null);
        }}
      />
    );
  }
  return (
    <>
      <div className="settings-title-row">
        <h3 className="settings__title">Accounts</h3>
        <button type="button" className="button button--primary" onClick={onAddAccount}>
          <Icon name="plus" size={13} />
          Add account
        </button>
      </div>
      <p className="settings__intro">
        Separate identities, including several subscriptions with the same provider.
        Disconnecting one account does not affect its siblings.
      </p>
      {accounts.length === 0 ? (
        <p className="note">
          No connected accounts. Add your first account to start monitoring.
        </p>
      ) : null}
      <RefreshNotice accounts={accounts} preferences={preferences} now={now} />
      {accounts.map((account) => (
        <ManagedAccount
          key={account.account_id}
          account={account}
          accounts={accounts}
          preferences={preferences}
          now={now}
          actions={actions}
          onDetails={() => {
            setSelectedId(account.account_id);
          }}
        />
      ))}
    </>
  );
}
