/**
 * Test setup.
 *
 * Adds the DOM matchers and clears the renderer store between tests, so one
 * test's snapshot never becomes another test's starting state.
 */
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach } from "vitest";

import { resetRendererState } from "../src/shared/state/store";

beforeEach(() => {
  resetRendererState();
});

afterEach(() => {
  resetRendererState();
});
