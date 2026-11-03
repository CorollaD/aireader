// AI bookshelf organisation: ask for a group (each book is in one) and missing authors, let the
// user review and edit the suggestions, and apply only what was accepted. Books that already
// have a group start unchecked, so nothing is moved without the user's say.

import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import type { Book, BookPatch, Suggestion } from "../types";
import { Modal } from "../ui";
import { BTN, BTN_PRIMARY, INPUT } from "../tw";
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
            <span className="self-center text-xs text-[var(--muted)]">{t("org.onlyAdd")}</span>
            <span className="flex-1" />
            <button className={BTN} onClick={props.onClose}>
              {t("common.cancel")}
            </button>
            <button className={BTN_PRIMARY} disabled={accepted === 0 || applying} onClick={apply}>
              {t("org.apply", { n: accepted })}
            </button>
          </>
        ) : (
          <button className={BTN} onClick={props.onClose}>
            {t("common.close")}
          </button>
        )
      }
    >
      {phase === "running" && (
        <p className="flex items-center gap-2 text-[var(--muted)]">
          <span className="d-loading d-loading-spinner d-loading-sm" /> {t("org.running")}
        </p>
      )}
      {phase === "error" && <p className="text-[var(--danger)]">{error}</p>}
      {phase === "review" &&
        (rows.length === 0 ? (
          <p className="text-[var(--muted)]">{t("org.nothing")}</p>
        ) : (
          <div className="flex flex-col divide-y divide-[var(--line)]">
            {rows.map((r, i) => (
              <div key={r.book.id} className={`flex items-start gap-3 py-2.5${r.accept ? "" : " opacity-50"}`}>
                <input
                  type="checkbox"
                  className="d-checkbox d-checkbox-sm d-checkbox-primary mt-0.5"
                  checked={r.accept}
                  onChange={(e) => update(i, { accept: e.target.checked })}
                />
                <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                  <strong className="truncate text-[13px]">{r.book.title}</strong>
                  {r.suggestion.group && (
                    <label className="flex items-center gap-2 text-[13px]">
                      <span className={`d-badge d-badge-sm shrink-0 ${r.book.group ? "d-badge-ghost" : "d-badge-outline text-[var(--muted)]"}`}>
                        {r.book.group ?? t("shelf.ungrouped")}
                      </span>
                      <span className="text-[var(--muted)]">→</span>
                      <input
                        className={`${INPUT} d-input-xs flex-1`}
                        value={r.group}
                        disabled={!r.accept}
                        aria-label={t("org.moveTo")}
                        onChange={(e) => update(i, { group: e.target.value })}
                      />
                    </label>
                  )}
                  {r.suggestion.author && (
                    <label className="flex cursor-pointer items-center gap-2">
                      <input
                        type="checkbox"
                        className="d-checkbox d-checkbox-xs"
                        checked={r.acceptAuthor}
                        disabled={!r.accept}
                        onChange={(e) => update(i, { acceptAuthor: e.target.checked })}
                      />
                      <span className="text-xs">{t("org.setAuthor", { author: r.suggestion.author })}</span>
                    </label>
                  )}
                </div>
              </div>
            ))}
            {error && <p className="pt-2 text-xs text-[var(--danger)]">{error}</p>}
          </div>
        ))}
    </Modal>
  );
}
