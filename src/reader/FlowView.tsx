// Reflowable text: one chapter at a time, with top-visible-block tracking for progress.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { api, errorText, imageUrl } from "../api";
import { isTyping } from "../ui";
import type { Settings } from "../settings";
import type { Block, Location, OpenedBook, Span } from "../types";
import type { Jump } from "./Reader";

/** Block index meaning "scroll to the end of the chapter". */
const END = -1;

function Spans({ spans }: { spans: Span[] }) {
  return (
    <>
      {spans.map((s, i) => {
        let node: ReactNode = s.text;
        if (s.code) node = <code>{node}</code>;
        if (s.italic) node = <em>{node}</em>;
        if (s.bold) node = <strong>{node}</strong>;
        if (s.link) node = <span className="link">{node}</span>;
        return <span key={i}>{node}</span>;
      })}
    </>
  );
}

function BlockView({ block, bookId }: { block: Block; bookId: string }) {
  switch (block.kind) {
    case "heading": {
      const Tag = `h${Math.min(Math.max(block.value.level, 1), 6)}` as "h1";
      return (
        <Tag>
          <Spans spans={block.value.spans} />
        </Tag>
      );
    }
    case "paragraph":
      return (
        <p>
          <Spans spans={block.value} />
        </p>
      );
    case "quote":
      return (
        <blockquote>
          <Spans spans={block.value} />
        </blockquote>
      );
    case "listItem":
      return (
        <div className="li" style={{ paddingLeft: `${block.value.depth * 1.5 + 1.6}em` }}>
          {block.value.marker && <span className="marker">{block.value.marker}</span>}
          <Spans spans={block.value.spans} />
        </div>
      );
    case "code":
      return <pre>{block.value}</pre>;
    case "image":
      return (
        <figure>
          <img src={imageUrl(bookId, block.value)} alt="" loading="lazy" decoding="async" />
        </figure>
      );
    case "rule":
      return <hr />;
  }
}

interface Props {
  book: OpenedBook;
  jump: Jump | null;
  highlight: { chapter: number; block: number } | null;
  settings: Settings;
  onLocation: (loc: Location) => void;
}

export default function FlowView({ book, jump, highlight, settings, onLocation }: Props) {
  const [chapter, setChapter] = useState(() => (jump?.loc.kind === "flow" ? jump.loc.chapter : 0));
  const [blocks, setBlocks] = useState<{ chapter: number; blocks: Block[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const article = useRef<HTMLElement>(null);
  const cache = useRef(new Map<number, Block[]>());
  /** Block to scroll to once the chapter has rendered. */
  const pending = useRef<number | null>(null);
  const lastReported = useRef<string>("");

  const goto = useCallback((ch: number, block: number) => {
    pending.current = block;
    setChapter(ch);
    // Same chapter: the layout effect will not re-run, so scroll right away.
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

  const report = useCallback(() => {
    const el = scroller.current;
    const art = article.current;
    if (!el || !art || !blocks) return;
    const children = art.querySelectorAll<HTMLElement>("[data-block]");
    const probe = el.scrollTop + 12;
    // Binary search for the first block whose bottom is below the probe line.
    let lo = 0;
    let hi = children.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      const c = children[mid];
      if (c.offsetTop + c.offsetHeight > probe) hi = mid;
      else lo = mid + 1;
    }
    const loc: Location = { kind: "flow", chapter: blocks.chapter, block: Math.max(0, lo) };
    const key = `${loc.chapter}:${loc.block}`;
    if (key !== lastReported.current) {
      lastReported.current = key;
      onLocation(loc);
    }
  }, [blocks, onLocation]);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el || !blocks || blocks.chapter !== chapter) return;
    const target = pending.current;
    pending.current = null;
    if (target === END) el.scrollTop = el.scrollHeight;
    else if (target !== null && target > 0) {
      const node = el.querySelector<HTMLElement>(`[data-block="${target}"]`);
      el.scrollTop = node ? node.offsetTop - 8 : 0;
    } else if (target === 0) el.scrollTop = 0;
    report();
  }, [blocks, chapter, report]);

  const next = useCallback(() => chapter + 1 < book.chapters.length && goto(chapter + 1, 0), [chapter, book, goto]);
  const prev = useCallback(() => chapter > 0 && goto(chapter - 1, 0), [chapter, goto]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      if (isTyping(e)) return;
      const el = scroller.current;
      if (!el) return;
      const page = el.clientHeight * 0.9;
      const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 2;
      const down = e.key === "PageDown" || (e.key === " " && !e.shiftKey);
      const up = e.key === "PageUp" || (e.key === " " && e.shiftKey);
      if (e.key === "ArrowRight") next();
      else if (e.key === "ArrowLeft") prev();
      else if (down) {
        if (atBottom) next();
        else el.scrollBy({ top: page });
      } else if (up) {
        if (el.scrollTop <= 0 && chapter > 0) goto(chapter - 1, END);
        else el.scrollBy({ top: -page });
      } else if (e.key === "ArrowDown") el.scrollBy({ top: 60 });
      else if (e.key === "ArrowUp") el.scrollBy({ top: -60 });
      else if (e.key === "Home") el.scrollTop = 0;
      else if (e.key === "End") el.scrollTop = el.scrollHeight;
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [next, prev, goto, chapter]);

  const scrollRaf = useRef(0);
  const onScroll = () => {
    cancelAnimationFrame(scrollRaf.current);
    scrollRaf.current = requestAnimationFrame(report);
  };

  const style = {
    fontSize: settings.fontSize,
    lineHeight: settings.lineHeight,
    maxWidth: settings.maxWidth,
  };
  const shown = blocks && blocks.chapter === chapter ? blocks.blocks : null;
  const hl = highlight && highlight.chapter === chapter ? highlight.block : -2;

  return (
    <div className="flow-scroller" ref={scroller} onScroll={onScroll}>
      {error && <div className="center-msg error">{error}</div>}
      {!shown && !error && <div className="center-msg muted">加载中…</div>}
      {shown && (
        <article className="flow" ref={article} style={style}>
          {shown.map((b, i) => (
            <div key={i} data-block={i} className={i === hl ? "hl" : undefined}>
              <BlockView block={b} bookId={book.id} />
            </div>
          ))}
          <nav className="chapter-nav">
            {chapter > 0 ? <button onClick={prev}>← 上一章</button> : <span />}
            <span className="muted small">
              {chapter + 1} / {book.chapters.length}
            </span>
            {chapter + 1 < book.chapters.length ? <button onClick={next}>下一章 →</button> : <span className="muted small">全书完</span>}
          </nav>
        </article>
      )}
    </div>
  );
}
