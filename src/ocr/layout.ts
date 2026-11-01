// Turns recognised lines into text runs in reading order. The recogniser reports lines from top
// to bottom across the page, which interleaves the columns of a two-column page.

import type { TextRun } from "../types";

/** A recognised line: its corner points, in pixels. */
export interface OcrLine {
  text: string;
  poly: readonly (readonly number[])[];
}

/** Lines beside each other on this many rows make a band two columns. */
const MIN_COLUMN_ROWS = 3;

const overlapsVertically = (a: TextRun, b: TextRun) => Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y) > Math.min(a.h, b.h) / 2;

/** Whether a stretch of lines between full-width ones is set in two columns. */
function twoColumns(left: TextRun[], right: TextRun[]): boolean {
  return left.filter((l) => right.some((r) => overlapsVertically(l, r))).length >= MIN_COLUMN_ROWS;
}

/** Lines from top to bottom, each left to right: boxes on one line rarely start level. */
function toLines(runs: TextRun[]): TextRun[][] {
  const lines: TextRun[][] = [];
  for (const run of [...runs].sort((a, b) => a.y - b.y)) {
    const line = lines[lines.length - 1];
    if (line && overlapsVertically(line[0], run)) line.push(run);
    else lines.push([run]);
  }
  return lines.map((line) => line.sort((a, b) => a.x - b.x));
}

export function readingOrder(runs: TextRun[]): TextRun[] {
  if (runs.length === 0) return [];
  const left0 = Math.min(...runs.map((r) => r.x));
  const right0 = Math.max(...runs.map((r) => r.x + r.w));
  const middle = (left0 + right0) / 2;
  const crosses = (r: TextRun) => r.x < middle && r.x + r.w > middle;

  const out: TextRun[] = [];
  let left: TextRun[] = [];
  let right: TextRun[] = [];
  const flush = () => {
    if (twoColumns(left, right)) out.push(...left, ...right);
    else out.push(...toLines([...left, ...right]).flat());
    left = [];
    right = [];
  };
  for (const line of toLines(runs)) {
    if (line.some(crosses)) {
      flush();
      out.push(...line);
    } else for (const run of line) (run.x + run.w <= middle ? left : right).push(run);
  }
  flush();
  return out;
}

/** Text runs for a page of `width` by `height` pixels, in fractions of the page. */
export function toRuns(lines: OcrLine[], width: number, height: number): TextRun[] {
  const runs = lines.flatMap(({ text, poly }) => {
    if (!text.trim() || poly.length === 0) return [];
    const xs = poly.map((p) => p[0]);
    const ys = poly.map((p) => p[1]);
    const [x, y] = [Math.min(...xs), Math.min(...ys)];
    return [{ text, x: x / width, y: y / height, w: (Math.max(...xs) - x) / width, h: (Math.max(...ys) - y) / height }];
  });
  return readingOrder(runs);
}
