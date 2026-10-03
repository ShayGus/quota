/**
 * The first read while the backend is still starting.
 *
 * Every window loads before the backend has finished starting, so its first
 * snapshot read can arrive too early. The host answers that read with
 * `initialization_pending`; the renderer must keep asking until the backend is
 * ready rather than give up, or the window shows no accounts until the next
 * scheduled refresh.
 */
import { describe, expect, it, vi } from "vitest";

import type * as Bindings from "../src/generated/bindings";
import type { AppSnapshot } from "../src/generated/bindings";
import { account, percent, snapshot, window as quotaWindow } from "./fixtures";

const host = vi.hoisted(() => ({
  reads: 0,
  pendingReads: 2,
  snapshot: undefined as AppSnapshot | undefined,
}));

vi.mock("../src/generated/bindings", async (original) => {
  const bindings = await original<typeof Bindings>();
  return {
    ...bindings,
    commands: {
      ...bindings.commands,
      getSnapshot: () => {
        host.reads += 1;
        return Promise.resolve(
          host.reads <= host.pendingReads || host.snapshot === undefined
            ? { status: "error", error: { kind: "initialization_pending" } }
            : { status: "ok", data: { snapshot: host.snapshot } },
        );
      },
    },
  };
});

import { reconcileSnapshot } from "../src/shared/ipc/subscription";
import { getRendererState } from "../src/shared/state/store";

describe("the first read at startup", () => {
  it("keeps asking while the backend starts, then shows the accounts", async () => {
    host.snapshot = snapshot("instance-1", 1, [
      account("a", "codex", 1, [quotaWindow("w", "session", percent(60))], { rank: 60 }),
    ]);
    await reconcileSnapshot();
    expect(host.reads).toBe(host.pendingReads + 1);
    expect(
      getRendererState().snapshot?.accounts.map((shown) => shown.account_id),
    ).toEqual(["a"]);
    expect(getRendererState().link).toBe("live");
  });
});
