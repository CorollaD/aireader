// AI bookshelf organisation: ask for a group (each book is in one) and missing authors, let the
// user review and edit the suggestions, and apply only what was accepted. Books that already
// have a group start unchecked, so nothing is moved without the user's say.

import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import type { Book, BookPatch, Suggestion } from "../types";
import { Modal } from "../ui";
import { t } from "../i18n";
import { useAi } from "./AiContext";

interface Row {
  suggestion: Suggestion;
  book: Book;
  accept: boolean;
  /** The group to move the book to (editable); empty keeps its current group. */
  group: string;
  acceptAuthor: boolean;
}

export default function OrganizeDialog(props: { books: Book[]; onApply: (changes: { id: string; patch: BookPatch }[]) => Promise<void>; onClose: () => void }) {
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
            .map((s) => {
              const book = byId.get(s.bookId)!;
              return {
                suggestion: s,
                book,
                group: s.group ?? "",
                // Moving a book out of its group needs an explicit tick; filling gaps doesn't.
                accept: !book.group || !s.group,
                acceptAuthor: !!s.author,
              };
            })
            .filter((r) => r.group || r.suggestion.author),
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
        const group = r.group.trim();
        if (group && group !== r.book.group) patch.group = group;
        if (r.acceptAuthor && r.suggestion.author) patch.author = r.suggestion.author;
        return { id: r.book.id, patch };
      })
      .filter((c) => c.patch.group || c.patch.author);
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
      title={t("org.title", { n: props.books.length })}
      width={640}
      onClose={props.onClose}
      footer={
        phase === "review" ? (
          <>
            <span className="muted small">{t("org.onlyAdd")}</span>
            <span className="spacer" />
            <button onClick={props.onClose}>{t("common.cancel")}</button>
            <button className="primary" disabled={accepted === 0 || applying} onClick={apply}>
              {t("org.apply", { n: accepted })}
            </button>
          </>
        ) : (
          <button onClick={props.onClose}>{t("common.close")}</button>
        )
      }
    >
      {phase === "running" && <p className="muted">{t("org.running")}</p>}
      {phase === "error" && <p className="error">{error}</p>}
      {phase === "review" &&
        (rows.length === 0 ? (
          <p className="muted">{t("org.nothing")}</p>
        ) : (
          <div className="organize-list">
            {rows.map((r, i) => (
              <div key={r.book.id} className={`organize-row${r.accept ? "" : " off"}`}>
                <input type="checkbox" checked={r.accept} onChange={(e) => update(i, { accept: e.target.checked })} />
                <div className="organize-main">
                  <div className="ellipsis">
                    <strong>{r.book.title}</strong>
                  </div>
                  {r.suggestion.group && (
                    <label className="organize-field">
                      <span className={`group-chip${r.book.group ? "" : " none"}`}>{r.book.group ?? t("shelf.ungrouped")}</span>
                      <span className="muted">→</span>
                      <input value={r.group} disabled={!r.accept} aria-label={t("org.moveTo")} onChange={(e) => update(i, { group: e.target.value })} />
                    </label>
                  )}
                  {r.suggestion.author && (
                    <label className="organize-field toggle">
                      <input type="checkbox" checked={r.acceptAuthor} disabled={!r.accept} onChange={(e) => update(i, { acceptAuthor: e.target.checked })} />
                      <span className="small">{t("org.setAuthor", { author: r.suggestion.author })}</span>
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
