// Reader side panel for AI: summaries of the chapter/page/book, and conversations about a
// selected passage.

import { useEffect, useRef, useState } from "react";
import { api, errorText, runAiTask } from "../api";
import type { AiEvent, BookEstimate, ChatAction, ChatTurn, Location, OpenedBook } from "../types";
import { Modal } from "../ui";
import { formatNumber, t, type MessageKey } from "../i18n";
import { Icon } from "../icons";
import type { TextRange } from "../reader/anchors";
import { useAi } from "./AiContext";
import Markdown from "./Markdown";

/** A request from the selection toolbar to explain/translate a passage. */
export interface ChatRequest {
  n: number;
  /** Where the passage is in a text book; absent for PDF selections (no notes there). */
  anchor: { chapter: number; range: TextRange } | null;
  selection: string;
  context: string;
  action: ChatAction;
}

type Status = "running" | "done" | "error" | "stopped";

interface Summary {
  title: string;
  text: string;
  status: Status;
  progress: { done: number; total: number; label: string } | null;
  cached: boolean;
  error: string | null;
  taskId: string | null;
}

interface Chat {
  request: ChatRequest;
  turns: ChatTurn[];
  status: Status;
  error: string | null;
  taskId: string | null;
}

const ACTION_LABEL: Record<ChatAction, MessageKey> = {
  explain: "ai.actionExplain",
  translate: "ai.actionTranslate",
  summarize: "ai.actionSummarize",
};

export default function AiPanel(props: {
  book: OpenedBook;
  location: Location | null;
  chatRequest: ChatRequest | null;
  onSaveNote: (req: ChatRequest, note: string) => void;
  onClose: () => void;
}) {
  const { book, location } = props;
  const ai = useAi();
  const [tab, setTab] = useState<"summary" | "chat">("summary");
  const [summary, setSummary] = useState<Summary | null>(null);
  const [chat, setChat] = useState<Chat | null>(null);
  const [question, setQuestion] = useState("");
  const [estimate, setEstimate] = useState<BookEstimate | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const bottom = useRef<HTMLDivElement>(null);

  const startSummary = async (title: string, task: Parameters<typeof runAiTask>[0]) => {
    if (!(await ai.ensureConsent())) return;
    setTab("summary");
    const base: Summary = { title, text: "", status: "running", progress: null, cached: false, error: null, taskId: null };
    setSummary(base);
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
      setEstimate(await api.aiEstimateBook(book.id));
    } catch (e) {
      setNotice(errorText(e));
    }
  };

  const runChat = (c: Chat) => {
    const taskId = runAiTask(
      {
        kind: "chat",
        bookTitle: book.title,
        selection: c.request.selection,
        context: c.request.context,
        action: c.request.action,
        // The pending (last) assistant turn is what is being generated.
        history: c.turns.slice(0, -1),
      },
      (e: AiEvent) =>
        setChat((cur) => {
          if (!cur || cur.taskId !== taskId) return cur;
          const turns = [...cur.turns];
          const last = turns[turns.length - 1];
          switch (e.type) {
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
    setChat({ ...c, taskId });
  };

  // A new request from the selection toolbar starts a new conversation.
  useEffect(() => {
    const req = props.chatRequest;
    if (!req) return;
    (async () => {
      if (!(await ai.ensureConsent())) return;
      setTab("chat");
      runChat({ request: req, turns: [{ role: "assistant", text: "" }], status: "running", error: null, taskId: null });
    })();
    // Only a new request (its nonce) should start a conversation.
  }, [props.chatRequest?.n]);

  useEffect(() => {
    // Block body: scrollIntoView may return a promise, which must not become the cleanup.
    bottom.current?.scrollIntoView({ block: "end" });
  }, [chat?.turns.length, summary?.status]);

  const ask = () => {
    if (!chat || chat.status === "running" || !question.trim()) return;
    const turns: ChatTurn[] = [...chat.turns, { role: "user", text: question.trim() }, { role: "assistant", text: "" }];
    setQuestion("");
    runChat({ ...chat, turns, status: "running", error: null });
  };

  const stop = (taskId: string | null) => {
    if (taskId) api.aiCancel(taskId).catch(() => {});
    setSummary((s) => (s && s.taskId === taskId ? { ...s, status: "stopped", progress: null } : s));
    setChat((c) => (c && c.taskId === taskId ? { ...c, status: "stopped" } : c));
  };

  const copy = (text: string) => navigator.clipboard.writeText(text).then(() => setNotice(t("common.copied")), () => {});

  if (!ai.ready) {
    return (
      <aside className="ai-panel">
        <PanelHead tab={tab} setTab={setTab} onSettings={ai.openSettings} onClose={props.onClose} showChat={false} />
        <div className="ai-empty">
          <p>{t("ai.notSetUp")}</p>
          <p className="muted small">{t("ai.notSetUpHint")}</p>
          <button className="primary" onClick={ai.openSettings}>
            {t("ai.openSettings")}
          </button>
        </div>
      </aside>
    );
  }

  const flow = book.kind === "flow";
  const lastAnswer = chat?.turns.filter((turn) => turn.role === "assistant" && turn.text).at(-1)?.text;

  return (
    <aside className="ai-panel">
      <PanelHead tab={tab} setTab={setTab} onSettings={ai.openSettings} onClose={props.onClose} showChat />
      {tab === "summary" && (
        <div className="ai-body">
          <div className="ai-actions">
            <button onClick={summarizeHere} disabled={summary?.status === "running"}>
              {flow ? t("ai.summarizeChapter") : t("ai.summarizePage")}
            </button>
            <button onClick={askBookSummary} disabled={summary?.status === "running"}>
              {t("ai.summarizeBook")}
            </button>
          </div>
          {!summary && <p className="muted small ai-hint">{t("ai.summaryHint")}</p>}
          {summary && (
            <div className="ai-result">
              <div className="ai-result-title">
                <span>{summary.title}</span>
                {summary.cached && <span className="badge-soft">{t("ai.cached")}</span>}
              </div>
              {summary.progress && (
                <div className="ai-progress">
                  <div className="bar">
                    <div style={{ width: `${(summary.progress.done / Math.max(1, summary.progress.total)) * 100}%` }} />
                  </div>
                  <span className="muted small">{summary.progress.label}</span>
                </div>
              )}
              {summary.text ? <Markdown text={summary.text} /> : summary.status === "running" && !summary.progress && <p className="muted small">{t("ai.thinking")}</p>}
              {summary.error && <p className="error small">{summary.error}</p>}
              {summary.status === "stopped" && <p className="muted small">{t("ai.stopped")}</p>}
              <div className="ai-result-actions">
                {summary.status === "running" ? (
                  <button onClick={() => stop(summary.taskId)}>{t("common.stop")}</button>
                ) : (
                  summary.text && <button onClick={() => copy(summary.text)}>{t("common.copy")}</button>
                )}
              </div>
            </div>
          )}
          <div ref={bottom} />
        </div>
      )}
      {tab === "chat" && (
        <div className="ai-body">
          {!chat ? (
            <p className="muted small ai-hint">{t("ai.chatHint")}</p>
          ) : (
            <>
              <blockquote className="ai-quote">
                <span className="badge-soft">{t(ACTION_LABEL[chat.request.action])}</span> {chat.request.selection}
              </blockquote>
              {chat.turns.map((turn, i) =>
                turn.role === "user" ? (
                  <div key={i} className="ai-user">
                    {turn.text}
                  </div>
                ) : (
                  <div key={i} className="ai-answer">
                    {turn.text ? <Markdown text={turn.text} /> : chat.status === "running" && <p className="muted small">{t("ai.thinking")}</p>}
                  </div>
                ),
              )}
              {chat.error && <p className="error small">{chat.error}</p>}
              <div className="ai-result-actions">
                {chat.status === "running" ? (
                  <button onClick={() => stop(chat.taskId)}>{t("common.stop")}</button>
                ) : (
                  lastAnswer && (
                    <>
                      <button onClick={() => copy(lastAnswer)}>{t("common.copy")}</button>
                      {chat.request.anchor && (
                        <button
                          onClick={() => {
                            props.onSaveNote(chat.request, lastAnswer);
                            setNotice(t("ai.savedNote"));
                          }}
                        >
                          {t("ai.saveNote")}
                        </button>
                      )}
                    </>
                  )
                )}
              </div>
              <div ref={bottom} />
            </>
          )}
          {chat && (
            <div className="ai-ask">
              <textarea
                value={question}
                placeholder={t("ai.askPlaceholder")}
                onChange={(e) => setQuestion(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                    e.preventDefault();
                    ask();
                  }
                }}
              />
              <button className="primary" onClick={ask} disabled={chat.status === "running" || !question.trim()}>
                {t("ai.send")}
              </button>
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
          title={t("ai.summarizeBook")}
          width={420}
          onClose={() => setEstimate(null)}
          footer={
            <>
              <button onClick={() => setEstimate(null)}>{t("common.cancel")}</button>
              <button
                className="primary"
                autoFocus
                onClick={() => {
                  setEstimate(null);
                  startSummary(t("ai.bookSummary"), { kind: "summarizeBook", bookId: book.id });
                }}
              >
                {t("common.start")}
              </button>
            </>
          }
        >
          {estimate.bookCached ? (
            <p>{t("ai.bookCached")}</p>
          ) : (
            <>
              <p>
                {estimate.cachedUnits > 0
                  ? t("ai.bookUnitsCached", { n: estimate.units, cached: estimate.cachedUnits })
                  : t("ai.bookUnits", { n: estimate.units })}{" "}
                {t("ai.bookPlan")}
              </p>
              <p className="muted small" style={{ marginTop: 8 }}>
                {t("ai.estimateTokens", { input: formatNumber(estimate.inputTokens), output: formatNumber(estimate.outputTokens) })}{" "}
                {estimate.cost !== null &&
                  (estimate.cost < 0.01 ? t("ai.estimateCostSmall") : t("ai.estimateCost", { cost: estimate.cost.toFixed(2) })) + " "}
                {t("ai.estimateNote")}
              </p>
            </>
          )}
        </Modal>
      )}
    </aside>
  );
}

function PanelHead(props: { tab: string; setTab: (t: "summary" | "chat") => void; onSettings: () => void; onClose: () => void; showChat: boolean }) {
  return (
    <div className="side-tabs ai-tabs">
      <button className={props.tab === "summary" ? "on" : ""} onClick={() => props.setTab("summary")}>
        {t("ai.summaryTab")}
      </button>
      {props.showChat && (
        <button className={props.tab === "chat" ? "on" : ""} onClick={() => props.setTab("chat")}>
          {t("ai.chatTab")}
        </button>
      )}
      <button className="icon tab-icon" title={t("ai.settings")} aria-label={t("ai.settings")} onClick={props.onSettings}>
        <Icon name="settings" />
      </button>
      <button className="icon tab-icon" title={t("common.close")} aria-label={t("common.close")} onClick={props.onClose}>
        <Icon name="close" />
      </button>
    </div>
  );
}
