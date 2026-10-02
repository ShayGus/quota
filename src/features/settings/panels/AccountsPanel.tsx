/**
 * Account management settings.
 *
 * Separate identities, including several subscriptions from one provider. The
 * provider mark is text, so a provider label can never inject markup (AC-37).
 * Disconnecting one account leaves its same-provider siblings untouched
 * (spec 4.6, AC-40).
 *
 * A connection is not finished when the backend accepts it. The attempt runs
 * afterwards, so this panel keeps the button busy and shows the attempt's real
 * result, including the exact thing the person has to do when a credential is
 * missing.
 */
import { useState, type JSX } from "react";

import type {
  AccountSnapshot,
  CommandError,
  ConnectionAttemptId,
  Preferences,
} from "../../../generated/bindings";
import { formatAge, instantOf } from "../../../shared/format/duration";
import { describeCommandError, launch } from "../../../shared/ipc/report";
import type { AttemptProgress } from "../../../shared/state/types";
import { Icon } from "../../../shared/ui/Icon";
import { RefreshNotice } from "../../../shared/ui/RefreshNotice";
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

/** One provider this panel can start a connection for. */
type ProviderChoice = (typeof CONNECTABLE_PROVIDERS)[number];

/** One connection attempt this panel started. */
interface LiveAttempt {
  /** The identity the backend issued for the attempt. */
  readonly attemptId: ConnectionAttemptId;
  /** The provider the attempt is for, so the advice can be specific. */
  readonly provider: ProviderChoice;
}

/**
 * The exact thing to do about a connection that could not be completed.
 *
 * A missing credential is repaired in the client that owns it, so the advice
 * names that client rather than asking the person to try Connect again.
 */
function recoveryFor(provider: ProviderChoice, error: CommandError): string {
  if (error.kind !== "reconnect_required") {
    return describeCommandError(error);
  }
  if (provider === "codex") {
    return "Codex is not signed in. Open a terminal, run codex login, then press Connect again.";
  }
  if (provider === "claude") {
    return "Claude Code is not signed in. Run claude in a terminal, sign in, then press Connect again.";
  }
  return "No OpenCode Go key was found. Sign in with OpenCode, or set OPENCODE_API_KEY, then press Connect again.";
}

/**
 * Starts a connection for one provider and shows what happens next.
 *
 * The backend accepts the attempt before it does any work, so the button stays
 * busy until the attempt reports a result. A refusal is reported rather than
 * swallowed, because a credential profile that is already connected is the
 * common one.
 */
function ConnectAccount({
  attempts,
  actions,
}: {
  readonly attempts: readonly AttemptProgress[];
  readonly actions: SettingsActions;
}): JSX.Element {
  const [provider, setProvider] = useState<ProviderChoice>("codex");
  const [nickname, setNickname] = useState("");
  const [live, setLive] = useState<LiveAttempt | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const progress =
    live === null
      ? undefined
      : attempts.find((entry) => entry.attemptId === live.attemptId);
  const running =
    submitting ||
    (live !== null &&
      (progress === undefined ||
        progress.progress.kind === "started" ||
        progress.progress.kind === "awaiting_user"));
  const connected = progress !== undefined && progress.progress.kind === "verified";
  const cancelled = progress !== undefined && progress.progress.kind === "cancelled";
  const failure =
    live !== null && progress !== undefined && progress.progress.kind === "failed"
      ? recoveryFor(live.provider, progress.progress.context.error)
      : null;

  const connect = async (): Promise<void> => {
    if (running) {
      return;
    }
    // Starting again replaces the previous result; a still-running attempt is
    // kept, because it has not finished yet.
    if (live !== null) {
      actions.clearConnectionAttempt(live.attemptId);
    }
    setRefusal(null);
    setLive(null);
    setSubmitting(true);
    try {
      const accepted = await actions.beginConnection({
        provider_id: provider,
        nickname: nickname.trim() === "" ? provider : nickname.trim(),
        profile_label: null,
      });
      if (accepted === null) {
        setRefusal(
          "Quota could not start that connection. The credential profile may already be connected.",
        );
        return;
      }
      setNickname("");
      setLive({ attemptId: accepted.attempt_id, provider });
    } finally {
      setSubmitting(false);
    }
  };

  const dismiss = (): void => {
    if (live !== null) {
      actions.clearConnectionAttempt(live.attemptId);
    }
    setLive(null);
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
          disabled={running}
          onChange={(event) => {
            setProvider(event.currentTarget.value as ProviderChoice);
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
          disabled={running}
          onChange={(event) => {
            setNickname(event.currentTarget.value);
          }}
        />
        <button type="submit" className="button" disabled={running}>
          {running ? "Connecting..." : "Connect"}
        </button>
      </div>
      {running ? (
        <p className="note" role="status">
          {`Checking the ${provider.replace(/_/g, " ")} credential and reading your usage.`}
        </p>
      ) : null}
      {connected ? (
        <p className="note" role="status">
          Connected. Quota is reading this account now.
        </p>
      ) : null}
      {cancelled ? (
        <p className="note" role="status">
          That connection was cancelled.
        </p>
      ) : null}
      {failure !== null ? (
        <p className="note" role="alert">
          {failure}
        </p>
      ) : null}
      {refusal !== null ? (
        <p className="note" role="status">
          {refusal}
        </p>
      ) : null}
      {live !== null && !running ? (
        <button type="button" className="text-button" onClick={dismiss}>
          Dismiss
        </button>
      ) : null}
    </form>
  );
}

/** The account management panel. */
export function AccountsPanel({
  accounts,
  preferences,
  attempts,
  now,
  actions,
}: {
  readonly accounts: readonly AccountSnapshot[];
  readonly preferences: Preferences | null;
  readonly attempts: readonly AttemptProgress[];
  readonly now: number;
  readonly actions: SettingsActions;
}): JSX.Element {
  if (accounts.length === 0) {
    return (
      <>
        <h3 className="settings__title">Accounts</h3>
        <p className="note">No account is monitored yet.</p>
        <ConnectAccount attempts={attempts} actions={actions} />
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
      <ConnectAccount attempts={attempts} actions={actions} />
      <RefreshNotice accounts={accounts} preferences={preferences} now={now} />
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
