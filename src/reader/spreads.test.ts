import { describe, expect, it } from "vitest";
import { spreads } from "./spreads";

describe("two-page spreads", () => {
  it("shows the cover alone, then pairs", () => {
    expect(spreads([1.5, 1.5, 1.5, 1.5, 1.5])).toEqual([[0], [1, 2], [3, 4]]);
    expect(spreads([1.5, 1.5, 1.5])).toEqual([[0], [1, 2]]);
    expect(spreads([1.5, 1.5])).toEqual([[0], [1]]);
    expect(spreads([])).toEqual([]);
  });

  it("keeps wide pages on their own", () => {
    expect(spreads([1.5, 1.5, 0.7, 1.5, 1.5])).toEqual([[0], [1], [2], [3, 4]]);
  });
});
