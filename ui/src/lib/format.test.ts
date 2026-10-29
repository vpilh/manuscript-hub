import { describe, expect, it } from "vitest";

import {
  confidenceLabel,
  confidenceWord,
  daysUntil,
  isoDate,
  isUrgent,
  longDate,
  parseIsoDate,
  relativeDue,
  statusLabel,
} from "./format";

describe("format", () => {
  it("labels every status", () => {
    expect(statusLabel("revision_required")).toBe("Revision required");
  });

  it("pads ISO dates", () => {
    expect(isoDate({ year: 2026, month: 3, day: 7 })).toBe("2026-03-07");
  });

  it("writes long dates in one English format", () => {
    expect(longDate({ year: 2026, month: 11, day: 12 })).toBe("12 November 2026");
  });

  it("counts days to a deadline in local calendar days", () => {
    const today = new Date(2026, 9, 29, 23, 30);
    expect(daysUntil({ year: 2026, month: 11, day: 12 }, today)).toBe(14);
    expect(daysUntil({ year: 2026, month: 10, day: 28 }, today)).toBe(-1);
  });

  it("describes relative deadlines", () => {
    expect(relativeDue(14)).toBe("in 14 days");
    expect(relativeDue(1)).toBe("tomorrow");
    expect(relativeDue(0)).toBe("today");
    expect(relativeDue(-1)).toBe("1 day overdue");
    expect(relativeDue(-3)).toBe("3 days overdue");
    expect(isUrgent(7)).toBe(true);
    expect(isUrgent(8)).toBe(false);
  });

  it("buckets and words confidence", () => {
    expect(confidenceLabel(0.9)).toBe("high");
    expect(confidenceLabel(0.5)).toBe("medium");
    expect(confidenceLabel(0.4)).toBe("low");
    expect(confidenceWord(0.4)).toBe("uncertain");
  });
});

describe("parseIsoDate", () => {
  it("accepts valid dates from date inputs", () => {
    expect(parseIsoDate("2026-11-12")).toEqual({ year: 2026, month: 11, day: 12 });
  });

  it("rejects empty and impossible dates", () => {
    expect(parseIsoDate("")).toBeNull();
    expect(parseIsoDate("2026-02-30")).toBeNull();
    expect(parseIsoDate("12.11.2026")).toBeNull();
  });
});
