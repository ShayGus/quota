import { useState, type JSX } from "react";

import type { AttemptRef, ProviderId } from "../../generated/bindings";
import { providerLabel } from "../../shared/format/provider";
import { describeCommandError, launch } from "../../shared/ipc/report";
import type { RendererState } from "../../shared/state/types";
import { Icon } from "../../shared/ui/Icon";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import type { SettingsActions } from "./Settings";

const PROVIDERS = ["codex", "claude", "open_code_go"] as const;

export function ConnectionWizard({
  state,
  actions,
  onDone,
}: {
  readonly state: RendererState;
  readonly actions: SettingsActions;
  readonly onDone: () => void;
}): JSX.Element {
  const [provider, setProvider] = useState<ProviderId | null>(null);
  const [nickname, setNickname] = useState("Personal");
  const [attempt, setAttempt] = useState<AttemptRef | null>(null);
  const [starting, setStarting] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const progress = state.attempts.find(
    (entry) => entry.attemptId === attempt?.id,
  )?.progress;
  const verified = progress?.kind === "verified";
  const busy =
    starting ||
    (attempt !== null &&
      (progress === undefined ||
        progress.kind === "started" ||
        progress.kind === "awaiting_user"));
  const step = provider === null ? 1 : verified ? 3 : 2;

  const connect = async (): Promise<void> => {
    if (provider === null || busy) return;
    setStarting(true);
    setConfirmed(false);
    setRefusal(null);
    setAttempt(null);
    try {
      const accepted = await actions.beginConnection({
        provider_id: provider,
        nickname: nickname.trim(),
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

  const cancel = async (): Promise<void> => {
    if (attempt !== null && busy) await actions.cancelConnection(attempt);
    onDone();
  };

  return (
    <section className="wizard" aria-label="Connect an account">
      <div className="detail__back">
        <button
          type="button"
          className="back-button"
          disabled={starting}
          onClick={() => launch(cancel())}
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
              onClick={() => setProvider(id)}
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
      ) : verified ? (
        <>
          <div className="success-icon">
            <Icon
              name={progress.context.state === "connected" ? "check" : "warning"}
              size={24}
            />
          </div>
          <h2>Verify your connection</h2>
          <p className="settings__intro">Review the result reported by the provider.</p>
          <dl className="detail__list">
            <div>
              <dt>Provider</dt>
              <dd>{providerLabel(provider)}</dd>
            </div>
            <div>
              <dt>Requested nickname</dt>
              <dd>{nickname.trim()}</dd>
            </div>
            <div>
              <dt>Connection</dt>
              <dd>{progress.context.state}</dd>
            </div>
          </dl>
          <p className="note">
            The host saves verified accounts before this step. Its attempt event reports
            connection state but does not identify the account or workspace; review those
            in Accounts.
          </p>
          <label className="checkline">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(event) => setConfirmed(event.currentTarget.checked)}
            />
            I will review the connected account in Accounts.
          </label>
          <div className="wizard-action">
            <button
              type="button"
              className="button button--primary"
              disabled={!confirmed}
              onClick={onDone}
            >
              Manage accounts
            </button>
          </div>
        </>
      ) : (
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
            onChange={(event) => setNickname(event.currentTarget.value)}
          />
          {progress?.kind === "awaiting_user" ? (
            <p className="note" role="status">
              Finish signing in with the provider's own tool.
            </p>
          ) : null}
          {progress?.kind === "failed" ? (
            <p className="note" role="alert">
              {describeCommandError(progress.context.error)}
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
              onClick={() => launch(connect())}
            >
              <Icon name={busy ? "clock" : "link"} size={14} />
              {busy ? "Verifying…" : "Connect"}
            </button>
          </div>
        </>
      )}
    </section>
  );
}
