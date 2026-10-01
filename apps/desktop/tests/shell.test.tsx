/**
 * Theme, always-on-top, and boundary behaviour.
 *
 * These exercise the whole window, with the IPC boundary replaced by a test
 * double, so the renderer's own behaviour is what is measured (spec 17.1).
 */
import { render, screen, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/** The recorded calls to the generated bindings' underlying transport. */
const invoked: { command: string; args: unknown }[] = [];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: unknown) => {
    invoked.push({ command, args });
    return Promise.resolve(null);
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

// `vi.mock` is hoisted above these imports by the test runner, so the transport
// is replaced before the module graph under test is evaluated.
import { App } from "../src/app/App";
import { FeatureBoundary } from "../src/app/ErrorBoundary";
import { acceptPreferences, acceptSnapshot } from "../src/shared/state/store";
import {
  account,
  percent,
  preferences,
  snapshot,
  window as quotaWindow,
} from "./fixtures";

/** One account with a single session window. */
function oneAccount(): ReturnType<typeof account>[] {
  return [
    account("a1", "codex", 1, [quotaWindow("w", "session", percent(72))], {
      rank: 72,
    }),
  ];
}

/** The recorded calls to one command. */
function commandsMatching(command: string): typeof invoked {
  return invoked.filter((call) => call.command === command);
}

/** Every recorded call except the one-time snapshot reconciliation at mount. */
const mutatingCommands = (): readonly string[] =>
  invoked.map((call) => call.command).filter((command) => command !== "get_snapshot");

beforeEach(() => {
  invoked.length = 0;
});

describe("the colour scheme", () => {
  it("applies the light scheme when the preference says light", async () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ theme: "light" }));
    render(<App />);

    await waitFor(() => {
      expect(document.documentElement.dataset["theme"]).toBe("light");
    });
  });

  it("applies the dark scheme when the preference says dark", async () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ theme: "dark" }));
    render(<App />);

    await waitFor(() => {
      expect(document.documentElement.dataset["theme"]).toBe("dark");
    });
  });
});

describe("the always-on-top control", () => {
  it("sends only the always-on-top preference and nothing else", async () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ always_on_top: false }));
    render(<App />);

    const pin = screen.getByRole("button", { name: /keep the window on top/i });
    await act(async () => {
      pin.click();
      // The command is issued from the handler and settles on a microtask.
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(commandsMatching("set_overview_always_on_top")).toHaveLength(1);
    });
    // Tauri names command arguments after the Rust parameter, so the request
    // struct travels under the `request` key.
    expect(commandsMatching("set_overview_always_on_top")[0]?.args).toEqual({
      always_on_top: true,
    });
    // The pin sends nothing else. The only other call is the one-time snapshot
    // reconciliation every window performs at mount.
    expect(mutatingCommands()).toEqual(["set_overview_always_on_top"]);
  });

  it("shows the selected state in words as well as in colour", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences({ always_on_top: true }));
    render(<App />);

    expect(screen.getByText("Always on top")).toBeTruthy();
    const pin = screen.getByRole("button", { name: /turn off always on top/i });
    expect(pin.getAttribute("aria-pressed")).toBe("true");
  });
});

describe("a render failure", () => {
  it("contains the failure without clearing accounts, and recovers in place", () => {
    acceptSnapshot(snapshot("instance-1", 1, oneAccount()));
    acceptPreferences(preferences());
    render(<App />);
    expect(screen.getAllByText("72%").length).toBeGreaterThan(0);

    // A surface failure inside the boundary leaves the store untouched.
    function Boom(): React.ReactElement {
      throw new Error("surface failed");
    }
    const quiet = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const { container } = render(
      <FeatureBoundary surface="overview">
        <Boom />
      </FeatureBoundary>,
    );
    quiet.mockRestore();

    expect(container.textContent).toContain("The overview view stopped rendering");
    // Accounts are still present in the store, and no polling was restarted.
    expect(screen.getAllByText("72%").length).toBeGreaterThan(0);
    expect(commandsMatching("refresh_accounts")).toHaveLength(0);
  });
});
