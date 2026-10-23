// Duplicate books: each group is one book stored in several files. The user picks the copy to
// keep (the best-scored one is preselected); the others' reading data moves onto it and they
// leave the shelf.

import { useState } from "react";
import { formatNumber, t } from "../i18n";
import { Icon } from "../icons";
import type { Book, DuplicateCopy, DuplicateGroup } from "../types";
import { Modal } from "../ui";
import { Cover, percent } from "./BookCard";

export function DedupeDialog(props: {
  groups: DuplicateGroup[];
  books: Map<string, Book>;
  /** Shown above the groups, e.g. after an import. */
  intro?: string;
  onApply: (merges: { keep: string; remove: string[] }[]) => Promise<void>;
  onClose: () => void;
}) {
  // Groups matched only by title and page count are left for the user to tick.
  const [selected, setSelected] = useState(() => props.groups.map((g) => g.confident));
  const [keep, setKeep] = useState(() => props.groups.map((g) => g.copies[0].id));
  const [busy, setBusy] = useState(false);

  const chosen = props.groups.map((g, i) => ({ g, i })).filter(({ i }) => selected[i]);
  const removing = chosen.reduce((n, { g }) => n + g.copies.length - 1, 0);

  const apply = async () => {
    setBusy(true);
    try {
      await props.onApply(
        chosen.map(({ g, i }) => ({ keep: keep[i], remove: g.copies.map((c) => c.id).filter((id) => id !== keep[i]) })),
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t("dd.title")}
      width={680}
      onClose={props.onClose}
      footer={
        props.groups.length > 0 ? (
          <>
            <span className="muted small">{removing > 0 && t("dd.footer", { n: removing })}</span>
            <span className="spacer" />
            <button onClick={props.onClose}>{t("common.cancel")}</button>
            <button className="primary" disabled={chosen.length === 0 || busy} onClick={apply}>
              {t("dd.apply", { n: chosen.length })}
            </button>
          </>
        ) : (
          <button onClick={props.onClose}>{t("common.close")}</button>
        )
      }
    >
      {props.groups.length === 0 ? (
        <p className="muted">{t("dd.none")}</p>
      ) : (
        <>
          <p className="muted small dedupe-intro">{props.intro ?? t("dd.intro")}</p>
          <div className="dedupe-list">
            {props.groups.map((g, i) => {
              const first = props.books.get(g.copies[0].id);
              return (
                <div key={g.copies[0].id} className={`dedupe-group${selected[i] ? "" : " off"}`}>
                  <label className="dedupe-head">
                    <input
                      type="checkbox"
                      checked={selected[i]}
                      onChange={(e) => setSelected((s) => s.map((v, j) => (j === i ? e.target.checked : v)))}
                    />
                    <strong className="ellipsis">{first?.title}</strong>
                    {!g.confident && <span className="badge-soft warn">{t("dd.guess")}</span>}
                  </label>
                  {g.copies.map((c, k) => {
                    const book = props.books.get(c.id);
                    if (!book) return null;
                    return (
                      <label key={c.id} className={`dedupe-copy${keep[i] === c.id ? " keep" : ""}`}>
                        <input
                          type="radio"
                          name={`keep-${i}`}
                          checked={keep[i] === c.id}
                          disabled={!selected[i]}
                          onChange={() => setKeep((ks) => ks.map((v, j) => (j === i ? c.id : v)))}
                        />
                        <span className="dedupe-cover">
                          <Cover book={book} small />
                        </span>
                        <span className="dedupe-main">
                          <span className="dedupe-line">
                            <span className="badge-soft">{book.format}</span>
                            <span className="ellipsis">{book.fileName}</span>
                            {k === 0 && <span className="badge-soft ok">{t("dd.recommended")}</span>}
                          </span>
                          <span className="dedupe-facts muted small">{facts(c, book).join(" · ")}</span>
                        </span>
                        <span className="dedupe-score" title={t("dd.scoreTitle")}>
                          {Math.round(c.score)}
                        </span>
                      </label>
                    );
                  })}
                </div>
              );
            })}
          </div>
        </>
      )}
    </Modal>
  );
}

/** What tells the copies apart, in reading order of importance. */
function facts(c: DuplicateCopy, book: Book): string[] {
  const out = [c.hasText ? t("dd.hasText") : t("dd.scan")];
  if (c.dpi) out.push(t("dd.dpi", { n: Math.round(c.dpi) }));
  if (c.pages) out.push(t("dd.pages", { n: c.pages }));
  if (c.hasText) out.push(t("dd.chars", { n: formatNumber(c.chars) }));
  if (c.tocEntries > 1) out.push(t("dd.toc", { n: c.tocEntries }));
  if (c.completeness < 0.9) out.push(t("dd.incomplete", { n: Math.round(c.completeness * 100) }));
  out.push(`${(book.size / 1024 / 1024).toFixed(1)} MB`);
  if (book.progress > 0) out.push(t("dd.read", { p: percent(book.progress) }));
  return out;
}

export function DedupeProgress(props: { done: number; total: number; name: string }) {
  return (
    <div className="modal-backdrop">
      <div className="modal import-progress">
        <div className="modal-title">
          <Icon name="layers" /> {t("dd.scanning")}
        </div>
        <div className="bar">
          <div style={{ width: props.total ? `${(props.done / props.total) * 100}%` : "0%" }} />
        </div>
        <div className="muted small ellipsis">{props.total ? `${props.done} / ${props.total}　${props.name}` : ""}</div>
      </div>
    </div>
  );
}
