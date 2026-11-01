// Looking up a paper: for a book on the shelf, to fill in its details, or to import a paper by
// its DOI, arXiv id, link or title. A lookup only runs when asked for, and sends what is typed.

import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import { t } from "../i18n";
import type { Book, PaperInfo } from "../types";
import { Modal } from "../ui";
import PaperPreview from "./PaperPreview";

const KINDS = ["article", "conference", "preprint", "book", "other"] as const;

const blank = (book: Book | null): PaperInfo => ({
  kind: "book",
  title: book?.title ?? "",
  authors: book?.author ? [book.author] : [],
  year: null,
  venue: null,
  volume: null,
  issue: null,
  pages: null,
  publisher: null,
  doi: null,
  arxiv: null,
  url: null,
  summary: null,
  pdfUrl: null,
});

const text = (value: string) => value.trim() || null;

export default function PaperDialog(props: { bookId?: string; onClose: () => void; onSaved?: (book: Book) => void; onImported?: (id: string) => void }) {
  const { bookId } = props;
  const [query, setQuery] = useState("");
  const [info, setInfo] = useState<PaperInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!bookId) return;
    api
      .listBooks()
      .then(async ({ books }) => {
        const found = books.find((b) => b.id === bookId) ?? null;
        setInfo(found?.paper ?? blank(found));
        const id = found?.paper?.doi ?? found?.paper?.arxiv ?? (await api.paperDetect(bookId));
        setQuery(id ?? found?.title ?? "");
      })
      .catch((e) => setError(errorText(e)));
  }, [bookId]);

  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const lookup = () => run(async () => setInfo(await api.paperLookup(query.trim())));
  const patch = (p: Partial<PaperInfo>) => setInfo((i) => i && { ...i, ...p });

  const save = () =>
    run(async () => {
      const saved = await api.paperSave(bookId!, info!);
      props.onSaved?.(saved);
      props.onClose();
    });

  const importPaper = () =>
    run(async () => {
      const { id } = await api.paperImport(info!);
      props.onImported?.(id);
      props.onClose();
    });

  const footer = (
    <>
      <button onClick={props.onClose}>{t("common.cancel")}</button>
      {bookId ? (
        <button className="primary" disabled={busy || !info?.title.trim()} onClick={save}>
          {t("common.save")}
        </button>
      ) : (
        <button className="primary" disabled={busy || !info?.pdfUrl} onClick={importPaper}>
          {busy ? t("paper.downloading") : t("paper.import")}
        </button>
      )}
    </>
  );

  return (
    <Modal title={bookId ? t("paper.details") : t("paper.importTitle")} width={560} onClose={props.onClose} footer={footer}>
      <div className="paper-dialog">
        <div className="inline-add">
          <input
            value={query}
            placeholder={t("paper.queryPlaceholder")}
            aria-label={t("paper.queryPlaceholder")}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && query.trim() && !busy && lookup()}
          />
          <button disabled={busy || !query.trim()} onClick={lookup}>
            {busy ? t("paper.looking") : t("paper.lookup")}
          </button>
        </div>
        <p className="muted small hint">{t("paper.lookupHint")}</p>
        {error && <p className="error small">{error}</p>}

        {info && !bookId && <PaperPreview info={info} />}
        {info && !bookId && !info.pdfUrl && <p className="error small">{t("paper.noOpenCopy")}</p>}

        {info && bookId && (
          <>
            <label className="field">
              <span>{t("paper.kind")}</span>
              <select value={info.kind} onChange={(e) => patch({ kind: e.target.value })}>
                {KINDS.map((k) => (
                  <option key={k} value={k}>
                    {t(`paper.kind.${k}`)}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>{t("paper.title")}</span>
              <input value={info.title} onChange={(e) => patch({ title: e.target.value })} />
            </label>
            <label className="field">
              <span>{t("paper.authors")}</span>
              <input
                value={info.authors.join("; ")}
                onChange={(e) =>
                  patch({
                    authors: e.target.value.split(";").map((a) => a.trimStart()),
                  })
                }
              />
            </label>
            <div className="paper-row">
              <label className="field">
                <span>{t("paper.year")}</span>
                <input
                  inputMode="numeric"
                  value={info.year ?? ""}
                  onChange={(e) => {
                    const year = Number.parseInt(e.target.value, 10);
                    patch({ year: Number.isNaN(year) ? null : year });
                  }}
                />
              </label>
              <label className="field grow">
                <span>{t("paper.venue")}</span>
                <input value={info.venue ?? ""} onChange={(e) => patch({ venue: text(e.target.value) })} />
              </label>
            </div>
            <div className="paper-row">
              <label className="field grow">
                <span>{t("paper.publisher")}</span>
                <input value={info.publisher ?? ""} onChange={(e) => patch({ publisher: text(e.target.value) })} />
              </label>
              <label className="field grow">
                <span>DOI</span>
                <input value={info.doi ?? ""} onChange={(e) => patch({ doi: text(e.target.value) })} />
              </label>
            </div>
          </>
        )}
      </div>
    </Modal>
  );
}
