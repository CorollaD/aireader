// Reader side panel for AI. "Chat" answers questions about the whole book from retrieved
// passages (with [n] citations that jump to the text), or about a selected passage (explain,
// translate, ask), and draws mind maps of the chapter, the book or the conversation. "Summary"
// summarises the chapter, page or whole book.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, errorText, runAiTask } from "../api";
import type { AiEvent, AiTask, BookEstimate, ChatAction, Location, MindMapScope, OpenedBook, Source } from "../types";
import { Modal } from "../ui";
import type { TextRange } from "../reader/anchors";
import { formatNumber, t, type MessageKey } from "../i18n";
import { Icon } from "../icons";
import { describe } from "../reader/locations";
import { useAi } from "./AiContext";
import Markdown, { type FenceRenderer } from "./Markdown";
import { MindMapCard, MindMapViewer } from "./MindMap";

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

type Status = "idle" | "running" | "done" | "error" | "stopped";
type Progress = { done: number; total: number; label: string };

interface Turn {
  role: "user" | "assistant";
  text: string;
  sources?: Source[];
  /** Set for answers drawn by the mind-map task. */
  mindmap?: { title: string; scope: MindMapScope; cached?: boolean };
  progress?: Progress | null;
}

interface Chat {
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
}

/** A mind map opened in the full-window viewer. */
interface MapView {
  outline: string;
  title: string;
  sources?: Source[];
}

const ACTION_LABEL: Record<ChatAction, MessageKey> = {
  explain: "ai.actionExplain",
  translate: "ai.actionTranslate",
  summarize: "ai.actionSummarize",
};

const EMPTY_CHAT: Chat = { turns: [], quote: null, status: "idle", error: null, taskId: null };

export default function AiPanel(props: {
  book: OpenedBook;
  location: Location | null;
  chatRequest: ChatRequest | null;
  onSaveNote: (quote: Quote, note: string) => void;
  onNavigate: (loc: Location) => void;
  onClose: () => void;
}) {
  const { book, location } = props;
  const ai = useAi();
  const [tab, setTab] = useState<"chat" | "summary">("chat");
  const [chat, setChat] = useState<Chat>(EMPTY_CHAT);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [question, setQuestion] = useState("");
  const [estimate, setEstimate] = useState<{ data: BookEstimate; purpose: "summary" | "mindmap" } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [mapMenu, setMapMenu] = useState(false);
  const [mapView, setMapView] = useState<MapView | null>(null);
  const body = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLTextAreaElement>(null);

  const flow = book.kind === "flow";
  const here = location ? describe(book, location) : null;

  // --- chat ------------------------------------------------------------------------------------

  /** Runs `task`; its output fills the last turn of `c`. */
  const start = (c: Chat, task: AiTask) => {
    const taskId = runAiTask(task, (e: AiEvent) =>
      setChat((cur) => {
        if (cur.taskId !== taskId) return cur;
        const turns = [...cur.turns];
        const last = turns[turns.length - 1];
        switch (e.type) {
          case "sources":
            turns[turns.length - 1] = { ...last, sources: e.sources };
            return { ...cur, turns };
          case "progress":
            turns[turns.length - 1] = { ...last, progress: e };
            return { ...cur, turns };
          case "delta":
            turns[turns.length - 1] = { ...last, text: last.text + e.text, progress: null };
            return { ...cur, turns };
          case "done": {
            const mindmap = last.mindmap && { ...last.mindmap, cached: e.cached };
            turns[turns.length - 1] = { ...last, text: e.text, progress: null, mindmap };
            return { ...cur, turns, status: "done" };
          }
          case "error":
            return { ...cur, turns: turns.slice(0, -1), status: "error", error: e.message };
        }
      }),
    );
    setChat({ ...c, status: "running", error: null, taskId });
  };

  /** Answers the conversation whose last turn is the (empty) answer being generated. */
  const run = (c: Chat) => {
    const history = c.turns.slice(0, -1).map(({ role, text }) => ({ role, text }));
    start(
      c,
      c.quote
        ? {
            kind: "chat",
            bookTitle: book.title,
            selection: c.quote.selection,
            context: c.quote.context,
            action: c.quote.action,
            history,
          }
        : { kind: "ask", bookId: book.id, location, history },
    );
  };

  const send = async (text: string) => {
    const q = text.trim();
    if (!q || chat.status === "running") return;
    if (!(await ai.ensureConsent())) return;
    setQuestion("");
    setTab("chat");
    run({ ...chat, turns: [...chat.turns, { role: "user", text: q }, { role: "assistant", text: "" }] });
  };

  /** Draws a mind map as a new exchange, or in place of the last answer (`replace`). */
  const drawMap = async (scope: MindMapScope, title: string, request: string, opts: { refresh?: boolean; replace?: boolean } = {}) => {
    if (chat.status === "running") return;
    if (!(await ai.ensureConsent())) return;
    setTab("chat");
    const answer: Turn = { role: "assistant", text: "", mindmap: { title, scope } };
    const turns = opts.replace ? [...chat.turns.slice(0, -1), answer] : [...chat.turns, { role: "user" as const, text: request }, answer];
    start({ ...chat, turns }, { kind: "mindMap", bookId: book.id, scope, refresh: opts.refresh });
  };

  const mapChapter = () => {
    if (!location) return;
    if (location.kind === "flow") {
      const title = book.chapters[location.chapter]?.title || t("loc.chapter", { n: location.chapter + 1 });
      drawMap({ kind: "chapter", chapter: location.chapter }, title, t("mm.requestChapter", { title }));
    } else {
      const from = Math.max(0, location.page - 1);
      const to = Math.min(book.pages.length - 1, location.page + 1);
      const title = t("loc.page", { n: location.page + 1 });
      drawMap({ kind: "pages", from, to }, title, t("mm.requestPages", { title }));
    }
  };

  const mapBook = async () => {
    if (!(await ai.ensureConsent())) return;
    try {
      setEstimate({ data: await api.aiEstimateBook(book.id), purpose: "mindmap" });
    } catch (e) {
      setNotice(errorText(e));
    }
  };

  const mapConversation = () => {
    const text = chat.turns
      .filter((turn) => turn.text && !turn.mindmap)
      .map((turn) => `${turn.role === "user" ? "Q" : "A"}: ${turn.text}`)
      .join("\n\n");
    const material = chat.quote ? `> ${chat.quote.selection}\n\n${text}` : text;
    const title = t("mm.conversationTitle");
    drawMap({ kind: "text", title, text: material }, title, t("mm.requestConversation"));
  };

  const regenerate = () => {
    if (chat.status === "running") return;
    const last = chat.turns[chat.turns.length - 1];
    if (last?.role === "assistant" && last.mindmap) {
      drawMap(last.mindmap.scope, last.mindmap.title, "", { refresh: true, replace: true });
      return;
    }
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

  // Keep the newest message in view while it streams.
  const lastText = chat.turns[chat.turns.length - 1]?.text;
  useEffect(() => {
    const el = body.current;
    if (el) el.scrollTop = el.scrollHeight;
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

  const askBookSummary = async () => {
    if (!(await ai.ensureConsent())) return;
    try {
      setEstimate({ data: await api.aiEstimateBook(book.id), purpose: "summary" });
    } catch (e) {
      setNotice(errorText(e));
    }
  };

  function stop(taskId: string | null) {
    if (!taskId) return;
    api.aiCancel(taskId).catch(() => {});
    setSummary((s) => (s && s.taskId === taskId && s.status === "running" ? { ...s, status: "stopped", progress: null } : s));
    setChat((c) => {
      if (c.taskId !== taskId || c.status !== "running") return c;
      // Drop an answer that never started.
      const last = c.turns[c.turns.length - 1];
      const turns = last?.role === "assistant" && !last.text ? c.turns.slice(0, -1) : c.turns;
      return { ...c, turns, status: "stopped" };
    });
  }

  const copy = (text: string) => navigator.clipboard.writeText(text).then(() => setNotice(t("common.copied")), () => {});

  useEffect(() => {
    if (!notice) return;
    const id = window.setTimeout(() => setNotice(null), 2500);
    return () => window.clearTimeout(id);
  }, [notice]);

  // --- rendering -------------------------------------------------------------------------------

  const header = (
    <div className="ai-head">
      <div className="ai-switch" role="tablist">
        <button role="tab" aria-selected={tab === "chat"} className={tab === "chat" ? "on" : ""} onClick={() => setTab("chat")}>
          <Icon name="message" size={14} />
          {t("ai.chatTab")}
        </button>
        <button role="tab" aria-selected={tab === "summary"} className={tab === "summary" ? "on" : ""} onClick={() => setTab("summary")}>
          <Icon name="bookOpen" size={14} />
          {t("ai.summaryTab")}
        </button>
      </div>
      <span className="spacer" />
      {tab === "chat" && ai.ready && (
        <button className="icon-button" title={t("ai.newChat")} aria-label={t("ai.newChat")} onClick={newChat} disabled={chat.turns.length === 0 && !chat.quote}>
          <Icon name="plus" />
        </button>
      )}
      <button className="icon-button" title={t("ai.settings")} aria-label={t("ai.settings")} onClick={ai.openSettings}>
        <Icon name="settings" />
      </button>
      <button className="icon-button" title={t("common.close")} aria-label={t("common.close")} onClick={props.onClose}>
        <Icon name="close" />
      </button>
    </div>
  );

  if (!ai.ready) {
    return (
      <aside className="ai-panel">
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
  const suggestions: { label: string; run: () => void }[] = [
    ...(flow
      ? (["ai.suggestChapter", "ai.suggestPeople"] as MessageKey[])
      : (["ai.suggestPage", "ai.suggestTerms"] as MessageKey[])
    ).map((k) => ({ label: t(k), run: () => send(t(k)) })),
    { label: flow ? t("mm.suggestChapter") : t("mm.suggestPages"), run: mapChapter },
  ];
  const hasConversation = chat.turns.some((turn) => turn.text && !turn.mindmap);

  return (
    <aside className="ai-panel" onClick={() => setMapMenu(false)}>
      {header}

      {tab === "chat" && (
        <>
          <div className="ai-scroll" ref={body}>
            {chat.turns.length === 0 && !chat.quote && (
              <div className="ai-welcome">
                <div className="ai-orb">
                  <Icon name="sparkles" size={22} />
                </div>
                <div className="ai-welcome-title">{t("ai.emptyTitle")}</div>
                <p className="muted small">{t("ai.emptyHint")}</p>
                <div className="ai-suggestions">
                  {suggestions.map((s) => (
                    <button key={s.label} onClick={s.run}>
                      {s.label}
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
                    chat.quote?.anchor && turn.text && !turn.mindmap
                      ? () => {
                          props.onSaveNote(chat.quote!, turn.text);
                          setNotice(t("ai.savedNote"));
                        }
                      : undefined
                  }
                  onRegenerate={!running && i === chat.turns.length - 1 ? regenerate : undefined}
                  onNavigate={props.onNavigate}
                  onOpenMap={setMapView}
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
                <span className="ellipsis">{here ? t("ai.basisAt", { place: here }) : t("ai.basis")}</span>
              </div>
            )}
            <div className="ai-input">
              <div className="menu-anchor">
                <button
                  className={`ai-tool${mapMenu ? " on" : ""}`}
                  title={t("mm.create")}
                  aria-label={t("mm.create")}
                  disabled={running}
                  onClick={(e) => {
                    e.stopPropagation();
                    setMapMenu((m) => !m);
                  }}
                >
                  <Icon name="mindmap" size={17} />
                </button>
                {mapMenu && (
                  <div className="menu dropup" onClick={(e) => e.stopPropagation()}>
                    <div className="menu-label">{t("mm.create")}</div>
                    <button onClick={() => (setMapMenu(false), mapChapter())} disabled={!location}>
                      <Icon name={flow ? "bookOpen" : "fitPage"} /> {flow ? t("mm.scopeChapter") : t("mm.scopePages")}
                    </button>
                    <button onClick={() => (setMapMenu(false), mapBook())}>
                      <Icon name="library" /> {t("mm.scopeBook")}
                    </button>
                    <button onClick={() => (setMapMenu(false), mapConversation())} disabled={!hasConversation}>
                      <Icon name="message" /> {t("mm.scopeConversation")}
                    </button>
                  </div>
                )}
              </div>
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
            <div className="ai-input-hint muted">{lastAnswer || chat.quote ? t("ai.enterHint") : t("ai.selectHint")}</div>
          </div>
        </>
      )}

      {tab === "summary" && (
        <div className="ai-scroll" ref={body}>
          <div className="ai-cards">
            <button className="ai-card" onClick={summarizeHere} disabled={summary?.status === "running" || !location}>
              <Icon name={flow ? "bookOpen" : "fitPage"} size={18} />
              <span className="ai-card-title">{flow ? t("ai.summarizeChapter") : t("ai.summarizePage")}</span>
              <span className="muted small ellipsis">{here ?? ""}</span>
            </button>
            <button className="ai-card" onClick={askBookSummary} disabled={summary?.status === "running"}>
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
                  </>
                )}
              </div>
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
                  if (purpose === "mindmap") drawMap({ kind: "book" }, book.title, t("mm.requestBook"));
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
      {mapView && (
        <MindMapViewer
          outline={mapView.outline}
          title={mapView.title}
          onCite={mapView.sources?.length ? (n) => mapView.sources?.[n - 1] && props.onNavigate(mapView.sources[n - 1].location) : undefined}
          onClose={() => setMapView(null)}
        />
      )}
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
  onOpenMap: (view: MapView) => void;
}) {
  const { turn } = props;
  const sources = turn.sources ?? [];
  const onCite = sources.length ? (n: number) => sources[n - 1] && props.onNavigate(sources[n - 1].location) : undefined;
  const cite = onCite
    ? { count: sources.length, onCite, title: (n: number) => `${sources[n - 1].title} — ${sources[n - 1].excerpt}` }
    : undefined;
  // A mind map the model wrote in a ```mindmap block of an ordinary answer.
  const fence: FenceRenderer = (lang, code) =>
    lang === "mindmap" ? (
      <MindMapCard
        outline={code}
        title={t("mm.title")}
        streaming={props.streaming}
        onOpen={() => props.onOpenMap({ outline: code, title: t("mm.title"), sources })}
      />
    ) : null;
  const map = turn.mindmap;
  const kind = map ? "mindmap" : props.translation ? "translation" : "";
  return (
    <div className={`msg ai ${kind}${props.streaming ? " streaming" : ""}`}>
      <div className="msg-head">
        <Icon name={map ? "mindmap" : props.translation ? "languages" : "sparkles"} size={13} />
        <span className="ellipsis">{map ? map.title : props.translation ? t("ai.translation") : "AI"}</span>
        {map?.cached && <span className="badge-soft">{t("ai.cached")}</span>}
      </div>
      {turn.progress && <ProgressBar progress={turn.progress} />}
      {map ? (
        turn.text ? (
          <MindMapCard
            outline={turn.text}
            title={map.title}
            streaming={props.streaming}
            onOpen={() => props.onOpenMap({ outline: turn.text, title: map.title })}
          />
        ) : (
          props.streaming && !turn.progress && <Dots />
        )
      ) : turn.text ? (
        <Markdown text={turn.text} cite={cite} fence={fence} />
      ) : (
        props.streaming && <Dots />
      )}
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
            <Icon name="copy" size={13} /> {map ? t("mm.copyOutline") : t("common.copy")}
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
        </div>
      )}
    </div>
  );
}
