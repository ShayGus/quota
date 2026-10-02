/**
 * The popover's content-fitted height, as the wireframe sizes it.
 */
import { describe, expect, it } from "vitest";

import {
  contentHeight,
  fittedHeight,
  POPOVER_MAX_HEIGHT,
  POPOVER_MIN_HEIGHT,
} from "../src/app/useFitContentHeight";

describe("the popover height", () => {
  it("follows the content between the window minimum and the wireframe ceiling", () => {
    expect(fittedHeight(379, 1100)).toBe(379);
    expect(fittedHeight(120, 1100)).toBe(POPOVER_MIN_HEIGHT);
    expect(fittedHeight(1400, 1100)).toBe(POPOVER_MAX_HEIGHT);
  });

  it("never exceeds the space the screen has", () => {
    expect(fittedHeight(700, 500)).toBe(500);
    expect(fittedHeight(700, 100)).toBe(POPOVER_MIN_HEIGHT);
  });

  it("adds the header, the surface's own height, and the footer", () => {
    const root = document.createElement("div");
    root.innerHTML =
      '<header class="app-header"></header><main class="app-main"><div></div></main><footer class="app-footer"></footer>';
    const [header, main, footer] = root.children;
    Object.defineProperty(header, "offsetHeight", { value: 65 });
    Object.defineProperty(footer, "offsetHeight", { value: 42 });
    Object.defineProperty(main?.firstElementChild, "scrollHeight", { value: 271 });
    expect(contentHeight(root)).toBe(378);
  });

  it("asks for the ceiling when the surface is missing", () => {
    expect(contentHeight(document.createElement("div"))).toBe(POPOVER_MAX_HEIGHT);
  });
});
