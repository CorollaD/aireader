import { describe, expect, it } from "vitest";
import type { Block, Highlight } from "../types";
import { blockText, rangeText, resolveHighlights, splitSpans } from "./anchors";

const para = (text: string): Block => ({ kind: "paragraph", value: [{ text }] });
const blocks: Block[] = [para("Hello world"), { kind: "image", value: 1 }, para("Second paragraph here")];

const highlight = (h: Partial<Highlight>): Highlight => ({
  id: "h",
  chapter: 0,
  startBlock: 0,
  start: 0,
  endBlock: 0,
  end: 5,
  text: "Hello",
  color: "yellow",
  created: 0,
  updated: 0,
  ...h,
});

describe("anchors", () => {
  it("reads text across blocks", () => {
    expect(blockText(blocks[1])).toBe("");
    expect(rangeText(blocks, { startBlock: 0, start: 6, endBlock: 2, end: 6 })).toContain("world");
  });

  it("places highlights on their blocks", () => {
    const { marks, moved } = resolveHighlights(blocks, [highlight({})]);
    expect(moved).toEqual([]);
    expect(marks.get(0)).toEqual([{ id: "h", start: 0, end: 5, color: "yellow", note: false }]);
  });

  it("re-anchors a highlight by its text when offsets are stale", () => {
    const stale = highlight({ startBlock: 0, start: 0, endBlock: 0, end: 6, text: "paragraph" });
    const { marks, moved } = resolveHighlights(blocks, [stale]);
    expect(moved).toEqual([{ id: "h", startBlock: 2, start: 7, endBlock: 2, end: 16 }]);
    expect(marks.get(2)?.[0]).toMatchObject({ start: 7, end: 16 });
  });

  it("drops highlights whose text is gone", () => {
    const { marks } = resolveHighlights(blocks, [highlight({ text: "nowhere", end: 7 })]);
    expect(marks.size).toBe(0);
  });

  it("splits spans at highlight edges, later highlights winning", () => {
    const spans = [{ text: "abc" }, { text: "def", bold: true }];
    const pieces = splitSpans(spans, [
      { id: "1", start: 1, end: 5, color: "yellow", note: false },
      { id: "2", start: 4, end: 6, color: "blue", note: true },
    ]);
    expect(pieces.map((p) => [p.text, p.mark?.id ?? null])).toEqual([
      ["a", null],
      ["bc", "1"],
      ["d", "1"],
      ["e", "2"],
      ["f", "2"],
    ]);
    expect(splitSpans(spans, undefined).map((p) => p.text)).toEqual(["abc", "def"]);
  });
});
