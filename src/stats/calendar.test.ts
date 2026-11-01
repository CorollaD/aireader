import { describe, expect, it } from "vitest";
import { levelOf, localDay, weeksOf } from "./calendar";

describe("reading calendar", () => {
  it("lays a year out in Monday-first weeks", () => {
    const weeks = weeksOf(2026);
    // 1 January 2026 is a Thursday.
    expect(weeks[0].days).toEqual([null, null, null, "2026-01-01", "2026-01-02", "2026-01-03", "2026-01-04"]);
    expect(weeks[0].month).toBe(0);
    const days = weeks.flatMap((w) => w.days).filter(Boolean);
    expect(days).toHaveLength(365);
    expect(days[days.length - 1]).toBe("2026-12-31");
    expect(weeks.map((w) => w.month).filter((m) => m !== null)).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    expect(
      weeksOf(2028)
        .flatMap((w) => w.days)
        .filter(Boolean),
    ).toHaveLength(366);
  });

  it("shades by fixed amounts of time", () => {
    expect([0, 30, 60, 14 * 60, 15 * 60, 30 * 60, 3600, 7200].map(levelOf)).toEqual([0, 0, 1, 1, 2, 3, 4, 4]);
  });

  it("formats local dates", () => {
    expect(localDay(new Date(2026, 2, 5))).toBe("2026-03-05");
  });
});
