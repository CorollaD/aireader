// Reader side panel for AI: summaries of the chapter/page/book, and conversations about a
// selected passage.

import { useEffect, useRef, useState } from "react";
import { api, errorText, runAiTask } from "../api";
import type { AiEvent, BookEstimate, ChatAction, ChatTurn, Location, OpenedBook } from "../types";
import { Modal } from "../ui";
import type { TextRange } from "../reader/anchors";
import { useAi } from "./AiContext";
import Markdown from "./Markdown";

/** A request from the selection toolbar to explain/translate a passage. */
export interface ChatRequest {
  n: number;
  chapter: number;
  range: TextRange;
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

const ACTION_LABEL: Record<ChatAction, string> = { explain: "解释", translate: "翻译", summarize: "概括" };

function formatTokens(n: number) {
  return n >= 10_000 ? `${(n / 10_000).toFixed(1)} 万` : String(n);
}

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
      const title = book.chapters[location.chapter]?.title || `第 ${location.chapter + 1} 章`;
      startSummary(`本章：${title}`, { kind: "summarizeChapter", bookId: book.id, chapter: location.chapter });
    } else {
      startSummary(`第 ${location.page + 1} 页`, { kind: "summarizePages", bookId: book.id, from: location.page, to: location.page });
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

  const copy = (text: string) => navigator.clipboard.writeText(text).then(() => setNotice("已复制"), () => {});

  if (!ai.ready) {
    return (
      <aside className="ai-panel">
        <PanelHead tab={tab} setTab={setTab} onSettings={ai.openSettings} onClose={props.onClose} showChat={false} />
        <div className="ai-empty">
          <p>还没有配置 AI 服务。</p>
          <p className="muted small">可以使用 Claude、OpenAI、DeepSeek、Kimi、智谱、Gemini 等云端服务，或本机的 Ollama / llama.cpp。</p>
          <button className="primary" onClick={ai.openSettings}>
            打开 AI 设置
          </button>
        </div>
      </aside>
    );
  }

  const flow = book.kind === "flow";
  const lastAnswer = chat?.turns.filter((t) => t.role === "assistant" && t.text).at(-1)?.text;

  return (
    <aside className="ai-panel">
      <PanelHead tab={tab} setTab={setTab} onSettings={ai.openSettings} onClose={props.onClose} showChat={flow} />
      {tab === "summary" && (
        <div className="ai-body">
          <div className="ai-actions">
            <button onClick={summarizeHere} disabled={summary?.status === "running"}>
              {flow ? "总结本章" : "总结本页"}
            </button>
            <button onClick={askBookSummary} disabled={summary?.status === "running"}>
              总结全书
            </button>
          </div>
          {!summary && <p className="muted small ai-hint">选择要总结的范围。{flow && "在正文中选中文字，可以让 AI 解释或翻译。"}</p>}
          {summary && (
            <div className="ai-result">
              <div className="ai-result-title">
                <span>{summary.title}</span>
                {summary.cached && <span className="badge-soft">已缓存</span>}
              </div>
              {summary.progress && (
                <div className="ai-progress">
                  <div className="bar">
                    <div style={{ width: `${(summary.progress.done / Math.max(1, summary.progress.total)) * 100}%` }} />
                  </div>
                  <span className="muted small">{summary.progress.label}</span>
                </div>
              )}
              {summary.text ? <Markdown text={summary.text} /> : summary.status === "running" && !summary.progress && <p className="muted small">思考中…</p>}
              {summary.error && <p className="error small">{summary.error}</p>}
              {summary.status === "stopped" && <p className="muted small">已停止</p>}
              <div className="ai-result-actions">
                {summary.status === "running" ? (
                  <button onClick={() => stop(summary.taskId)}>停止</button>
                ) : (
                  summary.text && <button onClick={() => copy(summary.text)}>复制</button>
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
            <p className="muted small ai-hint">在正文中选中文字，点击工具条上的「解释」或「翻译」。</p>
          ) : (
            <>
              <blockquote className="ai-quote">
                <span className="badge-soft">{ACTION_LABEL[chat.request.action]}</span> {chat.request.selection}
              </blockquote>
              {chat.turns.map((t, i) =>
                t.role === "user" ? (
                  <div key={i} className="ai-user">
                    {t.text}
                  </div>
                ) : (
                  <div key={i} className="ai-answer">
                    {t.text ? <Markdown text={t.text} /> : chat.status === "running" && <p className="muted small">思考中…</p>}
                  </div>
                ),
              )}
              {chat.error && <p className="error small">{chat.error}</p>}
              <div className="ai-result-actions">
                {chat.status === "running" ? (
                  <button onClick={() => stop(chat.taskId)}>停止</button>
                ) : (
                  lastAnswer && (
                    <>
                      <button onClick={() => copy(lastAnswer)}>复制</button>
                      <button
                        onClick={() => {
                          props.onSaveNote(chat.request, lastAnswer);
                          setNotice("已存为笔记");
                        }}
                      >
                        存为笔记
                      </button>
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
                placeholder="继续追问…（Enter 发送，Shift+Enter 换行）"
                onChange={(e) => setQuestion(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                    e.preventDefault();
                    ask();
                  }
                }}
              />
              <button className="primary" onClick={ask} disabled={chat.status === "running" || !question.trim()}>
                发送
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
          title="总结全书"
          width={420}
          onClose={() => setEstimate(null)}
          footer={
            <>
              <button onClick={() => setEstimate(null)}>取消</button>
              <button
                className="primary"
                autoFocus
                onClick={() => {
                  setEstimate(null);
                  startSummary("全书总结", { kind: "summarizeBook", bookId: book.id });
                }}
              >
                开始
              </button>
            </>
          }
        >
          {estimate.bookCached ? (
            <p>这本书已经有当前模型生成的全书总结，将直接显示，不再请求 AI 服务。</p>
          ) : (
            <>
              <p>
                全书共 {estimate.units} 个部分
                {estimate.cachedUnits > 0 && `，其中 ${estimate.cachedUnits} 个已有总结`}。将先逐个总结，再汇总成全书总结。
              </p>
              <p className="muted small" style={{ marginTop: 8 }}>
                预计输入约 {formatTokens(estimate.inputTokens)} tokens，输出约 {formatTokens(estimate.outputTokens)} tokens
                {estimate.cost !== null && `，费用约 $${estimate.cost < 0.01 ? "0.01 以内" : estimate.cost.toFixed(2)}`}
                。实际用量以服务商账单为准。
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
        AI 总结
      </button>
      {props.showChat && (
        <button className={props.tab === "chat" ? "on" : ""} onClick={() => props.setTab("chat")}>
          划词对话
        </button>
      )}
      <button className="icon tab-icon" title="AI 设置" onClick={props.onSettings}>
        ⚙
      </button>
      <button className="icon tab-icon" title="关闭" onClick={props.onClose}>
        ×
      </button>
    </div>
  );
}
