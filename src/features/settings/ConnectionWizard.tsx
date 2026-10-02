/**
 * Add an account: Provider → Connect → Verify.
 *
 * Connecting uses the provider's existing local sign-in; credentials never
 * enter this window. A verified attempt is held by the host as a pending
 * candidate: nothing is saved and no monitoring starts until the person
 * confirms it and names it here. Leaving the wizard by any route, whether
 * Back, Cancel, Escape, or opening a fresh wizard, discards the candidate, so
 * an unconfirmed account is never left behind.
 */
import { useEffect, useRef, useState, type JSX } from "react";

import type { AttemptRef } from "../../generated/bindings";
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
  "beginConnection" | "cancelConnection" | "confirmConnection"
>;

export function ConnectionWizard({
  state,
  actions,
  onDone,
}: {
  readonly state: RendererState;
  readonly actions: WizardActions;
  /** Leaves the wizard. `added` says whether an account was saved. */
  readonly onDone: (added: boolean) => void;
}): JSX.Element {
  const [provider, setProvider] = useState<(typeof PROVIDERS)[number] | null>(null);
  const [nickname, setNickname] = useState(DEFAULT_NICKNAME);
  const [attempt, setAttempt] = useState<AttemptRef | null>(null);
  const [starting, setStarting] = useState(false);
  const [adding, setAdding] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const progress = state.attempts.find(
    (entry) => entry.attemptId === attempt?.id,
  )?.progress;
  const candidate =
    progress?.kind === "awaiting_confirmation" ? progress.context.candidate : null;
  const accounts = state.snapshot?.accounts ?? [];
  // A candidate is not in the account list yet, so it takes the label the alias
  // helper gives an account it has not numbered, as details does.
  const alias = accountLabel(state.preferences, accounts, "");
  const busy =
    starting ||
    (attempt !== null &&
      (progress === undefined ||
        progress.kind === "started" ||
        progress.kind === "awaiting_user"));
  const step = provider === null ? 1 : candidate !== null ? 3 : 2;

  // What must be discarded if the wizard goes away: a running attempt or a
  // held candidate. Kept in a ref so the unmount cleanup sees the latest.
  const pending = useRef<AttemptRef | null>(null);
  const live =
    attempt !== null && !adding && (busy || candidate !== null) ? attempt : null;
  useEffect(() => {
    pending.current = live;
  }, [live]);
  useEffect(() => {
    return () => {
      const held = pending.current;
      if (held !== null) {
        launch(actions.cancelConnection(held));
      }
    };
  }, [actions]);

  const connect = async (): Promise<void> => {
    if (provider === null || busy) return;
    setStarting(true);
    setConfirmed(false);
    setRefusal(null);
    setAttempt(null);
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

  /** Discards the live attempt or candidate, if any, and forgets it. */
  const discard = async (): Promise<void> => {
    pending.current = null;
    setAttempt(null);
    setConfirmed(false);
    if (live !== null) await actions.cancelConnection(live);
  };

  const add = async (): Promise<void> => {
    if (attempt === null || adding || !confirmed || nickname.trim().length === 0) return;
    setAdding(true);
    try {
      if (await actions.confirmConnection(attempt, nickname.trim())) {
        pending.current = null;
        onDone(true);
      }
    } finally {
      setAdding(false);
    }
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
  } else if (candidate !== null) {
    const identity = candidate.identity;
    const windows = candidate.windows.length;
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
            <dd>{providerLabel(candidate.provider_id)}</dd>
          </div>
          <div>
            <dt>Account</dt>
            <dd>{alias || identity.principal_label}</dd>
          </div>
          <div>
            <dt>Workspace</dt>
            <dd>
              {alias ? "Workspace hidden" : (identity.workspace_label ?? "Not reported")}
            </dd>
          </div>
          <div>
            <dt>Quota reading</dt>
            <dd>
              {windows === 0
                ? "Not reported"
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
          Shown below the provider name. Nothing is saved until you add the account.
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
          <button
            type="button"
            className="button"
            disabled={adding}
            onClick={() => {
              launch(discard());
            }}
          >
            Back
          </button>
          <button
            type="button"
            className="button primary"
            disabled={adding || !confirmed || nickname.trim().length === 0}
            onClick={() => {
              launch(add());
            }}
          >
            <Icon name={adding ? "clock" : "plus"} />
            {adding ? "Adding…" : "Add account"}
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
          A connection is added only after its account identity and an actual quota
          reading have been verified and you confirm it.
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
          disabled={starting || adding}
          onClick={() => {
            if (live === null) {
              onDone(false);
              return;
            }
            launch(
              discard().then(() => {
                onDone(false);
              }),
            );
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
