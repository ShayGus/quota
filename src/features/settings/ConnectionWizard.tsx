/**
 * Add an account: Provider → Connect → Verify.
 *
 * Connecting uses the provider's existing local sign-in; credentials never
 * enter this window. The host saves an account as soon as it verifies one, and
 * its result does not say which account that was, so the wizard records the
 * provider's accounts when Connect is pressed and identifies the new one when
 * it arrives in the snapshot. The person then confirms the identity and names
 * the account; going back or cancelling removes an account they did not confirm.
 */
import { useState, type JSX } from "react";

import type {
  AccountId,
  AccountSnapshot,
  AttemptRef,
  ProviderId,
} from "../../generated/bindings";
import { accountLabel } from "../../shared/format/alias";
import { providerLabel } from "../../shared/format/provider";
import { describeCommandError, launch } from "../../shared/ipc/report";
import type { RendererState } from "../../shared/state/types";
import { Icon } from "../../shared/ui/Icon";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import type { SettingsActions } from "./Settings";

const PROVIDERS = ["codex", "claude", "open_code_go"] as const;

/** The quota windows each provider reports, as the picker lists them. */
const PROVIDER_WINDOWS: Record<(typeof PROVIDERS)[number], string> = {
  codex: "5-hour · Weekly",
  claude: "5-hour · Weekly · Model-specific",
  open_code_go: "5-hour · Weekly · Monthly",
};

const AUTHENTICATION_RECOVERY = {
  codex:
    "Codex sign-in is required. Open a terminal, run codex login, then press Connect again.",
  claude:
    "Claude Code sign-in is required. Run claude in a terminal, sign in, then press Connect again.",
  open_code_go:
    "OpenCode Go sign-in is required. Sign in with OpenCode, or set OPENCODE_API_KEY, then press Connect again.",
};

/** The nickname a new account starts with, as the wireframe suggests it. */
const DEFAULT_NICKNAME = "Personal";

/** The actions the wizard needs. */
export type WizardActions = Pick<
  SettingsActions,
  "beginConnection" | "cancelConnection" | "renameAccount" | "disconnectAccount"
>;

/**
 * The account a verified attempt saved: one of the provider's connected
 * accounts that was not there when Connect was pressed.
 *
 * Verification can arrive before the snapshot that carries the new account, so
 * an account that already existed is never taken for it: confirming, naming,
 * or discarding must only ever touch the account this attempt added.
 */
function savedAccount(
  accounts: readonly AccountSnapshot[],
  provider: ProviderId,
  before: ReadonlySet<AccountId>,
): AccountSnapshot | null {
  return (
    accounts
      .filter(
        (account) =>
          account.provider_id === provider &&
          account.connection_state === "connected" &&
          !before.has(account.account_id),
      )
      .sort((a, b) => b.connection_ordinal - a.connection_ordinal)[0] ?? null
  );
}

export function ConnectionWizard({
  state,
  actions,
  onDone,
}: {
  readonly state: RendererState;
  readonly actions: WizardActions;
  /** Leaves the wizard, whether or not an account was added. */
  readonly onDone: () => void;
}): JSX.Element {
  const [provider, setProvider] = useState<(typeof PROVIDERS)[number] | null>(null);
  const [nickname, setNickname] = useState(DEFAULT_NICKNAME);
  const [before, setBefore] = useState<ReadonlySet<AccountId>>(new Set());
  const [attempt, setAttempt] = useState<AttemptRef | null>(null);
  const [starting, setStarting] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const progress = state.attempts.find(
    (entry) => entry.attemptId === attempt?.id,
  )?.progress;
  const accounts = state.snapshot?.accounts ?? [];
  const saved =
    provider !== null && progress?.kind === "verified"
      ? savedAccount(accounts, provider, before)
      : null;
  const busy =
    starting ||
    (attempt !== null &&
      (progress === undefined ||
        progress.kind === "started" ||
        progress.kind === "awaiting_user" ||
        // Verified, but the account has not arrived in the snapshot yet.
        (progress.kind === "verified" && saved === null)));
  const step = provider === null ? 1 : saved !== null ? 3 : 2;

  const connect = async (): Promise<void> => {
    if (provider === null || busy) return;
    setStarting(true);
    setConfirmed(false);
    setRefusal(null);
    setAttempt(null);
    setBefore(
      new Set(
        accounts
          .filter((account) => account.provider_id === provider)
          .map((account) => account.account_id),
      ),
    );
    try {
      const accepted = await actions.beginConnection({
        provider_id: provider,
        nickname: nickname.trim() || DEFAULT_NICKNAME,
        profile_label: null,
      });
      setAttempt(accepted);
      if (accepted === null)
        setRefusal(
          "The connection was refused. Check the provider's local sign-in and try again.",
        );
    } finally {
      setStarting(false);
    }
  };

  /** Removes an account the person did not confirm, then forgets the attempt. */
  const discardUnconfirmed = (): void => {
    if (saved !== null) {
      actions.disconnectAccount(saved.account_id);
    }
    setAttempt(null);
    setConfirmed(false);
  };

  const cancel = async (): Promise<void> => {
    if (attempt !== null && busy) await actions.cancelConnection(attempt);
    discardUnconfirmed();
    onDone();
  };

  const finish = (): void => {
    if (saved === null || !confirmed || nickname.trim().length === 0) return;
    const name = nickname.trim();
    if (name !== saved.nickname) {
      actions.renameAccount(saved.account_id, name);
    }
    onDone();
  };

  const steps = (
    <div className="step-line" aria-label="Connection steps">
      {["Provider", "Connect", "Verify"].map((label, index) => (
        <StepMarker key={label} index={index} label={label} step={step} />
      ))}
    </div>
  );

  let body: JSX.Element;
  if (provider === null) {
    body = (
      <>
        <h2>Add a subscription</h2>
        <p className="intro">
          Choose a provider to connect its account. Only real quota windows will appear in
          your overview.
        </p>
        {PROVIDERS.map((id) => (
          <button
            key={id}
            type="button"
            className="provider-pick"
            onClick={() => {
              setProvider(id);
            }}
          >
            <ProviderMark providerId={id} />
            <span className="provider-copy">
              <span className="provider-name">{providerLabel(id)}</span>
              <span className="provider-meta">{PROVIDER_WINDOWS[id]}</span>
            </span>
            <Icon name="chevron-right" />
          </button>
        ))}
        <div className="note">
          Quota reads the provider's existing local sign-in. It never asks for a password,
          and credentials never enter this window.
        </div>
      </>
    );
  } else if (saved !== null) {
    const alias = accountLabel(state.preferences, accounts, saved.account_id);
    const identity = saved.identity;
    const windows = saved.windows.length;
    body = (
      <>
        <div className="success-icon">
          <Icon name="check" />
        </div>
        <h2>Is this the right account?</h2>
        <p className="intro">
          Confirm the identity before adding this subscription to your overview.
        </p>
        <dl className="detail-list">
          <div>
            <dt>Provider</dt>
            <dd>{providerLabel(provider)}</dd>
          </div>
          <div>
            <dt>Account</dt>
            <dd>{alias || identity?.principal_label || "Not reported"}</dd>
          </div>
          <div>
            <dt>Workspace</dt>
            <dd>
              {alias ? "Workspace hidden" : (identity?.workspace_label ?? "Not reported")}
            </dd>
          </div>
          <div>
            <dt>Quota reading</dt>
            <dd>
              {windows === 0
                ? "Not reported yet"
                : `${String(windows)} ${windows === 1 ? "window" : "windows"} available`}
            </dd>
          </div>
        </dl>
        <label className="field-label" htmlFor="account-nickname">
          Account nickname
        </label>
        <input
          type="text"
          id="account-nickname"
          autoComplete="off"
          maxLength={32}
          value={nickname}
          placeholder="For example: Personal"
          required
          onChange={(event) => {
            setNickname(event.currentTarget.value);
          }}
        />
        <div className="form-hint">
          Shown below the provider name. Going back or cancelling removes this account
          again.
        </div>
        <label className="checkline">
          <input
            id="confirm-account"
            type="checkbox"
            checked={confirmed}
            onChange={(event) => {
              setConfirmed(event.currentTarget.checked);
            }}
          />
          This is the account I intended to connect.
        </label>
        <div className="wizard-action">
          <button type="button" className="button" onClick={discardUnconfirmed}>
            Back
          </button>
          <button
            type="button"
            className="button primary"
            disabled={!confirmed || nickname.trim().length === 0}
            onClick={finish}
          >
            <Icon name="plus" />
            Add account
          </button>
        </div>
      </>
    );
  } else {
    body = (
      <>
        <h2>Connect {providerLabel(provider)}</h2>
        <p className="intro">
          Quota uses this provider's existing local sign-in to read your quota.
        </p>
        <div className="connection-box">
          <div className="identity">
            <ProviderMark providerId={provider} />
            <span className="provider-copy">
              <span className="provider-name">{providerLabel(provider)}</span>
              <span className="provider-meta">Existing local sign-in</span>
            </span>
          </div>
          <h3>Quota-only connection</h3>
          <p>
            No sign-in opens here and no inference is submitted. The next step shows the
            account the provider verified, for you to confirm.
          </p>
        </div>
        {progress?.kind === "verified" ? (
          <div className="note" role="status">
            The provider verified the connection. Waiting for the new account to arrive;
            if it was already connected, Cancel and find it in your overview.
          </div>
        ) : null}
        {progress?.kind === "awaiting_user" ? (
          <div className="note" role="status">
            Finish signing in with the provider's own tool.
          </div>
        ) : null}
        {progress?.kind === "failed" ? (
          <div className="note" role="alert">
            {progress.context.error.kind === "reconnect_required"
              ? AUTHENTICATION_RECOVERY[provider]
              : describeCommandError(progress.context.error)}
          </div>
        ) : null}
        {progress?.kind === "cancelled" ? (
          <div className="note" role="status">
            Connection cancelled. You can try again.
          </div>
        ) : null}
        {refusal === null ? null : (
          <div className="note" role="alert">
            {refusal}
          </div>
        )}
        <div className="note">
          A connection is complete only after its account identity and an actual quota
          reading have been verified.
        </div>
        <div className="wizard-action">
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={() => {
              setProvider(null);
              setAttempt(null);
              setRefusal(null);
            }}
          >
            Back
          </button>
          <button
            type="button"
            className="button primary"
            disabled={busy}
            onClick={() => {
              launch(connect());
            }}
          >
            <Icon name={busy ? "clock" : "arrow-right"} />
            {busy ? "Verifying…" : "Connect"}
          </button>
        </div>
      </>
    );
  }

  return (
    <section aria-label="Connect an account">
      <div className="back-row">
        <button
          type="button"
          className="back-button"
          disabled={starting}
          onClick={() => {
            launch(cancel());
          }}
        >
          <Icon name="arrow-left" />
          Cancel
        </button>
        <span className="badge">LOCAL SIGN-IN</span>
      </div>
      <div className="wizard">
        {steps}
        {body}
      </div>
    </section>
  );
}

/** One numbered step, with the rule that joins it to the next. */
function StepMarker({
  index,
  label,
  step,
}: {
  readonly index: number;
  readonly label: string;
  readonly step: number;
}): JSX.Element {
  const current = step === index + 1;
  return (
    <>
      {index > 0 ? <span className="step-rule" /> : null}
      <span
        className={`step${current ? " selected" : ""}`}
        aria-current={current ? "step" : undefined}
      >
        {index + 1}
      </span>
      <span>{label}</span>
    </>
  );
}
