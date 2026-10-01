/**
 * Formatting rules.
 *
 * These cover the arithmetic-free part of the renderer: how a reading becomes a
 * label, and how a boundary becomes a countdown. A missing reading must never
 * become a number (AC-06, AC-07, AC-08, AC-14).
 */
import { describe, expect, it } from "vitest";

import {
  arcFraction,
  formatRemaining,
  remainingPercent,
  severityOf,
} from "../src/shared/format/allowance";
import {
  boundaryLead,
  formatAge,
  formatBoundary,
  formatCountdown,
  instantOf,
} from "../src/shared/format/duration";
import { percent, unavailable } from "./fixtures";

describe("the remaining-allowance label", () => {
  it("shows a whole percentage without decimals", () => {
    expect(formatRemaining(percent(72))).toBe("72%");
    expect(formatRemaining(percent(53.4))).toBe("53%");
  });

  it("labels a positive sub-one remainder as <1%, never as 0%", () => {
    expect(formatRemaining(percent(0.42))).toBe("<1%");
  });

  it("keeps a decimal at the top of the range, so a near-full value never reads 100%", () => {
    expect(formatRemaining(percent(99.6))).toBe("99.6%");
    expect(formatRemaining(percent(100))).toBe("100.0%");
  });

  it("says a genuine zero is zero, as the specification's examples do", () => {
    expect(formatRemaining(percent(0))).toBe("0%");
  });

  it("draws an overspent reading as zero while keeping the original value", () => {
    expect(formatRemaining(percent(-37.5))).toBe("0%");
    expect(remainingPercent(percent(-37.5))).toBe(-37.5);
  });

  it("names every unavailable reason instead of inventing a number", () => {
    expect(formatRemaining(unavailable())).toBe("Not reported");
    expect(formatRemaining({ kind: "unavailable", value: "unsupported" })).toBe(
      "Not supported",
    );
    expect(formatRemaining({ kind: "unavailable", value: "invalid_response" })).toBe(
      "Unreadable",
    );
    expect(formatRemaining({ kind: "unavailable", value: "not_applicable" })).toBe(
      "Not applicable",
    );
  });

  it("never produces a numeric label from a reason it does not recognise", () => {
    const unknown = {
      kind: "unavailable" as const,
      value: "from_a_newer_backend" as never,
    };
    expect(formatRemaining(unknown)).toBe("No reading");
  });

  it("says unlimited and not-entitled in words", () => {
    expect(formatRemaining({ kind: "unlimited" })).toBe("Unlimited");
    expect(formatRemaining({ kind: "not_entitled" })).toBe("Not included");
  });

  it("derives a percentage from a quantity with a denominator", () => {
    const measurement = {
      kind: "quantity" as const,
      value: {
        unit: { kind: "requests" as const },
        precision: 0,
        used: 40,
        remaining: 60,
        limit: 100,
      },
    };
    expect(formatRemaining(measurement)).toBe("60%");
  });

  it("leaves a count without a denominator in its native state", () => {
    const measurement = {
      kind: "quantity" as const,
      value: {
        unit: { kind: "requests" as const },
        precision: 0,
        used: 12,
        remaining: 340,
        limit: null,
      },
    };
    expect(formatRemaining(measurement)).toBe("No reading");
    expect(remainingPercent(measurement)).toBeNull();
  });

  it("keeps an amount with no cap from becoming a percentage", () => {
    const measurement = {
      kind: "money" as const,
      value: {
        currency: "USD",
        scale: 2,
        used_minor_units: 2000,
        remaining_minor_units: 3000,
        limit_minor_units: null,
      },
    };
    expect(remainingPercent(measurement)).toBeNull();
  });
});

describe("severity", () => {
  it("uses the documented boundaries", () => {
    expect(severityOf(percent(0))).toBe("danger");
    expect(severityOf(percent(10))).toBe("danger");
    expect(severityOf(percent(20))).toBe("warn");
    expect(severityOf(percent(20.5))).toBe("good");
  });

  it("refuses to call an unknown reading healthy or critical", () => {
    expect(severityOf(unavailable())).toBe("pending");
  });
});

describe("arcs", () => {
  it("returns no arc for a reading without a number", () => {
    expect(arcFraction(unavailable())).toBeNull();
    expect(arcFraction({ kind: "unlimited" })).toBeNull();
  });

  it("clamps a known arc into the drawable range", () => {
    expect(arcFraction(percent(72))).toBe(0.72);
    expect(arcFraction(percent(-5))).toBe(0);
    expect(arcFraction(percent(140))).toBe(1);
  });
});

describe("boundaries", () => {
  const now = Date.parse("2026-10-01T12:00:00.000Z");

  it("never shows a negative countdown", () => {
    expect(formatCountdown(now - 60_000, now)).toBe("due");
    expect(formatCountdown(now + 30_000, now)).toBe("under 1m");
  });

  it("formats a countdown in days and hours", () => {
    expect(formatCountdown(now + 3 * 86_400_000 + 3_600_000, now)).toBe("3d 1h");
    expect(formatCountdown(now + 3_600_000 + 60_000, now)).toBe("1h 1m");
    expect(formatCountdown(now + 5 * 60_000, now)).toBe("5m");
  });

  it("names what a boundary actually does", () => {
    expect(
      boundaryLead({ at: "2026-10-02T00:00:00.000Z", kind: "next_replenishment" }),
    ).toBe("Next replenishment");
    expect(boundaryLead({ at: "2026-10-02T00:00:00.000Z", kind: "full_reset" })).toBe(
      "Resets",
    );
  });

  it("does not guess a reset time that was never reported", () => {
    expect(formatBoundary(null, now)).toBe("No reported reset");
    expect(formatBoundary({ at: "not a timestamp", kind: "unknown" }, now)).toBe(
      "No reported reset",
    );
  });

  it("reports a boundary that has passed as due", () => {
    expect(
      formatBoundary({ at: "2026-10-01T11:00:00.000Z", kind: "full_reset" }, now),
    ).toBe("Resets · due");
  });

  it("reads a usable instant and rejects an unusable one", () => {
    expect(instantOf("2026-10-01T12:00:00.000Z")).toBe(now);
    expect(instantOf("nonsense")).toBeNull();
    expect(instantOf(null)).toBeNull();
  });
});

describe("age labels", () => {
  const now = Date.parse("2026-10-01T12:00:00.000Z");

  it("describes how old a reading is", () => {
    expect(formatAge(now - 5_000, now)).toBe("under a minute ago");
    expect(formatAge(now - 18_000, now)).toBe("under a minute ago");
    expect(formatAge(now - 22 * 60_000, now)).toBe("22m ago");
    expect(formatAge(now - 3 * 3_600_000, now)).toBe("3h ago");
    expect(formatAge(now - 26 * 3_600_000, now)).toBe("1d ago");
  });

  it("does not report a negative age as a stale reading", () => {
    expect(formatAge(now + 5_000, now)).toBe("just now");
  });
});
