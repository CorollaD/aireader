// Reflowable text, one chapter at a time, either scrolled vertically or laid out in CSS columns
// and turned page by page. Renders highlights and reports the visible blocks and selections.

import {
  createContext,
  forwardRef,
  memo,
  useCallback,
  useContext,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { api, errorText, imageUrl } from "../api";
import { isTyping } from "../ui";
import { t } from "../i18n";
import { Icon } from "../icons";
import type { Settings } from "../settings";
import type { Block, Highlight, Location, OpenedBook, Span } from "../types";
import { blockText, resolveHighlights, selectionRange, splitSpans, type BlockMark, type SelectionInfo, type TextRange } from "./anchors";
import type { Jump } from "./Reader";

/** Block index meaning "the end of the chapter". */
const END = -1;
/** Vertical padding of a page in paged mode. */
const PAGE_PAD = 36;

/** Follows an internal link. */
const LinkContext = createContext<(loc: Location) => void>(() => {});

function Spans({ spans, marks }: { spans: Span[]; marks?: BlockMark[] }) {
  const follow = useContext(LinkContext);
  return (
    <>
      {splitSpans(spans, marks).map((p, i) => {
        let node: ReactNode = p.text;
        if (p.span.code) node = <code>{node}</code>;
        if (p.span.italic) node = <em>{node}</em>;
        if (p.span.bold) node = <strong>{node}</strong>;
        if (p.span.target) {
          const target = p.span.target;
          node = (
            <a
              className="link internal"
              href="#"
              onClick={(e) => {
                e.preventDefault();
                follow(target);
              }}
            >
              {node}
            </a>
          );
        } else if (p.span.link) node = <span className="link">{node}</span>;
        if (p.mark) {
          node = (
            <mark className={`hl hl-${p.mark.color}${p.mark.note ? " has-note" : ""}`} data-hl={p.mark.id}>
              {node}
            </mark>
          );
        }
        return <span key={i}>{node}</span>;
      })}
    </>
  );
}

// Memoised: a chapter can have thousands of blocks, and most re-renders change none of them.
const BlockView = memo(function BlockView({ block, bookId, marks }: { block: Block; bookId: string; marks?: BlockMark[] }) {
  switch (block.kind) {
    case "heading": {
      const Tag = `h${Math.min(Math.max(block.value.level, 1), 6)}` as "h1";
      return (
        <Tag>
          <Spans spans={block.value.spans} marks={marks} />
        </Tag>
      );
    }
    case "paragraph":
      // Plain-text books often indent with full-width spaces already.
      return (
        <p className={block.value[0]?.text.startsWith("　") ? "pre-indented" : undefined}>
          <Spans spans={block.value} marks={marks} />
        </p>
      );
    case "quote":
      return (
        <blockquote>
          <Spans spans={block.value} marks={marks} />
        </blockquote>
      );
    case "listItem":
      return (
        <div className="li" style={{ paddingLeft: `${block.value.depth * 1.5 + 1.6}em` }}>
          {block.value.marker && (
            <span className="marker" data-skip>
              {block.value.marker}
            </span>
          )}
          <Spans spans={block.value.spans} marks={marks} />
        </div>
      );
    case "code":
      return (
        <pre>
          <Spans spans={[{ text: block.value }]} marks={marks} />
        </pre>
      );
    case "image":
      return (
        <figure>
          <img src={imageUrl(bookId, block.value)} alt="" loading="lazy" decoding="async" />
        </figure>
      );
    case "rule":
      return <hr />;
  }
});

export interface FlowSelection extends SelectionInfo {
  chapter: number;
}

export interface FlowHandle {
  /** Plain text of a block in a chapter that has been loaded, for bookmark excerpts. */
  blockText(chapter: number, block: number): string | null;
}

interface Props {
  book: OpenedBook;
  jump: Jump | null;
  /** Block to emphasise briefly, e.g. a search hit. */
  flash: { chapter: number; block: number } | null;
  settings: Settings;
  highlights: Highlight[];
  /** First visible block and the last block (partly) visible. */
  onLocation: (loc: Location, lastVisible: number) => void;
  onSelect: (selection: FlowSelection | null) => void;
  onMarkClick: (id: string, rect: DOMRect) => void;
  /** Highlights whose stored position no longer matched and were found again by their text. */
  onMoved: (moved: (TextRange & { id: string })[]) => void;
  /** An internal link was clicked. */
  onLink: (loc: Location) => void;
}

function useSize(ref: React.RefObject<HTMLElement>) {
  const [size, setSize] = useState({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => setSize({ w: el.clientWidth, h: el.clientHeight });
    const ro = new ResizeObserver(update);
    ro.observe(el);
    update();
    return () => ro.disconnect();
  }, [ref]);
  return size;
}

const FlowView = forwardRef<FlowHandle, Props>(function FlowView(props, ref) {
  const { book, jump, flash, settings, highlights, onLocation, onSelect, onMarkClick, onMoved, onLink } = props;
  // Stable for the link context, so following a link re-renders no blocks.
  const linkRef = useRef(onLink);
  linkRef.current = onLink;
  const follow = useCallback((loc: Location) => linkRef.current(loc), []);
  const paged = settings.flowMode === "paged";
  const [chapter, setChapter] = useState(() => (jump?.loc.kind === "flow" ? jump.loc.chapter : 0));
  const [blocks, setBlocks] = useState<{ chapter: number; blocks: Block[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const [pageCount, setPageCount] = useState(1);
  const scroller = useRef<HTMLDivElement>(null);
  const article = useRef<HTMLElement>(null);
  const cache = useRef(new Map<number, Block[]>());
  /** Block to show once the chapter has rendered (END for the last page). */
  const pending = useRef<number | null>(null);
  /** Block kept in view across re-layouts in paged mode. */
  const anchor = useRef(0);
  const lastReported = useRef("");
  const size = useSize(scroller);

  useImperativeHandle(ref, () => ({
    blockText: (ch, b) => {
      const bl = cache.current.get(ch)?.[b];
      return bl ? blockText(bl) : null;
    },
  }));

  const goto = useCallback((ch: number, block: number) => {
    pending.current = block;
    setChapter(ch);
    // Same chapter: re-trigger the layout effect so it scrolls.
    setBlocks((b) => (b && b.chapter === ch ? { ...b } : b));
  }, []);

  useEffect(() => {
    if (jump?.loc.kind === "flow") goto(jump.loc.chapter, jump.loc.block);
  }, [jump, goto]);

  useEffect(() => {
    let cancelled = false;
    const cached = cache.current.get(chapter);
    if (cached) {
      setBlocks({ chapter, blocks: cached });
      return;
    }
    api
      .getChapter(book.id, chapter)
      .then((b) => {
        cache.current.set(chapter, b);
        if (!cancelled) setBlocks({ chapter, blocks: b });
      })
      .catch((e) => !cancelled && setError(errorText(e)));
    return () => {
      cancelled = true;
    };
  }, [book.id, chapter]);

  const shown = blocks && blocks.chapter === chapter ? blocks.blocks : null;

  const resolved = useMemo(
    () =>
      shown
        ? resolveHighlights(
            shown,
            highlights.filter((h) => h.chapter === chapter),
          )
        : null,
    [shown, highlights, chapter],
  );
  useEffect(() => {
    if (resolved?.moved.length) onMoved(resolved.moved);
  }, [resolved, onMoved]);

  const blockEls = () => [...(article.current?.querySelectorAll<HTMLElement>("[data-block]") ?? [])];

  // --- paged layout -------------------------------------------------------------------------

  // One column is capped at the text width setting and centred; the gap between pages is
  // wide enough that neighbouring pages stay outside the visible area.
  const available = Math.max(200, size.w - 2 * settings.margin);
  const pageWidth = settings.columns === 1 ? Math.min(available, settings.maxWidth) : available;
  const sideMargin = (size.w - pageWidth) / 2;
  const gap = Math.max(48, 2 * sideMargin);
  const stride = pageWidth + gap;
  const pageOf = useCallback((el: HTMLElement) => Math.max(0, Math.floor((el.offsetLeft + 1) / stride)), [stride]);

  const report = useCallback(() => {
    const el = scroller.current;
    if (!el || !shown) return;
    const els = blockEls();
    if (els.length === 0) return;
    let first: number;
    let last: number;
    if (paged) {
      const pages = els.map(pageOf);
      const startsHere = pages.findIndex((p) => p === page);
      first = startsHere >= 0 ? startsHere : Math.max(0, pages.findIndex((p) => p > page) - 1);
      if (first < 0) first = els.length - 1;
      last = pages.reduce((acc, p, i) => (p <= page ? i : acc), first);
    } else {
      const top = el.scrollTop + 12;
      const bottom = el.scrollTop + el.clientHeight;
      // Blocks are laid out top to bottom: binary search for the first one reaching below `top`.
      let lo = 0;
      let hi = els.length;
      while (lo < hi) {
        const mid = (lo + hi) >> 1;
        if (els[mid].offsetTop + els[mid].offsetHeight > top) hi = mid;
        else lo = mid + 1;
      }
      first = lo < els.length ? lo : els.length - 1;
      last = first;
      while (last + 1 < els.length && els[last + 1].offsetTop < bottom) last++;
    }
    anchor.current = first;
    const key = `${chapter}:${first}:${last}`;
    if (key !== lastReported.current) {
      lastReported.current = key;
      onLocation({ kind: "flow", chapter, block: first }, last);
    }
  }, [shown, paged, page, pageOf, chapter, onLocation]);
  // Deferred reports must use the latest closure (the page may have changed meanwhile).
  const reportRef = useRef(report);
  reportRef.current = report;

  // Position the view after a chapter loads, a jump, a re-layout or a mode switch.
  useLayoutEffect(() => {
    const el = scroller.current;
    const art = article.current;
    if (!el || !art || !shown) return;
    const target = pending.current;
    pending.current = null;
    const els = blockEls();
    if (paged) {
      const nav = art.querySelector<HTMLElement>(".chapter-nav");
      const count = Math.max(1, (nav ? pageOf(nav) : 0) + 1);
      setPageCount(count);
      let p: number;
      if (target === END) p = count - 1;
      else {
        const b = target ?? anchor.current;
        p = els[b] ? pageOf(els[b]) : 0;
      }
      setPage(Math.min(p, count - 1));
    } else if (target === END) el.scrollTop = el.scrollHeight;
    else if (target !== null) {
      el.scrollTop = target > 0 && els[target] ? els[target].offsetTop - 8 : 0;
    } else if (els[anchor.current]) {
      el.scrollTop = els[anchor.current].offsetTop - 8;
    }
    requestAnimationFrame(() => reportRef.current());
    // Typography and size changes re-layout the text, so they re-run this too.
  }, [
    shown,
    chapter,
    paged,
    stride,
    size.h,
    settings.fontSize,
    settings.lineHeight,
    settings.fontFamily,
    settings.columns,
    settings.paragraphSpacing,
    settings.maxWidth,
  ]);

  useEffect(report, [page, report]);

  const next = useCallback(() => chapter + 1 < book.chapters.length && goto(chapter + 1, 0), [chapter, book, goto]);
  const prev = useCallback(() => chapter > 0 && goto(chapter - 1, 0), [chapter, goto]);
  const nextPage = useCallback(() => {
    if (page + 1 < pageCount) setPage(page + 1);
    else next();
  }, [page, pageCount, next]);
  const prevPage = useCallback(() => {
    if (page > 0) setPage(page - 1);
    else if (chapter > 0) goto(chapter - 1, END);
  }, [page, chapter, goto]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || isTyping(e)) return;
      const el = scroller.current;
      if (!el) return;
      const down = e.key === "PageDown" || (e.key === " " && !e.shiftKey);
      const up = e.key === "PageUp" || (e.key === " " && e.shiftKey);
      if (paged) {
        if (e.key === "ArrowRight" || e.key === "ArrowDown" || down) nextPage();
        else if (e.key === "ArrowLeft" || e.key === "ArrowUp" || up) prevPage();
        else if (e.key === "Home") setPage(0);
        else if (e.key === "End") setPage(pageCount - 1);
        else return;
      } else {
        const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 2;
        if (e.key === "ArrowRight") next();
        else if (e.key === "ArrowLeft") prev();
        else if (down) {
          if (atBottom) next();
          else el.scrollBy({ top: el.clientHeight * 0.9 });
        } else if (up) {
          if (el.scrollTop <= 0 && chapter > 0) goto(chapter - 1, END);
          else el.scrollBy({ top: -el.clientHeight * 0.9 });
        } else if (e.key === "ArrowDown") el.scrollBy({ top: 60 });
        else if (e.key === "ArrowUp") el.scrollBy({ top: -60 });
        else if (e.key === "Home") el.scrollTop = 0;
        else if (e.key === "End") el.scrollTop = el.scrollHeight;
        else return;
      }
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [paged, next, prev, nextPage, prevPage, pageCount, goto, chapter]);

  // Wheel turns pages in paged mode (with a short cooldown so one gesture is one page).
  const wheel = useRef({ acc: 0, until: 0 });
  const onWheel = (e: React.WheelEvent) => {
    if (!paged) return;
    const w = wheel.current;
    if (Date.now() < w.until) return;
    w.acc += Math.abs(e.deltaY) > Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
    if (Math.abs(w.acc) > 60) {
      if (w.acc > 0) nextPage();
      else prevPage();
      w.acc = 0;
      w.until = Date.now() + 350;
    }
  };

  const scrollRaf = useRef(0);
  const onScroll = () => {
    cancelAnimationFrame(scrollRaf.current);
    scrollRaf.current = requestAnimationFrame(() => reportRef.current());
  };

  // Selections are reported after the mouse or keyboard finishes changing them.
  const checkSelection = useCallback(() => {
    if (!article.current || !shown) return;
    const sel = selectionRange(article.current, shown);
    onSelect(sel ? { ...sel, chapter } : null);
  }, [shown, chapter, onSelect]);
  useEffect(() => {
    const onChange = () => {
      if (window.getSelection()?.isCollapsed) onSelect(null);
    };
    document.addEventListener("selectionchange", onChange);
    return () => document.removeEventListener("selectionchange", onChange);
  }, [onSelect]);

  const onClick = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest("a.internal")) return;
    const mark = target.closest<HTMLElement>("mark[data-hl]");
    if (mark && window.getSelection()?.isCollapsed) {
      e.stopPropagation();
      onMarkClick(mark.dataset.hl!, mark.getBoundingClientRect());
      return;
    }
    // Paged mode: clicking the outer thirds turns the page.
    if (!paged || !window.getSelection()?.isCollapsed || target.closest("button, a, img")) return;
    const rect = scroller.current!.getBoundingClientRect();
    const x = (e.clientX - rect.left) / rect.width;
    if (x < 0.3) prevPage();
    else if (x > 0.7) nextPage();
  };

  const flashBlock = flash && flash.chapter === chapter ? flash.block : -2;
  const classes = ["flow", paged ? "paged" : "", settings.indent ? "indent" : "", settings.justify ? "justify" : ""].join(" ");
  const style: React.CSSProperties & Record<string, string | number> = {
    fontSize: settings.fontSize,
    lineHeight: settings.lineHeight,
    "--para-gap": `${settings.paragraphSpacing}em`,
  };
  if (paged) {
    const height = Math.max(200, size.h - 2 * PAGE_PAD);
    Object.assign(style, {
      width: pageWidth,
      height,
      margin: `${PAGE_PAD}px ${sideMargin}px`,
      columnCount: settings.columns,
      columnGap: gap,
      transform: `translateX(${-page * stride}px)`,
      "--page-h": `${height}px`,
    });
  } else {
    Object.assign(style, { maxWidth: settings.maxWidth + 2 * settings.margin, paddingLeft: settings.margin, paddingRight: settings.margin });
  }

  return (
    <div
      className={`flow-scroller${paged ? " paged" : ""}`}
      ref={scroller}
      onScroll={onScroll}
      onWheel={onWheel}
      onClick={onClick}
      onMouseMove={(e) => {
        if (!paged) return;
        const r = e.currentTarget.getBoundingClientRect();
        const x = (e.clientX - r.left) / r.width;
        e.currentTarget.dataset.zone = e.buttons ? "" : x < 0.3 ? "prev" : x > 0.7 ? "next" : "";
      }}
      onMouseLeave={(e) => (e.currentTarget.dataset.zone = "")}
      onMouseUp={() => setTimeout(checkSelection, 0)}
      onKeyUp={(e) => e.shiftKey && checkSelection()}
    >
      {error && <div className="center-msg error">{error}</div>}
      {!shown && !error && <div className="center-msg muted">{t("common.loading")}</div>}
      {/* Paged mode: the outer thirds turn the page; a faint arrow shows it on hover. */}
      {paged && shown && (
        <>
          <div className="turn-zone prev" aria-hidden>
            <Icon name="chevronLeft" />
          </div>
          <div className="turn-zone next" aria-hidden>
            <Icon name="chevronRight" />
          </div>
        </>
      )}
      {shown && (
        <article className={classes} ref={article} style={style}>
          <LinkContext.Provider value={follow}>
            {shown.map((b, i) => (
              <div key={i} data-block={i} className={i === flashBlock ? "flash" : undefined}>
                <BlockView block={b} bookId={book.id} marks={resolved?.marks.get(i)} />
              </div>
            ))}
          </LinkContext.Provider>
          <nav className="chapter-nav">
            {chapter > 0 ? (
              <button onClick={prev}>
                <Icon name="chevronLeft" /> {t("reader.prevChapter")}
              </button>
            ) : (
              <span />
            )}
            <span className="muted small">
              {chapter + 1} / {book.chapters.length}
            </span>
            {chapter + 1 < book.chapters.length ? (
              <button onClick={next}>
                {t("reader.nextChapter")} <Icon name="chevronRight" />
              </button>
            ) : (
              <span className="muted small">{t("reader.theEnd")}</span>
            )}
          </nav>
        </article>
      )}
      {paged && shown && (
        <div className="page-indicator muted small">
          {page + 1} / {pageCount}
        </div>
      )}
    </div>
  );
});

export default FlowView;
