/**
 * A modal confirmation, drawn as the wireframe's dialog.
 *
 * The platform `<dialog>` traps focus and closes on Escape. Where it is not
 * available, as under a test DOM, the dialog is shown with the `open`
 * attribute instead, which keeps it in the accessibility tree all the same.
 */
import { useEffect, useId, useRef, type JSX, type ReactNode } from "react";

/** A dialog with a title, a body, and a cancel and optional confirm action. */
export function Dialog({
  title,
  children,
  confirmLabel,
  confirmDisabled = false,
  onConfirm,
  onClose,
}: {
  readonly title: string;
  readonly children: ReactNode;
  /** The confirm button's text, or `null` for a dialog that only informs. */
  readonly confirmLabel: string | null;
  readonly confirmDisabled?: boolean;
  readonly onConfirm?: () => void;
  readonly onClose: () => void;
}): JSX.Element {
  const ref = useRef<HTMLDialogElement | null>(null);
  const titleId = useId();
  useEffect(() => {
    const dialog = ref.current;
    if (dialog === null || dialog.open) {
      return;
    }
    const trigger = document.activeElement;
    if (typeof dialog.showModal === "function") {
      dialog.showModal();
    } else {
      dialog.setAttribute("open", "");
    }
    return () => {
      if (trigger instanceof HTMLElement && trigger.isConnected) {
        trigger.focus({ preventScroll: true });
      }
    };
  }, []);
  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <h2 id={titleId}>{title}</h2>
      <div>{children}</div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose}>
          {confirmLabel === null ? "Close" : "Cancel"}
        </button>
        {confirmLabel === null ? null : (
          <button
            type="button"
            className="button primary"
            disabled={confirmDisabled}
            onClick={() => {
              onConfirm?.();
              onClose();
            }}
          >
            {confirmLabel}
          </button>
        )}
      </div>
    </dialog>
  );
}
