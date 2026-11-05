/**
 * The setting for the key combination that finds the mini widget.
 *
 * It defaults to Ctrl+Alt+Q, and can be changed or turned off, because another
 * app may hold it already. The host registers a combination before saving it,
 * so a combination that is taken says so here and the old one goes on working.
 * A desktop that gives apps no global shortcuts, as Wayland doesn't, isn't
 * offered the setting at all.
 */
import { useEffect, useState, type JSX, type KeyboardEvent } from "react";

import type { FindShortcutStatus, Preferences } from "../../../generated/bindings";
import { launch } from "../../../shared/ipc/report";
import { SettingRow } from "../Primitives";
import type { SettingsActions } from "../Settings";

/** The combination a new install starts with, and the one Turn on restores. */
const DEFAULT_SHORTCUT = "Ctrl+Alt+Q";

/** The keys that only qualify another key, which a combination can't end on. */
const MODIFIER_KEYS: ReadonlySet<string> = new Set([
  "Control",
  "Alt",
  "AltGraph",
  "Shift",
  "Meta",
  "OS",
]);

/**
 * A key press as the host reads a combination, such as `Ctrl+Alt+Q`, or `null`
 * while only modifiers are down or none that makes it global is held. The
 * physical key is used, so the combination doesn't depend on the keyboard's
 * language.
 */
export function combinationOf(
  event: Pick<
    KeyboardEvent,
    "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
  >,
): string | null {
  if (MODIFIER_KEYS.has(event.key) || !(event.ctrlKey || event.altKey || event.metaKey)) {
    return null;
  }
  const parts = [
    event.ctrlKey ? "Ctrl" : null,
    event.altKey ? "Alt" : null,
    event.shiftKey ? "Shift" : null,
    event.metaKey ? "Super" : null,
    event.code.replace(/^(Key|Digit)/, ""),
  ];
  return parts.filter((part) => part !== null).join("+");
}

/** The setting's row, and what it has to say under it. */
export function FindShortcut({
  preferences,
  actions,
}: {
  readonly preferences: Preferences;
  readonly actions: Pick<SettingsActions, "findShortcutStatus" | "setFindShortcut">;
}): JSX.Element | null {
  const [status, setStatus] = useState<FindShortcutStatus | null>(null);
  const [recording, setRecording] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const shortcut = preferences.find_widget_shortcut;
  useEffect(() => {
    let current = true;
    launch(
      actions.findShortcutStatus().then((answer) => {
        if (current) setStatus(answer);
      }),
    );
    return () => {
      current = false;
    };
  }, [actions, shortcut]);
  if (status === null || !status.supported) {
    return null;
  }
  const choose = (next: string | null): void => {
    setRecording(false);
    setFailure(null);
    launch(actions.setFindShortcut(next).then(setFailure));
  };
  const taken =
    failure === null && shortcut !== null && !status.active
      ? `${shortcut} is taken by another app. Pick another.`
      : null;
  const message = failure ?? taken;
  return (
    <>
      <SettingRow
        label="Find the widget"
        description="Makes the mini widget flash, from anywhere. Clicking the tray icon does the same."
        control={
          <div className="find-shortcut">
            <button
              type="button"
              className={`button${recording ? " recording" : ""}`}
              aria-label={
                recording
                  ? "Press the new shortcut, or Esc to cancel"
                  : `Change the shortcut${shortcut === null ? "" : `, now ${shortcut}`}`
              }
              onClick={() => {
                setFailure(null);
                setRecording(true);
              }}
              onBlur={() => {
                setRecording(false);
              }}
              onKeyDown={(event) => {
                if (!recording) {
                  return;
                }
                event.preventDefault();
                event.stopPropagation();
                if (event.key === "Escape") {
                  setRecording(false);
                  return;
                }
                const combination = combinationOf(event);
                if (combination !== null) {
                  choose(combination);
                }
              }}
            >
              {recording ? "Press keys…" : (shortcut ?? "Off")}
            </button>
            <button
              type="button"
              className="button"
              onClick={() => {
                choose(shortcut === null ? DEFAULT_SHORTCUT : null);
              }}
            >
              {shortcut === null ? "Turn on" : "Turn off"}
            </button>
          </div>
        }
      />
      {message === null ? null : (
        <p className="find-shortcut-message" role="alert">
          {message}
        </p>
      )}
    </>
  );
}
