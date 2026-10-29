import { describe, expect, it } from "vitest";

import { confidenceLabel, daysUntil, isoDate, statusLabel } from "./format";

describe("format", () => {
  it("labels every status", () => {
    expect(statusLabel("revision_required")).toBe("Revision required");
  });

  it("pads ISO dates", () => {
    expect(isoDate({ year: 2026, month: 3, day: 7 })).toBe("2026-03-07");
  });

  it("counts days to a deadline in local calendar days", () => {
    const today = new Date(2026, 9, 29, 23, 30);
    expect(daysUntil({ year: 2026, month: 11, day: 12 }, today)).toBe(14);
    expect(daysUntil({ year: 2026, month: 10, day: 28 }, today)).toBe(-1);
  });

  it("buckets confidence", () => {
    expect(confidenceLabel(0.9)).toBe("high");
    expect(confidenceLabel(0.5)).toBe("medium");
    expect(confidenceLabel(0.4)).toBe("low");
  });
});
