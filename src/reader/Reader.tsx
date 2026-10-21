import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, errorText } from "../api";
import type { UpdateSettings } from "../App";
import type { Settings } from "../settings";
import type { Hit, Location, OpenedBook } from "../types";
import FlowView from "./FlowView";
import PagedView from "./PagedView";
import SettingsPanel from "./SettingsPanel";
import { isTyping } from "../ui";

/** A navigation request; `n` makes repeated jumps to the same place distinct. */
export interface Jump {
  loc: Location;
  n: number;
}

const order = (l: Location): [number, number] => (l.kind === "flow" ? [l.chapter, l.block] : [l.page, 0]);
const before = (a: Location, b: Location) => {
  const [x, y] = [order(a), order(b)];
  return x[0] < y[0] || (x[0] === y[0] && x[1] <= y[1]);
};

function progressOf(book: OpenedBook, loc: Location): number {
  if (loc.kind === "page") return book.pages.length <= 1 ? 1 : loc.page / (book.pages.length - 1);
  const blocks = Math.max(1, book.chapters[loc.chapter]?.blocks ?? 1);
  return Math.min(1, (loc.chapter + loc.block / blocks) / Math.max(1, book.chapters.length));
}

export default function Reader(props: { bookId: string; settings: Settings; update: UpdateSettings; onClose: () => void }) {
  const { settings, update } = props;
  const [book, setBook] = useState<OpenedBook | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [location, setLocation] = useState<Location | null>(null);
  const [jump, setJump] = useState<Jump | null>(null);
  const [panel, setPanel] = useState(false);
  const [search, setSearch] = useState<{ open: boolean; query: string; hits: Hit[] | null; busy: boolean }>({
    open: false,
    query: "",
    hits: null,
    busy: false,
  });
  const [highlight, setHighlight] = useState<{ chapter: number; block: number } | null>(null);
  const [zoom, setZoom] = useState(1);
  const searchInput = useRef<HTMLInputElement>(null);
  const tocList = useRef<HTMLDivElement>(null);
  const pendingSave = useRef<{ loc: Location; progress: number } | null>(null);
  const saveTimer = useRef<number>();

  useEffect(() => {
    api
      .openBook(props.bookId)
      .then((b) => {
        setBook(b);
        const start: Location = b.position ?? (b.kind === "flow" ? { kind: "flow", chapter: 0, block: 0 } : { kind: "page", page: 0 });
        // Guard against stale positions from an earlier, different parse.
        const valid = start.kind === "flow" ? b.kind === "flow" && start.chapter < b.chapters.length : b.kind === "paged" && start.page < b.pages.length;
        setJump({ loc: valid ? start : b.kind === "flow" ? { kind: "flow", chapter: 0, block: 0 } : { kind: "page", page: 0 }, n: 0 });
        getCurrentWindow().setTitle(`${b.title} — AiReader`).catch(() => {});
      })
      .catch((e) => setError(errorText(e)));
  }, [props.bookId]);

  const flush = useCallback(() => {
    window.clearTimeout(saveTimer.current);
    const p = pendingSave.current;
    pendingSave.current = null;
    if (p) api.saveProgress(props.bookId, p.loc, p.progress).catch(() => {});
  }, [props.bookId]);
  useEffect(() => flush, [flush]);

  const onLocation = useCallback(
    (loc: Location) => {
      setLocation(loc);
      if (!book) return;
      pendingSave.current = { loc, progress: progressOf(book, loc) };
      window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(flush, 800);
    },
    [book, flush],
  );

  const go = useCallback((loc: Location) => setJump((j) => ({ loc, n: (j?.n ?? 0) + 1 })), []);

  const currentToc = useMemo(() => {
    if (!book || !location) return -1;
    let idx = -1;
    book.toc.forEach((e, i) => {
      if (before(e.location, location)) idx = i;
    });
    return idx;
  }, [book, location]);

  useEffect(() => {
    tocList.current?.querySelector(".toc-item.active")?.scrollIntoView({ block: "nearest" });
  }, [currentToc, settings.showToc]);

  const runSearch = async () => {
    if (!search.query.trim()) return;
    setSearch((s) => ({ ...s, busy: true }));
    try {
      const hits = await api.searchBook(props.bookId, search.query);
      setSearch((s) => ({ ...s, hits, busy: false }));
    } catch (e) {
      setSearch((s) => ({ ...s, hits: [], busy: false }));
      setError(errorText(e));
    }
  };

  const isFlow = book?.kind === "flow";

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cmd = e.metaKey || e.ctrlKey;
      const typing = isTyping(e);
      if (cmd && e.key.toLowerCase() === "f" && isFlow) {
        e.preventDefault();
        setSearch((s) => ({ ...s, open: true }));
        setTimeout(() => searchInput.current?.focus(), 0);
      } else if (cmd && (e.key === "=" || e.key === "+")) {
        e.preventDefault();
        if (isFlow) update({ fontSize: Math.min(36, settings.fontSize + 1) });
        else setZoom((z) => Math.min(5, z * 1.2));
      } else if (cmd && e.key === "-") {
        e.preventDefault();
        if (isFlow) update({ fontSize: Math.max(13, settings.fontSize - 1) });
        else setZoom((z) => Math.max(0.3, z / 1.2));
      } else if (e.key === "Escape") {
        if (panel) setPanel(false);
        else if (search.open) setSearch((s) => ({ ...s, open: false }));
        else if (!typing) props.onClose();
      } else if (!typing && !cmd && e.key.toLowerCase() === "t") {
        update({ showToc: !settings.showToc });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isFlow, settings.fontSize, settings.showToc, panel, search.open, update, props.onClose]);

  if (error && !book) {
    return (
      <div className="reader">
        <div className="center-msg">
          <p className="error">{error}</p>
          <button onClick={props.onClose}>返回书架</button>
        </div>
      </div>
    );
  }
  if (!book) return <div className="reader center-msg muted">正在打开…</div>;

  const progress = location ? progressOf(book, location) : 0;
  const chapterTitle = location?.kind === "flow" ? book.chapters[location.chapter]?.title : null;

  return (
    <div className="reader" onClick={() => setPanel(false)}>
      <header className="reader-bar">
        <button onClick={props.onClose} title="返回书架 (Esc)">
          ← 书架
        </button>
        {book.toc.length > 0 && (
          <button className={settings.showToc ? "on" : ""} onClick={() => update({ showToc: !settings.showToc })} title="目录 (T)">
            目录
          </button>
        )}
        <div className="reader-title ellipsis">
          <strong>{book.title}</strong>
          {chapterTitle && <span className="muted"> · {chapterTitle}</span>}
        </div>
        {book.kind === "paged" && location?.kind === "page" && (
          <div className="page-controls">
            <input
              className="page-input"
              key={location.page}
              defaultValue={location.page + 1}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  const n = parseInt((e.target as HTMLInputElement).value, 10);
                  if (n > 0) go({ kind: "page", page: n - 1 });
                  (e.target as HTMLInputElement).blur();
                }
              }}
            />
            <span className="muted">/ {book.pages.length}</span>
            <button onClick={() => setZoom((z) => Math.max(0.3, z / 1.2))} title="缩小">
              －
            </button>
            <span className="zoom">{Math.round(zoom * 100)}%</span>
            <button onClick={() => setZoom((z) => Math.min(5, z * 1.2))} title="放大">
              ＋
            </button>
            <div className="segmented">
              <button className={settings.pageLayout === "continuous" ? "on" : ""} onClick={() => update({ pageLayout: "continuous" })}>
                连续
              </button>
              <button className={settings.pageLayout === "single" ? "on" : ""} onClick={() => update({ pageLayout: "single" })}>
                单页
              </button>
            </div>
          </div>
        )}
        <span className="muted progress-label">{Math.round(progress * 100)}%</span>
        {isFlow && (
          <button
            className={search.open ? "on" : ""}
            title="全文搜索 (Ctrl/⌘+F)"
            onClick={() => {
              setSearch((s) => ({ ...s, open: !s.open }));
              setTimeout(() => searchInput.current?.focus(), 0);
            }}
          >
            搜索
          </button>
        )}
        <div className="panel-anchor">
          <button
            className={panel ? "on" : ""}
            onClick={(e) => {
              e.stopPropagation();
              setPanel((p) => !p);
            }}
            title="阅读设置"
          >
            Aa
          </button>
          {panel && <SettingsPanel settings={settings} update={update} textOptions={isFlow} />}
        </div>
      </header>

      <div className="reader-body">
        {settings.showToc && book.toc.length > 0 && (
          <aside className="toc" ref={tocList}>
            {book.toc.map((e, i) => (
              <button
                key={i}
                className={`toc-item${i === currentToc ? " active" : ""}`}
                style={{ paddingLeft: 12 + e.depth * 14 }}
                onClick={() => {
                  setHighlight(null);
                  go(e.location);
                }}
                title={e.title}
              >
                {e.title}
              </button>
            ))}
          </aside>
        )}
        <section className="reader-view">
          {isFlow ? (
            <FlowView book={book} jump={jump} highlight={highlight} settings={settings} onLocation={onLocation} />
          ) : (
            <PagedView book={book} jump={jump} layout={settings.pageLayout} zoom={zoom} onLocation={onLocation} />
          )}
          <div className="reader-progress">
            <div style={{ width: `${progress * 100}%` }} />
          </div>
        </section>
        {search.open && isFlow && (
          <aside className="search-panel">
            <input
              ref={searchInput}
              className="full"
              placeholder="搜索全文，回车确认"
              value={search.query}
              onChange={(e) => setSearch((s) => ({ ...s, query: e.target.value }))}
              onKeyDown={(e) => e.key === "Enter" && runSearch()}
            />
            <div className="muted small search-status">
              {search.busy ? "搜索中…" : search.hits ? `找到 ${search.hits.length}${search.hits.length >= 1000 ? "+" : ""} 处` : ""}
            </div>
            <div className="search-hits">
              {search.hits?.map((h, i) => (
                <button
                  key={i}
                  className="hit"
                  onClick={() => {
                    setHighlight({ chapter: h.chapter, block: h.block });
                    go({ kind: "flow", chapter: h.chapter, block: h.block });
                  }}
                >
                  <span className="hit-chapter">{book.chapters[h.chapter]?.title || `第 ${h.chapter + 1} 章`}</span>
                  <span>{h.snippet}</span>
                </button>
              ))}
            </div>
          </aside>
        )}
      </div>
    </div>
  );
}
