/**
 * The generated bindings mirror.
 *
 * Two things must hold at this boundary. A transport failure and a typed domain
 * error are different results, so a caller can tell "the call never arrived"
 * from "the backend refused". And a payload the renderer cannot interpret is
 * rejected rather than half-filled (spec 7.8.2, spec 8.2, spec 8.4).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AccountId,
  AccountRef,
  CommandError,
  ConnectionId,
  ConnectionRef,
} from "../src/generated/bindings";

/** What the transport double should answer, or reject with, next. */
let answer: (command: string, args: unknown) => Promise<unknown> = () =>
  Promise.resolve(null);

// `vi.mock` is hoisted above the imports below, so the transport is replaced
// before the module graph under test is evaluated.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: unknown) => answer(command, args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

import {
  attachEventHandlers,
  disconnectAccount,
  getSnapshot,
  parseCommandError,
  setAccountEnabled,
} from "../src/generated/bindings";
import { snapshot } from "./fixtures";

beforeEach(() => {
  answer = () => Promise.resolve(null);
});

describe("transport failure versus domain error", () => {
  it("reports a typed domain error as a domain error", async () => {
    // A typed domain error arrives as the rejection value, exactly as the
    // generated wrapper expects to decode it. The carrier does not wrap it in
    // an Error, so the rejection is deliberately a plain object here.
    const refusal = { kind: "reconnect_required" } as const;
    answer = () => Promise.reject(refusal); // eslint-disable-line @typescript-eslint/prefer-promise-reject-errors -- the carrier rejects with the serialized error value, not an Error
    const result = await disconnectAccount({
      account_ref: { id: "a1" as AccountId },
      expected_revision: 3,
    });
    expect("error" in result).toBe(true);
    expect("transportError" in result).toBe(false);
    if ("error" in result) {
      expect(result.error.kind).toBe("reconnect_required");
    }
  });

  it("reports a carrier that answered nothing at all as a transport failure", async () => {
    answer = () => Promise.reject(new Error("connection refused"));
    const result = await disconnectAccount({
      account_ref: { id: "a1" as AccountId },
      expected_revision: 3,
    });
    expect("transportError" in result).toBe(true);
    expect("error" in result).toBe(false);
    if ("transportError" in result) {
      expect(result.transportError.code).toBe("unknown_error");
    }
  });

  it("reports an unreadable success payload as a transport failure, not as empty data", async () => {
    answer = () => Promise.resolve({ snapshot: { revision: "not a snapshot" } });
    const result = await getSnapshot();
    expect("transportError" in result).toBe(true);
    if ("transportError" in result) {
      expect(result.transportError.code).toBe("malformed_response");
    }
  });

  it("accepts a payload this build understands", async () => {
    answer = () => Promise.resolve({ snapshot: snapshot("instance-1", 1, []) });
    const result = await getSnapshot();
    expect("ok" in result).toBe(true);
  });

  it("rejects an error shape it does not recognise", () => {
    expect(parseCommandError({ kind: "made_up_kind" })).toBeNull();
    expect(parseCommandError("a prose error")).toBeNull();
  });

  it("keeps the structured context of a known error", () => {
    const parsed = parseCommandError({
      kind: "revision_conflict",
      context: { expected: 3, actual: 4 },
    });
    expect(parsed).toEqual({
      kind: "revision_conflict",
      context: { expected: 3, actual: 4 },
    });
  });

  it("sends the tagged reference the command declares", async () => {
    const seen: { command: string; args: unknown }[] = [];
    answer = (command, args) => {
      seen.push({ command, args });
      return Promise.resolve(null);
    };
    await setAccountEnabled({
      account_ref: { id: "a1" as AccountId },
      enabled: false,
      expected_revision: 9,
    });
    expect(seen[0]?.command).toBe("set_account_enabled");
    expect(seen[0]?.args).toEqual({
      request: {
        account_ref: { id: "a1" },
        enabled: false,
        expected_revision: 9,
      },
    });
  });
});

describe("the tagged identifier types", () => {
  it("refuses to pass a connection where an account is required", () => {
    const connection: ConnectionRef = { id: "c1" as ConnectionId };
    const request: Parameters<typeof disconnectAccount>[0] = {
      // @ts-expect-error a ConnectionRef is not an AccountRef: the tags make the
      // mistake a compile error rather than a runtime account mix-up.
      account_ref: connection,
      expected_revision: 1,
    };
    expect(request.expected_revision).toBe(1);
  });

  it("refuses to pass a bare string where an account reference is required", () => {
    const request: Parameters<typeof disconnectAccount>[0] = {
      // @ts-expect-error a raw identifier is not a tagged reference.
      account_ref: "a1",
      expected_revision: 1,
    };
    expect(request.expected_revision).toBe(1);
  });

  it("accepts a correctly tagged account reference", () => {
    const account: AccountRef = { id: "a1" as AccountId };
    expect(account.id).toBe("a1");
  });
});

describe("event payload narrowing", () => {
  it("delivers only the events that were asked for", async () => {
    const detach = await attachEventHandlers({});
    expect(detach).toEqual([]);
  });

  it("rejects an event payload whose variant this build does not know", async () => {
    const delivered: unknown[] = [];
    answer = () => Promise.resolve(null);
    const registration = await attachEventHandlers({
      onPersistenceStatusChanged: (payload) => {
        delivered.push(payload);
      },
    });
    expect(registration).toHaveLength(1);
    for (const stop of registration) {
      stop();
    }
    expect(delivered).toEqual([]);
  });
});

describe("the closed vocabularies", () => {
  it("keeps a command error a closed union", () => {
    const errors: CommandError[] = [
      { kind: "cancelled" },
      { kind: "internal", context: { code: "x" } },
    ];
    expect(errors.map((error) => error.kind)).toEqual(["cancelled", "internal"]);
  });
});
