// Open sources to add with one click, and libraries to visit in the browser.

import { useState } from "react";
import { api, errorText } from "../api";
import { t } from "../i18n";
import { Icon } from "../icons";
import type { OpdsCatalog } from "../types";
import { Modal } from "../ui";
import { CATALOGS, GUTENDEX, WEBSITES, WIKISOURCES, type Source } from "./sources";

export default function SourcesDialog(props: { catalogs: OpdsCatalog[]; onAdd: (source: Source) => void; onClose: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const added = (s: Source) => props.catalogs.some((c) => c.url === s.url);

  const section = (title: string, hint: string, sources: Source[]) => (
    <section className="source-group">
      <div className="panel-label">{title}</div>
      <p className="muted small hint">{hint}</p>
      {sources.map((s) => (
        <div key={s.url} className="source-row">
          <span className="ellipsis grow">{s.title}</span>
          {added(s) ? <span className="muted small">{t("opds.sourceAdded")}</span> : <button onClick={() => props.onAdd(s)}>{t("common.add")}</button>}
        </div>
      ))}
    </section>
  );

  return (
    <Modal title={t("opds.sources")} width={560} onClose={props.onClose}>
      {error && <p className="error small">{error}</p>}
      <div className="source-list">
        {section(t("opds.groupCatalogs"), t("opds.groupCatalogsHint"), CATALOGS)}
        {section(t("opds.groupWikisource"), t("opds.groupWikisourceHint"), WIKISOURCES)}
        {section(t("opds.groupGutendex"), t("opds.groupGutendexHint"), GUTENDEX)}
        <section className="source-group">
          <div className="panel-label">{t("opds.groupWebsites")}</div>
          <p className="muted small hint">{t("opds.groupWebsitesHint")}</p>
          {WEBSITES.map((site) => (
            <div key={site.url} className="source-row">
              <span className="ellipsis grow">{site.title}</span>
              <span className="muted small">{site.note}</span>
              <button onClick={() => api.openWebsite(site.url).catch((e) => setError(errorText(e)))}>
                <Icon name="globe" size={14} /> {t("opds.openSite")}
              </button>
            </div>
          ))}
        </section>
      </div>
    </Modal>
  );
}
