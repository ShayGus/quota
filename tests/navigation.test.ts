/**
 * The surface a full window opens at when the host creates it for one, as
 * settings' Details asks for while the widget is the view.
 */
import { describe, expect, it } from "vitest";

import { initialTarget } from "../src/shared/ipc/navigation";

describe("the surface a new full window opens at", () => {
  it("is the overview without a detail address", () => {
    expect(initialTarget("")).toEqual({ view: "overview" });
    expect(initialTarget("#/")).toEqual({ view: "overview" });
    expect(initialTarget("#/detail/")).toEqual({ view: "overview" });
  });

  it("is an account's detail, at one window when named", () => {
    expect(initialTarget("#/detail/a%2F1")).toEqual({
      view: "detail",
      accountId: "a/1",
      windowId: null,
    });
    expect(initialTarget("#/detail/a1/w%201")).toEqual({
      view: "detail",
      accountId: "a1",
      windowId: "w 1",
    });
  });

  it("is the overview when an id does not decode", () => {
    expect(initialTarget("#/detail/%E0%A4%A")).toEqual({ view: "overview" });
  });
});
