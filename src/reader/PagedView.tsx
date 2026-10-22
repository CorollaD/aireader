// Fixed-layout pages (PDF, comics), rendered by the backend through `book://` URLs.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, pageUrl } from "../api";
import { isTyping } from "../ui";
import type { Location, OpenedBook, TextRun } from "../types";
import type { Jump } from "./Reader";
import { pixelRatio } from "../zoom";
import { t } from "../i18n";
import { Icon } from "../icons";

const GAP = 14;
/** Pages within this many screens of the viewport get their images loaded. */
const PRELOAD_SCREENS = 1.5;

/** Render width in device pixels, rounded up so small resizes reuse cached images. */
function renderWidth(cssWidth: number): number {
  const px = cssWidth * pixelRatio();
  return Math.min(4096, Math.ceil(px / 256) * 256);
}

function useSize(ref: React.RefObject<HTMLElement>) {
  const [size, setSize] = useState({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, [ref]);
  return size;
}

/** Text runs per book and page, fetched once. */
const textCache = new Map<string, Promise<TextRun[]>>();
export function pageText(bookId: string, page: number): Promise<TextRun[]> {
  const key = `${bookId}:${page}`;
  let p = textCache.get(key);
  if (!p) {
    p = api.getPageText(bookId, page).catch(() => []);
    textCache.set(key, p);
  }
  return p;
}

/**
 * Invisible, selectable text placed over the page image (like pdf.js). Runs are positioned on
 * the unrotated page, and the whole layer is rotated with the page.
 */
function TextLayer(props: { bookId: string; index: number; width: number; height: number; rotation: number }) {
  const [runs, setRuns] = useState<TextRun[] | null>(null);
  const layer = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let alive = true;
    pageText(props.bookId, props.index).then((r) => alive && setRuns(r));
    return () => {
      alive = false;
    };
  }, [props.bookId, props.index]);
  const quarter = props.rotation === 90 || props.rotation === 270;
  const [w, h] = quarter ? [props.height, props.width] : [props.width, props.height];

  // Stretch each run to its width in the PDF. The rendered width is measured from the DOM (all
  // reads first, then all writes) because canvas measurements differ from layout by a few %.
  useLayoutEffect(() => {
    const spans = [...(layer.current?.querySelectorAll<HTMLElement>(".run") ?? [])];
    if (!runs || spans.length !== runs.length) return;
    const natural = spans.map((el) => el.offsetWidth);
    spans.forEach((el, i) => {
      el.style.transform = natural[i] > 0 ? `scaleX(${(runs[i].w * w) / natural[i]})` : "";
    });
  }, [runs, w, h]);

  if (!runs?.length) return null;
  return (
    <div
      ref={layer}
      className="text-layer"
      data-page={props.index}
      style={{
        width: w,
        height: h,
        left: (props.width - w) / 2,
        top: (props.height - h) / 2,
        transform: props.rotation ? `rotate(${props.rotation}deg)` : undefined,
      }}
    >
      {runs.map((r, i) => {
        const next = runs[i + 1];
        // Separators make copied text read naturally: a line break between lines, a space
        // between runs on the same line.
        const newLine = next && Math.abs(next.y - r.y) > r.h / 2;
        return (
          <span key={i}>
            <span
              className="run"
              style={{ left: r.x * w, top: r.y * h, fontSize: r.h * h }}
            >
              {r.text}
            </span>
            {next && (newLine ? <br /> : <span className="sep"> </span>)}
          </span>
        );
      })}
    </div>
  );
}

/** A page box; the rendered image, once loaded, covers the page-number placeholder. */
function Page(props: {
  bookId: string;
  index: number;
  width: number;
  height: number;
  rotation: number;
  load: boolean;
  style?: React.CSSProperties;
}) {
  return (
    <div className="page" data-page={props.index} style={{ width: props.width, height: props.height, ...props.style }}>
      <span className="page-number">{props.index + 1}</span>
      {props.load && (
        <img
          src={pageUrl(props.bookId, props.index, renderWidth(props.width), props.rotation)}
          alt=""
          draggable={false}
          decoding="async"
        />
      )}
      {props.load && <TextLayer bookId={props.bookId} index={props.index} width={props.width} height={props.height} rotation={props.rotation} />}
    </div>
  );
}

/** A text selection on a PDF page. */
export interface PageSelection {
  page: number;
  text: string;
  rect: DOMRect;
}

/** The current selection inside a text layer, or null. */
function pageSelection(): PageSelection | null {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  const node = range.startContainer;
  const layer = (node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement)?.closest<HTMLElement>(".text-layer");
  const text = sel.toString().trim();
  if (!layer || !text) return null;
  return { page: Number(layer.dataset.page), text, rect: range.getBoundingClientRect() };
}

interface Props {
  book: OpenedBook;
  jump: Jump | null;
  layout: "continuous" | "single";
  fit: "width" | "page";
  zoom: number;
  /** Clockwise page rotation in degrees. */
  rotation: number;
  onLocation: (loc: Location, lastVisible: number) => void;
  onSelect: (selection: PageSelection | null) => void;
}

export default function PagedView({ book, jump, layout, fit, zoom, rotation, onLocation, onSelect }: Props) {
  const scroller = useRef<HTMLDivElement>(null);
  const size = useSize(scroller);
  const [page, setPage] = useState(() => (jump?.loc.kind === "page" ? jump.loc.page : 0));
  // Pages whose images are loaded; starts around the initial page so resuming mid-book
  // does not fetch the first pages.
  const [range, setRange] = useState<[number, number]>(() => {
    const p = jump?.loc.kind === "page" ? jump.loc.page : 0;
    return [Math.max(0, p - 1), p + 2];
  });
  const count = book.pages.length;
  const pageRef = useRef(page);
  pageRef.current = page;

  // Quarter turns swap each page's width and height.
  const quarter = rotation === 90 || rotation === 270;
  const aspects = useMemo(() => book.pages.map((a) => (quarter ? 1 / a : a)), [book.pages, quarter]);
  const fitWidth = Math.min(size.w - 48, 1400);
  // "Fit page" sizes pages so a typical (first) page fits the window height.
  const fitPage = Math.min(fitWidth, (size.h - 2 * GAP) / (aspects[0] ?? 1.414));
  const width = Math.max(120, (fit === "width" ? fitWidth : fitPage) * zoom);
  const tops = useMemo(() => {
    const t: number[] = [];
    let y = GAP;
    for (const aspect of aspects) {
      t.push(y);
      y += width * aspect + GAP;
    }
    t.push(y);
    return t;
  }, [aspects, width]);

  const goto = useCallback(
    (p: number) => {
      const target = Math.max(0, Math.min(count - 1, p));
      setPage(target);
      if (layout === "continuous" && scroller.current) scroller.current.scrollTop = tops[target] - GAP;
    },
    [count, layout, tops],
  );

  // Only explicit jumps move the view, so `goto` is deliberately not a dependency.
  useEffect(() => {
    if (jump?.loc.kind === "page") goto(jump.loc.page);
  }, [jump]);

  // Keep the current page in view when zoom, layout or window size change.
  useLayoutEffect(() => {
    if (layout === "continuous" && scroller.current && size.w > 0) scroller.current.scrollTop = tops[pageRef.current] - GAP;
  }, [layout, tops, size.w]);

  const [lastVisible, setLastVisible] = useState(page);
  useEffect(() => onLocation({ kind: "page", page }, layout === "single" ? page : lastVisible), [page, lastVisible, layout, onLocation]);

  const onScroll = () => {
    const el = scroller.current;
    if (!el || layout !== "continuous") return;
    const top = el.scrollTop;
    const probe = top + el.clientHeight * 0.3;
    const find = (y: number) => {
      let lo = 0;
      let hi = count - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (tops[mid] <= y) lo = mid;
        else hi = mid - 1;
      }
      return lo;
    };
    const margin = el.clientHeight * PRELOAD_SCREENS;
    setRange([find(top - margin), find(top + el.clientHeight + margin)]);
    setPage(find(probe));
    setLastVisible(find(top + el.clientHeight - 1));
  };
  useEffect(onScroll, [tops, layout]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      if (isTyping(e)) return;
      const el = scroller.current;
      if (!el) return;
      const single = layout === "single";
      const down = e.key === "PageDown" || (e.key === " " && !e.shiftKey);
      const up = e.key === "PageUp" || (e.key === " " && e.shiftKey);
      if (e.key === "ArrowRight" || (single && (down || e.key === "ArrowDown"))) goto(pageRef.current + 1);
      else if (e.key === "ArrowLeft" || (single && (up || e.key === "ArrowUp"))) goto(pageRef.current - 1);
      else if (down) el.scrollBy({ top: el.clientHeight * 0.9 });
      else if (up) el.scrollBy({ top: -el.clientHeight * 0.9 });
      else if (e.key === "ArrowDown") el.scrollBy({ top: 60 });
      else if (e.key === "ArrowUp") el.scrollBy({ top: -60 });
      else if (e.key === "Home") goto(0);
      else if (e.key === "End") goto(count - 1);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [goto, layout, count]);

  useEffect(() => {
    const onChange = () => {
      if (window.getSelection()?.isCollapsed) onSelect(null);
    };
    document.addEventListener("selectionchange", onChange);
    return () => document.removeEventListener("selectionchange", onChange);
  }, [onSelect]);
  const checkSelection = () => setTimeout(() => onSelect(pageSelection()), 0);

  if (count === 0) return <div className="center-msg muted">{t("reader.noPages")}</div>;

  if (layout === "single") {
    const aspect = aspects[page] ?? 1.414;
    const base = fit === "width" ? size.w - 32 : Math.min(size.w - 32, (size.h - 32) / aspect);
    const w = Math.max(120, base) * zoom;
    return (
      <div className="paged-scroller single" ref={scroller} onMouseUp={checkSelection}>
        <div className="single-stage" style={{ minWidth: w + 32, minHeight: w * aspect + 32 }}>
          {size.w > 0 && (
            <Page key={page} bookId={book.id} index={page} width={w} height={w * aspect} rotation={rotation} load />
          )}
        </div>
        <button className="page-turn left" onClick={() => goto(page - 1)} disabled={page === 0} aria-label={t("reader.prevPage")}>
          <Icon name="chevronLeft" size={30} />
        </button>
        <button className="page-turn right" onClick={() => goto(page + 1)} disabled={page >= count - 1} aria-label={t("reader.nextPage")}>
          <Icon name="chevronRight" size={30} />
        </button>
        {page + 1 < count && <img className="prefetch" src={pageUrl(book.id, page + 1, renderWidth(w), rotation)} alt="" />}
      </div>
    );
  }

  return (
    <div className="paged-scroller" ref={scroller} onScroll={onScroll} onMouseUp={checkSelection}>
      <div className="pages" style={{ height: tops[count], width: Math.max(width + 48, size.w) }}>
        {size.w > 0 &&
          aspects.map((aspect, i) =>
            // Pages far from the viewport are not mounted at all; the container keeps the height.
            i >= range[0] - 10 && i <= range[1] + 10 ? (
              <Page
                key={i}
                bookId={book.id}
                index={i}
                width={width}
                height={width * aspect}
                rotation={rotation}
                load={i >= range[0] && i <= range[1]}
                style={{ position: "absolute", top: tops[i], left: `calc(50% - ${width / 2}px)` }}
              />
            ) : null,
          )}
      </div>
    </div>
  );
}
