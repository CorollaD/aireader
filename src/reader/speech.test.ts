import { describe, expect, it } from "vitest";
import type { Block } from "../types";
import { chapterSentences, sentenceAt, speechLang, splitSentences } from "./speech";

const pieces = (text: string) => splitSentences(text).map(([a, b]) => text.slice(a, b));

describe("reading aloud", () => {
  it("cuts Chinese and English sentences", () => {
    expect(pieces("话说天下大势。分久必合！合久必分？")).toEqual(["话说天下大势。", "分久必合！", "合久必分？"]);
    expect(pieces("  It was dark. He ran home.  ")).toEqual(["It was dark.", "He ran home."]);
  });

  it("skips pieces with nothing to say and cuts long sentences", () => {
    expect(pieces("……——")).toEqual([]);
    const long = `${"字".repeat(150)}，${"字".repeat(150)}。`;
    const parts = pieces(long);
    expect(parts.length).toBe(2);
    expect(parts[0].endsWith("，")).toBe(true);
    expect(parts.join("")).toBe(long);
    const unbroken = pieces("字".repeat(500));
    expect(Math.max(...unbroken.map((p) => p.length))).toBeLessThanOrEqual(220);
    expect(unbroken.join("").length).toBe(500);
  });

  it("keeps offsets into the block text", () => {
    const blocks: Block[] = [
      { kind: "image", value: 1 },
      { kind: "paragraph", value: [{ text: "One. " }, { text: "Two.", bold: true }] },
    ];
    expect(chapterSentences(blocks)).toEqual([
      { block: 1, start: 0, end: 4, text: "One." },
      { block: 1, start: 5, end: 9, text: "Two." },
    ]);
  });

  it("finds the sentence around a word", () => {
    expect(sentenceAt("One here. Two there.", 12)).toBe("Two there.");
    expect(sentenceAt("One here. Two there.", 0)).toBe("One here.");
  });

  it("speaks each script in its language", () => {
    expect(speechLang("你好", "en")).toBe("zh-CN");
    expect(speechLang("你好", "zh-TW")).toBe("zh-TW");
    expect(speechLang("こんにちは", "en")).toBe("ja-JP");
    expect(speechLang("안녕하세요", "en")).toBe("ko-KR");
    expect(speechLang("hello", "zh-CN")).toBe("en-US");
    expect(speechLang("hola", "es")).toBe("es");
  });
});
