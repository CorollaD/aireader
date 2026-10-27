// Grouping of fixed-layout pages for two-page viewing.

/**
 * Pages shown together in two-page mode: the first page (the cover) alone, then pairs. Wide
 * pages, which are spreads already, stand alone.
 */
export function spreads(aspects: number[]): number[][] {
  const wide = (i: number) => aspects[i] < 1;
  const out: number[][] = aspects.length ? [[0]] : [];
  let i = 1;
  while (i < aspects.length) {
    if (wide(i) || i + 1 >= aspects.length || wide(i + 1)) {
      out.push([i]);
      i += 1;
    } else {
      out.push([i, i + 1]);
      i += 2;
    }
  }
  return out;
}
