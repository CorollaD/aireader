// Duplicate books: each group is one book stored in several files. The user picks the copy to
// keep (the best-scored one is preselected); the others' reading data moves onto it and they
// leave the shelf.

import { useState } from "react";
import { formatNumber, formatSize, t } from "../i18n";
import { Icon } from "../icons";
import type { Book, DuplicateCopy, DuplicateGroup } from "../types";
import { Modal } from "../ui";
import { BTN, BTN_PRIMARY } from "../tw";
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
      await props.onApply(chosen.map(({ g, i }) => ({ keep: keep[i], remove: g.copies.map((c) => c.id).filter((id) => id !== keep[i]) })));
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
            <span className="self-center text-xs text-[var(--muted)]">{removing > 0 && t("dd.footer", { n: removing })}</span>
            <span className="flex-1" />
            <button className={BTN} onClick={props.onClose}>
              {t("common.cancel")}
            </button>
            <button className={BTN_PRIMARY} disabled={chosen.length === 0 || busy} onClick={apply}>
              {t("dd.apply", { n: chosen.length })}
            </button>
          </>
        ) : (
          <button className={BTN} onClick={props.onClose}>
            {t("common.close")}
          </button>
        )
      }
    >
      {props.groups.length === 0 ? (
        <p className="text-[var(--muted)]">{t("dd.none")}</p>
      ) : (
        <>
          <p className="mb-3 text-xs leading-relaxed text-[var(--muted)]">{props.intro ?? t("dd.intro")}</p>
          <div className="flex flex-col gap-3">
            {props.groups.map((g, i) => {
              const first = props.books.get(g.copies[0].id);
              return (
                <div key={g.copies[0].id} className={`rounded-xl border border-[var(--line)] p-3${selected[i] ? "" : " opacity-55"}`}>
                  <label className="mb-2 flex cursor-pointer items-center gap-2.5">
                    <input
                      type="checkbox"
                      className="d-checkbox d-checkbox-sm d-checkbox-primary"
                      checked={selected[i]}
                      onChange={(e) => setSelected((s) => s.map((v, j) => (j === i ? e.target.checked : v)))}
                    />
                    <strong className="truncate text-[13px]">{first?.title}</strong>
                  </label>
                  {!g.confident && <p className="mb-2 text-xs text-[var(--warn)]">{t("dd.guess")}</p>}
                  {g.copies.map((c, k) => {
                    const book = props.books.get(c.id);
                    if (!book) return null;
                    return (
                      <label
                        key={c.id}
                        className={`flex cursor-pointer items-center gap-3 rounded-lg px-2 py-1.5 ${keep[i] === c.id ? "bg-[var(--accent-soft)]" : "hover:bg-[var(--hover)]"}`}
                      >
                        <input
                          type="radio"
                          className="d-radio d-radio-sm d-radio-primary"
                          name={`keep-${i}`}
                          checked={keep[i] === c.id}
                          disabled={!selected[i]}
                          onChange={() => setKeep((ks) => ks.map((v, j) => (j === i ? c.id : v)))}
                        />
                        <span className="book3d flat w-8 shrink-0">
                          <span className="book3d-face">
                            <Cover book={book} small />
                          </span>
                        </span>
                        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                          <span className="flex items-center gap-1.5 text-[13px]">
                            <span className="d-badge d-badge-ghost d-badge-sm font-semibold">{book.format}</span>
                            <span className="truncate">{book.fileName}</span>
                            {k === 0 && <span className="d-badge d-badge-success d-badge-soft d-badge-sm font-semibold">{t("dd.recommended")}</span>}
                          </span>
                          <span className="text-xs text-[var(--muted)]">{facts(c, book).join(" · ")}</span>
                        </span>
                        <span className="text-lg font-bold text-[var(--muted)] tabular-nums" title={t("dd.scoreTitle")}>
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
  out.push(formatSize(book.size));
  if (book.progress > 0) out.push(t("dd.read", { p: percent(book.progress) }));
  return out;
}

export function DedupeProgress(props: { done: number; total: number; name: string }) {
  return (
    <div className="d-modal d-modal-open">
      <div className="d-modal-box flex w-[420px] flex-col gap-3" role="dialog" aria-modal="true" aria-label={t("dd.scanning")}>
        <h2 className="flex items-center gap-2 text-[17px] font-semibold">
          <Icon name="layers" /> {t("dd.scanning")}
        </h2>
        <progress className="d-progress d-progress-primary w-full" value={props.total ? props.done : undefined} max={props.total || 1} />
        <div className="truncate text-xs text-[var(--muted)]">{props.total ? `${props.done} / ${props.total}　${props.name}` : ""}</div>
      </div>
    </div>
  );
}
