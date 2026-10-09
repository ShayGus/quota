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
  GroupSnapshot,
  Preferences,
} from "../../../generated/bindings";
import { accountLabel, displayName } from "../../../shared/format/alias";
import { hasHideableKeyLimit } from "../../../shared/format/balance";
import { groupById, groupLabel, isGroupable } from "../../../shared/format/group";
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
  | { readonly kind: "new-group"; readonly account: AccountSnapshot }
  | {
      readonly kind: "rename-group";
      readonly account: AccountSnapshot;
      readonly group: GroupSnapshot;
    }
  | { readonly kind: "reconnect"; readonly account: AccountSnapshot }
  | { readonly kind: "disconnect"; readonly account: AccountSnapshot };

/**
 * Why the order buttons cannot move an account. The host ranks accounts by
 * their least remaining allowance and has no command to reorder them.
 */
const ORDER_FIXED = "Order follows the least remaining allowance first";

/** The choice for "New group…" in the group list. */
const NEW_GROUP = "new";

/**
 * Which provider account a key belongs to: no group, a group of the same
 * provider, or a new one. The provider names no account, so the person
 * decides which keys belong together.
 */
function GroupChoice({
  account,
  label,
  groups,
  preferences,
  alias,
  actions,
  onPending,
}: {
  readonly account: AccountSnapshot;
  readonly label: string;
  readonly groups: readonly GroupSnapshot[];
  readonly preferences: Preferences | null;
  readonly alias: string;
  readonly actions: SettingsActions;
  readonly onPending: (pending: Pending) => void;
}): JSX.Element {
  const provider = providerLabel(account.provider_id);
  const choices = groups.filter((group) => group.provider_id === account.provider_id);
  const current =
    account.group === null ? undefined : groupById(groups, account.group.id);
  return (
    <div className="account-manage-option">
      <span>
        <span className="account-manage-option-label">Account group</span>
        <small>Put the keys of one {provider} account together to see its total.</small>
      </span>
      <span className="account-group-controls">
        <select
          aria-label={`Account group of ${provider} ${label}`}
          value={current?.id ?? ""}
          onChange={(event) => {
            const value = event.currentTarget.value;
            if (value === NEW_GROUP) {
              onPending({ kind: "new-group", account });
            } else {
              actions.setAccountGroup(account.account_id, value === "" ? null : value);
            }
          }}
        >
          <option value="">Not grouped</option>
          {choices.map((group) => (
            <option key={group.id} value={group.id}>
              {groupLabel(preferences, groups, group)}
            </option>
          ))}
          <option value={NEW_GROUP}>New group…</option>
        </select>
        {current === undefined ? null : (
          <button
            type="button"
            className="text-btn"
            disabled={alias !== ""}
            title={alias === "" ? undefined : "Show account names to rename"}
            onClick={() => {
              onPending({ kind: "rename-group", account, group: current });
            }}
          >
            Rename group
          </button>
        )}
      </span>
    </div>
  );
}

/** One managed account. */
function ManagedAccount({
  account,
  label,
  alias,
  groups,
  preferences,
  actions,
  onPending,
}: {
  readonly account: AccountSnapshot;
  readonly label: string;
  readonly alias: string;
  readonly groups: readonly GroupSnapshot[];
  readonly preferences: Preferences | null;
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
      {/* A key in a group always shows its limit, which is what tells the keys apart. */}
      {hasHideableKeyLimit(account) && account.group === null ? (
        <div className="account-manage-option">
          <span>
            <span className="account-manage-option-label">
              Show this key's spend limit
            </span>
            <small>
              Off when the key is only for Quota. The account balance is shown either way.
            </small>
          </span>
          <button
            type="button"
            role="switch"
            className="switch"
            aria-checked={account.show_key_limit}
            aria-label={`Show the key spend limit of ${provider} ${label}`}
            onClick={() => {
              actions.setKeyLimitShown(account.account_id, !account.show_key_limit);
            }}
          />
        </div>
      ) : null}
      {isGroupable(account.provider_id) ? (
        <GroupChoice
          account={account}
          label={label}
          groups={groups}
          preferences={preferences}
          alias={alias}
          actions={actions}
          onPending={onPending}
        />
      ) : null}
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
          title={alias === "" ? undefined : "Show account names to rename"}
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
  const [groupName, setGroupName] = useState(
    pending.kind === "rename-group" ? pending.group.name : "",
  );
  switch (pending.kind) {
    case "new-group":
      return (
        <Dialog
          title="New account group"
          confirmLabel="Create group"
          confirmDisabled={groupName.trim().length === 0}
          onConfirm={() => {
            actions.createAccountGroup(groupName.trim(), [account.account_id]);
          }}
          onClose={onClose}
        >
          <p>
            Name the {provider} account this key belongs to. Add its other keys to the
            same group from their own settings.
          </p>
          <NicknameField
            id="group-name-input"
            label="Group name"
            placeholder="For example: Work"
            value={groupName}
            hidden={alias !== ""}
            onChange={setGroupName}
          />
        </Dialog>
      );
    case "rename-group":
      return (
        <Dialog
          title="Rename account group"
          confirmLabel="Save name"
          confirmDisabled={groupName.trim().length === 0}
          onConfirm={() => {
            const next = groupName.trim();
            if (next !== pending.group.name) {
              actions.renameAccountGroup(pending.group.id, next);
            }
          }}
          onClose={onClose}
        >
          <p>The new name applies to every key in the group.</p>
          <NicknameField
            id="group-name-input"
            label="Group name"
            placeholder="For example: Work"
            value={groupName}
            hidden={alias !== ""}
            onChange={setGroupName}
          />
        </Dialog>
      );
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
            Quota checks this account again with the sign-in it already uses. There is
            nothing to type here.
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
  groups,
  preferences,
  actions,
  onAddAccount,
}: {
  readonly accounts: readonly AccountSnapshot[];
  /** The account groups, for the group choice of each key. */
  readonly groups: readonly GroupSnapshot[];
  readonly preferences: Preferences | null;
  readonly actions: SettingsActions;
  /** Opens the add-account page of this window. */
  readonly onAddAccount: () => void;
}): JSX.Element {
  const [pending, setPending] = useState<Pending | null>(null);
  const aliasOf = (id: AccountId): string => accountLabel(preferences, accounts, id);
  return (
    <>
      <SettingsTitle
        title="Accounts"
        intro="Manage connected identities and their monitoring."
        action={
          <button type="button" className="button primary" onClick={onAddAccount}>
            <Icon name="plus" />
            Add account
          </button>
        }
      />
      {accounts.length === 0 ? (
        <div className="empty compact">
          <h2>No connected accounts</h2>
          <p>Add an account to start monitoring its quota.</p>
          <button type="button" className="button" onClick={onAddAccount}>
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
            groups={groups}
            preferences={preferences}
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
