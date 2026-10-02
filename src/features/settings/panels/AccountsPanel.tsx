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
import { accountLabel, displayName } from "../../../shared/format/alias";
import { providerLabel } from "../../../shared/format/provider";
import { launch } from "../../../shared/ipc/report";
import { Dialog } from "../../../shared/ui/Dialog";
import { Icon } from "../../../shared/ui/Icon";
import { Identity } from "../../overview/ProviderCard";
import { NicknameField, SettingsTitle } from "../Primitives";
import type { SettingsActions } from "../Settings";

/** The confirmation an account action is waiting on, if any. */
type Pending =
  | { readonly kind: "rename"; readonly account: AccountSnapshot }
  | { readonly kind: "reconnect"; readonly account: AccountSnapshot }
  | { readonly kind: "disconnect"; readonly account: AccountSnapshot };

/**
 * Why the order buttons cannot move an account. The host ranks accounts by
 * their least remaining allowance and has no command to reorder them.
 */
const ORDER_FIXED = "Order follows the least remaining allowance first";

/** One managed account. */
function ManagedAccount({
  account,
  label,
  alias,
  actions,
  onPending,
}: {
  readonly account: AccountSnapshot;
  readonly label: string;
  readonly alias: string;
  readonly actions: SettingsActions;
  readonly onPending: (pending: Pending) => void;
}): JSX.Element {
  const provider = providerLabel(account.provider_id);
  const windows = account.windows.length;
  return (
    <article className="account-manage-card" aria-label={`Manage ${provider} ${label}`}>
      <div className="account-manage-head">
        <div className="identity">
          <Identity account={account} label={label} />
        </div>
        <button
          type="button"
          role="switch"
          className="switch"
          aria-checked={account.monitoring_enabled}
          aria-label={`Monitor ${provider} ${label}`}
          onClick={() => {
            actions.setAccountEnabled(account.account_id, !account.monitoring_enabled);
          }}
        />
      </div>
      <div className="provider-meta" style={{ marginTop: "8px", maxWidth: "none" }}>
        {alias
          ? "Identity hidden"
          : (account.identity?.principal_label ?? "Identity not verified")}
        {alias || account.identity?.workspace_label == null
          ? ""
          : ` · ${account.identity.workspace_label}`}{" "}
        · {windows} {windows === 1 ? "window" : "windows"}
      </div>
      <div className="account-manage-actions">
        <button
          type="button"
          className="text-btn"
          onClick={() => {
            actions.showAccountDetail(account.account_id);
          }}
        >
          Details
        </button>
        <button
          type="button"
          className="text-btn"
          disabled={alias !== ""}
          title={alias === "" ? undefined : "Show account labels to rename"}
          onClick={() => {
            onPending({ kind: "rename", account });
          }}
        >
          Rename
        </button>
        <button
          type="button"
          className="text-btn"
          onClick={() => {
            onPending({ kind: "reconnect", account });
          }}
        >
          Reconnect
        </button>
        <button
          type="button"
          className="text-btn danger"
          onClick={() => {
            onPending({ kind: "disconnect", account });
          }}
        >
          Disconnect
        </button>
        <span className="spacer">
          <button
            type="button"
            className="icon-btn"
            aria-label={`Move ${provider} ${label} up`}
            title={ORDER_FIXED}
            disabled
          >
            <Icon name="chevron-up" />
          </button>
          <button
            type="button"
            className="icon-btn"
            aria-label={`Move ${provider} ${label} down`}
            title={ORDER_FIXED}
            disabled
          >
            <Icon name="chevron-down" />
          </button>
        </span>
      </div>
    </article>
  );
}

/** The dialog for the account action awaiting confirmation. */
function PendingDialog({
  pending,
  label,
  alias,
  actions,
  onClose,
}: {
  readonly pending: Pending;
  readonly label: string;
  readonly alias: string;
  readonly actions: SettingsActions;
  readonly onClose: () => void;
}): JSX.Element {
  const { account } = pending;
  const provider = providerLabel(account.provider_id);
  const [nickname, setNickname] = useState(account.nickname);
  switch (pending.kind) {
    case "rename":
      return (
        <Dialog
          title="Rename account"
          confirmLabel="Save nickname"
          confirmDisabled={nickname.trim().length === 0}
          onConfirm={() => {
            const next = nickname.trim();
            if (next !== account.nickname) {
              actions.renameAccount(account.account_id, next);
            }
          }}
          onClose={onClose}
        >
          <p>Use a short label to distinguish this subscription.</p>
          <NicknameField
            id="rename-input"
            value={nickname}
            hidden={alias !== ""}
            onChange={setNickname}
          />
        </Dialog>
      );
    case "reconnect":
      return (
        <Dialog
          title={`Reconnect ${provider}`}
          confirmLabel="Reconnect"
          onConfirm={() => {
            launch(actions.reconnectAccount(account.account_id));
          }}
          onClose={onClose}
        >
          <p>
            Quota checks this account again through the provider's existing local sign-in.
            No sign-in opens here and no credentials enter this window.
          </p>
          <p>
            <strong>
              {alias ? "Identity hidden" : (account.identity?.principal_label ?? label)}
            </strong>
            <br />
            Workspace:{" "}
            {alias ? "hidden" : (account.identity?.workspace_label ?? "not reported")}
          </p>
        </Dialog>
      );
    case "disconnect":
      return (
        <Dialog
          title={`Disconnect ${provider}?`}
          confirmLabel="Disconnect"
          onConfirm={() => {
            actions.disconnectAccount(account.account_id);
          }}
          onClose={onClose}
        >
          <p>
            Remove {label} from Quota. This will not sign you out of {provider} or its
            command-line tool, and other {provider} accounts stay connected.
          </p>
        </Dialog>
      );
  }
}

/** The account management panel. */
export function AccountsPanel({
  accounts,
  preferences,
  actions,
}: {
  readonly accounts: readonly AccountSnapshot[];
  readonly preferences: Preferences | null;
  readonly actions: SettingsActions;
}): JSX.Element {
  const [pending, setPending] = useState<Pending | null>(null);
  const aliasOf = (id: AccountId): string => accountLabel(preferences, accounts, id);
  return (
    <>
      <SettingsTitle
        title="Accounts"
        intro="Manage connected identities and their monitoring."
        action={
          <button
            type="button"
            className="button primary"
            onClick={actions.showAddAccount}
          >
            <Icon name="plus" />
            Add account
          </button>
        }
      />
      {accounts.length === 0 ? (
        <div className="empty compact">
          <h2>No connected accounts</h2>
          <p>Add an account to start monitoring its quota.</p>
          <button type="button" className="button" onClick={actions.showAddAccount}>
            Add account
          </button>
        </div>
      ) : (
        accounts.map((account) => (
          <ManagedAccount
            key={account.account_id}
            account={account}
            label={displayName(preferences, accounts, account)}
            alias={aliasOf(account.account_id)}
            actions={actions}
            onPending={setPending}
          />
        ))
      )}
      <div className="note">
        Disconnecting an account removes it from Quota only. It never signs you out of the
        provider or its command-line tool.
      </div>
      {pending === null ? null : (
        <PendingDialog
          key={`${pending.kind}:${pending.account.account_id}`}
          pending={pending}
          label={displayName(preferences, accounts, pending.account)}
          alias={aliasOf(pending.account.account_id)}
          actions={actions}
          onClose={() => {
            setPending(null);
          }}
        />
      )}
    </>
  );
}
