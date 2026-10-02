/**
 * The generated bindings boundary.
 *
 * The file under `src/generated` is machine output from `tauri-specta`. Two
 * things must hold at this boundary. It is genuinely generated, so a hand edit
 * is visible. And a typed domain failure is a different result from a transport
 * rejection, so a caller can tell "the backend refused" from "the call never
 * arrived" (spec 7.8.2, spec 8.2, spec 8.4).
 *
 * Byte-for-byte equality with the exporter is proved by the Rust test
 * `src-tauri/tests/bindings.rs`, which regenerates the file and compares.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: () => Promise.reject(new Error("transport is down")),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

import { commands, events } from "../src/generated/bindings";
import { reportAsync } from "../src/shared/ipc/report";
import { getRendererState, resetRendererState } from "../src/shared/state/store";

beforeEach(() => {
  resetRendererState();
});

describe("the generated bindings module", () => {
  // These assert the exported surface the renderer actually calls. The file's
  // own text is not inspected: a name can survive in a comment while the
  // listener is registered under a different one. `cargo xtask bindings
  // --check` owns the comparison between the generated file and the Rust
  // exporter, so the file's fidelity is verified where it is produced.
  it("exposes every command the renderer calls", () => {
    for (const command of [
      "getSnapshot",
      "updatePreferences",
      "setPollingPreferences",
      "beginConnection",
      "cancelConnection",
      "confirmConnection",
      "reconnectAccount",
      "setAccountEnabled",
      "renameAccount",
      "disconnectAccount",
      "clearLocalHistory",
      "setMonitoringState",
      "setOverviewMode",
      "setOverviewAlwaysOnTop",
      "fitOverviewHeight",
      "openProviderUsagePage",
      "openSettingsWindow",
      "exportSanitizedDiagnostics",
      "listProviderCapabilities",
    ]) {
      expect(typeof (commands as Record<string, unknown>)[command]).toBe("function");
    }
  });

  it("no longer exposes the retired window-fit and position-reset commands", () => {
    for (const command of ["fitOverviewToAccounts", "resetOverviewPosition"]) {
      expect((commands as Record<string, unknown>)[command]).toBeUndefined();
    }
  });

  it("exposes every event the renderer listens to", () => {
    for (const event of [
      "snapshotUpdated",
      "preferencesChanged",
      "monitoringStateChanged",
      "overviewWindowStateChanged",
      "persistenceStatusChanged",
      "connectionProgressChanged",
    ]) {
      expect(typeof (events as Record<string, { listen: unknown }>)[event]?.listen).toBe(
        "function",
      );
    }
  });
});

describe("a command result", () => {
  it("records a typed domain failure as a domain failure", async () => {
    const delivered = await reportAsync(
      Promise.resolve({
        status: "error" as const,
        error: { kind: "account_not_found" as const },
      }),
    );

    expect(delivered).toBeNull();
    expect(getRendererState().failure?.kind).toBe("domain");
  });

  it("records a rejected call as a transport failure", async () => {
    const delivered = await reportAsync(commands.getSnapshot());

    expect(delivered).toBeNull();
    expect(getRendererState().failure?.kind).toBe("transport");
  });
});
