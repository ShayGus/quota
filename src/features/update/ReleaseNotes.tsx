/**
 * What's new: the panel the first run of a new version opens, with the release
 * notes built into that version.
 *
 * It is drawn like the update pop-up, from the settings window's header and the
 * dialog's button. It only shows words and closes; the host decides when it opens.
 */
import { useEffect, useRef, useState, type JSX } from "react";

import type { ReleaseNotes } from "../../generated/bindings";
import { launch } from "../../shared/ipc/report";
import { readReleaseNotes } from "../../shared/ipc/update";
import { Icon } from "../../shared/ui/Icon";

/** The panel's window content. */
export function ReleaseNotesWindow({
  onClose,
}: {
  readonly onClose: () => void;
}): JSX.Element {
  const [notes, setNotes] = useState<ReleaseNotes | null>(null);
  const close = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    let current = true;
    launch(
      readReleaseNotes().then((answer) => {
        if (current) setNotes(answer);
      }),
    );
    return () => {
      current = false;
    };
  }, []);

  useEffect(() => {
    close.current?.focus({ preventScroll: true });
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  return (
    <div
      className="window update-window release-notes"
      role="dialog"
      aria-label="What's new"
    >
      <header className="settings-head" data-tauri-drag-region>
        <div data-tauri-drag-region>
          <h2 data-tauri-drag-region>What&apos;s new</h2>
          <p data-tauri-drag-region>
            {notes === null ? "Quota" : `Quota ${notes.version}`}
          </p>
        </div>
        <button
          type="button"
          className="icon-btn"
          aria-label="Close what's new"
          title="Close what's new"
          onClick={onClose}
        >
          <Icon name="close" />
        </button>
      </header>
      <main className="release-notes-body">
        <ul>
          {(notes?.items ?? []).map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      </main>
      <footer className="dialog-actions update-actions">
        <button ref={close} type="button" className="button primary" onClick={onClose}>
          Close
        </button>
      </footer>
    </div>
  );
}
