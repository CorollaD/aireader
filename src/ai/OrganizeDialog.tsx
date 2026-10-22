// AI bookshelf organisation: ask for group/author suggestions, let the user review and edit
// them, and apply only what was accepted.

import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import type { Book, BookPatch, Suggestion } from "../types";
import { Modal } from "../ui";
import { useAi } from "./AiContext";

interface Row {
  suggestion: Suggestion;
  book: Book;
  accept: boolean;
  /** Editable, separated by 、 or commas. */
  groups: string;
  acceptAuthor: boolean;
}

const splitGroups = (s: string) =>
  s
    .split(/[、,，;；]/)
    .map((g) => g.trim())
    .filter(Boolean);

export default function OrganizeDialog(props: {
  books: Book[];
  onApply: (changes: { id: string; patch: BookPatch }[]) => Promise<void>;
  onClose: () => void;
}) {
  const ai = useAi();
  const [phase, setPhase] = useState<"running" | "review" | "error">("running");
  const [rows, setRows] = useState<Row[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      if (!(await ai.ensureConsent())) {
        props.onClose();
        return;
      }
      try {
        const suggestions = await api.aiOrganize(props.books.map((b) => b.id));
        if (cancelled) return;
        const byId = new Map(props.books.map((b) => [b.id, b]));
        setRows(
          suggestions
            .filter((s) => byId.has(s.bookId))
            .map((s) => ({
              suggestion: s,
              book: byId.get(s.bookId)!,
              // Groups the book already has are not new.
              groups: s.groups.filter((g) => !byId.get(s.bookId)!.groups.includes(g)).join("、"),
              accept: true,
              acceptAuthor: !!s.author,
            }))
            .filter((r) => r.groups || r.suggestion.author),
        );
        setPhase("review");
      } catch (e) {
        if (!cancelled) {
          setError(errorText(e));
          setPhase("error");
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // Runs once when the dialog opens.
  }, []);

  const update = (i: number, patch: Partial<Row>) => setRows((rs) => rs.map((r, j) => (j === i ? { ...r, ...patch } : r)));

  const apply = async () => {
    setApplying(true);
    const changes = rows
      .filter((r) => r.accept)
      .map((r) => {
        const patch: BookPatch = {};
        const added = splitGroups(r.groups).filter((g) => !r.book.groups.includes(g));
        if (added.length) patch.groups = [...r.book.groups, ...added];
        if (r.acceptAuthor && r.suggestion.author) patch.author = r.suggestion.author;
        return { id: r.book.id, patch };
      })
      .filter((c) => c.patch.groups || c.patch.author);
    try {
      await props.onApply(changes);
      props.onClose();
    } catch (e) {
      setError(errorText(e));
      setApplying(false);
    }
  };

  const accepted = rows.filter((r) => r.accept).length;

  return (
    <Modal
      title={`AI 整理（${props.books.length} 本书）`}
      width={640}
      onClose={props.onClose}
      footer={
        phase === "review" ? (
          <>
            <span className="muted small">只会添加分组，不会移除书原有的分组</span>
            <span className="spacer" />
            <button onClick={props.onClose}>取消</button>
            <button className="primary" disabled={accepted === 0 || applying} onClick={apply}>
              应用 {accepted} 项
            </button>
          </>
        ) : (
          <button onClick={props.onClose}>关闭</button>
        )
      }
    >
      {phase === "running" && <p className="muted">正在请 AI 分析书目，书多时需要一些时间…</p>}
      {phase === "error" && <p className="error">{error}</p>}
      {phase === "review" &&
        (rows.length === 0 ? (
          <p className="muted">AI 没有给出新的分组建议，书架看起来已经整理好了。</p>
        ) : (
          <div className="organize-list">
            {rows.map((r, i) => (
              <div key={r.book.id} className={`organize-row${r.accept ? "" : " off"}`}>
                <input type="checkbox" checked={r.accept} onChange={(e) => update(i, { accept: e.target.checked })} />
                <div className="organize-main">
                  <div className="ellipsis">
                    <strong>{r.book.title}</strong>
                    {r.book.groups.length > 0 && <span className="muted small">　现有：{r.book.groups.join("、")}</span>}
                  </div>
                  <label className="organize-field">
                    <span className="muted small">添加分组</span>
                    <input value={r.groups} disabled={!r.accept} onChange={(e) => update(i, { groups: e.target.value })} />
                  </label>
                  {r.suggestion.author && (
                    <label className="organize-field toggle">
                      <input
                        type="checkbox"
                        checked={r.acceptAuthor}
                        disabled={!r.accept}
                        onChange={(e) => update(i, { acceptAuthor: e.target.checked })}
                      />
                      <span className="small">作者设为「{r.suggestion.author}」</span>
                    </label>
                  )}
                </div>
              </div>
            ))}
            {error && <p className="error small">{error}</p>}
          </div>
        ))}
    </Modal>
  );
}
