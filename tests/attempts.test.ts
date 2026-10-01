/**
 * Connection-attempt ordering in the renderer store.
 *
 * The backend accepts an attempt before it does any work, so the command reply
 * and the progress events for the same attempt race. A terminal failure must
 * survive a later, older `Started` value from the reply that started it, and a
 * finished attempt must not be kept forever.
 */
import { describe, expect, it } from "vitest";

import { acceptAttempt, clearAttempt, getRendererState } from "../src/shared/state/store";

const ATTEMPT = "synthetic-attempt";

describe("connection attempt revisions", () => {
  it("keeps a failure that arrived before the reply that started the attempt", () => {
    acceptAttempt({
      attemptId: ATTEMPT,
      revision: 2,
      progress: { kind: "failed", context: { error: { kind: "reconnect_required" } } },
    });
    acceptAttempt({ attemptId: ATTEMPT, revision: 0, progress: { kind: "started" } });

    expect(getRendererState().attempts).toEqual([
      {
        attemptId: ATTEMPT,
        revision: 2,
        progress: { kind: "failed", context: { error: { kind: "reconnect_required" } } },
      },
    ]);
  });

  it("never rolls a terminal result back to a running one", () => {
    acceptAttempt({
      attemptId: ATTEMPT,
      revision: 3,
      progress: { kind: "verified", context: { state: "connected" } },
    });
    acceptAttempt({ attemptId: ATTEMPT, revision: 1, progress: { kind: "started" } });

    expect(getRendererState().attempts[0]?.progress.kind).toBe("verified");
  });

  it("ignores a repeated revision rather than rewriting the same step", () => {
    acceptAttempt({ attemptId: ATTEMPT, revision: 1, progress: { kind: "started" } });
    const first = getRendererState();
    acceptAttempt({ attemptId: ATTEMPT, revision: 1, progress: { kind: "started" } });

    expect(getRendererState()).toBe(first);
  });

  it("accepts a genuinely newer step", () => {
    acceptAttempt({ attemptId: ATTEMPT, revision: 1, progress: { kind: "started" } });
    acceptAttempt({
      attemptId: ATTEMPT,
      revision: 2,
      progress: { kind: "awaiting_user" },
    });

    expect(getRendererState().attempts).toHaveLength(1);
    expect(getRendererState().attempts[0]?.progress.kind).toBe("awaiting_user");
  });

  it("keeps two attempts apart", () => {
    acceptAttempt({ attemptId: "one", revision: 1, progress: { kind: "started" } });
    acceptAttempt({ attemptId: "two", revision: 1, progress: { kind: "started" } });

    expect(getRendererState().attempts).toHaveLength(2);
  });
});

describe("finishing an attempt", () => {
  it("drops a finished attempt and keeps a running one", () => {
    acceptAttempt({ attemptId: "running", revision: 1, progress: { kind: "started" } });
    acceptAttempt({
      attemptId: ATTEMPT,
      revision: 2,
      progress: { kind: "failed", context: { error: { kind: "reconnect_required" } } },
    });

    clearAttempt(ATTEMPT);
    expect(getRendererState().attempts.map((entry) => entry.attemptId)).toEqual([
      "running",
    ]);

    clearAttempt("running");
    expect(getRendererState().attempts.map((entry) => entry.attemptId)).toEqual([
      "running",
    ]);
  });

  it("forgets an attempt nobody asked about", () => {
    clearAttempt("never-started");
    expect(getRendererState().attempts).toEqual([]);
  });
});
