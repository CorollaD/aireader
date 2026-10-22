// Fixed-layout pages (PDF, comics), rendered by the backend through `book://` URLs.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { pageUrl } from "../api";
import { isTyping } from "../ui";
import type { Location, OpenedBook } from "../types";
import type { Jump } from "./Reader";

const GAP = 14;
/** Pages within this many screens of the viewport get their images loaded. */
const PRELOAD_SCREENS = 1.5;

/** Render width in device pixels, rounded up so small resizes reuse cached images. */
function renderWidth(cssWidth: number): number {
  const px = cssWidth * window.devicePixelRatio;
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

/** A page box; the rendered image, once loaded, covers the page-number placeholder. */
function Page(props: { bookId: string; index: number; width: number; height: number; load: boolean; style?: React.CSSProperties }) {
  return (
    <div className="page" style={{ width: props.width, height: props.height, ...props.style }}>
      <span className="page-number">{props.index + 1}</span>
      {props.load && (
        <img src={pageUrl(props.bookId, props.index, renderWidth(props.width))} alt="" draggable={false} decoding="async" />
      )}
    </div>
  );
}

interface Props {
  book: OpenedBook;
  jump: Jump | null;
  layout: "continuous" | "single";
  zoom: number;
  onLocation: (loc: Location) => void;
}

export default function PagedView({ book, jump, layout, zoom, onLocation }: Props) {
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

  const width = Math.max(120, Math.min(size.w - 48, 1000) * zoom);
  const tops = useMemo(() => {
    const t: number[] = [];
    let y = GAP;
    for (const aspect of book.pages) {
      t.push(y);
      y += width * aspect + GAP;
    }
    t.push(y);
    return t;
  }, [book.pages, width]);

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

  useEffect(() => onLocation({ kind: "page", page }), [page, onLocation]);

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

  if (count === 0) return <div className="center-msg muted">没有页面</div>;

  if (layout === "single") {
    const aspect = book.pages[page] ?? 1.414;
    const fit = Math.max(120, Math.min(size.w - 32, (size.h - 32) / aspect));
    const w = fit * zoom;
    return (
      <div className="paged-scroller single" ref={scroller}>
        <div className="single-stage" style={{ minWidth: w + 32, minHeight: w * aspect + 32 }}>
          {size.w > 0 && <Page key={page} bookId={book.id} index={page} width={w} height={w * aspect} load />}
        </div>
        <button className="page-turn left" onClick={() => goto(page - 1)} disabled={page === 0} aria-label="上一页">
          ‹
        </button>
        <button className="page-turn right" onClick={() => goto(page + 1)} disabled={page >= count - 1} aria-label="下一页">
          ›
        </button>
        {page + 1 < count && <img className="prefetch" src={pageUrl(book.id, page + 1, renderWidth(w))} alt="" />}
      </div>
    );
  }

  return (
    <div className="paged-scroller" ref={scroller} onScroll={onScroll}>
      <div className="pages" style={{ height: tops[count], width: Math.max(width + 48, size.w) }}>
        {size.w > 0 &&
          book.pages.map((aspect, i) =>
            // Pages far from the viewport are not mounted at all; the container keeps the height.
            i >= range[0] - 10 && i <= range[1] + 10 ? (
              <Page
                key={i}
                bookId={book.id}
                index={i}
                width={width}
                height={width * aspect}
                load={i >= range[0] && i <= range[1]}
                style={{ position: "absolute", top: tops[i], left: `calc(50% - ${width / 2}px)` }}
              />
            ) : null,
          )}
      </div>
    </div>
  );
}
