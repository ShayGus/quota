/**
 * Test setup.
 *
 * The renderer store is module state, so it is reset around every test: one
 * test's snapshot never becomes another test's starting state. No matcher
 * library is installed; assertions use Vitest's own matchers on text and
 * attributes, which keeps this project free of a second assertion vocabulary.
 */
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";

import { resetRendererState } from "../src/shared/state/store";
import { NOW } from "./fixtures";

// React 19 reads this flag to decide whether `act` may batch updates. Vitest
// runs with `globals: false`, which is what Testing Library's automatic setup
// keys off, so the flag is set explicitly here.
declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

// jsdom does not implement `matchMedia`. The renderer only ever asks it for the
// operating system colour scheme, so the double answers that one question and
// reports "light".
if (typeof window.matchMedia !== "function") {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: (query: string): MediaQueryList => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    }),
  });
}

beforeEach(() => {
  resetRendererState();
  // Fixture boundaries and readings are anchored to NOW. Only the clock is
  // faked, so timers and promises keep running normally.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  // Testing Library registers its own cleanup only when Vitest globals are on.
  // This project keeps globals off, so cleanup is registered here.
  cleanup();
  resetRendererState();
  vi.useRealTimers();
});
