// A paper's details, as found by a lookup.

import { t } from "../i18n";
import type { PaperInfo } from "../types";

/** "Family, Given" as people write it. */
export function displayName(name: string): string {
  const [family, given] = name.split(",").map((s) => s.trim());
  return given ? `${given} ${family}` : family;
}

export default function PaperPreview({ info }: { info: PaperInfo }) {
  const source = [info.venue, info.year].filter(Boolean).join(" · ");
  return (
    <div className="paper-preview">
      <div className="paper-title">{info.title}</div>
      {info.authors.length > 0 && <div className="muted small">{info.authors.map(displayName).join(", ")}</div>}
      {source && <div className="muted small">{source}</div>}
      {info.summary ? <p className="small paper-abstract">{info.summary}</p> : <p className="muted small">{t("paper.noAbstract")}</p>}
      {info.url && <div className="muted small paper-url">{info.url}</div>}
    </div>
  );
}
