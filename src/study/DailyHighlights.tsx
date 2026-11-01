// Looking back on a few old highlights a day. There is nothing to get right: each one is kept,
// to return later and later, or dismissed for good.

import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import { t } from "../i18n";
import { Icon } from "../icons";
import type { Settings } from "../settings";
import type { DailyHighlight, Location } from "../types";
import { Toast, isTyping } from "../ui";
import { startOfToday } from "./intervals";
import LookupDialog from "./LookupDialog";

export default function DailyHighlights(props: { settings: Settings; onOpen: (bookId: string, location: Location) => void }) {
  const [list, setList] = useState<DailyHighlight[] | null>(null);
  const [card, setCard] = useState<DailyHighlight | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const current = list?.[0] ?? null;

  useEffect(() => {
    api
      .dailyHighlights(startOfToday())
      .then(setList)
      .catch((e) => {
        setList([]);
        setToast(errorText(e));
      });
  }, []);

  const next = (keep: boolean) => {
    if (!current) return;
    api.highlightSeen(current.id, keep).catch((e) => setToast(errorText(e)));
    setList((l) => (l ?? []).slice(1));
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!current || card || isTyping(e) || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key === " " || e.key === "Enter") {
        e.preventDefault();
        next(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!list) return <div className="center-msg muted">{t("common.loading")}</div>;
  if (!current) {
    return (
      <div className="review-done">
        <div className="ai-orb">
          <Icon name="sparkles" size={22} />
        </div>
        <div className="ai-welcome-title">{t("daily.doneTitle")}</div>
        <p className="muted small">{t("daily.doneHint")}</p>
        {toast && <Toast message={toast} onDone={() => setToast(null)} />}
      </div>
    );
  }
  return (
    <div className="review-stage">
      <div className="muted small">{t("cards.left", { n: list.length })}</div>
      <article className="flashcard">
        <blockquote className="daily-quote">{current.text}</blockquote>
        {current.note && <div className="flashcard-back">{current.note}</div>}
        <div className="muted small flashcard-source">
          <span className="ellipsis">{current.bookTitle}</span>
          <button className="link-button" onClick={() => props.onOpen(current.bookId, current.location)}>
            {t("cards.openSource")}
          </button>
        </div>
      </article>
      <div className="review-answers">
        <button onClick={() => next(false)}>{t("daily.dismiss")}</button>
        <button onClick={() => setCard(current)}>{t("daily.makeCard")}</button>
        <button className="primary" onClick={() => next(true)}>
          {t("daily.keep")}
        </button>
      </div>
      {card && (
        <LookupDialog
          request={{ word: card.text, context: "", location: card.location, back: card.note ?? "" }}
          book={{ id: card.bookId, title: card.bookTitle }}
          settings={props.settings}
          onSaved={() => setToast(t("cards.saved"))}
          onClose={() => setCard(null)}
        />
      )}
      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
