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
});
