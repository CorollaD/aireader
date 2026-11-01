// Finding numbered citations like [12], [1, 2] or [3-5] in a run of text.

/** A piece of a run of text; `cites` holds the reference numbers when it is a citation. */
export interface Segment {
  text: string;
  cites?: number[];
}

const CITATION = /\[(\d{1,3}(?:\s*[-–—,，;]\s*\d{1,3})*)\]/g;
/** A wider range than this is not a citation, more likely an interval in a formula. */
const MAX_RANGE = 40;

/** The numbers a citation's inner text stands for; null if any is outside the list. */
function numbers(inner: string, count: number): number[] | null {
  const out: number[] = [];
  for (const part of inner.split(/[,，;]/)) {
    const [from, to] = part.split(/[-–—]/).map((n) => Number.parseInt(n.trim(), 10));
    const last = to ?? from;
    if (last < from || last - from > MAX_RANGE) return null;
    for (let n = from; n <= last; n++) {
      if (n < 1 || n > count) return null;
      out.push(n);
    }
  }
  return out;
}

/** Splits text around its citations of a list of `count` references. */
export function citationSegments(text: string, count: number): Segment[] {
  const out: Segment[] = [];
  let at = 0;
  for (const match of text.matchAll(CITATION)) {
    const cites = numbers(match[1], count);
    if (!cites) continue;
    if (match.index > at) out.push({ text: text.slice(at, match.index) });
    out.push({ text: match[0], cites });
    at = match.index + match[0].length;
  }
  if (at < text.length) out.push({ text: text.slice(at) });
  return out;
}
