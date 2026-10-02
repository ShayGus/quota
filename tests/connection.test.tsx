import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

import { actions as hostActions } from "../src/app/actions";
import { Settings, type SettingsActions } from "../src/features/settings/Settings";
import {
  acceptAttempt,
  acceptPreferences,
  acceptSnapshot,
  getRendererState,
} from "../src/shared/state/store";
import { useRendererState } from "../src/shared/state/useRendererState";
import {
  account,
  percent,
  preferences,
  snapshot,
  window as quotaWindow,
} from "./fixtures";

function settingsActions(): SettingsActions {
  return {
    savePreferences: vi.fn(),
    setMonitoring: vi.fn(),
    savePollingPreferences: vi.fn(),
    setAlwaysOnTop: vi.fn(),
    setOverviewMode: vi.fn(),
    fitToAccounts: vi.fn(),
    resetPosition: vi.fn(),
    setAccountEnabled: vi.fn(),
    renameAccount: vi.fn(),
    disconnectAccount: vi.fn(),
    openUsagePage: vi.fn(),
    beginConnection: vi.fn(async () => ({ id: "attempt-1" })),
    cancelConnection: vi.fn(async () => undefined),
    reconnectAccount: vi.fn(async () => undefined),
    clearHistory: vi.fn(),
    exportDiagnostics: vi.fn(),
  };
}

function Harness({ actions }: { readonly actions: SettingsActions }): React.ReactElement {
  const state = useRendererState();
  return <Settings state={state} actions={actions} />;
}

beforeEach(() => {
  window.location.hash = "#/settings/connect";
  acceptPreferences(preferences());
  acceptSnapshot(snapshot("instance-1", 1, [account("a", "claude", 1, [])]));
});

describe("Provider → Connect → Verify", () => {
  it("waits for its own verification event and lets the user review accounts", async () => {
    const actions = settingsActions();
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    expect(screen.getByRole("heading", { name: "Connect Claude" })).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Account nickname"), {
      target: { value: "Work" },
    });
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Connect" })),
    );
    expect(actions.beginConnection).toHaveBeenCalledWith({
      provider_id: "claude",
      nickname: "Work",
      profile_label: null,
    });
    expect(screen.queryByRole("heading", { name: "Verify your connection" })).toBeNull();
    act(() =>
      acceptAttempt({
        attemptId: "another-attempt",
        revision: 2,
        progress: { kind: "verified", context: { state: "connected" } },
      }),
    );
    expect(screen.queryByRole("heading", { name: "Verify your connection" })).toBeNull();
    act(() =>
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: { kind: "verified", context: { state: "connected" } },
      }),
    );
    expect(screen.getByRole("heading", { name: "Verify your connection" })).toBeTruthy();
    expect(screen.getByText("Work")).toBeTruthy();
    const manage = screen.getByRole("button", {
      name: "Manage accounts",
    }) as HTMLButtonElement;
    expect(manage.disabled).toBe(true);
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(manage);
    await waitFor(() =>
      expect(screen.getByRole("article", { name: "Manage a" })).toBeTruthy(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    await waitFor(() =>
      expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeTruthy(),
    );
  });

  it("starts a fresh wizard when Add reopens settings on the same connection section", async () => {
    window.location.hash = "#/settings/connect/request-1";
    const actions = settingsActions();
    const { container } = render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    fireEvent.change(screen.getByLabelText("Account nickname"), {
      target: { value: "Work" },
    });
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Connect" })),
    );
    act(() =>
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: { kind: "verified", context: { state: "connected" } },
      }),
    );
    expect(screen.getByRole("heading", { name: "Verify your connection" })).toBeTruthy();
    container.hidden = true;
    act(() => {
      window.location.hash = "#/settings/connect/request-2";
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    container.hidden = false;
    expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Verify your connection" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /^Codex/ }));
    expect((screen.getByLabelText("Account nickname") as HTMLInputElement).value).toBe(
      "Personal",
    );
    vi.mocked(actions.beginConnection).mockResolvedValueOnce({ id: "attempt-2" });
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Connect" })),
    );
    act(() =>
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 3,
        progress: { kind: "verified", context: { state: "connected" } },
      }),
    );
    expect(screen.queryByRole("heading", { name: "Verify your connection" })).toBeNull();
    expect(screen.getByRole("heading", { name: "Connect Codex" })).toBeTruthy();
  });

  it("shows snapshot identities and readings without attributing saved accounts to the attempt", async () => {
    const actions = settingsActions();
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Codex/ }));
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Connect" })),
    );
    act(() =>
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: { kind: "verified", context: { state: "connected" } },
      }),
    );
    expect(
      screen.getByText(/Account, workspace, and quota reading have not arrived/),
    ).toBeTruthy();
    act(() =>
      acceptSnapshot(
        snapshot("instance-1", 2, [
          account("a", "claude", 1, []),
          account("b", "codex", 2, [quotaWindow("quota", "session", percent(72))]),
        ]),
      ),
    );
    expect(screen.getByRole("article", { name: "Saved b" }).textContent).toContain(
      "b@example.test",
    );
    expect(screen.getByRole("article", { name: "Saved b" }).textContent).toContain(
      "Home",
    );
    expect(screen.getByRole("article", { name: "Saved b" }).textContent).toContain(
      "72% remaining (last reported)",
    );
    expect(screen.queryByText("a@example.test")).toBeNull();
    expect(screen.getByText(/does not identify which account it saved/)).toBeTruthy();
    act(() =>
      acceptPreferences(
        preferences({
          privacy: {
            alias_mode: "stable_aliases",
            retain_history: false,
            export_identities: false,
          },
        }),
      ),
    );
    expect(
      screen.getByRole("article", { name: "Saved Account 2" }).textContent,
    ).toContain("Workspace hidden");
    expect(screen.queryByText("b@example.test")).toBeNull();
    expect(screen.queryByText("Home")).toBeNull();
  });

  it("shows connection failures and cancels a running attempt by its identity", async () => {
    const actions = settingsActions();
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Codex/ }));
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Connect" })),
    );
    act(() =>
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: {
          kind: "failed",
          context: { error: { kind: "secure_store_unavailable" } },
        },
      }),
    );
    expect(screen.getByRole("alert").textContent).toContain("key store is unavailable");
    vi.mocked(actions.beginConnection).mockResolvedValueOnce({ id: "attempt-2" });
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Connect" })),
    );
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Cancel" })),
    );
    expect(actions.cancelConnection).toHaveBeenCalledWith({ id: "attempt-2" });
  });

  it("preserves verification received before the begin command returns", async () => {
    invoke.mockImplementation(async () => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: { kind: "verified", context: { state: "connected" } },
      });
      return { attempt_ref: { id: "attempt-1" }, attempt_id: "attempt-1" };
    });
    const actions = {
      ...settingsActions(),
      beginConnection: hostActions.beginConnection,
    };
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Connect" })),
    );
    expect(screen.getByRole("heading", { name: "Verify your connection" })).toBeTruthy();
    act(() =>
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 1,
        progress: { kind: "started" },
      }),
    );
    expect(screen.getByRole("heading", { name: "Verify your connection" })).toBeTruthy();
    expect(getRendererState().attempts[0]?.revision).toBe(2);
  });

  it("keeps a refused connection on Connect with a useful message", async () => {
    const actions = { ...settingsActions(), beginConnection: vi.fn(async () => null) };
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^OpenCode Go/ }));
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "Connect" })),
    );
    expect(screen.getByRole("alert").textContent).toContain("connection was refused");
    expect(screen.queryByRole("heading", { name: "Verify your connection" })).toBeNull();
  });
});
