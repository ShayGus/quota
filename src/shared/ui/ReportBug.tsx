/**
 * Report a bug: a button that opens a small menu with the two ways to report.
 *
 * Opening an issue hands GitHub's form, prefilled by the host, to the system
 * browser. Copying the prompt puts a host-built request for an AI agent on the
 * clipboard. The host gathers what either one says about this installation,
 * so nothing the window shows, such as an account name, can reach a report.
 *
 * The full window's header drops the menu over the content. The mini widget
 * places it beneath its accounts, so the widget grows to fit it rather than
 * cutting it off.
 */
import { useEffect, useId, useRef, useState, type JSX } from "react";

import { launch } from "../ipc/report";
import { Icon } from "./Icon";

/** The confirmation once the prompt is on the clipboard. */
export const PROMPT_COPIED =
  "Prompt copied. Paste it into your AI agent to report the bug.";

/** What the person picks in the menu. */
export interface ReportBugActions {
  /** Opens the prefilled issue form in the browser. */
  readonly openIssue: () => Promise<void>;
  /** Copies the agent prompt; true once it is on the clipboard. */
  readonly copyPrompt: () => Promise<boolean>;
  /** Confirms a copied prompt where the surface shows its notices. */
  readonly onCopied: () => void;
}

/** The menu's trigger and, while open, the menu. */
export function ReportBugMenu({
  actions,
  variant,
}: {
  readonly actions: ReportBugActions;
  /** `header` for the full window, `widget` for the mini widget. */
  readonly variant: "header" | "widget";
}): JSX.Element {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement | null>(null);
  const menuId = useId();
  useDismiss(open, root, setOpen);
  const choose = (action: () => Promise<void>): void => {
    setOpen(false);
    launch(action());
  };
  return (
    <div className={`report-bug report-bug-${variant}`} ref={root}>
      <button
        type="button"
        className={variant === "header" ? "icon-btn" : "widget-report"}
        aria-label="Report a bug"
        title="Report a bug"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={() => {
          setOpen((current) => !current);
        }}
      >
        <Icon name="bug" />
      </button>
      {open ? (
        <div className="report-menu" role="menu" id={menuId} aria-label="Report a bug">
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              choose(actions.openIssue);
            }}
          >
            <Icon name="external" />
            <span>Open an issue on GitHub</span>
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              choose(async () => {
                if (await actions.copyPrompt()) {
                  actions.onCopied();
                }
              });
            }}
          >
            <Icon name="terminal" />
            <span>Copy a prompt for an AI agent</span>
          </button>
        </div>
      ) : null}
    </div>
  );
}

/** Closes the menu on Escape or on a press anywhere outside it. */
function useDismiss(
  open: boolean,
  root: { readonly current: HTMLElement | null },
  setOpen: (open: boolean) => void,
): void {
  useEffect(() => {
    if (!open) {
      return;
    }
    const onPointer = (event: PointerEvent): void => {
      if (!(event.target instanceof Node) || !root.current?.contains(event.target)) {
        setOpen(false);
      }
    };
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        // The window's own Escape would hide it; here it only closes the menu.
        event.preventDefault();
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", onPointer);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [open, root, setOpen]);
}
