/**
 * Account management settings.
 *
 * Separate identities, including several subscriptions from one provider. The
 * provider mark is text, so a provider label can never inject markup (AC-37).
 * Disconnecting one account leaves its same-provider siblings untouched
 * (spec 4.6, AC-40).
 */
import { useState, type JSX } from "react";

import type { AccountSnapshot } from "../../../generated/bindings";
import { formatAge, instantOf } from "../../../shared/format/duration";
import { Icon } from "../../../shared/ui/Icon";
import { STATUS_ICON } from "../statusIcons";
import type { SettingsActions } from "../Settings";
import { statusOf } from "../../overview/status";

/** One managed account. */
function ManagedAccount({
  account,
  now,
  actions,
}: {
  readonly account: AccountSnapshot;
  readonly now: number;
  readonly actions: SettingsActions;
}): JSX.Element {
  const [nickname, setNickname] = useState(account.nickname);
  const status = statusOf(account);
  const lastSuccess = instantOf(account.last_success_at);
  const dirty = nickname.trim().length > 0 && nickname.trim() !== account.nickname;
  return (
    <article className="account-manage-card" aria-label={`Manage ${account.nickname}`}>
      <div className="account-manage-card__head">
        <div>
          <span className="account-manage-card__name">{account.nickname}</span>
          <span className="account-manage-card__meta">
            {account.provider_id}
            {account.identity?.workspace_label != null
              ? ` · ${account.identity.workspace_label}`
              : ""}
            {` · ${String(account.windows.length)} limits`}
          </span>
        </div>
        <button
          type="button"
          role="switch"
          className="switch"
          aria-checked={account.monitoring_enabled}
          aria-label={`Monitor ${account.nickname}`}
          onClick={() => {
            actions.setAccountEnabled(account.account_id, !account.monitoring_enabled);
          }}
        />
      </div>
      <dl className="detail__list">
        <div>
          <dt>State</dt>
          <dd>
            <Icon name={STATUS_ICON[status.icon]} size={11} />
            {status.text}
          </dd>
        </div>
        <div>
          <dt>Last accepted reading</dt>
          <dd>{lastSuccess === null ? "None yet" : formatAge(lastSuccess, now)}</dd>
        </div>
        <div>
          <dt>Connection</dt>
          <dd>
            {account.connection_state} · generation {String(account.connection_generation)}
          </dd>
        </div>
      </dl>
      <div className="account-manage-card__actions">
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
          }}
        >
          Save name
        </button>
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
  now,
  actions,
}: {
  readonly accounts: readonly AccountSnapshot[];
  readonly now: number;
  readonly actions: SettingsActions;
}): JSX.Element {
  if (accounts.length === 0) {
    return (
      <>
        <h3 className="settings__title">Accounts</h3>
        <p className="note">No account is monitored yet.</p>
      </>
    );
  }
  return (
    <>
      <h3 className="settings__title">Accounts</h3>
      <p className="settings__intro">
        Separate identities, including several subscriptions with the same provider.
        Disconnecting one account does not affect its siblings.
      </p>
      {accounts.map((account) => (
        <ManagedAccount
          key={account.account_id}
          account={account}
          now={now}
          actions={actions}
        />
      ))}
    </>
  );
}
