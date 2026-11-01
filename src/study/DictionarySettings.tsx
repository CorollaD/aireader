// The Dictionaries tab of the Settings dialog.

import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import { formatNumber, t } from "../i18n";
import { Icon } from "../icons";
import type { DictRecord } from "../types";

export default function DictionarySettings() {
  const [records, setRecords] = useState<DictRecord[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .listDictionaries()
      .then(setRecords)
      .catch((e) => setError(errorText(e)));
  }, []);

  const save = (next: DictRecord[]) => {
    setRecords(next);
    api.setDictionaries(next).catch((e) => setError(errorText(e)));
  };

  const add = async () => {
    setBusy(true);
    setError(null);
    try {
      const list = await api.importDictionary();
      if (list) setRecords(list);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  /** Swaps a dictionary with its neighbour: earlier ones are looked in first. */
  const move = (i: number, delta: number) => {
    if (!records) return;
    const next = [...records];
    [next[i], next[i + delta]] = [next[i + delta], next[i]];
    save(next);
  };

  return (
    <div className="panel-section">
      <div className="panel-label">{t("settings.tabDictionary")}</div>
      <p className="muted small hint">{t("dict.hint")}</p>
      {records?.length === 0 && <p className="muted small">{t("dict.none")}</p>}
      {records?.map((d, i) => (
        <div key={d.id} className="dict-row">
          <label className="toggle">
            <input type="checkbox" checked={d.enabled} onChange={(e) => save(records.map((r) => (r.id === d.id ? { ...r, enabled: e.target.checked } : r)))} />
            <span className="ellipsis">{d.name}</span>
          </label>
          <span className="muted small">{t("dict.words", { n: formatNumber(d.words) })}</span>
          <span className="spacer" />
          <button className="icon" disabled={i === 0} title={t("dict.moveUp")} aria-label={t("dict.moveUp")} onClick={() => move(i, -1)}>
            <Icon name="arrowUp" size={14} />
          </button>
          <button className="icon" title={t("common.delete")} aria-label={t("common.delete")} onClick={() => save(records.filter((r) => r.id !== d.id))}>
            <Icon name="close" size={14} />
          </button>
        </div>
      ))}
      <button disabled={busy} onClick={add}>
        {busy ? t("dict.adding") : t("dict.add")}
      </button>
      {error && <p className="error small">{error}</p>}
    </div>
  );
}
