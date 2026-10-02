/**
 * The popover's content height, which the host fits the window to.
 */
import { describe, expect, it } from "vitest";

import { contentHeight } from "../src/app/useFitContentHeight";

describe("the popover content height", () => {
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

  it("reports nothing while the surface is missing", () => {
    expect(contentHeight(document.createElement("div"))).toBeNull();
  });
});
