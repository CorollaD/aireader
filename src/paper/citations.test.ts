import { describe, expect, it } from "vitest";
import { citationSegments } from "./citations";

describe("citations in text", () => {
  it("finds single, listed and ranged citations", () => {
    expect(citationSegments("as in [12] and", 20)).toEqual([{ text: "as in " }, { text: "[12]", cites: [12] }, { text: " and" }]);
    expect(citationSegments("[1, 3]", 5)).toEqual([{ text: "[1, 3]", cites: [1, 3] }]);
    expect(citationSegments("[2-4]", 5)).toEqual([{ text: "[2-4]", cites: [2, 3, 4] }]);
    expect(citationSegments("[1–2, 5]", 5)).toEqual([{ text: "[1–2, 5]", cites: [1, 2, 5] }]);
  });

  it("leaves alone what can't be a citation of this list", () => {
    expect(citationSegments("see [99]", 20)).toEqual([{ text: "see [99]" }]);
    expect(citationSegments("[0]", 20)).toEqual([{ text: "[0]" }]);
    expect(citationSegments("x in [1-200]", 300)).toEqual([{ text: "x in [1-200]" }]);
    expect(citationSegments("[5-2]", 20)).toEqual([{ text: "[5-2]" }]);
    expect(citationSegments("[a]", 20)).toEqual([{ text: "[a]" }]);
  });

  it("keeps the text whole", () => {
    const text = "a [1] b [2] c";
    expect(
      citationSegments(text, 2)
        .map((s) => s.text)
        .join(""),
    ).toBe(text);
    expect(citationSegments("", 2)).toEqual([]);
  });
});
