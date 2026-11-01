// Cutting text into sentences for reading aloud, and choosing the language to speak them in.

import { language } from "../i18n";
import type { Block } from "../types";
import { blockText } from "./anchors";

/** Longer sentences are cut at commas or spaces: engines can stall on very long utterances. */
const MAX_CHARS = 220;

export interface Sentence {
  block: number;
  /** UTF-16 offsets in the block's text. */
  start: number;
  end: number;
  text: string;
}

/** Sentences as [start, end) offsets; the spaces around them are left out. */
export function splitSentences(text: string): [number, number][] {
  const pieces: [number, number][] = [];
  const add = (from: number, to: number) => {
    if (to - from <= MAX_CHARS) {
      pieces.push([from, to]);
      return;
    }
    // Cut at the last comma or space in reach, or just at the limit.
    const window = text.slice(from, from + MAX_CHARS);
    const cut = Math.max(...[...window.matchAll(/[,，、;；:：\s]/g)].map((m) => (m.index ?? 0) + 1), 0);
    const at = from + (cut > MAX_CHARS / 2 ? cut : MAX_CHARS);
    pieces.push([from, at]);
    add(at, to);
  };
  for (const { segment, index } of new Intl.Segmenter(undefined, { granularity: "sentence" }).segment(text)) {
    add(index, index + segment.length);
  }
  return pieces.flatMap(([from, to]) => {
    const part = text.slice(from, to);
    const start = from + (part.length - part.trimStart().length);
    const end = to - (part.length - part.trimEnd().length);
    // Punctuation alone has nothing to say.
    return end > start && /[\p{L}\p{N}]/u.test(part) ? [[start, end] as [number, number]] : [];
  });
}

/** Every sentence of a chapter in order. */
export function chapterSentences(blocks: Block[]): Sentence[] {
  return blocks.flatMap((b, block) => {
    const text = blockText(b);
    return splitSentences(text).map(([start, end]) => ({ block, start, end, text: text.slice(start, end) }));
  });
}

/** The sentence of `text` around `offset`. */
export function sentenceAt(text: string, offset: number): string {
  const found = splitSentences(text).find(([, end]) => offset < end);
  return found ? text.slice(found[0], found[1]) : text.trim();
}

/** The language to speak `text` in: from its script, otherwise the interface language. */
export function speechLang(text: string, fallback: string = language()): string {
  if (/[぀-ヿ]/.test(text)) return "ja-JP";
  if (/[가-힯]/.test(text)) return "ko-KR";
  if (/[一-鿿]/.test(text)) return fallback.startsWith("zh") ? fallback : "zh-CN";
  return fallback.startsWith("zh") || fallback === "ja" || fallback === "ko" ? "en-US" : fallback;
}
