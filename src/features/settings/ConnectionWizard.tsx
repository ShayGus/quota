/**
 * Add an account: Provider → Connect → Verify.
 *
 * Connecting uses the provider's existing local sign-in, or, for a provider
 * with no local tool, an API key the person pastes, which goes straight to the
 * host. A verified attempt is held by the host as a pending
 * candidate: nothing is saved and no monitoring starts until the person
 * confirms it and names it here. Leaving the wizard by any route, whether
 * Back, Cancel, Escape, or opening a fresh wizard, discards the candidate, so
 * an unconfirmed account is never left behind.
 */
import { useEffect, useId, useRef, useState, type JSX } from "react";

import type { AttemptRef, BrowserSignIn, QuotaWindow } from "../../generated/bindings";
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
import {
  AUTHENTICATION_RECOVERY,
  PROVIDER_WINDOWS,
  PROVIDERS,
  SIGN_IN,
  SWITCH_ACCOUNT,
  credentialStoreName,
  type OfferedProvider,
} from "./providers";

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
  const [provider, setProvider] = useState<OfferedProvider | null>(null);
  const [nickname, setNickname] = useState(DEFAULT_NICKNAME);
  // A pasted API key, held only until the host has it.
  const [apiKey, setApiKey] = useState("");
  const signIn = provider === null ? null : SIGN_IN[provider];
  const takesKey = signIn?.kind === "api_key";
  // A provider whose own CLI can sign it in takes a key, but does not need one.
  const needsKey = takesKey && signIn.cli === undefined;
  const browser = signIn?.kind === "browser" ? signIn : null;
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

  /** Starts the attempt; `viaBrowser` signs in on the provider's own page. */
  const connect = async (viaBrowser = false): Promise<void> => {
    if (provider === null || busy || (needsKey && apiKey.trim() === "")) return;
    setStarting(true);
    setNotice(null);
    setRefusal(null);
    setAttempt(null);
    try {
      const accepted = await actions.beginConnection({
        provider_id: provider,
        nickname: nickname.trim() || DEFAULT_NICKNAME,
        profile_label: null,
        credential: takesKey && apiKey.trim() !== "" ? apiKey.trim() : null,
        browser_sign_in: viaBrowser,
      });
      if (!mounted.current) {
        if (accepted !== null) await actions.cancelConnection(accepted);
        return;
      }
      setAttempt(accepted);
      // The host holds the key from here on, so the window lets go of it.
      if (accepted !== null) setApiKey("");
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
          Codex, Claude, OpenCode Go and Cursor use the sign-in their own apps keep on
          this computer. Grok and Muse Code sign in on their own page in your browser, and
          the others take an API key. Quota keeps what it is given in{" "}
          {credentialStoreName()} and never asks for a password.
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
          {signIn?.kind === "api_key"
            ? `Paste an API key from ${signIn.keyPage}. Quota checks it with ${providerLabel(provider)} before anything is saved.`
            : signIn?.kind === "browser"
              ? `Sign in to your ${signIn.account} account in your browser. Quota checks the account before anything is saved.`
              : "Quota uses this provider's existing local sign-in to read your quota."}
        </p>
        <div className="connection-box">
          <div className="identity">
            <ProviderMark providerId={provider} />
            <span className="provider-copy">
              <span className="provider-name">{providerLabel(provider)}</span>
              <span className="provider-meta">
                {takesKey
                  ? "API key"
                  : browser
                    ? "Browser sign-in"
                    : "Existing local sign-in"}
              </span>
            </span>
          </div>
          {signIn?.kind === "api_key" ? (
            <ApiKeyField
              value={apiKey}
              placeholder={signIn.placeholder}
              providerName={providerLabel(provider)}
              cli={signIn.cli}
              disabled={busy}
              onChange={setApiKey}
            />
          ) : browser ? (
            <BrowserSignInBox
              account={browser.account}
              cli={browser.cli}
              signIn={
                progress?.kind === "awaiting_user" ? progress.context.sign_in : null
              }
            />
          ) : (
            <>
              <h3>Quota-only connection</h3>
              <p>
                No sign-in opens here and no inference is submitted. The next step shows
                the account the provider verified, for you to confirm.
              </p>
            </>
          )}
        </div>
        {progress?.kind === "awaiting_user" && progress.context.sign_in === null ? (
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
              setApiKey("");
            }}
          >
            Back
          </button>
          {browser === null ? null : (
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() => {
                launch(connect(false));
              }}
            >
              Use the {browser.cli.name} sign-in
            </button>
          )}
          <button
            type="button"
            className="button primary"
            disabled={busy || (needsKey && apiKey.trim() === "")}
            onClick={() => {
              launch(connect(browser !== null));
            }}
          >
            <Icon name={busy ? "clock" : "arrow-right"} />
            {busy
              ? progress?.kind === "awaiting_user" && progress.context.sign_in !== null
                ? "Waiting for the browser…"
                : "Verifying…"
              : browser
                ? "Sign in with browser"
                : "Connect"}
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
        <span className="badge">
          {takesKey ? "API KEY" : browser ? "BROWSER SIGN-IN" : "LOCAL SIGN-IN"}
        </span>
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

/** The API key a provider without a local tool is signed in with. */
function ApiKeyField({
  value,
  placeholder,
  providerName,
  cli,
  disabled,
  onChange,
}: {
  readonly value: string;
  readonly placeholder: string;
  readonly providerName: string;
  readonly cli: { readonly name: string; readonly signIn: string } | undefined;
  readonly disabled: boolean;
  readonly onChange: (value: string) => void;
}): JSX.Element {
  const hintId = useId();
  return (
    <div className="api-key">
      <label className="field-label" htmlFor="provider-api-key">
        API key
      </label>
      <input
        type="password"
        id="provider-api-key"
        autoComplete="off"
        spellCheck={false}
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        aria-describedby={hintId}
        onChange={(event) => {
          onChange(event.currentTarget.value);
        }}
      />
      <div className="form-hint" id={hintId}>
        {cli === undefined
          ? ""
          : `Or leave it empty to use the sign-in the ${cli.name} keeps on this computer (${cli.signIn}). `}
        Quota keeps a key in {credentialStoreName()} and sends it only to {providerName},
        to read your quota. It never submits a request to a model.
      </div>
    </div>
  );
}

/** What a browser sign-in shows: how it works, then the code to enter. */
function BrowserSignInBox({
  account,
  cli,
  signIn,
}: {
  readonly account: string;
  readonly cli: { readonly name: string; readonly signIn: string };
  readonly signIn: BrowserSignIn | null;
}): JSX.Element {
  if (signIn === null) {
    return (
      <>
        <h3>Sign in on {account}&apos;s page</h3>
        <p>
          Quota opens {account}&apos;s sign-in page in your browser and shows a code to
          enter there. It never sees your password, and it sends nothing to a model. Or
          use the sign-in the {cli.name} keeps on this computer ({cli.signIn}).
        </p>
      </>
    );
  }
  return (
    <div className="sign-in-code" role="status">
      <span className="field-label">Enter this code on {account}&apos;s page</span>
      <strong>{signIn.user_code}</strong>
      <span className="form-hint">
        Quota opened {signIn.verification_uri} in your browser. If it did not open, go
        there yourself.
      </span>
    </div>
  );
}
