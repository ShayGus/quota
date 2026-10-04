import { describe, expect, it } from "vitest";

import { describeCommandError } from "../src/shared/ipc/report";

describe("describing a command error", () => {
  it("says a provider answer this build cannot read is the provider's format", () => {
    expect(
      describeCommandError({ kind: "internal", context: { code: "unsupported_schema" } }),
    ).toBe("The provider answered in a format this version of Quota cannot read yet.");
  });

  it("keeps any other internal failure generic", () => {
    expect(
      describeCommandError({ kind: "internal", context: { code: "join_failed" } }),
    ).toBe("Quota hit an internal problem.");
  });

  it("repeats the reason a refused sign-in was given, log location and all", () => {
    const reason =
      "The provider did not accept this sign-in, so nothing was added. " +
      "The log is at /home/person/.local/share/app.quota.monitor/logs/quota.log";
    expect(describeCommandError({ kind: "provider_refused", context: { reason } })).toBe(
      reason,
    );
  });

  it("repeats the reason a refused window operation was given", () => {
    const reason =
      "the browser did not open https://auth.meta.com/oauth/device/?code=ABCD-EFGH. " +
      "The log is at C:\\Users\\person\\AppData\\Local\\app.quota.monitor\\logs\\quota.log";
    expect(
      describeCommandError({
        kind: "native_operation_failed",
        context: { operation: "open_external", reason },
      }),
    ).toBe(reason);
  });

  it("keeps the login-item sentence even when the host supplied a reason", () => {
    expect(
      describeCommandError({
        kind: "native_operation_failed",
        context: {
          operation: "launch_at_login",
          reason: "the operating system refused it",
        },
      }),
    ).toBe("Windows did not change the login item. Launch at login is unchanged.");
  });

  it("falls back to a generic sentence only when the host sent no reason", () => {
    expect(
      describeCommandError({
        kind: "native_operation_failed",
        context: { operation: "something_else", reason: "" },
      }),
    ).toBe("The system refused that window operation.");
  });
});
