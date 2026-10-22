import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, errorText } from "../api";
import type { UpdateSettings } from "../App";
import type { Settings } from "../settings";
import type { Hit, HighlightColor, Location, OpenedBook } from "../types";
import { isTyping, Modal, Toast } from "../ui";
import AiPanel, { type ChatRequest } from "../ai/AiPanel";
import { useAi } from "../ai/AiContext";
import type { ChatAction } from "../types";
import { NoteEditor, SelectionToolbar } from "./Annotate";
import FlowView, { type FlowHandle, type FlowSelection } from "./FlowView";
import { describe, isValid, progressOf, sameLocation, START } from "./locations";
import PagedView, { pageText, type PageSelection } from "./PagedView";
import ProgressBar from "./ProgressBar";
import SettingsPanel from "./SettingsPanel";
import { SHORTCUTS } from "./shortcuts";
import Sidebar, { type SidebarTab } from "./Sidebar";
import { useAnnotations } from "./useAnnotations";
import { useReadingStats } from "./useReadingStats";

/** A navigation request; `n` makes repeated jumps to the same place distinct. */
export interface Jump {
  loc: Location;
  n: number;
}

const HISTORY_LIMIT = 50;

async function toggleFullscreen() {
  const win = getCurrentWindow();
  await win.setFullscreen(!(await win.isFullscreen()));
}

export default function Reader(props: { bookId: string; settings: Settings; update: UpdateSettings; onClose: () => void }) {
  const { settings, update, bookId } = props;
  const [book, setBook] = useState<OpenedBook | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [location, setLocation] = useState<Location | null>(null);
  const [lastVisible, setLastVisible] = useState(0);
  const [jump, setJump] = useState<Jump | null>(null);
  const [panel, setPanel] = useState(false);
  const [search, setSearch] = useState<{ open: boolean; query: string; hits: Hit[] | null; busy: boolean }>({
    open: false,
    query: "",
    hits: null,
    busy: false,
  });
  const [flash, setFlash] = useState<{ chapter: number; block: number } | null>(null);
  const [zoom, setZoom] = useState(1);
  const [rotation, setRotation] = useState(0);
  const [sideTab, setSideTab] = useState<SidebarTab>("toc");
  const [focus, setFocus] = useState(false);
  const [help, setHelp] = useState(false);
  const [selection, setSelection] = useState<FlowSelection | null>(null);
  const [activeMark, setActiveMark] = useState<{ id: string; rect: DOMRect } | null>(null);
  const [noteFor, setNoteFor] = useState<string | null>(null);
  const [history, setHistory] = useState<{ back: Location[]; forward: Location[] }>({ back: [], forward: [] });
  const [toast, setToast] = useState<string | null>(null);
  const [aiOpen, setAiOpen] = useState(false);
  const [chatRequest, setChatRequest] = useState<ChatRequest | null>(null);
  const [pageSel, setPageSel] = useState<PageSelection | null>(null);
  const ai = useAi();
  const searchInput = useRef<HTMLInputElement>(null);
  const flowRef = useRef<FlowHandle>(null);
  const pendingSave = useRef<{ loc: Location; progress: number } | null>(null);
  const saveTimer = useRef<number>();

  const ann = useAnnotations(bookId);
  const stats = useReadingStats(book, location);

  useEffect(() => {
    api
      .openBook(bookId)
      .then((b) => {
        setBook(b);
        setRotation(b.rotation);
        const start = b.position && isValid(b, b.position) ? b.position : START[b.kind];
        setJump({ loc: start, n: 0 });
        getCurrentWindow().setTitle(`${b.title} — AiReader`).catch(() => {});
      })
      .catch((e) => setError(errorText(e)));
  }, [bookId]);

  const flush = useCallback(() => {
    window.clearTimeout(saveTimer.current);
    const p = pendingSave.current;
    pendingSave.current = null;
    if (p) api.saveProgress(bookId, p.loc, p.progress).catch(() => {});
  }, [bookId]);
  useEffect(() => flush, [flush]);

  const onLocation = useCallback(
    (loc: Location, last: number) => {
      setLocation(loc);
      setLastVisible(last);
      if (!book) return;
      pendingSave.current = { loc, progress: progressOf(book, loc) };
      window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(flush, 800);
    },
    [book, flush],
  );

  const go = useCallback((loc: Location) => setJump((j) => ({ loc, n: (j?.n ?? 0) + 1 })), []);

  /** Jumps to `loc`, remembering the current position for "back". */
  const navigate = useCallback(
    (loc: Location, flashBlock = false) => {
      if (location && !sameLocation(location, loc)) {
        setHistory((h) => ({ back: [...h.back, location].slice(-HISTORY_LIMIT), forward: [] }));
      }
      setFlash(flashBlock && loc.kind === "flow" ? { chapter: loc.chapter, block: loc.block } : null);
      go(loc);
    },
    [location, go],
  );

  const back = useCallback(() => {
    const target = history.back[history.back.length - 1];
    if (!target) return;
    setHistory({ back: history.back.slice(0, -1), forward: location ? [...history.forward, location] : history.forward });
    go(target);
  }, [history, location, go]);

  const forward = useCallback(() => {
    const target = history.forward[history.forward.length - 1];
    if (!target) return;
    setHistory({ back: location ? [...history.back, location] : history.back, forward: history.forward.slice(0, -1) });
    go(target);
  }, [history, location, go]);

  // --- bookmarks -----------------------------------------------------------------------------

  /** The bookmark inside the visible part of the page, if any. */
  const currentBookmark = useMemo(() => {
    if (!location) return null;
    return (
      ann.data.bookmarks.find((b) => {
        const l = b.location;
        if (l.kind === "flow" && location.kind === "flow") {
          return l.chapter === location.chapter && l.block >= location.block && l.block <= lastVisible;
        }
        return l.kind === "page" && location.kind === "page" && l.page >= location.page && l.page <= lastVisible;
      }) ?? null
    );
  }, [ann.data.bookmarks, location, lastVisible]);

  const toggleBookmark = useCallback(() => {
    if (!book || !location) return;
    if (currentBookmark) {
      ann.removeBookmark(currentBookmark.id);
      setToast("已删除书签");
      return;
    }
    const excerpt =
      location.kind === "flow" ? (flowRef.current?.blockText(location.chapter, location.block) ?? "").trim().slice(0, 100) : "";
    ann.addBookmark({ location, title: describe(book, location), excerpt });
    setToast("已添加书签");
  }, [book, location, currentBookmark, ann]);

  // --- highlights ----------------------------------------------------------------------------

  const clearSelection = () => {
    window.getSelection()?.removeAllRanges();
    setSelection(null);
  };

  const highlightSelection = (color: HighlightColor) => {
    if (!selection) return null;
    const { chapter, startBlock, start, endBlock, end, text } = selection;
    const h = ann.addHighlight({ chapter, startBlock, start, endBlock, end, text, color, note: null });
    clearSelection();
    return h;
  };

  const { updateHighlight } = ann;
  const onMoved = useCallback(
    (moved: { id: string; startBlock: number; start: number; endBlock: number; end: number }[]) => {
      for (const { id, ...range } of moved) updateHighlight(id, range);
    },
    [updateHighlight],
  );
  const onSelect = useCallback((sel: FlowSelection | null) => {
    setSelection(sel);
    if (sel) setActiveMark(null);
  }, []);
  const onMarkClick = useCallback((id: string, rect: DOMRect) => setActiveMark({ id, rect }), []);

  const copy = (text: string) => {
    navigator.clipboard.writeText(text).then(
      () => setToast("已复制"),
      () => setToast("复制失败"),
    );
  };

  const openChat = (req: Omit<ChatRequest, "n">) => {
    setChatRequest((r) => ({ ...req, n: (r?.n ?? 0) + 1 }));
    setAiOpen(true);
    setSearch((s) => ({ ...s, open: false }));
  };

  /** Sends the current text selection to the AI panel. */
  const askAi = (action: ChatAction) => {
    if (!selection) return;
    const { chapter, startBlock, start, endBlock, end, text } = selection;
    // Context: the selected paragraphs plus one on each side, capped.
    const context: string[] = [];
    for (let b = Math.max(0, startBlock - 1); b <= endBlock + 1; b++) {
      const t = flowRef.current?.blockText(chapter, b);
      if (t) context.push(t);
    }
    openChat({ anchor: { chapter, range: { startBlock, start, endBlock, end } }, selection: text, context: context.join("\n").slice(0, 2000), action });
    clearSelection();
  };

  /** Sends a PDF selection to the AI panel, with the page's text as context. */
  const askAiPage = async (action: ChatAction) => {
    if (!pageSel) return;
    const { page, text } = pageSel;
    window.getSelection()?.removeAllRanges();
    setPageSel(null);
    const runs = await pageText(bookId, page);
    const context = runs.map((r) => r.text).join(" ").slice(0, 2000);
    openChat({ anchor: null, selection: text, context, action });
  };

  /** Stores an AI answer as the note of a highlight on the asked-about passage. */
  const saveAiNote = (req: ChatRequest, answer: string) => {
    if (!req.anchor) return;
    const { chapter, range: r } = req.anchor;
    const existing = ann.data.highlights.find(
      (h) => h.chapter === chapter && h.startBlock === r.startBlock && h.start === r.start && h.endBlock === r.endBlock && h.end === r.end,
    );
    if (existing) {
      ann.updateHighlight(existing.id, { note: existing.note ? `${existing.note}\n\n${answer}` : answer });
    } else {
      ann.addHighlight({ chapter, ...r, text: req.selection, color: "blue", note: answer });
    }
  };

  const exportNotes = async () => {
    try {
      const path = await api.exportNotes(bookId);
      if (path) setToast(`已导出到 ${path}`);
    } catch (e) {
      setToast(errorText(e));
    }
  };

  // --- search --------------------------------------------------------------------------------

  const runSearch = async () => {
    if (!search.query.trim()) return;
    setSearch((s) => ({ ...s, busy: true }));
    try {
      const hits = await api.searchBook(bookId, search.query);
      setSearch((s) => ({ ...s, hits, busy: false }));
    } catch (e) {
      setSearch((s) => ({ ...s, hits: [], busy: false }));
      setToast(errorText(e));
    }
  };

  const isFlow = book?.kind === "flow";

  const rotate = () => {
    const next = (rotation + 90) % 360;
    setRotation(next);
    api.updateBook(bookId, { rotation: next }).catch(() => {});
  };

  // --- keyboard ------------------------------------------------------------------------------

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cmd = e.metaKey || e.ctrlKey;
      const typing = isTyping(e);
      const key = e.key.toLowerCase();
      if (cmd && key === "f" && e.ctrlKey && e.metaKey) {
        e.preventDefault();
        toggleFullscreen().catch(() => {});
      } else if (e.key === "F11") {
        e.preventDefault();
        toggleFullscreen().catch(() => {});
      } else if (cmd && key === "f" && isFlow) {
        e.preventDefault();
        setSearch((s) => ({ ...s, open: true }));
        setTimeout(() => searchInput.current?.focus(), 0);
      } else if (cmd && key === "d") {
        e.preventDefault();
        toggleBookmark();
      } else if (cmd && (e.key === "=" || e.key === "+")) {
        e.preventDefault();
        if (isFlow) update({ fontSize: Math.min(36, settings.fontSize + 1) });
        else setZoom((z) => Math.min(5, z * 1.2));
      } else if (cmd && e.key === "-") {
        e.preventDefault();
        if (isFlow) update({ fontSize: Math.max(13, settings.fontSize - 1) });
        else setZoom((z) => Math.max(0.3, z / 1.2));
      } else if ((e.altKey && e.key === "ArrowLeft") || (cmd && e.key === "[")) {
        e.preventDefault();
        back();
      } else if ((e.altKey && e.key === "ArrowRight") || (cmd && e.key === "]")) {
        e.preventDefault();
        forward();
      } else if (e.key === "Escape") {
        // Open dialogs handle Escape themselves.
        if (document.querySelector(".modal-backdrop")) return;
        if (selection || activeMark || pageSel) {
          clearSelection();
          setActiveMark(null);
          setPageSel(null);
        } else if (panel) setPanel(false);
        else if (search.open) setSearch((s) => ({ ...s, open: false }));
        else if (aiOpen) setAiOpen(false);
        else if (focus) setFocus(false);
        else if (!typing) props.onClose();
      } else if (typing || cmd || e.altKey) {
        return;
      } else if (key === "t") {
        update({ showToc: !settings.showToc });
      } else if (key === "f") {
        setFocus((f) => !f);
      } else if (e.key === "?") {
        setHelp(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isFlow, settings.fontSize, settings.showToc, panel, search.open, aiOpen, focus, selection, activeMark, pageSel, update, props.onClose, toggleBookmark, back, forward]);

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

  const chapterTitle = location?.kind === "flow" ? book.chapters[location.chapter]?.title : null;
  const mark = activeMark ? ann.data.highlights.find((h) => h.id === activeMark.id) : null;
  const noteHighlight = noteFor ? ann.data.highlights.find((h) => h.id === noteFor) : null;

  return (
    <div className={`reader${focus ? " focus" : ""}`} onClick={() => setPanel(false)}>
      {focus && <div className="focus-hotzone" />}
      <header className="reader-bar">
        <button onClick={props.onClose} title="返回书架 (Esc)">
          ← 书架
        </button>
        <button className={settings.showToc ? "on" : ""} onClick={() => update({ showToc: !settings.showToc })} title="侧栏 (T)">
          ☰
        </button>
        <button onClick={back} disabled={history.back.length === 0} title="后退 (Alt+←)">
          ‹
        </button>
        <button onClick={forward} disabled={history.forward.length === 0} title="前进 (Alt+→)">
          ›
        </button>
        <div className="reader-title ellipsis">
          <strong>{book.title}</strong>
          {chapterTitle && <span className="muted"> · {chapterTitle}</span>}
        </div>
        {book.kind === "paged" && (
          <div className="page-controls">
            <button onClick={() => setZoom((z) => Math.max(0.3, z / 1.2))} title="缩小">
              －
            </button>
            <span className="zoom">{Math.round(zoom * 100)}%</span>
            <button onClick={() => setZoom((z) => Math.min(5, z * 1.2))} title="放大">
              ＋
            </button>
            <div className="segmented">
              <button className={settings.pageFit === "width" ? "on" : ""} onClick={() => (update({ pageFit: "width" }), setZoom(1))}>
                适宽
              </button>
              <button className={settings.pageFit === "page" ? "on" : ""} onClick={() => (update({ pageFit: "page" }), setZoom(1))}>
                适页
              </button>
            </div>
            <button onClick={rotate} title="顺时针旋转 90°">
              ⟳
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
        <button className={currentBookmark ? "on bookmark-on" : ""} onClick={toggleBookmark} title="书签 (⌘/Ctrl+D)">
          {currentBookmark ? "★" : "☆"} 书签
        </button>
        {isFlow && (
          <button
            className={search.open ? "on" : ""}
            title="全文搜索 (Ctrl/⌘+F)"
            onClick={() => {
              setSearch((s) => ({ ...s, open: !s.open }));
              setAiOpen(false);
              setTimeout(() => searchInput.current?.focus(), 0);
            }}
          >
            搜索
          </button>
        )}
        <button
          className={aiOpen ? "on" : ""}
          title="AI 总结与划词解释"
          onClick={() => {
            setAiOpen((o) => !o);
            setSearch((s) => ({ ...s, open: false }));
          }}
        >
          AI
        </button>
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
          {panel && (
            <SettingsPanel
              settings={settings}
              update={update}
              textOptions={isFlow}
              onFocusMode={() => (setFocus(true), setPanel(false))}
              onFullscreen={() => toggleFullscreen().catch(() => {})}
              onShortcuts={() => (setHelp(true), setPanel(false))}
            />
          )}
        </div>
      </header>

      <div className="reader-body">
        {settings.showToc && !focus && (
          <Sidebar
            book={book}
            tab={sideTab}
            onTab={setSideTab}
            location={location}
            bookmarks={ann.data.bookmarks}
            highlights={ann.data.highlights}
            onNavigate={navigate}
            onRenameBookmark={(id, title) => ann.updateBookmark(id, { title })}
            onRemoveBookmark={ann.removeBookmark}
            onEditNote={setNoteFor}
            onRemoveHighlight={ann.removeHighlight}
            onExport={exportNotes}
          />
        )}
        <section className="reader-view">
          {isFlow ? (
            <FlowView
              ref={flowRef}
              book={book}
              jump={jump}
              flash={flash}
              settings={settings}
              highlights={ann.data.highlights}
              onLocation={onLocation}
              onSelect={onSelect}
              onMarkClick={onMarkClick}
              onMoved={onMoved}
            />
          ) : (
            <PagedView
              book={book}
              jump={jump}
              layout={settings.pageLayout}
              fit={settings.pageFit}
              zoom={zoom}
              rotation={rotation}
              onLocation={onLocation}
              onSelect={setPageSel}
            />
          )}
          {!focus && <ProgressBar book={book} location={location} stats={stats} onJump={(loc) => navigate(loc)} />}
        </section>
        {aiOpen && !focus && (
          <AiPanel book={book} location={location} chatRequest={chatRequest} onSaveNote={saveAiNote} onClose={() => setAiOpen(false)} />
        )}
        {search.open && isFlow && !focus && (
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
                <button key={i} className="hit" onClick={() => navigate({ kind: "flow", chapter: h.chapter, block: h.block }, true)}>
                  <span className="hit-chapter">{book.chapters[h.chapter]?.title || `第 ${h.chapter + 1} 章`}</span>
                  <span>{h.snippet}</span>
                </button>
              ))}
            </div>
          </aside>
        )}
      </div>

      {selection && (
        <SelectionToolbar
          rect={selection.rect}
          onColor={(c) => highlightSelection(c)}
          onNote={() => {
            const h = highlightSelection("yellow");
            if (h) setNoteFor(h.id);
          }}
          onCopy={() => {
            copy(selection.text);
            clearSelection();
          }}
          extra={
            ai.ready
              ? [
                  { label: "解释", title: "让 AI 解释选中的文字", run: () => askAi("explain") },
                  { label: "翻译", title: "让 AI 翻译选中的文字", run: () => askAi("translate") },
                ]
              : undefined
          }
        />
      )}
      {pageSel && (
        <SelectionToolbar
          rect={pageSel.rect}
          onCopy={() => {
            copy(pageSel.text);
            window.getSelection()?.removeAllRanges();
            setPageSel(null);
          }}
          extra={
            ai.ready
              ? [
                  { label: "解释", title: "让 AI 解释选中的文字", run: () => askAiPage("explain") },
                  { label: "翻译", title: "让 AI 翻译选中的文字", run: () => askAiPage("translate") },
                ]
              : undefined
          }
        />
      )}
      {mark && activeMark && !selection && (
        <SelectionToolbar
          rect={activeMark.rect}
          current={mark.color}
          onColor={(color) => {
            ann.updateHighlight(mark.id, { color });
            setActiveMark(null);
          }}
          onNote={() => {
            setNoteFor(mark.id);
            setActiveMark(null);
          }}
          onCopy={() => {
            copy(mark.text);
            setActiveMark(null);
          }}
          onRemove={() => {
            ann.removeHighlight(mark.id);
            setActiveMark(null);
          }}
        />
      )}
      {noteHighlight && (
        <NoteEditor
          quote={noteHighlight.text}
          note={noteHighlight.note ?? ""}
          onSave={(note) => ann.updateHighlight(noteHighlight.id, { note: note.trim() || null })}
          onClose={() => setNoteFor(null)}
        />
      )}
      {help && (
        <Modal title="快捷键" width={520} onClose={() => setHelp(false)}>
          <div className="shortcuts">
            {SHORTCUTS.map((g) => (
              <div key={g.group}>
                <div className="panel-label">{g.group}</div>
                {g.items.map(([k, d]) => (
                  <div key={k} className="shortcut-row">
                    <kbd>{k}</kbd>
                    <span>{d}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        </Modal>
      )}
      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
