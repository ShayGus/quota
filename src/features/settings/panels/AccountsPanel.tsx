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
import { launch } from "../../../shared/ipc/report";
import { Icon } from "../../../shared/ui/Icon";
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
            <Icon name={status.icon} size={11} />
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
            {account.connection_state} · generation{" "}
            {String(account.connection_generation)}
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

/** The providers a person can connect, as the contract spells them. */
const CONNECTABLE_PROVIDERS = ["codex", "claude", "open_code_go"] as const;

/**
 * Starts a connection for one provider.
 *
 * A refusal is reported rather than swallowed. The common one is a credential
 * profile that is already connected, and a person who pressed Connect needs to
 * know that is why nothing appeared.
 */
function ConnectAccount({ actions }: { readonly actions: SettingsActions }): JSX.Element {
  const [provider, setProvider] =
    useState<(typeof CONNECTABLE_PROVIDERS)[number]>("codex");
  const [nickname, setNickname] = useState("");
  const [starting, setStarting] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);

  const connect = async (): Promise<void> => {
    setStarting(true);
    setRefusal(null);
    try {
      const accepted = await actions.beginConnection({
        provider_id: provider,
        nickname: nickname.trim() === "" ? provider : nickname.trim(),
        profile_label: null,
      });
      if (accepted === null) {
        setRefusal(
          "That provider could not be connected. Its credential profile may already be connected.",
        );
      } else {
        setNickname("");
      }
    } finally {
      setStarting(false);
    }
  };

  return (
    <form
      className="connect-account"
      onSubmit={(event) => {
        event.preventDefault();
        connect().catch(() => {
          // The refusal is already reported in the panel below the form.
        });
      }}
    >
      <label className="control-label" htmlFor="connect-provider">
        Add an account
      </label>
      <div className="connect-account__row">
        <select
          id="connect-provider"
          value={provider}
          onChange={(event) => {
            setProvider(event.target.value as (typeof CONNECTABLE_PROVIDERS)[number]);
          }}
        >
          {CONNECTABLE_PROVIDERS.map((id) => (
            <option key={id} value={id}>
              {id.replace(/_/g, " ")}
            </option>
          ))}
        </select>
        <input
          type="text"
          value={nickname}
          placeholder="Label (optional)"
          aria-label="Label for the new account"
          onChange={(event) => {
            setNickname(event.target.value);
          }}
        />
        <button type="submit" className="button" disabled={starting}>
          {starting ? "Connecting..." : "Connect"}
        </button>
      </div>
      {refusal !== null ? (
        <p className="note" role="status">
          {refusal}
        </p>
      ) : null}
    </form>
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
        <ConnectAccount actions={actions} />
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
      <ConnectAccount actions={actions} />
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
