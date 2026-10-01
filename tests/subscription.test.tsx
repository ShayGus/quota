/**
 * The subscription lifecycle under Strict Mode.
 *
 * React 19 Strict Mode mounts, unmounts, and mounts again in development. The
 * renderer must end with exactly one live event-listener set, and a registration
 * that settles after cleanup must be unsubscribed immediately (spec 7.8.3,
 * AC-85).
 *
 * One subscription registers one listener per backend event, so the counts here
 * are per listener: six events means six live listeners for one subscription.
 */
import { render, waitFor } from "@testing-library/react";
import { StrictMode } from "react";
import { describe, expect, it, vi } from "vitest";

/** The wire events the renderer subscribes to. */
const EVENTS = 6;

/** Live listeners. */
let active = 0;
/** Every listener registration attempted. */
let registrations = 0;
/** The delay, in milliseconds, each registration waits before settling. */
const settleDelays: number[] = [];

// `vi.mock` is hoisted above the imports below, so the transport is replaced
// before the module graph under test is evaluated.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: () => Promise.resolve(null),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => {
    registrations += 1;
    const delay = settleDelays.shift() ?? 0;
    return new Promise<() => void>((resolve) => {
      setTimeout(() => {
        active += 1;
        resolve(() => {
          active -= 1;
        });
      }, delay);
    });
  },
}));

import { App } from "../src/app/App";
import { subscriberCount } from "../src/shared/state/store";

describe("Strict Mode", () => {
  it("leaves exactly one live listener set after the mount/unmount/mount cycle", async () => {
    settleDelays.length = 0;
    active = 0;
    registrations = 0;
    const { unmount } = render(
      <StrictMode>
        <App />
      </StrictMode>,
    );

    await waitFor(() => {
      expect(active).toBe(EVENTS);
    });
    // Strict Mode ran the Effect twice, so two listener sets were requested.
    expect(registrations).toBe(EVENTS * 2);
    // Exactly one set is live: the first was unsubscribed when its cleanup ran.
    expect(active).toBe(EVENTS);

    unmount();
    await waitFor(() => {
      expect(active).toBe(0);
    });
    expect(subscriberCount()).toBe(0);
  });

  it("unsubscribes a listener set that settles after cleanup", async () => {
    settleDelays.length = 0;
    active = 0;
    registrations = 0;
    // The second registration set settles only after the component has unmounted.
    for (let index = 0; index < EVENTS; index += 1) {
      settleDelays.push(0, 40);
    }
    const { unmount } = render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
    unmount();

    await waitFor(() => {
      expect(active).toBe(0);
    });
    expect(subscriberCount()).toBe(0);
  });
});
