// Exporting highlights, notes and bookmarks: of one book from the reader, or of every book from
// Settings.

import { useState } from "react";
import { api, errorText, type NotesFormat } from "./api";
import { t, type MessageKey } from "./i18n";

const FORMATS: { key: NotesFormat; label: MessageKey; hint: MessageKey }[] = [
  { key: "markdown", label: "export.markdown", hint: "export.markdownHint" },
  { key: "obsidian", label: "export.obsidian", hint: "export.obsidianHint" },
  { key: "csv", label: "export.csv", hint: "export.csvHint" },
  { key: "json", label: "export.json", hint: "export.jsonHint" },
];

/** `bookId` limits the export to that book; `onDone` receives the path written. */
export default function ExportNotes(props: { bookId?: string; onDone?: (path: string) => void }) {
  const [format, setFormat] = useState<NotesFormat>("markdown");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const run = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const path = await api.exportNotes(props.bookId ?? null, format);
      if (path) {
        setMessage({ ok: true, text: t("reader.exported", { path }) });
        props.onDone?.(path);
      }
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="export-notes">
      <div className="inline-add">
        <select value={format} onChange={(e) => setFormat(e.target.value as NotesFormat)} aria-label={t("export.format")}>
          {FORMATS.map((f) => (
            <option key={f.key} value={f.key}>
              {t(f.label)}
            </option>
          ))}
        </select>
        <button className="primary" disabled={busy} onClick={run}>
          {busy ? t("export.exporting") : t("side.export")}
        </button>
      </div>
      <p className="muted small hint">{t(FORMATS.find((f) => f.key === format)!.hint)}</p>
      {message && <p className={message.ok ? "ok-text small" : "error small"}>{message.text}</p>}
    </div>
  );
}
