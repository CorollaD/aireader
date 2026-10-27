// Reader side panel for AI, with three tabs. "Chat" answers questions about the whole book from
// retrieved passages (with [n] citations that jump to the text), or about a selected passage
// (explain, translate, ask). "Summary" summarises the chapter, page or whole book. "Mind map"
// draws mind maps of the chapter, the book or the conversation.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, errorText, runAiTask } from "../api";
import type { AiEvent, AiTask, BookEstimate, ChatAction, ChatRecord, Location, MindMapScope, OpenedBook, SemanticStatus, Source, Usage } from "../types";
import { Modal } from "../ui";
import type { TextRange } from "../reader/anchors";
import { formatDate, formatNumber, t, type MessageKey } from "../i18n";
import { Icon, type IconName } from "../icons";
import { describe } from "../reader/locations";
import { MOD } from "../reader/shortcuts";
import { useAi } from "./AiContext";
import { presetOf } from "./presets";
import Markdown from "./Markdown";
import { MindMapPreview, MindMapViewer, outlineMarkdown, saveMindMap } from "./MindMap";

/** A passage the conversation is about (from the selection toolbar). */
export interface Quote {
  /** Where the passage is in a text book; absent for PDF selections (no notes there). */
  anchor: { chapter: number; range: TextRange } | null;
  selection: string;
  context: string;
  /** Explain/translate start the conversation; null waits for the reader's question. */
  action: ChatAction | null;
}

/** A request from the selection toolbar; `n` makes repeated requests distinct. */
export interface ChatRequest extends Quote {
  n: number;
}

type Tab = "chat" | "summary" | "map";
type Status = "idle" | "running" | "done" | "error" | "stopped";
type Progress = { done: number; total: number; label: string };

interface Turn {
  role: "user" | "assistant";
  text: string;
  sources?: Source[];
  usage?: Usage;
}

/** Adds one request's usage to a running total. */
function addUsage(total: Usage | undefined, e: Usage): Usage {
  if (!total) return { input: e.input, output: e.output, cost: e.cost };
  return {
    input: total.input + e.input,
    output: total.output + e.output,
    cost: total.cost === null && e.cost === null ? null : (total.cost ?? 0) + (e.cost ?? 0),
  };
}

/** "1.2k → 340 tokens · $0.0031" */
function usageLabel(u: Usage): string {
  const k = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k` : String(n));
  const cost = u.cost !== null && u.cost > 0 ? ` · $${u.cost < 0.01 ? u.cost.toFixed(4) : u.cost.toFixed(2)}` : "";
  return t("ai.usage", { input: k(u.input), output: k(u.output) }) + cost;
}

interface Chat {
  /** Saved conversation id; assigned when the first request is sent. */
  id: string | null;
  turns: Turn[];
  quote: Quote | null;
  status: Status;
  error: string | null;
  taskId: string | null;
}

interface Summary {
  title: string;
  text: string;
  status: Status;
  progress: Progress | null;
  cached: boolean;
  error: string | null;
  taskId: string | null;
  task: AiTask;
  usage?: Usage;
}

/** A mind map drawn in this session. */
interface MapItem {
  id: number;
  title: string;
  /** What it covers, e.g. "This chapter". */
  scopeLabel: string;
  scope: MindMapScope;
  outline: string;
  status: Status;
  progress: Progress | null;
  cached: boolean;
  error: string | null;
  taskId: string | null;
}

const ACTION_LABEL: Record<ChatAction, MessageKey> = {
  explain: "ai.actionExplain",
  translate: "ai.actionTranslate",
  summarize: "ai.actionSummarize",
};

const EMPTY_CHAT: Chat = { id: null, turns: [], quote: null, status: "idle", error: null, taskId: null };

/** What a saved conversation is called in the history list. */
function chatTitle(c: Chat): string {
  const first = c.turns.find((turn) => turn.role === "user")?.text ?? c.quote?.selection ?? "";
  const line = first.replace(/\s+/g, " ").trim();
  return line.length > 40 ? `${line.slice(0, 40)}…` : line || t("ai.chatTab");
}

/** A saved conversation back as a chat (stored data is checked loosely). */
function restoreChat(r: ChatRecord): Chat {
  const data = (r.data ?? {}) as { turns?: Turn[]; quote?: Quote | null };
  return { ...EMPTY_CHAT, id: r.id, turns: Array.isArray(data.turns) ? data.turns : [], quote: data.quote ?? null };
}

const TABS: { key: Tab; icon: IconName; label: MessageKey }[] = [
  { key: "chat", icon: "message", label: "ai.chatTab" },
  { key: "summary", icon: "bookOpen", label: "ai.summaryTab" },
  { key: "map", icon: "mindmap", label: "ai.mapTab" },
];

export default function AiPanel(props: {
  book: OpenedBook;
  location: Location | null;
  chatRequest: ChatRequest | null;
  onSaveNote: (quote: Quote, note: string) => void;
  onNavigate: (loc: Location) => void;
  onClose: () => void;
  width: number;
}) {
  const { book, location } = props;
  const ai = useAi();
  const [tab, setTab] = useState<Tab>("chat");
  const [chat, setChat] = useState<Chat>(EMPTY_CHAT);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [maps, setMaps] = useState<MapItem[]>([]);
  const [mapId, setMapId] = useState<number | null>(null);
  const [viewing, setViewing] = useState<MapItem | null>(null);
  const [question, setQuestion] = useState("");
  const [estimate, setEstimate] = useState<{ data: BookEstimate; purpose: "summary" | "mindmap" } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const body = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLTextAreaElement>(null);
  const mapSvg = useRef<SVGSVGElement>(null);
  const nextMapId = useRef(1);
  const [sessions, setSessions] = useState<ChatRecord[]>([]);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [semantic, setSemantic] = useState<SemanticStatus | null>(null);
  const [indexing, setIndexing] = useState<{ done: number; total: number; taskId: string } | null>(null);

  // The book's semantic index state; refreshed when the AI settings change.
  const refreshSemantic = () => api.aiSemanticStatus(book.id).then(setSemantic, () => setSemantic(null));
  useEffect(() => {
    refreshSemantic();
  }, [book.id, ai.status]);

  /** Embeds the book's passages with the configured embedding model (only when asked). */
  const buildIndex = async () => {
    if (indexing || !semantic?.available) return;
    if (!semantic.local && !(await ai.ensureConsent())) return;
    const taskId = runAiTask({ kind: "buildIndex", bookId: book.id }, (e: AiEvent) => {
      switch (e.type) {
        case "progress":
          setIndexing((x) => (x && x.taskId === taskId ? { ...x, done: e.done, total: e.total } : x));
          break;
        case "done":
          setIndexing(null);
          setNotice(t("ai.indexBuilt", { n: e.text }));
          refreshSemantic();
          break;
        case "error":
          setIndexing(null);
          setNotice(e.message);
          refreshSemantic();
          break;
      }
    });
    setIndexing({ done: 0, total: 0, taskId });
  };

  const flow = book.kind === "flow";
  const here = location ? describe(book, location) : null;

  // Saved conversations: the latest one is shown again when the book is opened.
  useEffect(() => {
    let alive = true;
    api
      .listChats(book.id)
      .then((list) => {
        if (!alive) return;
        setSessions(list);
        if (list[0]) setChat((c) => (c.turns.length || c.quote ? c : restoreChat(list[0])));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [book.id]);

  // A conversation is saved whenever an answer finishes (or stops).
  useEffect(() => {
    if (chat.status === "idle" || chat.status === "running" || !chat.id || chat.turns.length === 0) return;
    const record: ChatRecord = {
      id: chat.id,
      title: chatTitle(chat),
      updated: Math.floor(Date.now() / 1000),
      data: { turns: chat.turns, quote: chat.quote },
    };
    api
      .saveChat(book.id, record)
      .then(() => setSessions((s) => [record, ...s.filter((x) => x.id !== record.id)]))
      .catch(() => {});
  }, [chat.status, chat.id, chat.turns.length]);

  // The history menu closes on a click anywhere else.
  useEffect(() => {
    if (!historyOpen) return;
    const close = (e: MouseEvent) => {
      if (!(e.target as Element).closest?.(".chat-history-anchor")) setHistoryOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [historyOpen]);

  const openSession = (r: ChatRecord) => {
    stop(chat.taskId);
    setChat(restoreChat(r));
    setHistoryOpen(false);
  };

  const deleteSession = (id: string) => {
    api.deleteChat(id).catch(() => {});
    setSessions((s) => s.filter((x) => x.id !== id));
    if (chat.id === id) setChat(EMPTY_CHAT);
  };

  // --- chat ------------------------------------------------------------------------------------

  /** Answers the conversation whose last turn is the (empty) answer being generated. */
  const run = (c: Chat) => {
    const history = c.turns.slice(0, -1).map(({ role, text }) => ({ role, text }));
    const task: AiTask = c.quote
      ? {
          kind: "chat",
          bookTitle: book.title,
          selection: c.quote.selection,
          context: c.quote.context,
          action: c.quote.action,
          history,
        }
      : { kind: "ask", bookId: book.id, location, history };
    const taskId = runAiTask(task, (e: AiEvent) =>
      setChat((cur) => {
        if (cur.taskId !== taskId) return cur;
        const turns = [...cur.turns];
        const last = turns[turns.length - 1];
        switch (e.type) {
          case "sources":
            turns[turns.length - 1] = { ...last, sources: e.sources };
            return { ...cur, turns };
          case "usage":
            turns[turns.length - 1] = { ...last, usage: addUsage(last.usage, e) };
            return { ...cur, turns };
          case "delta":
            turns[turns.length - 1] = { ...last, text: last.text + e.text };
            return { ...cur, turns };
          case "done":
            turns[turns.length - 1] = { ...last, text: e.text };
            return { ...cur, turns, status: "done" };
          case "error":
            return { ...cur, turns: turns.slice(0, -1), status: "error", error: e.message };
          default:
            return cur;
        }
      }),
    );
    setChat({ ...c, id: c.id ?? crypto.randomUUID(), status: "running", error: null, taskId });
  };

  const send = async (text: string) => {
    const q = text.trim();
    if (!q || chat.status === "running") return;
    if (!(await ai.ensureConsent())) return;
    setQuestion("");
    setTab("chat");
    run({ ...chat, turns: [...chat.turns, { role: "user", text: q }, { role: "assistant", text: "" }] });
  };

  const regenerate = () => {
    if (chat.status === "running") return;
    const last = chat.turns[chat.turns.length - 1];
    const turns = last?.role === "assistant" ? chat.turns.slice(0, -1) : chat.turns;
    run({ ...chat, turns: [...turns, { role: "assistant", text: "" }] });
  };

  const newChat = () => {
    stop(chat.taskId);
    setChat(EMPTY_CHAT);
    setQuestion("");
    setTimeout(() => input.current?.focus(), 0);
  };

  // A request from the selection toolbar starts a conversation about that passage.
  useEffect(() => {
    const req = props.chatRequest;
    if (!req) return;
    (async () => {
      setTab("chat");
      const { n: _, ...quote } = req;
      if (!quote.action) {
        stop(chat.taskId);
        setChat({ ...EMPTY_CHAT, quote });
        setTimeout(() => input.current?.focus(), 0);
        return;
      }
      if (!(await ai.ensureConsent())) return;
      run({ ...EMPTY_CHAT, quote, turns: [{ role: "assistant", text: "" }] });
    })();
    // Only a new request (its nonce) should start a conversation.
  }, [props.chatRequest?.n]);

  // Keep the newest output in view while it streams, unless the reader has scrolled up to read
  // something earlier; a new question or tab follows the output again.
  const stick = useRef(true);
  const onScroll = () => {
    const el = body.current;
    if (el) stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
  };
  useEffect(() => {
    stick.current = true;
  }, [chat.turns.length, tab]);
  const lastText = chat.turns[chat.turns.length - 1]?.text;
  useEffect(() => {
    const el = body.current;
    if (el && tab !== "map" && stick.current) el.scrollTop = el.scrollHeight;
  }, [chat.turns.length, lastText, summary?.text, tab]);

  // The question box grows with its text.
  useLayoutEffect(() => {
    const el = input.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  }, [question, tab]);

  // --- summaries -------------------------------------------------------------------------------

  const startSummary = async (title: string, task: AiTask) => {
    if (!(await ai.ensureConsent())) return;
    setTab("summary");
    const base: Summary = { title, text: "", status: "running", progress: null, cached: false, error: null, taskId: null, task };
    const taskId = runAiTask(task, (e: AiEvent) =>
      setSummary((s) => {
        if (!s || s.taskId !== taskId) return s;
        switch (e.type) {
          case "delta":
            return { ...s, text: s.text + e.text, progress: null };
          case "progress":
            return { ...s, progress: e };
          case "usage":
            return { ...s, usage: addUsage(s.usage, e) };
          case "done":
            return { ...s, text: e.text, cached: e.cached, status: "done", progress: null };
          case "error":
            return { ...s, status: "error", error: e.message, progress: null };
          default:
            return s;
        }
      }),
    );
    setSummary({ ...base, taskId });
  };

  const summarizeHere = () => {
    if (!location) return;
    if (location.kind === "flow") {
      const title = book.chapters[location.chapter]?.title || t("loc.chapter", { n: location.chapter + 1 });
      startSummary(t("ai.thisChapter", { title }), { kind: "summarizeChapter", bookId: book.id, chapter: location.chapter });
    } else {
      startSummary(t("loc.page", { n: location.page + 1 }), { kind: "summarizePages", bookId: book.id, from: location.page, to: location.page });
    }
  };

  const askEstimate = async (purpose: "summary" | "mindmap") => {
    if (!(await ai.ensureConsent())) return;
    try {
      setEstimate({ data: await api.aiEstimateBook(book.id), purpose });
    } catch (e) {
      setNotice(errorText(e));
    }
  };

  // --- mind maps -------------------------------------------------------------------------------

  /** Draws a map as a new item, or in place of `replace` (regenerating it). */
  const drawMap = async (scope: MindMapScope, title: string, scopeLabel: string, replace?: MapItem) => {
    if (!(await ai.ensureConsent())) return;
    setTab("map");
    const id = replace?.id ?? nextMapId.current++;
    const item: MapItem = {
      id,
      title,
      scopeLabel,
      scope,
      outline: "",
      status: "running",
      progress: null,
      cached: false,
      error: null,
      taskId: null,
    };
    const taskId = runAiTask({ kind: "mindMap", bookId: book.id, scope, refresh: !!replace }, (e: AiEvent) =>
      setMaps((ms) =>
        ms.map((m) => {
          if (m.id !== id || m.taskId !== taskId) return m;
          switch (e.type) {
            case "delta":
              return { ...m, outline: m.outline + e.text, progress: null };
            case "progress":
              return { ...m, progress: e };
            case "done":
              return { ...m, outline: e.text, cached: e.cached, status: "done", progress: null };
            case "error":
              return { ...m, status: "error", error: e.message, progress: null };
            default:
              return m;
          }
        }),
      ),
    );
    setMaps((ms) => (replace ? ms.map((m) => (m.id === id ? { ...item, taskId } : m)) : [...ms, { ...item, taskId }]));
    setMapId(id);
  };

  const mapHere = () => {
    if (!location) return;
    if (location.kind === "flow") {
      const title = book.chapters[location.chapter]?.title || t("loc.chapter", { n: location.chapter + 1 });
      drawMap({ kind: "chapter", chapter: location.chapter }, title, t("mm.scopeChapter"));
    } else {
      const from = Math.max(0, location.page - 1);
      const to = Math.min(book.pages.length - 1, location.page + 1);
      drawMap({ kind: "pages", from, to }, t("loc.page", { n: location.page + 1 }), t("mm.scopePages"));
    }
  };

  const conversationTurns = chat.turns.filter((turn) => turn.text);
  const mapConversation = () => {
    const text = conversationTurns.map((turn) => `${turn.role === "user" ? "Q" : "A"}: ${turn.text}`).join("\n\n");
    const material = chat.quote ? `> ${chat.quote.selection}\n\n${text}` : text;
    const title = t("mm.conversationTitle");
    drawMap({ kind: "text", title, text: material }, title, t("mm.scopeConversation"));
  };

  const exportMap = async (m: MapItem, kind: "png" | "svg") => {
    if (!mapSvg.current) return;
    try {
      const path = await saveMindMap(mapSvg.current, m.title, kind);
      if (path) setNotice(t("reader.exported", { path }));
    } catch (e) {
      setNotice(errorText(e));
    }
  };

  function stop(taskId: string | null) {
    if (!taskId) return;
    api.aiCancel(taskId).catch(() => {});
    setSummary((s) => (s && s.taskId === taskId && s.status === "running" ? { ...s, status: "stopped", progress: null } : s));
    setMaps((ms) => ms.map((m) => (m.taskId === taskId && m.status === "running" ? { ...m, status: "stopped", progress: null } : m)));
    setChat((c) => {
      if (c.taskId !== taskId || c.status !== "running") return c;
      // Drop an answer that never started.
      const last = c.turns[c.turns.length - 1];
      const turns = last?.role === "assistant" && !last.text ? c.turns.slice(0, -1) : c.turns;
      return { ...c, turns, status: "stopped" };
    });
  }

  const copy = (text: string) =>
    navigator.clipboard.writeText(text).then(
      () => setNotice(t("common.copied")),
      () => {},
    );

  useEffect(() => {
    if (!notice) return;
    const id = window.setTimeout(() => setNotice(null), 2500);
    return () => window.clearTimeout(id);
  }, [notice]);

  // --- rendering -------------------------------------------------------------------------------

  const header = (
    <div className="ai-head">
      <div className="ai-switch" role="tablist">
        {TABS.map((tb) => (
          <button
            key={tb.key}
            role="tab"
            aria-selected={tab === tb.key}
            className={tab === tb.key ? "on" : ""}
            title={t(tb.label)}
            onClick={() => setTab(tb.key)}
          >
            <Icon name={tb.icon} size={14} />
            <span className="tab-label">{t(tb.label)}</span>
          </button>
        ))}
      </div>
      <span className="spacer" />
      {tab === "chat" && ai.ready && sessions.length > 0 && (
        <div className="menu-anchor chat-history-anchor">
          <button
            className={`icon-button${historyOpen ? " on" : ""}`}
            title={t("ai.history")}
            aria-label={t("ai.history")}
            onClick={() => setHistoryOpen((o) => !o)}
          >
            <Icon name="clock" />
          </button>
          {historyOpen && (
            <div className="menu dropdown chat-history">
              {sessions.map((r) => (
                <div key={r.id} className={`chat-session${r.id === chat.id ? " current" : ""}`}>
                  <button className="chat-session-open" onClick={() => openSession(r)}>
                    <span className="ellipsis">{r.title}</span>
                    <span className="muted small">{formatDate(r.updated)}</span>
                  </button>
                  <button className="icon-button" title={t("common.delete")} aria-label={t("common.delete")} onClick={() => deleteSession(r.id)}>
                    <Icon name="close" size={13} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
      {tab === "chat" && ai.ready && (
        <button
          className="icon-button"
          title={t("ai.newChat")}
          aria-label={t("ai.newChat")}
          onClick={newChat}
          disabled={chat.turns.length === 0 && !chat.quote}
        >
          <Icon name="plus" />
        </button>
      )}
      <button className="icon-button" title={t("ai.settings")} aria-label={t("ai.settings")} onClick={ai.openSettings}>
        <Icon name="settings" />
      </button>
      <button className="icon-button" title={`${t("reader.collapseAi")} (${MOD}+Shift+A)`} aria-label={t("reader.collapseAi")} onClick={props.onClose}>
        <Icon name="panelRight" />
      </button>
    </div>
  );

  if (!ai.ready) {
    return (
      <aside className="ai-panel" style={{ width: props.width }}>
        {header}
        <div className="ai-welcome">
          <div className="ai-orb">
            <Icon name="sparkles" size={22} />
          </div>
          <div className="ai-welcome-title">{t("ai.notSetUp")}</div>
          <p className="muted small">{t("ai.notSetUpHint")}</p>
          <button className="primary" onClick={ai.openSettings}>
            {t("ai.openSettings")}
          </button>
        </div>
      </aside>
    );
  }

  const running = chat.status === "running";
  const lastAnswer = [...chat.turns].reverse().find((turn) => turn.role === "assistant" && turn.text);
  const suggestions: MessageKey[] = flow
    ? ["ai.suggestChapter", "ai.suggestPeople", "ai.suggestHard"]
    : ["ai.suggestPage", "ai.suggestTerms", "ai.suggestHard"];
  const map = maps.find((m) => m.id === mapId) ?? null;
  const mapBusy = maps.some((m) => m.status === "running");

  return (
    <aside className="ai-panel" style={{ width: props.width }}>
      {header}

      {tab === "chat" && (
        <>
          <div className="ai-scroll" ref={body} onScroll={onScroll}>
            {chat.turns.length === 0 && !chat.quote && (
              <div className="ai-welcome">
                <div className="ai-orb">
                  <Icon name="sparkles" size={22} />
                </div>
                <div className="ai-welcome-title">{t("ai.emptyTitle")}</div>
                <p className="muted small">{t("ai.emptyHint")}</p>
                <div className="ai-suggestions">
                  {suggestions.map((k) => (
                    <button key={k} onClick={() => send(t(k))}>
                      {t(k)}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {chat.quote && <QuoteCard quote={chat.quote} onRemove={() => setChat((c) => ({ ...c, quote: null }))} canRemove={!running} />}
            {chat.turns.map((turn, i) =>
              turn.role === "user" ? (
                <div key={i} className="msg user">
                  {turn.text}
                </div>
              ) : (
                <Answer
                  key={i}
                  turn={turn}
                  translation={!!chat.quote && chat.quote.action === "translate" && i === 0}
                  streaming={running && i === chat.turns.length - 1}
                  last={i === chat.turns.length - 1}
                  onCopy={() => copy(turn.text)}
                  onNote={
                    chat.quote?.anchor && turn.text
                      ? () => {
                          props.onSaveNote(chat.quote!, turn.text);
                          setNotice(t("ai.savedNote"));
                        }
                      : undefined
                  }
                  onRegenerate={!running && i === chat.turns.length - 1 ? regenerate : undefined}
                  onNavigate={props.onNavigate}
                />
              ),
            )}
            {chat.status === "stopped" && <div className="ai-status muted small">{t("ai.stopped")}</div>}
            {chat.error && (
              <div className="ai-error">
                <span>{chat.error}</span>
                <button className="link-button" onClick={regenerate}>
                  {t("ai.retry")}
                </button>
              </div>
            )}
          </div>

          <div className="ai-composer">
            {!chat.quote && (
              <div className="ai-basis muted">
                <Icon name="search" size={12} />
                <span className="ellipsis">
                  {(() => {
                    const how = semantic && semantic.passages > 0 ? t("ai.basisHybrid") : t("ai.basisKeyword");
                    return here ? t("ai.basedOnAt", { how, place: here }) : t("ai.basedOn", { how });
                  })()}
                </span>
                <span className="spacer" />
                {indexing ? (
                  <>
                    <span className="index-progress">{t("ai.indexing", { done: indexing.done, total: indexing.total || "…" })}</span>
                    <button className="link-button" onClick={() => (stop(indexing.taskId), setIndexing(null))}>
                      {t("common.stop")}
                    </button>
                  </>
                ) : (
                  semantic?.available &&
                  semantic.passages === 0 && (
                    <button className="link-button" title={t("ai.buildIndexTitle")} onClick={buildIndex}>
                      {semantic.stale ? t("ai.rebuildIndex") : t("ai.buildIndex")}
                    </button>
                  )
                )}
              </div>
            )}
            <div className="ai-input">
              <textarea
                ref={input}
                rows={1}
                value={question}
                placeholder={chat.quote ? t("ai.askSelectionPlaceholder") : t("ai.askBookPlaceholder")}
                onChange={(e) => setQuestion(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                    e.preventDefault();
                    send(question);
                  }
                }}
              />
              {running ? (
                <button className="ai-send stop" title={t("common.stop")} aria-label={t("common.stop")} onClick={() => stop(chat.taskId)}>
                  <Icon name="stop" size={14} filled />
                </button>
              ) : (
                <button className="ai-send" title={t("ai.send")} aria-label={t("ai.send")} disabled={!question.trim()} onClick={() => send(question)}>
                  <Icon name="arrowUp" size={16} />
                </button>
              )}
            </div>
            <div className="ai-input-foot">
              <ModelSwitch disabled={running} />
              <span className="ai-input-hint muted ellipsis">{lastAnswer || chat.quote ? t("ai.enterHint") : t("ai.selectHint")}</span>
            </div>
          </div>
        </>
      )}

      {tab === "summary" && (
        <div className="ai-scroll" ref={body} onScroll={onScroll}>
          <div className="ai-cards">
            <button className="ai-card" onClick={summarizeHere} disabled={summary?.status === "running" || !location}>
              <Icon name={flow ? "bookOpen" : "fitPage"} size={18} />
              <span className="ai-card-title">{flow ? t("ai.summarizeChapter") : t("ai.summarizePage")}</span>
              <span className="muted small ellipsis">{here ?? ""}</span>
            </button>
            <button className="ai-card" onClick={() => askEstimate("summary")} disabled={summary?.status === "running"}>
              <Icon name="library" size={18} />
              <span className="ai-card-title">{t("ai.summarizeBook")}</span>
              <span className="muted small ellipsis">{book.title}</span>
            </button>
          </div>
          {!summary && <p className="muted small ai-hint">{t("ai.summaryHint")}</p>}
          {summary && (
            <div className="ai-result">
              <div className="ai-result-title">
                <span className="ellipsis">{summary.title}</span>
                {summary.cached && <span className="badge-soft">{t("ai.cached")}</span>}
              </div>
              {summary.progress && <ProgressBar progress={summary.progress} />}
              {summary.text ? (
                <div className={summary.status === "running" ? "streaming" : undefined}>
                  <Markdown text={summary.text} />
                </div>
              ) : (
                summary.status === "running" && !summary.progress && <Dots />
              )}
              {summary.error && <div className="ai-error">{summary.error}</div>}
              {summary.status === "stopped" && <div className="muted small">{t("ai.stopped")}</div>}
              <div className="msg-actions visible">
                {summary.status === "running" ? (
                  <button onClick={() => stop(summary.taskId)}>
                    <Icon name="stop" size={12} filled /> {t("common.stop")}
                  </button>
                ) : (
                  <>
                    {summary.text && (
                      <button onClick={() => copy(summary.text)}>
                        <Icon name="copy" size={13} /> {t("common.copy")}
                      </button>
                    )}
                    {!summary.cached && (
                      <button onClick={() => startSummary(summary.title, summary.task)}>
                        <Icon name="refresh" size={13} /> {t("ai.regenerate")}
                      </button>
                    )}
                    {summary.usage && <span className="usage muted">{usageLabel(summary.usage)}</span>}
                  </>
                )}
              </div>
            </div>
          )}
        </div>
      )}

      {tab === "map" && (
        <div className="ai-scroll" ref={body} onScroll={onScroll}>
          <div className="ai-cards three">
            <button className="ai-card" onClick={mapHere} disabled={mapBusy || !location}>
              <Icon name={flow ? "bookOpen" : "fitPage"} size={18} />
              <span className="ai-card-title">{flow ? t("mm.scopeChapter") : t("mm.scopePages")}</span>
              <span className="muted small ellipsis">{here ?? ""}</span>
            </button>
            <button className="ai-card" onClick={() => askEstimate("mindmap")} disabled={mapBusy}>
              <Icon name="library" size={18} />
              <span className="ai-card-title">{t("mm.scopeBook")}</span>
              <span className="muted small ellipsis">{book.title}</span>
            </button>
            <button className="ai-card" onClick={mapConversation} disabled={mapBusy || conversationTurns.length === 0}>
              <Icon name="message" size={18} />
              <span className="ai-card-title">{t("mm.scopeConversation")}</span>
              <span className="muted small ellipsis">{t("mm.messages", { n: conversationTurns.length })}</span>
            </button>
          </div>
          {!map && <p className="muted small ai-hint">{t("mm.emptyHint")}</p>}
          {map && (
            <div className="ai-result">
              <div className="ai-result-title">
                <Icon name="mindmap" size={15} />
                <span className="ellipsis">{map.title}</span>
                <span className="badge-soft">{map.scopeLabel}</span>
                {map.cached && <span className="badge-soft">{t("ai.cached")}</span>}
              </div>
              {map.progress && <ProgressBar progress={map.progress} />}
              {map.outline ? (
                <MindMapPreview outline={map.outline} title={map.title} streaming={map.status === "running"} svgRef={mapSvg} onOpen={() => setViewing(map)} />
              ) : (
                map.status === "running" && !map.progress && <Dots />
              )}
              {map.error && <div className="ai-error">{map.error}</div>}
              {map.status === "stopped" && <div className="muted small">{t("ai.stopped")}</div>}
              <div className="msg-actions visible wrap">
                {map.status === "running" ? (
                  <button onClick={() => stop(map.taskId)}>
                    <Icon name="stop" size={12} filled /> {t("common.stop")}
                  </button>
                ) : (
                  <>
                    {map.outline && (
                      <>
                        <button onClick={() => setViewing(map)}>
                          <Icon name="maximize" size={13} /> {t("mm.open")}
                        </button>
                        <button onClick={() => copy(outlineMarkdown(map.outline, map.title))}>
                          <Icon name="copy" size={13} /> {t("mm.copyOutline")}
                        </button>
                        <button onClick={() => exportMap(map, "png")}>
                          <Icon name="filePlus" size={13} /> PNG
                        </button>
                        <button onClick={() => exportMap(map, "svg")}>
                          <Icon name="filePlus" size={13} /> SVG
                        </button>
                      </>
                    )}
                    <button onClick={() => drawMap(map.scope, map.title, map.scopeLabel, map)} disabled={mapBusy}>
                      <Icon name="refresh" size={13} /> {t("ai.regenerate")}
                    </button>
                  </>
                )}
              </div>
            </div>
          )}
          {maps.length > 1 && (
            <div className="mm-history">
              <div className="panel-label">{t("mm.history")}</div>
              {[...maps].reverse().map((m) => (
                <button key={m.id} className={`mm-history-item${m.id === mapId ? " on" : ""}`} onClick={() => setMapId(m.id)}>
                  <Icon name="mindmap" size={13} />
                  <span className="ellipsis">{m.title}</span>
                  <span className="muted small">{m.scopeLabel}</span>
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {notice && (
        <div className="ai-notice" onClick={() => setNotice(null)}>
          {notice}
        </div>
      )}
      {estimate && (
        <Modal
          title={estimate.purpose === "mindmap" ? t("mm.bookTitle") : t("ai.summarizeBook")}
          width={420}
          onClose={() => setEstimate(null)}
          footer={
            <>
              <button onClick={() => setEstimate(null)}>{t("common.cancel")}</button>
              <button
                className="primary"
                autoFocus
                onClick={() => {
                  const purpose = estimate.purpose;
                  setEstimate(null);
                  if (purpose === "mindmap") drawMap({ kind: "book" }, book.title, t("mm.scopeBook"));
                  else startSummary(t("ai.bookSummary"), { kind: "summarizeBook", bookId: book.id });
                }}
              >
                {t("common.start")}
              </button>
            </>
          }
        >
          {estimate.purpose === "summary" && estimate.data.bookCached ? (
            <p>{t("ai.bookCached")}</p>
          ) : (
            <>
              <p>
                {estimate.data.cachedUnits > 0
                  ? t("ai.bookUnitsCached", { n: estimate.data.units, cached: estimate.data.cachedUnits })
                  : t("ai.bookUnits", { n: estimate.data.units })}{" "}
                {estimate.purpose === "mindmap" ? t("mm.bookPlan") : t("ai.bookPlan")}
              </p>
              <p className="muted small" style={{ marginTop: 8 }}>
                {t("ai.estimateTokens", { input: formatNumber(estimate.data.inputTokens), output: formatNumber(estimate.data.outputTokens) })}{" "}
                {estimate.data.cost !== null &&
                  (estimate.data.cost < 0.01 ? t("ai.estimateCostSmall") : t("ai.estimateCost", { cost: estimate.data.cost.toFixed(2) })) + " "}
                {t("ai.estimateNote")}
              </p>
            </>
          )}
        </Modal>
      )}
      {viewing && <MindMapViewer outline={viewing.outline} title={viewing.title} onClose={() => setViewing(null)} />}
    </aside>
  );
}

function Dots() {
  return (
    <span className="ai-dots" aria-label={t("ai.thinking")}>
      <i />
      <i />
      <i />
    </span>
  );
}

function ProgressBar({ progress }: { progress: Progress }) {
  return (
    <div className="ai-progress">
      <div className="bar">
        <div style={{ width: `${(progress.done / Math.max(1, progress.total)) * 100}%` }} />
      </div>
      <span className="muted small ellipsis">
        {progress.total > 1 && `${progress.done}/${progress.total} · `}
        {progress.label}
      </span>
    </div>
  );
}

function QuoteCard(props: { quote: Quote; onRemove: () => void; canRemove: boolean }) {
  const [open, setOpen] = useState(false);
  const long = props.quote.selection.length > 120;
  return (
    <div className="ai-quote-card">
      <div className="ai-quote-head">
        <span className="badge-soft">{props.quote.action ? t(ACTION_LABEL[props.quote.action]) : t("ai.actionAsk")}</span>
        <span className="muted small">{t("ai.original")}</span>
        <span className="spacer" />
        {props.canRemove && (
          <button className="icon-button small" title={t("ai.removeQuote")} aria-label={t("ai.removeQuote")} onClick={props.onRemove}>
            <Icon name="close" size={13} />
          </button>
        )}
      </div>
      <div className={`ai-quote-text${open ? " open" : ""}`}>{props.quote.selection}</div>
      {long && (
        <button className="link-button" onClick={() => setOpen((o) => !o)}>
          {open ? t("ai.showLess") : t("ai.showMore")}
        </button>
      )}
    </div>
  );
}

function Answer(props: {
  turn: Turn;
  translation: boolean;
  streaming: boolean;
  last: boolean;
  onCopy: () => void;
  onNote?: () => void;
  onRegenerate?: () => void;
  onNavigate: (loc: Location) => void;
}) {
  const { turn } = props;
  const sources = turn.sources ?? [];
  const cite = sources.length
    ? {
        count: sources.length,
        onCite: (n: number) => sources[n - 1] && props.onNavigate(sources[n - 1].location),
        title: (n: number) => `${sources[n - 1].title} — ${sources[n - 1].excerpt}`,
      }
    : undefined;
  return (
    <div className={`msg ai${props.translation ? " translation" : ""}${props.streaming ? " streaming" : ""}`}>
      <div className="msg-head">
        <Icon name={props.translation ? "languages" : "sparkles"} size={13} />
        <span>{props.translation ? t("ai.translation") : "AI"}</span>
      </div>
      {turn.text ? <Markdown text={turn.text} cite={cite} /> : props.streaming && <Dots />}
      {sources.length > 0 && !props.streaming && (
        <details className="msg-sources">
          <summary>{t("ai.sources", { n: sources.length })}</summary>
          {sources.map((s) => (
            <button key={s.n} className="source" onClick={() => props.onNavigate(s.location)}>
              <span className="cite static">{s.n}</span>
              <span className="source-main">
                <span className="source-title ellipsis">{s.title}</span>
                <span className="source-excerpt">{s.excerpt}</span>
              </span>
            </button>
          ))}
        </details>
      )}
      {turn.text && !props.streaming && (
        <div className={`msg-actions${props.last ? " visible" : ""}`}>
          <button onClick={props.onCopy}>
            <Icon name="copy" size={13} /> {t("common.copy")}
          </button>
          {props.onNote && (
            <button onClick={props.onNote}>
              <Icon name="note" size={13} /> {t("ai.saveNote")}
            </button>
          )}
          {props.onRegenerate && (
            <button onClick={props.onRegenerate}>
              <Icon name="refresh" size={13} /> {t("ai.regenerate")}
            </button>
          )}
          {turn.usage && <span className="usage muted">{usageLabel(turn.usage)}</span>}
        </div>
      )}
    </div>
  );
}

/** Switches the chat model of the configured service without opening Settings. */
function ModelSwitch({ disabled }: { disabled: boolean }) {
  const ai = useAi();
  const status = ai.status;
  if (!status) return null;
  const current = status.config.model;
  const models = [...new Set([current, ...(presetOf(status.config.provider)?.models ?? [])])].filter(Boolean);
  if (models.length < 2) return null;
  return (
    <select
      className="model-switch"
      value={current}
      disabled={disabled}
      title={t("ai.model")}
      aria-label={t("ai.model")}
      onChange={(e) => {
        api
          .aiSetConfig({ ...status.config, model: e.target.value })
          .then(ai.setStatus)
          .catch(() => {});
      }}
    >
      {models.map((m) => (
        <option key={m} value={m}>
          {m}
        </option>
      ))}
    </select>
  );
}
