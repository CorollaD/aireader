import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, errorText } from "../api";
import { onBeforeExit } from "../lifecycle";
import type { UpdateSettings } from "../App";
import { isDark, type Settings } from "../settings";
import { dragRegion } from "../titlebar";
import type { Card, Highlight, HighlightColor, Hit, Location, OpenedBook, Reference } from "../types";
import { isTyping, Modal, Toast } from "../ui";
import { t } from "../i18n";
import { Icon } from "../icons";
import { useOpenSettings } from "../SettingsDialog";
import ExportNotes from "../ExportNotes";
import LookupDialog, { type LookupRequest } from "../study/LookupDialog";
import { sentenceAt } from "./speech";
import AiPanel, { type ChatRequest, type Quote } from "../ai/AiPanel";
import { useAi } from "../ai/AiContext";
import type { ChatAction } from "../types";
import { NoteEditor, SelectionToolbar } from "./Annotate";
import FlowView, { type FlowHandle, type FlowSelection } from "./FlowView";
import { describe, isValid, progressOf, sameLocation, START } from "./locations";
import PagedView, { CitationLinks, pageText, type PageSelection } from "./PagedView";
import CiteDialog from "../paper/CiteDialog";
import CitationCard from "../paper/CitationCard";
import PaperDialog from "../paper/PaperDialog";
import { useOcr } from "../ocr/useOcr";
import ProgressBar from "./ProgressBar";
import SettingsPanel from "./SettingsPanel";
import { MOD } from "./shortcuts";
import Sidebar, { type SidebarTab } from "./Sidebar";
import { useAnnotations } from "./useAnnotations";
import { useReadingStats } from "./useReadingStats";
import { useSpeech, useVoices } from "./useSpeech";
import ReadAloudBar from "./ReadAloudBar";
import { MIN_TEXT, Resizer, fitWidth, minWidth, usePanelWidths, type Panel } from "./Resizer";
import { mobile } from "../platform";

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

/** Opens a book at a search result, with the search shown. */
export interface OpenAt {
  location: Location;
  query?: string;
}

const ocrHiddenKey = (bookId: string) => `ocr-suggestion-hidden:${bookId}`;

function storedFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) !== null;
  } catch {
    return false;
  }
}

function setStoredFlag(key: string) {
  try {
    localStorage.setItem(key, "1");
  } catch {
    // Only a convenience: the suggestion comes back next time.
  }
}

export default function Reader(props: { bookId: string; openAt: OpenAt | null; settings: Settings; update: UpdateSettings; onClose: () => void }) {
  const { settings, update, bookId, openAt } = props;
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
  const [moreMenu, setMoreMenu] = useState(false);
  const [selection, setSelection] = useState<FlowSelection | null>(null);
  const [activeMark, setActiveMark] = useState<{ id: string; rect: DOMRect } | null>(null);
  const [noteFor, setNoteFor] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [cards, setCards] = useState<Card[]>([]);
  const [lookup, setLookup] = useState<(LookupRequest & { card?: Card }) | null>(null);
  const [history, setHistory] = useState<{ back: Location[]; forward: Location[] }>({ back: [], forward: [] });
  const [toast, setToast] = useState<string | null>(null);
  const [aiOpen, setAiOpen] = useState(false);
  // On phones the contents are a drawer over the text: closed until asked for.
  const [phoneToc, setPhoneToc] = useState(false);
  const [chatRequest, setChatRequest] = useState<ChatRequest | null>(null);
  const [pageSel, setPageSel] = useState<PageSelection | null>(null);
  const ai = useAi();
  const openSettings = useOpenSettings();
  const searchInput = useRef<HTMLInputElement>(null);
  const flowRef = useRef<FlowHandle>(null);
  const pendingSave = useRef<{ loc: Location; progress: number } | null>(null);
  const saveTimer = useRef<number | undefined>(undefined);

  const ann = useAnnotations(bookId);
  const ocr = useOcr(bookId, book?.title ?? "", book?.format === "PDF" || book?.format === "DJVU");
  const isPdf = book?.format === "PDF";
  const [references, setReferences] = useState<Reference[]>([]);
  const [cited, setCited] = useState<{ numbers: number[]; rect: DOMRect } | null>(null);
  const [paperDialog, setPaperDialog] = useState(false);
  const [citing, setCiting] = useState(false);
  const [ocrHidden, setOcrHidden] = useState(() => storedFlag(ocrHiddenKey(bookId)));
  useEffect(() => {
    setReferences([]);
    if (!isPdf) return;
    api
      .paperReferences(bookId)
      .then(setReferences)
      .catch(() => {});
  }, [bookId, isPdf]);
  const citationLinks = useMemo(
    () => ({ count: references.length, onCite: (numbers: number[], rect: DOMRect) => setCited({ numbers, rect }) }),
    [references.length],
  );
  const panels = usePanelWidths(bookId);
  const body = useRef<HTMLDivElement>(null);
  const [bodyWidth, setBodyWidth] = useState(0);
  useEffect(() => {
    const el = body.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setBodyWidth(el.clientWidth));
    ro.observe(el);
    setBodyWidth(el.clientWidth);
    return () => ro.disconnect();
  }, [book]);
  const voices = useVoices();
  const speech = useSpeech(book, settings, setToast, (loc) => go(loc));
  const stats = useReadingStats(book, location, speech.state === "playing");

  useEffect(() => {
    api
      .openBook(bookId)
      .then((b) => {
        setBook(b);
        setRotation(b.rotation);
        const saved = b.position && isValid(b, b.position) ? b.position : START[b.kind];
        const at = openAt && isValid(b, openAt.location) ? openAt.location : null;
        const start = at ?? saved;
        setJump({ loc: start, n: 0 });
        if (at) setFlash(at.kind === "flow" ? { chapter: at.chapter, block: at.block } : null);
        const query = openAt?.query;
        if (at && query) {
          // Shows every match in the book; the passage found may be a long one.
          setSearch({ open: true, query, hits: null, busy: true });
          api.searchBook(bookId, query).then(
            (hits) => setSearch((s) => ({ ...s, hits, busy: false })),
            () => setSearch((s) => ({ ...s, hits: [], busy: false })),
          );
        }
        getCurrentWindow()
          .setTitle(`${b.title} — AiReader`)
          .catch(() => {});
      })
      .catch((e) => setError(errorText(e)));
  }, [bookId]);

  const refreshCards = useCallback(() => {
    api
      .listCards(bookId)
      .then(setCards)
      .catch(() => {});
  }, [bookId]);
  useEffect(refreshCards, [refreshCards]);

  const cardFromHighlight = (h: Highlight) =>
    setLookup({ word: h.text, context: "", location: { kind: "flow", chapter: h.chapter, block: h.startBlock }, back: h.note ?? "" });

  const flush = useCallback(() => {
    window.clearTimeout(saveTimer.current);
    const p = pendingSave.current;
    pendingSave.current = null;
    return p ? api.saveProgress(bookId, p.loc, p.progress).catch(() => {}) : undefined;
  }, [bookId]);
  useEffect(() => {
    const unregister = onBeforeExit(flush);
    return () => {
      unregister();
      flush();
    };
  }, [flush]);

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
      setToast(t("reader.bookmarkRemoved"));
      return;
    }
    const excerpt = location.kind === "flow" ? (flowRef.current?.blockText(location.chapter, location.block) ?? "").trim().slice(0, 100) : "";
    ann.addBookmark({ location, title: describe(book, location), excerpt });
    setToast(t("reader.bookmarkAdded"));
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
      () => setToast(t("common.copied")),
      () => setToast(t("common.copyFailed")),
    );
  };

  const openChat = (req: Omit<ChatRequest, "n">) => {
    setChatRequest((r) => ({ ...req, n: (r?.n ?? 0) + 1 }));
    setAiOpen(true);
    setSearch((s) => ({ ...s, open: false }));
  };

  /** Sends the current text selection to the AI panel. */
  const askAi = (action: ChatAction | null) => {
    if (!selection) return;
    const { chapter, startBlock, start, endBlock, end, text } = selection;
    // Context: the selected paragraphs plus one on each side, capped.
    const context: string[] = [];
    for (let b = Math.max(0, startBlock - 1); b <= endBlock + 1; b++) {
      const text = flowRef.current?.blockText(chapter, b);
      if (text) context.push(text);
    }
    openChat({ anchor: { chapter, range: { startBlock, start, endBlock, end } }, selection: text, context: context.join("\n").slice(0, 2000), action });
    clearSelection();
  };

  /** Sends a PDF selection to the AI panel, with the page's text as context. */
  const askAiPage = async (action: ChatAction | null) => {
    if (!pageSel) return;
    const { page, text } = pageSel;
    window.getSelection()?.removeAllRanges();
    setPageSel(null);
    const runs = await pageText(bookId, page);
    const context = runs
      .map((r) => r.text)
      .join(" ")
      .slice(0, 2000);
    openChat({ anchor: null, selection: text, context, action });
  };

  /** Stores an AI answer as the note of a highlight on the asked-about passage. */
  const saveAiNote = (req: Quote, answer: string) => {
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
  // PDFs are searched through their text layer; comics have no text.
  const searchable = isFlow || book?.format === "PDF";
  const canSpeak = searchable && voices.length > 0;

  /** Starts reading where the reader is; while reading, pauses or resumes. */
  const toggleSpeech = () => {
    if (speech.state === "playing") speech.pause();
    else if (speech.state === "paused") speech.resume();
    else if (location) speech.start(location.kind === "flow" ? { kind: "flow", chapter: location.chapter, block: location.block } : location);
  };

  const rotate = () => {
    const next = (rotation + 90) % 360;
    setRotation(next);
    api.updateBook(bookId, { rotation: next }).catch(() => {});
  };

  // --- keyboard ------------------------------------------------------------------------------

  // Which panels are open.
  const aiShown = aiOpen && !focus;
  const searchShown = search.open && searchable && !focus;
  const rightPanel: Panel | null = aiShown ? "ai" : searchShown ? "search" : null;
  // Too narrow for both panels and the text: the one just opened on the right wins for now.
  const tocYields = !!rightPanel && bodyWidth > 0 && bodyWidth < minWidth("toc") + minWidth(rightPanel) + MIN_TEXT;
  const tocShown = (mobile ? phoneToc : settings.showToc) && !focus && !tocYields;
  const toggleToc = () => {
    if (mobile) {
      setPhoneToc((open) => !open);
      setAiOpen(false);
      setSearch((s) => ({ ...s, open: false }));
      return;
    }
    if (!tocYields) return update({ showToc: !settings.showToc });
    setAiOpen(false);
    setSearch((s) => ({ ...s, open: false }));
    update({ showToc: true });
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cmd = e.metaKey || e.ctrlKey;
      const typing = isTyping(e);
      const key = e.key.toLowerCase();
      // ⌃⌘F on macOS is handled by the app menu.
      if (e.key === "F11") {
        e.preventDefault();
        toggleFullscreen().catch(() => {});
      } else if (cmd && key === "f" && searchable) {
        e.preventDefault();
        setSearch((s) => ({ ...s, open: true }));
        setTimeout(() => searchInput.current?.focus(), 0);
      } else if (key === "r" && !cmd && !e.altKey && !typing && canSpeak) {
        e.preventDefault();
        toggleSpeech();
      } else if (cmd && e.shiftKey && key === "a") {
        e.preventDefault();
        setAiOpen((o) => !o);
        setSearch((s) => ({ ...s, open: false }));
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
        } else if (moreMenu) setMoreMenu(false);
        else if (panel) setPanel(false);
        else if (search.open) setSearch((s) => ({ ...s, open: false }));
        else if (aiOpen) setAiOpen(false);
        else if (focus) setFocus(false);
        else if (!typing) props.onClose();
      } else if (typing || cmd || e.altKey) {
        return;
      } else if (key === "t") {
        toggleToc();
      } else if (key === "f") {
        setFocus((f) => !f);
      } else if (e.key === "?") {
        openSettings("shortcuts");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    isFlow,
    searchable,
    canSpeak,
    speech.state,
    location,
    settings.fontSize,
    settings.showToc,
    tocYields,
    panel,
    moreMenu,
    search.open,
    aiOpen,
    focus,
    selection,
    activeMark,
    pageSel,
    update,
    props.onClose,
    toggleBookmark,
    back,
    forward,
    openSettings,
  ]);

  // The reader keeps its text at the size chosen under Reading, whatever the interface zoom.
  const flowSettings = useMemo(() => {
    const k = settings.uiScale;
    return k === 1 ? settings : { ...settings, fontSize: settings.fontSize / k, maxWidth: settings.maxWidth / k, margin: settings.margin / k };
  }, [settings]);

  if (error && !book) {
    return (
      <div className="reader">
        <div className="center-msg">
          <p className="error">{error}</p>
          <button onClick={props.onClose}>{t("reader.backToShelf")}</button>
        </div>
      </div>
    );
  }
  if (!book) return <div className="reader center-msg muted">{t("reader.opening")}</div>;

  const chapterTitle = location?.kind === "flow" ? book.chapters[location.chapter]?.title : null;

  // Panel widths; the text always keeps room, so panels shrink in narrow windows.
  const tocWidth = tocShown ? fitWidth("toc", panels.widths.toc, bodyWidth - (rightPanel ? minWidth(rightPanel) : 0)) : 0;
  const rightWidth = rightPanel ? fitWidth(rightPanel, panels.widths[rightPanel], bodyWidth - tocWidth) : 0;
  const mark = activeMark ? ann.data.highlights.find((h) => h.id === activeMark.id) : null;
  const noteHighlight = noteFor ? ann.data.highlights.find((h) => h.id === noteFor) : null;

  return (
    <div
      className={`reader${focus ? " focus" : ""}`}
      data-theme={isDark(settings.background) ? "dark" : undefined}
      onClick={() => (setPanel(false), setMoreMenu(false))}
    >
      {focus && <div className="focus-hotzone" />}
      <header className="reader-bar" {...dragRegion}>
        <div className="bar-group">
          <button className="icon-button wide" onClick={props.onClose} title={`${t("reader.backToShelf")} (Esc)`}>
            <Icon name="arrowLeft" />
            <span className="label">{t("reader.shelf")}</span>
          </button>
          <button className={`icon-button${tocShown ? " on" : ""}`} onClick={toggleToc} title={`${t("reader.sidebar")} (T)`} aria-label={t("reader.sidebar")}>
            <Icon name="panelLeft" />
          </button>
          <button
            className="icon-button nav-tool"
            onClick={back}
            disabled={history.back.length === 0}
            title={`${t("reader.back")} (Alt+←)`}
            aria-label={t("reader.back")}
          >
            <Icon name="chevronLeft" />
          </button>
          <button
            className="icon-button nav-tool"
            onClick={forward}
            disabled={history.forward.length === 0}
            title={`${t("reader.forward")} (Alt+→)`}
            aria-label={t("reader.forward")}
          >
            <Icon name="chevronRight" />
          </button>
        </div>
        <div className="reader-title ellipsis">
          <strong>{book.title}</strong>
          {chapterTitle && <span className="muted"> · {chapterTitle}</span>}
        </div>
        <div className="bar-group">
          {book.kind === "paged" && (
            <div className="tool-group">
              <button
                className="icon-button"
                onClick={() => setZoom((z) => Math.max(0.3, z / 1.2))}
                title={t("reader.zoomOut")}
                aria-label={t("reader.zoomOut")}
              >
                <Icon name="minus" />
              </button>
              <button className="zoom" onClick={() => setZoom(1)} title={t("reader.zoomReset")}>
                {Math.round(zoom * 100)}%
              </button>
              <button className="icon-button" onClick={() => setZoom((z) => Math.min(5, z * 1.2))} title={t("reader.zoomIn")} aria-label={t("reader.zoomIn")}>
                <Icon name="plus" />
              </button>
              <span className="tool-sep fit-tool" />
              <button
                className={`icon-button fit-tool${settings.pageFit === "width" ? " on" : ""}`}
                onClick={() => (update({ pageFit: "width" }), setZoom(1))}
                title={t("rs.fitWidth")}
                aria-label={t("rs.fitWidth")}
              >
                <Icon name="fitWidth" />
              </button>
              <button
                className={`icon-button fit-tool${settings.pageFit === "page" ? " on" : ""}`}
                onClick={() => (update({ pageFit: "page" }), setZoom(1))}
                title={t("rs.fitPage")}
                aria-label={t("rs.fitPage")}
              >
                <Icon name="fitPage" />
              </button>
              <button
                className={`icon-button fit-tool${settings.pageFit === "height" ? " on" : ""}`}
                onClick={() => (update({ pageFit: "height" }), setZoom(1))}
                title={t("rs.fitHeight")}
                aria-label={t("rs.fitHeight")}
              >
                <Icon name="fitHeight" />
              </button>
              <button className="icon-button fit-tool" onClick={rotate} title={t("reader.rotate")} aria-label={t("reader.rotate")}>
                <Icon name="rotate" />
              </button>
            </div>
          )}
          <button
            className={`icon-button${currentBookmark ? " on bookmark-on" : ""}`}
            onClick={toggleBookmark}
            title={`${t("reader.bookmark")} (${MOD}+D)`}
            aria-label={t("reader.bookmark")}
          >
            <Icon name="bookmark" filled={!!currentBookmark} />
          </button>
          {canSpeak && (
            <button
              className={`icon-button${speech.state !== "idle" ? " on" : ""}`}
              title={`${t("reader.readAloud")} (R)`}
              aria-label={t("reader.readAloud")}
              onClick={toggleSpeech}
            >
              <Icon name="volume" />
            </button>
          )}
          {searchable && (
            <button
              className={`icon-button${search.open ? " on" : ""}`}
              title={`${t("reader.search")} (${MOD}+F)`}
              aria-label={t("reader.search")}
              onClick={() => {
                setSearch((s) => ({ ...s, open: !s.open }));
                setAiOpen(false);
                setTimeout(() => searchInput.current?.focus(), 0);
              }}
            >
              <Icon name="search" />
            </button>
          )}
          <button
            className={`icon-button wide ai-button${aiOpen ? " on" : ""}`}
            title={`${t("reader.ai")} (${MOD}+Shift+A)`}
            aria-label={t("reader.ai")}
            onClick={() => {
              setAiOpen((o) => !o);
              setSearch((s) => ({ ...s, open: false }));
            }}
          >
            <Icon name="sparkles" />
            AI
          </button>
          <div className="menu-anchor">
            <button
              className={`icon-button${panel ? " on" : ""}`}
              onClick={(e) => {
                e.stopPropagation();
                setMoreMenu(false);
                setPanel((p) => !p);
              }}
              title={t("reader.appearance")}
              aria-label={t("reader.appearance")}
            >
              <Icon name="type" />
            </button>
            {panel && <SettingsPanel settings={settings} update={update} textOptions={isFlow} onMore={() => (setPanel(false), openSettings("reading"))} />}
          </div>
          <div className="menu-anchor">
            <button
              className={`icon-button${moreMenu ? " on" : ""}`}
              onClick={(e) => {
                e.stopPropagation();
                setPanel(false);
                setMoreMenu((m) => !m);
              }}
              title={t("common.more")}
              aria-label={t("common.more")}
            >
              <Icon name="more" />
            </button>
            {moreMenu && (
              <div className="menu dropdown" onClick={(e) => (e.stopPropagation(), setMoreMenu(false))}>
                {book.kind === "paged" && (
                  <>
                    <div className="narrow-tools">
                      <button className={settings.pageFit === "width" ? "checked" : ""} onClick={() => (update({ pageFit: "width" }), setZoom(1))}>
                        <Icon name="fitWidth" /> {t("rs.fitWidth")}
                      </button>
                      <button className={settings.pageFit === "page" ? "checked" : ""} onClick={() => (update({ pageFit: "page" }), setZoom(1))}>
                        <Icon name="fitPage" /> {t("rs.fitPage")}
                      </button>
                      <button className={settings.pageFit === "height" ? "checked" : ""} onClick={() => (update({ pageFit: "height" }), setZoom(1))}>
                        <Icon name="fitHeight" /> {t("rs.fitHeight")}
                      </button>
                      <button onClick={rotate}>
                        <Icon name="rotate" /> {t("reader.rotate")}
                      </button>
                      <hr />
                    </div>
                    <button className={settings.pageLayout === "continuous" ? "checked" : ""} onClick={() => update({ pageLayout: "continuous" })}>
                      <Icon name="list" /> {t("rs.continuous")}
                    </button>
                    <button className={settings.pageLayout === "single" ? "checked" : ""} onClick={() => update({ pageLayout: "single" })}>
                      <Icon name="fitPage" /> {t("rs.single")}
                    </button>
                    <button className={settings.pageLayout === "spread" ? "checked" : ""} onClick={() => update({ pageLayout: "spread" })}>
                      <Icon name="spread" /> {t("rs.spread")}
                    </button>
                    {book.format === "PDF" && (
                      <>
                        <button onClick={() => setPaperDialog(true)}>
                          <Icon name="note" /> {t("paper.details")}
                        </button>
                        <button onClick={() => setCiting(true)}>
                          <Icon name="copy" /> {t("cite.action")}
                        </button>
                      </>
                    )}
                    {ocr.pending.length > 0 && !ocr.running && (
                      <button onClick={ocr.start}>
                        <Icon name="type" /> {t("ocr.recognise", { n: ocr.pending.length })}
                      </button>
                    )}
                    {book.format === "CBZ" && (
                      <button className={settings.comicRtl ? "checked" : ""} onClick={() => update({ comicRtl: !settings.comicRtl })}>
                        <Icon name="arrowLeft" /> {t("rs.comicRtl")}
                      </button>
                    )}
                    <hr />
                  </>
                )}
                <button onClick={() => setFocus(true)}>
                  <Icon name="focus" /> {t("reader.focus")}
                  <kbd>F</kbd>
                </button>
                <button onClick={() => toggleFullscreen().catch(() => {})}>
                  <Icon name="maximize" /> {t("reader.fullscreen")}
                  <kbd>F11</kbd>
                </button>
                <hr />
                <button onClick={() => openSettings("shortcuts")}>
                  <Icon name="keyboard" /> {t("reader.shortcuts")}
                  <kbd>?</kbd>
                </button>
                <button onClick={() => openSettings()}>
                  <Icon name="settings" /> {t("reader.settings")}
                  <kbd>{MOD},</kbd>
                </button>
              </div>
            )}
          </div>
        </div>
      </header>
      {ocr.scanned && !ocr.running && !ocrHidden && (
        <div className="reader-notice">
          <span className="grow">{t("ocr.suggest", { n: ocr.pending.length })}</span>
          <button className="primary" onClick={ocr.start}>
            {t("ocr.start")}
          </button>
          <button
            className="icon-button"
            aria-label={t("common.close")}
            onClick={() => {
              setStoredFlag(ocrHiddenKey(bookId));
              setOcrHidden(true);
            }}
          >
            <Icon name="close" />
          </button>
        </div>
      )}
      {paperDialog && (
        <PaperDialog bookId={bookId} onClose={() => setPaperDialog(false)} onSaved={(saved) => setBook((b) => b && { ...b, title: saved.title })} />
      )}
      {citing && <CiteDialog ids={[bookId]} onClose={() => setCiting(false)} />}
      {cited && (
        <CitationCard
          references={cited.numbers.flatMap((n) => references.filter((r) => r.number === n))}
          rect={cited.rect}
          onJump={(page) => {
            setCited(null);
            navigate({ kind: "page", page });
          }}
          onClose={() => setCited(null)}
        />
      )}
      {ocr.error && <div className="reader-notice error">{t("ocr.failed", { error: ocr.error })}</div>}

      <div className="reader-body" ref={body}>
        {mobile && tocShown && <div className="drawer-backdrop in-reader" onClick={() => setPhoneToc(false)} />}
        {tocShown && (
          <Sidebar
            width={tocWidth}
            onCollapse={() => (mobile ? setPhoneToc(false) : update({ showToc: false }))}
            book={book}
            tab={sideTab}
            onTab={setSideTab}
            location={location}
            bookmarks={ann.data.bookmarks}
            highlights={ann.data.highlights}
            onNavigate={(loc, flash) => (navigate(loc, flash), setPhoneToc(false))}
            onRenameBookmark={(id, title) => ann.updateBookmark(id, { title })}
            onRemoveBookmark={ann.removeBookmark}
            onEditNote={setNoteFor}
            onRemoveHighlight={ann.removeHighlight}
            onExport={() => setExporting(true)}
            cards={cards}
            onCardFromHighlight={cardFromHighlight}
            onEditCard={(card) => setLookup({ word: card.front, context: card.context, location: card.location, card })}
            onDeleteCard={(card) => api.deleteCard(card.id).then(refreshCards, () => {})}
          />
        )}
        {tocShown && (
          <Resizer
            panel="toc"
            side="left"
            width={tocWidth}
            max={fitWidth("toc", Infinity, bodyWidth - rightWidth)}
            onChange={(w) => panels.set("toc", w)}
            onReset={() => panels.reset("toc")}
          />
        )}
        <section className="reader-view">
          {isFlow ? (
            <FlowView
              ref={flowRef}
              book={book}
              jump={jump}
              flash={flash}
              settings={flowSettings}
              highlights={ann.data.highlights}
              onLocation={onLocation}
              onSelect={onSelect}
              onMarkClick={onMarkClick}
              onMoved={onMoved}
              onLink={(loc) => navigate(loc, true)}
              speaking={speech.position}
            />
          ) : (
            <CitationLinks.Provider value={citationLinks}>
              <PagedView
                book={book}
                jump={jump}
                layout={settings.pageLayout}
                fit={settings.pageFit}
                rtl={settings.comicRtl && book.format === "CBZ"}
                zoom={zoom}
                rotation={rotation}
                onLocation={onLocation}
                onSelect={setPageSel}
              />
            </CitationLinks.Provider>
          )}
          {speech.state !== "idle" && (
            <ReadAloudBar
              state={speech.state}
              voices={voices}
              settings={settings}
              update={update}
              onPlayPause={toggleSpeech}
              onStep={speech.step}
              onStop={speech.stop}
            />
          )}
          {!focus && <ProgressBar book={book} location={location} stats={stats} onJump={(loc) => navigate(loc)} />}
        </section>
        {rightPanel && (
          <Resizer
            panel={rightPanel}
            side="right"
            width={rightWidth}
            max={fitWidth(rightPanel, Infinity, bodyWidth - tocWidth)}
            onChange={(w) => panels.set(rightPanel, w)}
            onReset={() => panels.reset(rightPanel)}
          />
        )}
        {aiShown && (
          <AiPanel
            width={rightWidth}
            book={book}
            location={location}
            chatRequest={chatRequest}
            onSaveNote={saveAiNote}
            onNavigate={(loc) => navigate(loc, true)}
            onClose={() => setAiOpen(false)}
            highlights={ann.data.highlights}
            onCardsSaved={refreshCards}
          />
        )}
        {searchShown && (
          <aside className="search-panel" style={{ width: rightWidth }}>
            <input
              ref={searchInput}
              className="full"
              placeholder={t("reader.searchPlaceholder")}
              value={search.query}
              onChange={(e) => setSearch((s) => ({ ...s, query: e.target.value }))}
              onKeyDown={(e) => e.key === "Enter" && runSearch()}
            />
            <div className="muted small search-status">
              {search.busy
                ? t("reader.searching")
                : search.hits
                  ? t("reader.found", { n: `${search.hits.length}${search.hits.length >= 1000 ? "+" : ""}` })
                  : ""}
            </div>
            <div className="search-hits">
              {search.hits?.map((h, i) => (
                <button key={i} className="hit" onClick={() => navigate(h.location, true)}>
                  <span className="hit-chapter">
                    {h.location.kind === "flow"
                      ? book.chapters[h.location.chapter]?.title || t("loc.chapter", { n: h.location.chapter + 1 })
                      : t("loc.page", { n: h.location.page + 1 })}
                  </span>
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
          extra={[
            {
              label: t("cards.lookup"),
              icon: "bookOpen" as const,
              run: () => {
                const block = flowRef.current?.blockText(selection.chapter, selection.startBlock) ?? selection.text;
                setLookup({
                  word: selection.text.trim(),
                  context: sentenceAt(block, selection.start),
                  location: { kind: "flow", chapter: selection.chapter, block: selection.startBlock },
                });
                clearSelection();
              },
            },
            ...(canSpeak
              ? [
                  {
                    label: t("reader.readFromHere"),
                    icon: "volume" as const,
                    run: () => {
                      speech.start({ kind: "flow", chapter: selection.chapter, block: selection.startBlock, offset: selection.start });
                      clearSelection();
                    },
                  },
                ]
              : []),
            ...(ai.ready
              ? [
                  { label: t("reader.explain"), title: t("reader.explainTitle"), icon: "sparkles" as const, run: () => askAi("explain") },
                  { label: t("reader.translate"), title: t("reader.translateTitle"), icon: "languages" as const, run: () => askAi("translate") },
                  { label: t("reader.ask"), title: t("reader.askTitle"), icon: "message" as const, run: () => askAi(null) },
                ]
              : []),
          ]}
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
          extra={[
            {
              label: t("cards.lookup"),
              icon: "bookOpen" as const,
              run: () => {
                setLookup({ word: pageSel.text.trim(), context: "", location: { kind: "page", page: pageSel.page } });
                window.getSelection()?.removeAllRanges();
                setPageSel(null);
              },
            },
            ...(ai.ready
              ? [
                  { label: t("reader.explain"), title: t("reader.explainTitle"), icon: "sparkles" as const, run: () => askAiPage("explain") },
                  { label: t("reader.translate"), title: t("reader.translateTitle"), icon: "languages" as const, run: () => askAiPage("translate") },
                  { label: t("reader.ask"), title: t("reader.askTitle"), icon: "message" as const, run: () => askAiPage(null) },
                ]
              : []),
          ]}
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
          extra={[
            {
              label: t("daily.makeCard"),
              icon: "layers",
              run: () => {
                cardFromHighlight(mark);
                setActiveMark(null);
              },
            },
          ]}
        />
      )}
      {lookup && (
        <LookupDialog
          request={lookup}
          book={{ id: bookId, title: book.title }}
          card={lookup.card}
          settings={settings}
          onSaved={() => {
            refreshCards();
            setToast(t("cards.saved"));
          }}
          onClose={() => setLookup(null)}
        />
      )}
      {exporting && (
        <Modal title={t("side.exportTitle")} width={460} onClose={() => setExporting(false)}>
          <ExportNotes bookId={bookId} />
        </Modal>
      )}
      {noteHighlight && (
        <NoteEditor
          quote={noteHighlight.text}
          note={noteHighlight.note ?? ""}
          onSave={(note) => ann.updateHighlight(noteHighlight.id, { note: note.trim() || null })}
          onClose={() => setNoteFor(null)}
        />
      )}
      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
