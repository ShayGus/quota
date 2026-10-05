import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ReportBugMenu } from "../src/shared/ui/ReportBug";

function actions(copied = true): {
  openIssue: () => Promise<void>;
  copyPrompt: () => Promise<boolean>;
  onCopied: () => void;
} {
  return {
    openIssue: vi.fn(() => Promise.resolve()),
    copyPrompt: vi.fn(() => Promise.resolve(copied)),
    onCopied: vi.fn(),
  };
}

function openMenu(): void {
  fireEvent.click(screen.getByRole("button", { name: "Report a bug" }));
}

describe("Report a bug", () => {
  it("is closed until its button is pressed, then offers the two choices", () => {
    render(<ReportBugMenu actions={actions()} variant="header" />);
    const trigger = screen.getByRole("button", { name: "Report a bug" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    openMenu();
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Open an issue on GitHub",
      "Copy a prompt for an AI agent",
    ]);
  });

  it("opens the issue form and closes", () => {
    const report = actions();
    render(<ReportBugMenu actions={report} variant="header" />);
    openMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Open an issue on GitHub" }));
    expect(report.openIssue).toHaveBeenCalledOnce();
    expect(report.copyPrompt).not.toHaveBeenCalled();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("confirms a prompt that reached the clipboard", async () => {
    const report = actions(true);
    render(<ReportBugMenu actions={report} variant="header" />);
    openMenu();
    fireEvent.click(
      screen.getByRole("menuitem", { name: "Copy a prompt for an AI agent" }),
    );
    await vi.waitFor(() => {
      expect(report.onCopied).toHaveBeenCalledOnce();
    });
  });

  it("does not confirm a prompt the clipboard refused", async () => {
    const report = actions(false);
    render(<ReportBugMenu actions={report} variant="header" />);
    openMenu();
    fireEvent.click(
      screen.getByRole("menuitem", { name: "Copy a prompt for an AI agent" }),
    );
    await vi.waitFor(() => {
      expect(report.copyPrompt).toHaveBeenCalledOnce();
    });
    await Promise.resolve();
    expect(report.onCopied).not.toHaveBeenCalled();
  });

  it("closes on Escape without letting the window hide", () => {
    render(<ReportBugMenu actions={actions()} variant="header" />);
    openMenu();
    const escape = new KeyboardEvent("keydown", { key: "Escape", cancelable: true });
    act(() => {
      window.dispatchEvent(escape);
    });
    expect(escape.defaultPrevented).toBe(true);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("closes on a press outside it", () => {
    render(
      <>
        <ReportBugMenu actions={actions()} variant="header" />
        <p>elsewhere</p>
      </>,
    );
    openMenu();
    fireEvent.pointerDown(screen.getByText("elsewhere"));
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
