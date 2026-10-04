/**
 * The update pop-up.
 *
 * It is drawn from the same pieces as the settings window and its dialogs: the
 * settings header with its close button, the dialog's text and its two buttons,
 * the shared tokens and both themes. The host decides what it shows: an offer
 * naming both versions, the install in progress with its buttons disabled, or the
 * message that the install failed, with one button to close it.
 */
import { useEffect, useRef, type JSX } from "react";

import type { UpdatePrompt as Prompt, UpdateResponse } from "../../generated/bindings";
import { Icon } from "../../shared/ui/Icon";

/** The words for each state of the pop-up. */
function wording(prompt: Prompt): {
  readonly title: string;
  readonly subtitle: string;
  readonly text: string;
} {
  switch (prompt.kind) {
    case "offer":
      return {
        title: "Update available",
        subtitle: `Quota ${prompt.context.version}`,
        text: `Quota ${prompt.context.version} is available (you have ${prompt.context.current}). Install it now? Quota will restart.`,
      };
    case "installing":
      return {
        title: "Installing the update",
        subtitle: `Quota ${prompt.context.version}`,
        text: `Installing Quota ${prompt.context.version}…  Quota will restart when it is done.`,
      };
    case "failed":
      return {
        title: "Update failed",
        subtitle: "Nothing was changed",
        text: "The update could not be installed. Quota will keep running the current version.",
      };
  }
}

/** The pop-up's window content, for the prompt the host is showing. */
export function UpdateWindow({
  prompt,
  onRespond,
}: {
  readonly prompt: Prompt | null;
  readonly onRespond: (response: UpdateResponse) => void;
}): JSX.Element {
  const primary = useRef<HTMLButtonElement | null>(null);
  const kind = prompt?.kind ?? null;
  const busy = kind === "installing";
  const closing: UpdateResponse = kind === "failed" ? "dismiss" : "decline";

  useEffect(() => {
    // OK, or Close on the failure, takes the focus, so Enter answers it.
    primary.current?.focus({ preventScroll: true });
  }, [kind]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === "Escape" && !event.defaultPrevented && !busy && kind !== null) {
        event.preventDefault();
        onRespond(closing);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [busy, closing, kind, onRespond]);

  const words = prompt === null ? null : wording(prompt);
  return (
    <div className="window update-window" role="alertdialog" aria-busy={busy}>
      <header className="settings-head" data-tauri-drag-region>
        <div data-tauri-drag-region>
          <h2 data-tauri-drag-region>{words?.title ?? "Quota update"}</h2>
          <p data-tauri-drag-region>{words?.subtitle ?? "Checking what is available"}</p>
        </div>
        <button
          type="button"
          className="icon-btn"
          aria-label="Close update window"
          title="Close update window"
          disabled={busy || kind === null}
          onClick={() => {
            onRespond(closing);
          }}
        >
          <Icon name="close" />
        </button>
      </header>
      <main className="update-body">
        <p>{words?.text ?? ""}</p>
      </main>
      <footer className="dialog-actions update-actions">
        {kind === "failed" ? (
          <button
            ref={primary}
            type="button"
            className="button primary"
            onClick={() => {
              onRespond("dismiss");
            }}
          >
            Close
          </button>
        ) : (
          <>
            <button
              type="button"
              className="button"
              disabled={busy || kind === null}
              onClick={() => {
                onRespond("decline");
              }}
            >
              Cancel
            </button>
            <button
              ref={primary}
              type="button"
              className="button primary"
              disabled={busy || kind === null}
              onClick={() => {
                onRespond("install");
              }}
            >
              OK
            </button>
          </>
        )}
      </footer>
    </div>
  );
}
