// Arithmetic on reading positions: ordering, progress fractions, and reading-time estimates.

import type { Location, OpenedBook } from "../types";

export const START: Record<OpenedBook["kind"], Location> = {
  flow: { kind: "flow", chapter: 0, block: 0 },
  paged: { kind: "page", page: 0 },
};

const order = (l: Location): [number, number] => (l.kind === "flow" ? [l.chapter, l.block] : [l.page, 0]);

/** Whether `a` is at or before `b`. */
export function atOrBefore(a: Location, b: Location): boolean {
  const [x, y] = [order(a), order(b)];
  return x[0] < y[0] || (x[0] === y[0] && x[1] <= y[1]);
}

export function sameLocation(a: Location | null, b: Location | null): boolean {
  if (!a || !b) return a === b;
  const [x, y] = [order(a), order(b)];
  return a.kind === b.kind && x[0] === y[0] && x[1] === y[1];
}

/** Whether a stored location still exists in the opened book. */
export function isValid(book: OpenedBook, loc: Location): boolean {
  return loc.kind === "flow"
    ? book.kind === "flow" && loc.chapter < book.chapters.length
    : book.kind === "paged" && loc.page < book.pages.length;
}

/** Progress through the book in 0..1, measured in blocks (reflowable) or pages. */
export function progressOf(book: OpenedBook, loc: Location): number {
  if (loc.kind === "page") return book.pages.length <= 1 ? 1 : loc.page / (book.pages.length - 1);
  const total = book.chapters.reduce((n, c) => n + Math.max(1, c.blocks), 0);
  const before = book.chapters.slice(0, loc.chapter).reduce((n, c) => n + Math.max(1, c.blocks), 0);
  return total ? Math.min(1, (before + loc.block) / total) : 0;
}

/** The location at a progress fraction (the inverse of `progressOf`). */
export function locationAt(book: OpenedBook, fraction: number): Location {
  const f = Math.max(0, Math.min(1, fraction));
  if (book.kind === "paged") return { kind: "page", page: Math.round(f * Math.max(0, book.pages.length - 1)) };
  const total = book.chapters.reduce((n, c) => n + Math.max(1, c.blocks), 0);
  let target = Math.floor(f * total);
  for (let chapter = 0; chapter < book.chapters.length; chapter++) {
    const n = Math.max(1, book.chapters[chapter].blocks);
    if (target < n) return { kind: "flow", chapter, block: Math.min(target, Math.max(0, book.chapters[chapter].blocks - 1)) };
    target -= n;
  }
  const last = book.chapters.length - 1;
  return { kind: "flow", chapter: Math.max(0, last), block: 0 };
}

/** A short description of a location, e.g. for the progress slider's tooltip. */
export function describe(book: OpenedBook, loc: Location): string {
  if (loc.kind === "page") return `第 ${loc.page + 1} 页`;
  return book.chapters[loc.chapter]?.title || `第 ${loc.chapter + 1} 章`;
}

/** Reading position in characters (reflowable) or pages, for speed measurement. */
export function unitsAt(book: OpenedBook, loc: Location): number {
  if (loc.kind === "page") return loc.page;
  const before = book.chapters.slice(0, loc.chapter).reduce((n, c) => n + c.chars, 0);
  const ch = book.chapters[loc.chapter];
  return before + (ch ? (ch.chars * loc.block) / Math.max(1, ch.blocks) : 0);
}

/** Default speeds until the reader's own speed has been measured. */
export const DEFAULT_SPEED = { flow: 400, paged: 1 };

/** Remaining minutes in the current chapter (reflowable only) and in the whole book. */
export function remainingMinutes(book: OpenedBook, loc: Location, speed: number | null): { chapter: number | null; book: number } {
  const perMinute = speed && speed > 0 ? speed : DEFAULT_SPEED[book.kind];
  if (loc.kind === "page") return { chapter: null, book: (book.pages.length - 1 - loc.page) / perMinute };
  const total = book.chapters.reduce((n, c) => n + c.chars, 0);
  const here = unitsAt(book, loc);
  const ch = book.chapters[loc.chapter];
  const chapterEnd = unitsAt(book, { kind: "flow", chapter: loc.chapter, block: ch?.blocks ?? 0 });
  return { chapter: (chapterEnd - here) / perMinute, book: (total - here) / perMinute };
}

export function formatMinutes(minutes: number): string {
  if (minutes < 1) return "不到 1 分钟";
  if (minutes < 60) return `${Math.round(minutes)} 分钟`;
  const h = Math.floor(minutes / 60);
  const m = Math.round(minutes % 60);
  return m ? `${h} 小时 ${m} 分` : `${h} 小时`;
}

export function formatDuration(seconds: number): string {
  if (seconds < 60) return seconds > 0 ? "不到 1 分钟" : "0 分钟";
  return formatMinutes(seconds / 60);
}
