// What a numbered citation in a paper refers to. The list comes from the paper itself; looking
// a work up happens only when asked for, and sends that one reference.

import { useState } from "react";
import { api, errorText } from "../api";
import { t } from "../i18n";
import { Icon } from "../icons";
import type { PaperInfo, Reference } from "../types";
import PaperPreview from "./PaperPreview";

const WIDTH = 420;
const MARGIN = 12;

function Entry(props: { reference: Reference; onJump: (page: number) => void }) {
  const { reference } = props;
  const [state, setState] = useState<"idle" | "busy" | "none">("idle");
  const [info, setInfo] = useState<PaperInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [added, setAdded] = useState(false);

  const lookup = async () => {
    setState("busy");
    setError(null);
    try {
      const found = await api.paperLookupReference(reference.text);
      setInfo(found);
      setState(found ? "idle" : "none");
    } catch (e) {
      setError(errorText(e));
      setState("idle");
    }
  };

  const add = async () => {
    setState("busy");
    setError(null);
    try {
      await api.paperImport(info!);
      setAdded(true);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setState("idle");
    }
  };

  return (
    <div className="cite-entry">
      <p className="small">
        <strong>[{reference.number}]</strong> {reference.text}
      </p>
      {info && <PaperPreview info={info} />}
      {state === "none" && <p className="muted small">{t("paper.referenceNotFound")}</p>}
      {error && <p className="error small">{error}</p>}
      <div className="inline-add">
        {!info && (
          <button disabled={state === "busy"} onClick={lookup}>
            {state === "busy" ? t("paper.looking") : t("paper.lookupReference")}
          </button>
        )}
        {info?.pdfUrl && (
          <button disabled={state === "busy" || added} onClick={add}>
            {added ? t("paper.added") : state === "busy" ? t("paper.downloading") : t("paper.addToShelf")}
          </button>
        )}
        <button onClick={() => props.onJump(reference.page)}>{t("paper.showInList")}</button>
      </div>
    </div>
  );
}

export default function CitationCard(props: { references: Reference[]; rect: DOMRect; onJump: (page: number) => void; onClose: () => void }) {
  const left = Math.max(MARGIN, Math.min(props.rect.left, window.innerWidth - WIDTH - MARGIN));
  const below = props.rect.bottom < window.innerHeight / 2;
  const place = below ? { top: props.rect.bottom + 6 } : { bottom: window.innerHeight - props.rect.top + 6 };
  return (
    <div className="cite-card" style={{ left, width: WIDTH, ...place }} onClick={(e) => e.stopPropagation()}>
      <button className="icon-button cite-close" aria-label={t("common.close")} onClick={props.onClose}>
        <Icon name="close" />
      </button>
      {props.references.map((r) => (
        <Entry key={r.number} reference={r} onJump={props.onJump} />
      ))}
      <p className="muted small hint">{t("paper.lookupReferenceHint")}</p>
    </div>
  );
}
