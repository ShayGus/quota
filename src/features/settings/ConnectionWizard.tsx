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

import type { AttemptRef, QuotaWindow } from "../../generated/bindings";
import { accountLabel } from "../../shared/format/alias";
import { formatRemaining } from "../../shared/format/allowance";
import { providerLabel } from "../../shared/format/provider";
import { describeCommandError, launch } from "../../shared/ipc/report";
import type { RendererState } from "../../shared/state/types";
import { Icon } from "../../shared/ui/Icon";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import { NicknameField } from "./Primitives";
import type { SettingsActions } from "./Settings";
import { windowLabel } from "../overview/reading";

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

/** How to put a different account into each provider's own sign-in. */
const SWITCH_ACCOUNT = {
  codex:
    "To add a different Codex account, run codex login with it, then press Connect again.",
  claude:
    "To add a different Claude account, sign in to it in Claude Code (/login), then press Connect again.",
  open_code_go:
    "To add a different OpenCode Go account, sign in to it with OpenCode, then press Connect again.",
};

/**
 * A verified window's name on Verify: its period and the allowance it measures,
 * so two windows of one period stay distinct.
 */
function verifiedWindowName(window: QuotaWindow): string {
  const name = windowLabel(window);
  const scope = window.scope.label;
  return scope === "" || name.includes(scope) ? name : `${name} · ${scope}`;
}

/** The reading being approved, in the words the overview uses. */
function verifiedReading(window: QuotaWindow): string {
  const value = formatRemaining(window.measurement);
  return /\d/.test(value) ? `${value} remaining` : value;
}

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
  // Said on Connect after a verified account was turned down.
  const [notice, setNotice] = useState<string | null>(null);
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
  // Whether the wizard is still on screen, so an answer that arrives after it
  // has gone neither leaves an attempt running nor navigates.
  const mounted = useRef(false);
  // Whether the saved account has already been reported, so the reply and the
  // Verified event, whichever arrives first, finish the wizard only once.
  const finished = useRef(false);
  const live =
    attempt !== null && !adding && (busy || candidate !== null) ? attempt : null;
  useEffect(() => {
    pending.current = live;
  }, [live]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      const held = pending.current;
      if (held !== null) {
        launch(actions.cancelConnection(held));
      }
    };
  }, [actions]);

  /** Leaves the wizard once the host has saved the account. */
  const finish = (): void => {
    pending.current = null;
    if (finished.current || !mounted.current) return;
    finished.current = true;
    onDone(true);
  };

  // A save whose reply was lost still reports Verified, so the wizard leaves
  // instead of offering an Add account that can no longer succeed.
  const verified = progress?.kind === "verified";
  useEffect(() => {
    if (verified) finish();
  });

  const connect = async (): Promise<void> => {
    if (provider === null || busy) return;
    setStarting(true);
    setNotice(null);
    setRefusal(null);
    setAttempt(null);
    try {
      const accepted = await actions.beginConnection({
        provider_id: provider,
        nickname: nickname.trim() || DEFAULT_NICKNAME,
        profile_label: null,
      });
      if (!mounted.current) {
        if (accepted !== null) await actions.cancelConnection(accepted);
        return;
      }
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
    if (live !== null) await actions.cancelConnection(live);
  };

  const add = async (): Promise<void> => {
    if (attempt === null || adding || nickname.trim().length === 0) return;
    setAdding(true);
    try {
      if (await actions.confirmConnection(attempt, nickname.trim())) finish();
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
    const providerName = providerLabel(candidate.provider_id);
    const meta = [
      providerName,
      alias ? "Workspace hidden" : identity.workspace_label,
      identity.plan_label,
    ]
      .filter((part) => part !== null && part !== "")
      .join(" · ");
    body = (
      <>
        <h2>Add this account?</h2>
        <p className="intro">
          {providerName} verified this sign-in. Nothing is saved until you add it.
        </p>
        <div className="connection-box verified-account">
          <div className="identity">
            <ProviderMark providerId={candidate.provider_id} />
            <span className="provider-copy">
              <span className="provider-name">{alias || identity.principal_label}</span>
              <span className="provider-meta">{meta}</span>
            </span>
          </div>
          <dl className="detail-list">
            {candidate.windows.length === 0 ? (
              <div>
                <dt>Quota reading</dt>
                <dd>Not reported</dd>
              </div>
            ) : (
              candidate.windows.map((window) => (
                <div key={window.id}>
                  <dt>{verifiedWindowName(window)}</dt>
                  <dd>{verifiedReading(window)}</dd>
                </div>
              ))
            )}
          </dl>
        </div>
        <NicknameField
          id="account-nickname"
          value={nickname}
          hidden={alias !== ""}
          hint="Shown on the account's card in your overview."
          onChange={setNickname}
        />
        <div className="wizard-action">
          <button
            type="button"
            className="button"
            disabled={adding}
            onClick={() => {
              setNotice(SWITCH_ACCOUNT[provider]);
              launch(discard());
            }}
          >
            Not this account
          </button>
          <button
            type="button"
            className="button primary"
            disabled={adding || nickname.trim().length === 0}
            onClick={() => {
              launch(add());
            }}
          >
            <Icon name={adding ? "clock" : "plus"} />
            {adding ? "Adding…" : `Add ${providerName} account`}
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
        {notice === null ? null : (
          <div className="note" role="status">
            {notice}
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
