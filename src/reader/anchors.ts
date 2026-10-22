// Text positions inside a chapter: DOM selection → (block, offset), highlight re-anchoring, and
// splitting styled spans at highlight boundaries. Offsets are UTF-16 code units of a block's
// text, i.e. its spans concatenated (or the code block's string).

import type { Block, Highlight, HighlightColor, Span } from "../types";

export function blockSpans(block: Block): Span[] | null {
  switch (block.kind) {
    case "heading":
    case "listItem":
      return block.value.spans;
    case "paragraph":
    case "quote":
      return block.value;
    case "code":
      return [{ text: block.value }];
    default:
      return null;
  }
}

export function blockText(block: Block): string {
  return (blockSpans(block) ?? []).map((s) => s.text).join("");
}

/** A selection or highlight range within one chapter. */
export interface TextRange {
  startBlock: number;
  start: number;
  endBlock: number;
  end: number;
}

/** The text a range covers, blocks joined by newlines (the form stored in highlights). */
export function rangeText(blocks: Block[], r: TextRange): string {
  const parts: string[] = [];
  for (let b = r.startBlock; b <= r.endBlock && b < blocks.length; b++) {
    const text = blockText(blocks[b]);
    const from = b === r.startBlock ? r.start : 0;
    const to = b === r.endBlock ? r.end : text.length;
    if (to > from) parts.push(text.slice(from, to));
  }
  return parts.join("\n");
}

/** Text nodes inside `root` that belong to the block's text (skipping list markers etc.). */
function textNodes(root: Node): Text[] {
  const out: Text[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => (n.parentElement?.closest("[data-skip]") ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
  });
  for (let n = walker.nextNode(); n; n = walker.nextNode()) out.push(n as Text);
  return out;
}

/** Offset of a DOM position (node, offset) within a block element's text. */
function offsetIn(blockEl: HTMLElement, node: Node, offset: number): number {
  if (node.nodeType !== Node.TEXT_NODE) {
    // Element position: count the text of the children before `offset`.
    const range = document.createRange();
    range.setStart(blockEl, 0);
    range.setEnd(node, offset);
    const frag = range.cloneContents();
    return textNodes(frag).reduce((n, t) => n + t.data.length, 0);
  }
  let total = 0;
  for (const t of textNodes(blockEl)) {
    if (t === node) return total + Math.min(offset, t.data.length);
    total += t.data.length;
  }
  return total;
}

function blockOf(node: Node): HTMLElement | null {
  const el = node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  return (el?.closest("[data-block]") as HTMLElement | null) ?? null;
}

export interface SelectionInfo extends TextRange {
  text: string;
  /** Viewport rectangle of the selection, for positioning a toolbar. */
  rect: DOMRect;
}

/** The current DOM selection as a text range in `article`, or null if there is none. */
export function selectionRange(article: HTMLElement, blocks: Block[]): SelectionInfo | null {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  if (!article.contains(range.commonAncestorContainer)) return null;
  let startEl = blockOf(range.startContainer);
  let endEl = blockOf(range.endContainer);
  // A selection that starts or ends between blocks snaps to the nearest block boundary.
  if (!startEl || !endEl) {
    const all = [...article.querySelectorAll<HTMLElement>("[data-block]")].filter((el) => range.intersectsNode(el));
    if (all.length === 0) return null;
    startEl ??= all[0];
    endEl ??= all[all.length - 1];
  }
  const r: TextRange = {
    startBlock: Number(startEl.dataset.block),
    start: blockOf(range.startContainer) === startEl ? offsetIn(startEl, range.startContainer, range.startOffset) : 0,
    endBlock: Number(endEl.dataset.block),
    end: blockOf(range.endContainer) === endEl ? offsetIn(endEl, range.endContainer, range.endOffset) : blockText(blocks[Number(endEl.dataset.block)]).length,
  };
  const text = rangeText(blocks, r);
  if (!text.trim()) return null;
  return { ...r, text, rect: range.getBoundingClientRect() };
}

/** A highlight's portion inside one block. */
export interface BlockMark {
  id: string;
  start: number;
  end: number;
  color: HighlightColor;
  note: boolean;
}

/** Finds `text` in the chapter, returning its range. Used when stored offsets no longer match. */
function findText(blocks: Block[], text: string): TextRange | null {
  const texts = blocks.map(blockText);
  const joined = texts.join("\n");
  const at = joined.indexOf(text);
  if (at < 0) return null;
  const locate = (pos: number): [number, number] => {
    let base = 0;
    for (let b = 0; b < texts.length; b++) {
      if (pos <= base + texts[b].length) return [b, pos - base];
      base += texts[b].length + 1;
    }
    return [texts.length - 1, texts[texts.length - 1]?.length ?? 0];
  };
  const [startBlock, start] = locate(at);
  const [endBlock, end] = locate(at + text.length);
  return { startBlock, start, endBlock, end };
}

export interface Resolved {
  marks: Map<number, BlockMark[]>;
  /** Highlights whose stored offsets were wrong but whose text was found elsewhere. */
  moved: (TextRange & { id: string })[];
}

/** Places a chapter's highlights on its blocks, re-anchoring them by text when needed. */
export function resolveHighlights(blocks: Block[], highlights: Highlight[]): Resolved {
  const marks = new Map<number, BlockMark[]>();
  const moved: Resolved["moved"] = [];
  for (const h of highlights) {
    let r: TextRange | null = h;
    if (h.endBlock >= blocks.length || rangeText(blocks, h) !== h.text) {
      r = findText(blocks, h.text);
      if (!r) continue;
      moved.push({ id: h.id, ...r });
    }
    for (let b = r.startBlock; b <= r.endBlock; b++) {
      const len = blockText(blocks[b]).length;
      const start = b === r.startBlock ? r.start : 0;
      const end = b === r.endBlock ? r.end : len;
      if (end <= start) continue;
      const list = marks.get(b) ?? [];
      list.push({ id: h.id, start, end, color: h.color, note: !!h.note?.trim() });
      marks.set(b, list);
    }
  }
  return { marks, moved };
}

/** One rendered piece of a span: its text, style, and the highlight covering it (if any). */
export interface Piece {
  span: Span;
  text: string;
  mark: BlockMark | null;
}

/** Splits spans at highlight boundaries. Later highlights win where they overlap. */
export function splitSpans(spans: Span[], marks: BlockMark[] | undefined): Piece[] {
  if (!marks?.length) return spans.map((span) => ({ span, text: span.text, mark: null }));
  const cuts = new Set<number>();
  for (const m of marks) {
    cuts.add(m.start);
    cuts.add(m.end);
  }
  const pieces: Piece[] = [];
  let pos = 0;
  for (const span of spans) {
    const end = pos + span.text.length;
    const points = [pos, ...[...cuts].filter((c) => c > pos && c < end).sort((a, b) => a - b), end];
    for (let i = 0; i + 1 < points.length; i++) {
      const [a, b] = [points[i], points[i + 1]];
      if (b <= a) continue;
      let mark: BlockMark | null = null;
      for (const m of marks) if (m.start <= a && m.end >= b) mark = m;
      pieces.push({ span, text: span.text.slice(a - pos, b - pos), mark });
    }
    pos = end;
  }
  return pieces;
}
