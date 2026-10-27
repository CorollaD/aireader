// Mind maps: parsing the model's outline (a nested Markdown list) into a tree, and laying the
// tree out left to right.

export interface MindNode {
  /** Path from the root, e.g. "0.2.1"; stable while the outline only grows. */
  id: string;
  text: string;
  /** Citation numbers written after the text, e.g. "[2]". */
  cites: number[];
  children: MindNode[];
}

const ITEM = /^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$/;
const HEADING = /^(#{1,6})\s+(.*)$/;

function clean(text: string): { text: string; cites: number[] } {
  const cites: number[] = [];
  const stripped = text
    .replace(/\[(\d+(?:\s*[,，]\s*\d+)*)\]/g, (_, ns: string) => {
      for (const n of ns.split(/[,，]/)) cites.push(Number(n.trim()));
      return "";
    })
    .replace(/\*\*|__|`/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return { text: stripped, cites };
}

/**
 * Builds a tree from an outline. Text outside list items is ignored; headings become levels;
 * several top-level items are gathered under a root named `title`. Returns null when there is
 * nothing to draw yet.
 */
export function parseOutline(outline: string, title: string): MindNode | null {
  const fence = /```[^\n]*\n([\s\S]*?)(?:```|$)/.exec(outline);
  const body = fence ? fence[1] : outline;
  const items: { indent: number; text: string }[] = [];
  for (const raw of body.replace(/\t/g, "  ").split("\n")) {
    const heading = HEADING.exec(raw.trim());
    if (heading) {
      items.push({ indent: (heading[1].length - 1) * 2, text: heading[2] });
      continue;
    }
    const item = ITEM.exec(raw);
    if (item) items.push({ indent: item[1].length, text: item[2] });
  }
  const tops: MindNode[] = [];
  const stack: { indent: number; node: MindNode }[] = [];
  for (const it of items) {
    const { text, cites } = clean(it.text);
    if (!text) continue;
    while (stack.length && stack[stack.length - 1].indent >= it.indent) stack.pop();
    const siblings = stack.length ? stack[stack.length - 1].node.children : tops;
    const node: MindNode = { id: "", text, cites, children: [] };
    siblings.push(node);
    stack.push({ indent: it.indent, node });
  }
  if (tops.length === 0) return null;
  const root = tops.length === 1 ? tops[0] : { id: "", text: title, cites: [], children: tops };
  const number = (n: MindNode, id: string) => {
    n.id = id;
    n.children.forEach((c, i) => number(c, `${id}.${i}`));
  };
  number(root, "0");
  return root;
}

/** The outline back as Markdown (for copying). */
export function toMarkdown(root: MindNode): string {
  const lines: string[] = [];
  const walk = (n: MindNode, depth: number) => {
    const cites = n.cites.map((c) => `[${c}]`).join("");
    lines.push(`${"  ".repeat(depth)}- ${n.text}${cites ? ` ${cites}` : ""}`);
    n.children.forEach((c) => walk(c, depth + 1));
  };
  walk(root, 0);
  return lines.join("\n");
}

export function allIds(root: MindNode): string[] {
  const ids: string[] = [];
  const walk = (n: MindNode) => {
    if (n.children.length) ids.push(n.id);
    n.children.forEach(walk);
  };
  walk(root);
  return ids;
}

// --- layout ------------------------------------------------------------------------------------

export interface Placed {
  node: MindNode;
  depth: number;
  /** Top-level branch index (colour); -1 for the root. */
  branch: number;
  x: number;
  y: number;
  w: number;
  h: number;
  lines: string[];
  /** Width of the last line of text (citations follow it). */
  lastW: number;
  /** Children hidden by collapsing. */
  hidden: number;
  parent: Placed | null;
}

export const FONT_FAMILY = '-apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Segoe UI", sans-serif';

export function fontOf(depth: number) {
  return depth === 0 ? { size: 17, weight: 600 } : depth === 1 ? { size: 14, weight: 600 } : { size: 13, weight: 400 };
}

const PAD_X = [16, 12, 4];
const PAD_Y = [10, 7, 4];
const LINE = [24, 20, 19];
const MAX_W = [260, 220, 220];
const GAP_X = 44;
const GAP_Y = 10;
export const CITE_W = 18;

let ctx: CanvasRenderingContext2D | null = null;
function measure(text: string, depth: number): number {
  ctx ??= document.createElement("canvas").getContext("2d");
  if (!ctx) return text.length * fontOf(depth).size;
  const f = fontOf(depth);
  ctx.font = `${f.weight} ${f.size}px ${FONT_FAMILY}`;
  return ctx.measureText(text).width;
}

/** Breaks text into lines no wider than `max` (by character, so CJK and words both wrap). */
function wrap(text: string, depth: number, max: number): string[] {
  if (measure(text, depth) <= max) return [text];
  const lines: string[] = [];
  let line = "";
  for (const word of text.split(/(?<=\s)|(?=[　-鿿가-힯])/)) {
    if (line && measure(line + word, depth) > max) {
      lines.push(line.trimEnd());
      line = word.trimStart();
    } else line += word;
  }
  if (line) lines.push(line);
  return lines.slice(0, 4);
}

/** Positions every visible node. Coordinates start at (0, 0) for the tree's bounding box. */
export function layout(root: MindNode, collapsed: Set<string>): { nodes: Placed[]; width: number; height: number } {
  const nodes: Placed[] = [];
  const level = (d: number) => Math.min(d, 2);

  const build = (n: MindNode, depth: number, branch: number, parent: Placed | null): Placed => {
    const l = level(depth);
    const lines = wrap(n.text, depth, MAX_W[l]);
    const textW = Math.max(...lines.map((s) => measure(s, depth))) + n.cites.length * CITE_W;
    const p: Placed = {
      node: n,
      depth,
      branch,
      x: 0,
      y: 0,
      w: Math.ceil(textW + PAD_X[l] * 2),
      h: lines.length * LINE[l] + PAD_Y[l] * 2,
      lines,
      lastW: measure(lines[lines.length - 1], depth),
      hidden: collapsed.has(n.id) ? n.children.length : 0,
      parent,
    };
    nodes.push(p);
    return p;
  };

  // Subtree heights first, then positions.
  const kids = new Map<Placed, Placed[]>();
  const heights = new Map<Placed, number>();
  const measureTree = (n: MindNode, depth: number, branch: number, parent: Placed | null): Placed => {
    const p = build(n, depth, branch, parent);
    const children = p.hidden ? [] : n.children.map((c, i) => measureTree(c, depth + 1, depth === 0 ? i : branch, p));
    kids.set(p, children);
    const childH = children.reduce((s, c) => s + heights.get(c)!, 0) + Math.max(0, children.length - 1) * GAP_Y;
    heights.set(p, Math.max(p.h, childH));
    return p;
  };
  const top = measureTree(root, 0, -1, null);

  let width = 0;
  const place = (p: Placed, x: number, y0: number) => {
    const h = heights.get(p)!;
    p.x = x;
    p.y = y0 + (h - p.h) / 2;
    width = Math.max(width, x + p.w + (p.hidden ? 30 : 0));
    const children = kids.get(p)!;
    const childH = children.reduce((s, c) => s + heights.get(c)!, 0) + Math.max(0, children.length - 1) * GAP_Y;
    let y = y0 + (h - childH) / 2;
    for (const c of children) {
      place(c, x + p.w + GAP_X, y);
      y += heights.get(c)! + GAP_Y;
    }
  };
  place(top, 0, 0);
  return { nodes, width, height: heights.get(top)! };
}
