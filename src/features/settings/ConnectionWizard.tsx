import { useEffect, useRef, useState, type JSX } from "react";

import type { AttemptRef } from "../../generated/bindings";
import { accountLabel } from "../../shared/format/alias";
import { formatRemaining, hasReading } from "../../shared/format/allowance";
import { providerLabel } from "../../shared/format/provider";
import { describeCommandError, launch } from "../../shared/ipc/report";
import type { RendererState } from "../../shared/state/types";
import { Icon } from "../../shared/ui/Icon";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import type { SettingsActions } from "./Settings";

const PROVIDERS = ["codex", "claude", "open_code_go"] as const;

const AUTHENTICATION_RECOVERY = {
  codex:
    "Codex sign-in is required. Open a terminal, run codex login, then press Connect again.",
  claude:
    "Claude Code sign-in is required. Run claude in a terminal, sign in, then press Connect again.",
  open_code_go:
    "OpenCode Go sign-in is required. Sign in with OpenCode, or set OPENCODE_API_KEY, then press Connect again.",
};

export function ConnectionWizard({
  state,
  actions,
  onDone,
}: {
  readonly state: RendererState;
  readonly actions: SettingsActions;
  readonly onDone: () => void;
}): JSX.Element {
  const [provider, setProvider] = useState<(typeof PROVIDERS)[number] | null>(null);
  const [nickname, setNickname] = useState("Personal");
  const [attempt, setAttempt] = useState<AttemptRef | null>(null);
  const [starting, setStarting] = useState(false);
  const [adding, setAdding] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const mounted = useRef(false);
  const cancelConnection = actions.cancelConnection;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    return () => {
      if (attempt !== null) launch(cancelConnection(attempt));
    };
  }, [attempt, cancelConnection]);

  const progress = state.attempts.find(
    (entry) => entry.attemptId === attempt?.id,
  )?.progress;

  useEffect(() => {
    if (progress?.kind === "verified") onDone();
  }, [progress?.kind, onDone]);

  const candidate =
    progress?.kind === "awaiting_confirmation" ? progress.context.candidate : null;
  const accounts = state.snapshot?.accounts ?? [];
  // A candidate is not in the account list yet, so it falls outside the set the
  // alias index is built from and takes that helper's label for one it has not
  // numbered, exactly as the details screen does for an account it cannot place.
  const alias = accountLabel(state.preferences, accounts, "");
  const busy =
    starting ||
    (attempt !== null &&
      (progress === undefined ||
        progress.kind === "started" ||
        progress.kind === "awaiting_user"));
  const step = provider === null ? 1 : candidate !== null ? 3 : 2;

  const connect = async (): Promise<void> => {
    if (provider === null || busy) return;
    setStarting(true);
    setRefusal(null);
    setAttempt(null);
    try {
      const accepted = await actions.beginConnection({
        provider_id: provider,
        nickname: nickname.trim(),
        profile_label: null,
      });
      if (!mounted.current) {
        if (accepted !== null) await cancelConnection(accepted);
        return;
      }
      setAttempt(accepted);
      if (accepted === null)
        setRefusal(
          "The connection was refused. Check the provider's local sign-in and try again.",
        );
    } finally {
      if (mounted.current) setStarting(false);
    }
  };

  const add = async (): Promise<void> => {
    if (attempt === null || adding) return;
    setAdding(true);
    try {
      if ((await actions.confirmConnection(attempt)) && mounted.current) onDone();
    } finally {
      if (mounted.current) setAdding(false);
    }
  };

  return (
    <section className="wizard" aria-label="Connect an account">
      <div className="detail__back">
        <button
          type="button"
          className="back-button"
          disabled={starting}
          onClick={onDone}
        >
          <Icon name="arrow-left" size={13} />
          Cancel
        </button>
      </div>
      <div className="step-line" aria-label="Connection steps">
        {["Provider", "Connect", "Verify"].map((label, index) => (
          <span
            key={label}
            className="wizard-step"
            aria-current={step === index + 1 ? "step" : undefined}
          >
            <b>{index + 1}</b>
            {label}
          </span>
        ))}
      </div>
      {provider === null ? (
        <>
          <h2>Add a subscription</h2>
          <p className="settings__intro">
            Choose a provider. Only real quota windows will appear in your overview.
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
              <span>
                <strong>{providerLabel(id)}</strong>
                <small>Connect an existing local sign-in</small>
              </span>
              <Icon name="external" size={14} />
            </button>
          ))}
          <p className="note">
            Quota reads the provider's existing local credentials. Sign in with its own
            tool first; credentials never enter this window.
          </p>
        </>
      ) : candidate !== null ? (
        <>
          <div className="success-icon">
            <Icon name="check" size={24} />
          </div>
          <h2>Is this the right account?</h2>
          <p className="settings__intro">
            Confirm the identity before adding this subscription to your overview.
          </p>
          <dl className="detail__list">
            <div>
              <dt>Provider</dt>
              <dd>{providerLabel(candidate.provider_id)}</dd>
            </div>
            <div>
              <dt>Account</dt>
              <dd>{alias || candidate.identity.principal_label}</dd>
            </div>
            <div>
              <dt>Workspace</dt>
              <dd>
                {alias
                  ? "Workspace hidden"
                  : (candidate.identity.workspace_label ?? "Not reported")}
              </dd>
            </div>
            <div>
              <dt>Quota reading</dt>
              <dd>
                {candidate.windows.length === 0
                  ? "Not reported"
                  : candidate.windows.map((window) => (
                      <p key={window.id}>
                        {window.scope.label || "Allowance"}:{" "}
                        {formatRemaining(window.measurement)}{" "}
                        {hasReading(window.measurement)
                          ? "remaining (just verified)"
                          : "(not reported)"}
                      </p>
                    ))}
              </dd>
            </div>
          </dl>
          <p className="note">
            Nothing is saved yet and no monitoring has started. Add account saves this
            subscription; Cancel discards it and leaves no account behind.
          </p>
          <div className="wizard-action">
            <button
              type="button"
              className="button button--primary"
              disabled={adding}
              onClick={() => {
                launch(add());
              }}
            >
              <Icon name={adding ? "clock" : "plus"} size={14} />
              {adding ? "Adding…" : "Add account"}
            </button>
          </div>
        </>
      ) : progress?.kind !== "verified" ? (
        <>
          <h2>Connect {providerLabel(provider)}</h2>
          <p className="settings__intro">
            Use this provider's local sign-in to verify a quota-only connection.
          </p>
          <div className="connection-box">
            <div className="identity">
              <ProviderMark providerId={provider} />
              <strong>{providerLabel(provider)}</strong>
            </div>
            <h3>Existing local credentials</h3>
            <p>
              No inference is submitted. A connection succeeds only after the host
              verifies the provider identity and a quota reading.
            </p>
          </div>
          <label className="control-label" htmlFor="account-nickname">
            Account nickname
          </label>
          <input
            id="account-nickname"
            type="text"
            maxLength={64}
            value={nickname}
            disabled={busy}
            onChange={(event) => {
              setNickname(event.currentTarget.value);
            }}
          />
          {progress?.kind === "awaiting_user" ? (
            <p className="note" role="status">
              Finish signing in with the provider's own tool.
            </p>
          ) : null}
          {progress?.kind === "failed" ? (
            <p className="note" role="alert">
              {progress.context.error.kind === "reconnect_required"
                ? AUTHENTICATION_RECOVERY[provider]
                : describeCommandError(progress.context.error)}
            </p>
          ) : null}
          {progress?.kind === "cancelled" ? (
            <p className="note" role="status">
              Connection cancelled. You can try again.
            </p>
          ) : null}
          {refusal === null ? null : (
            <p className="note" role="alert">
              {refusal}
            </p>
          )}
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
              className="button button--primary"
              disabled={busy || nickname.trim().length === 0}
              onClick={() => {
                launch(connect());
              }}
            >
              <Icon name={busy ? "clock" : "link"} size={14} />
              {busy ? "Verifying…" : "Connect"}
            </button>
          </div>
        </>
      ) : null}
    </section>
  );
}
