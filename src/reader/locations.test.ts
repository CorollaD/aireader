import { describe, expect, it } from "vitest";
import type { OpenedBook } from "../types";
import { atOrBefore, isValid, locationAt, progressOf, remainingMinutes, sameLocation, unitsAt } from "./locations";

const base = { id: "b", title: "B", author: null, format: "EPUB", toc: [], position: null, readSeconds: 0, readingSpeed: null, rotation: 0 };
const flow: OpenedBook = {
  ...base,
  kind: "flow",
  chapters: [
    { title: "One", blocks: 10, chars: 1000 },
    { title: "Two", blocks: 30, chars: 3000 },
  ],
  pages: [],
};
const paged: OpenedBook = { ...base, kind: "paged", format: "PDF", chapters: [], pages: [1.4, 1.4, 1.4, 1.4, 1.4] };

describe("locations", () => {
  it("orders positions", () => {
    expect(atOrBefore({ kind: "flow", chapter: 0, block: 9 }, { kind: "flow", chapter: 1, block: 0 })).toBe(true);
    expect(atOrBefore({ kind: "flow", chapter: 1, block: 1 }, { kind: "flow", chapter: 1, block: 0 })).toBe(false);
    expect(sameLocation({ kind: "page", page: 2 }, { kind: "page", page: 2 })).toBe(true);
    expect(sameLocation(null, null)).toBe(true);
    expect(sameLocation({ kind: "page", page: 0 }, { kind: "flow", chapter: 0, block: 0 })).toBe(false);
  });

  it("converts between progress and positions", () => {
    expect(progressOf(flow, { kind: "flow", chapter: 1, block: 10 })).toBeCloseTo(0.5);
    expect(locationAt(flow, 0.5)).toEqual({ kind: "flow", chapter: 1, block: 10 });
    expect(locationAt(flow, 2)).toEqual({ kind: "flow", chapter: 1, block: 29 });
    expect(progressOf(paged, { kind: "page", page: 2 })).toBeCloseTo(0.5);
    expect(locationAt(paged, 1)).toEqual({ kind: "page", page: 4 });
    // Round trip for every block.
    for (let f = 0; f < 1; f += 0.025) {
      const loc = locationAt(flow, f);
      expect(locationAt(flow, progressOf(flow, loc))).toEqual(loc);
    }
  });

  it("checks positions against the book", () => {
    expect(isValid(flow, { kind: "flow", chapter: 2, block: 0 })).toBe(false);
    expect(isValid(paged, { kind: "page", page: 4 })).toBe(true);
    expect(isValid(paged, { kind: "flow", chapter: 0, block: 0 })).toBe(false);
  });

  it("estimates remaining time from characters", () => {
    expect(unitsAt(flow, { kind: "flow", chapter: 1, block: 15 })).toBe(2500);
    const left = remainingMinutes(flow, { kind: "flow", chapter: 1, block: 15 }, 500);
    expect(left.chapter).toBeCloseTo(3);
    expect(left.book).toBeCloseTo(3);
    expect(remainingMinutes(paged, { kind: "page", page: 1 }, null)).toEqual({ chapter: null, book: 3 });
  });
});
