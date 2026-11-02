// Citations of the given books: shown in a chosen format, to copy or save as a file.

import { useEffect, useState } from "react";
import { api, errorText, type CitationFormat } from "../api";
import { t } from "../i18n";
import { Modal } from "../ui";
import { Select } from "../Select";

const FORMATS: { key: CitationFormat; label: string }[] = [
  { key: "gbt7714", label: "GB/T 7714" },
  { key: "apa", label: "APA" },
  { key: "mla", label: "MLA" },
  { key: "chicago", label: "Chicago" },
  { key: "ieee", label: "IEEE" },
  { key: "bibtex", label: "BibTeX" },
  { key: "ris", label: "RIS" },
  { key: "csljson", label: "CSL-JSON" },
];
const REMEMBERED = "citation-format";

function rememberedFormat(): CitationFormat {
  try {
    const saved = localStorage.getItem(REMEMBERED);
    return FORMATS.find((f) => f.key === saved)?.key ?? "gbt7714";
  } catch {
    return "gbt7714";
  }
}

export default function CiteDialog(props: { ids: string[]; onClose: () => void }) {
  const { ids } = props;
  const [format, setFormat] = useState(rememberedFormat);
  const [text, setText] = useState("");
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    let alive = true;
    setNote(null);
    api
      .citationText(ids, format)
      .then((c) => alive && setText(c))
      .catch((e) => alive && setNote({ ok: false, text: errorText(e) }));
    return () => {
      alive = false;
    };
  }, [ids, format]);

  const choose = (next: CitationFormat) => {
    setFormat(next);
    try {
      localStorage.setItem(REMEMBERED, next);
    } catch {
      // Only a convenience.
    }
  };

  const copy = () =>
    navigator.clipboard.writeText(text).then(
      () => setNote({ ok: true, text: t("common.copied") }),
      () => setNote({ ok: false, text: t("common.copyFailed") }),
    );

  const save = () =>
    api
      .citationExport(ids, format)
      .then((path) => path && setNote({ ok: true, text: t("reader.exported", { path }) }))
      .catch((e) => setNote({ ok: false, text: errorText(e) }));

  const footer = (
    <>
      <button onClick={save} disabled={!text}>
        {t("cite.save")}
      </button>
      <button className="primary" onClick={copy} disabled={!text}>
        {t("common.copy")}
      </button>
    </>
  );

  return (
    <Modal title={t("cite.title", { n: ids.length })} width={600} onClose={props.onClose} footer={footer}>
      <label className="field">
        <span>{t("export.format")}</span>
        <Select full value={format} label={t("export.format")} options={FORMATS.map((f) => ({ value: f.key, label: f.label }))} onChange={choose} />
      </label>
      <textarea className="cite-text" readOnly value={text} rows={9} aria-label={t("cite.title", { n: ids.length })} />
      <p className="muted small hint">{t("cite.hint")}</p>
      {note && <p className={note.ok ? "ok-text small" : "error small"}>{note.text}</p>}
    </Modal>
  );
}
