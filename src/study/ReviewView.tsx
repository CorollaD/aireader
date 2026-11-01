// Reviewing flashcards, and managing them. A card is shown just before it would be forgotten:
// the reader recalls the answer, reveals it, and says how it went, which sets its next review.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorText } from "../api";
import { formatDate, t, type MessageKey } from "../i18n";
import { Icon } from "../icons";
import { speakOnce } from "../reader/useSpeech";
import type { Settings } from "../settings";
import type { Card, CardStats, Location } from "../types";
import { Toast, isTyping } from "../ui";
import { formatInterval, startOfToday } from "./intervals";
import LookupDialog, { type LookupRequest } from "./LookupDialog";

/** Cards due again within this time come back in the same session. */
const LEARN_AHEAD_SECONDS = 20 * 60;

const RATINGS: { value: 1 | 2 | 3 | 4; label: MessageKey; className: string }[] = [
  { value: 1, label: "cards.again", className: "danger" },
  { value: 2, label: "cards.hard", className: "" },
  { value: 3, label: "cards.good", className: "primary" },
  { value: 4, label: "cards.easy", className: "" },
];

export default function ReviewView(props: { settings: Settings; onBack: () => void; onOpen: (bookId: string, location: Location) => void }) {
  const [tab, setTab] = useState<"review" | "cards">("review");
  const [stats, setStats] = useState<CardStats | null>(null);
  const refreshStats = useCallback(() => {
    api
      .cardStats(startOfToday())
      .then(setStats)
      .catch(() => {});
  }, []);
  useEffect(refreshStats, [refreshStats]);

  return (
    <div className="review-view">
      <header className="review-head">
        <button onClick={props.onBack}>
          <Icon name="arrowLeft" size={15} /> {t("reader.shelf")}
        </button>
        <div className="segmented">
          <button className={tab === "review" ? "on" : ""} onClick={() => setTab("review")}>
            {t("cards.review")}
          </button>
          <button className={tab === "cards" ? "on" : ""} onClick={() => setTab("cards")}>
            {t("cards.all")}
          </button>
        </div>
        <span className="spacer" />
        {stats && <span className="muted small">{t("cards.stats", { due: stats.due, done: stats.reviewedToday, total: stats.total })}</span>}
      </header>
      {tab === "review" ? <Review {...props} onChanged={refreshStats} /> : <CardList settings={props.settings} onChanged={refreshStats} />}
    </div>
  );
}

function Review(props: { settings: Settings; onOpen: (bookId: string, location: Location) => void; onChanged: () => void }) {
  const [queue, setQueue] = useState<Card[] | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [gaps, setGaps] = useState<[number, number, number, number] | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const card = queue?.[0] ?? null;
  const busy = useRef(false);

  useEffect(() => {
    api
      .dueCards()
      .then(setQueue)
      .catch((e) => {
        setQueue([]);
        setToast(errorText(e));
      });
  }, []);

  const cardId = card?.id;
  useEffect(() => {
    setGaps(null);
    if (!cardId) return;
    api
      .cardIntervals(cardId)
      .then(setGaps)
      .catch(() => {});
  }, [cardId, queue?.length]);

  const answer = useCallback(
    async (rating: 1 | 2 | 3 | 4) => {
      if (!card || busy.current) return;
      busy.current = true;
      try {
        const next = await api.reviewCard(card.id, rating);
        const soon = next.due - Date.now() / 1000 < LEARN_AHEAD_SECONDS;
        setQueue((q) => [...(q ?? []).slice(1), ...(soon ? [next] : [])]);
        setRevealed(false);
        props.onChanged();
      } catch (e) {
        setToast(errorText(e));
      } finally {
        busy.current = false;
      }
    },
    [card, props.onChanged],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!card || isTyping(e) || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key === " " || e.key === "Enter") {
        e.preventDefault();
        if (revealed) answer(3);
        else setRevealed(true);
      } else if (revealed && ["1", "2", "3", "4"].includes(e.key)) {
        answer(Number(e.key) as 1 | 2 | 3 | 4);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [card, revealed, answer]);

  if (!queue) return <div className="center-msg muted">{t("common.loading")}</div>;
  if (!card) {
    return (
      <div className="review-done">
        <div className="ai-orb">
          <Icon name="sparkles" size={22} />
        </div>
        <div className="ai-welcome-title">{t("cards.doneTitle")}</div>
        <p className="muted small">{t("cards.doneHint")}</p>
        {toast && <Toast message={toast} onDone={() => setToast(null)} />}
      </div>
    );
  }
  return (
    <div className="review-stage">
      <div className="muted small">{t("cards.left", { n: queue.length })}</div>
      <article className="flashcard">
        <div className="flashcard-front">
          <span>{card.front}</span>
          <button className="icon-button" title={t("cards.speak")} aria-label={t("cards.speak")} onClick={() => speakOnce(card.front, props.settings)}>
            <Icon name="volume" size={16} />
          </button>
        </div>
        {card.context && card.context !== card.front && <blockquote className="note-quote">{card.context}</blockquote>}
        {revealed && (
          <>
            <hr />
            <div className="flashcard-back">{card.back || t("cards.noBack")}</div>
            <div className="muted small flashcard-source">
              <span className="ellipsis">{card.bookTitle}</span>
              {card.bookId && card.location && (
                <button className="link-button" onClick={() => props.onOpen(card.bookId!, card.location!)}>
                  {t("cards.openSource")}
                </button>
              )}
            </div>
          </>
        )}
      </article>
      {revealed ? (
        <div className="review-answers">
          {RATINGS.map((r, i) => (
            <button key={r.value} className={r.className} onClick={() => answer(r.value)}>
              <span>{t(r.label)}</span>
              <small>
                {r.value} · {gaps ? formatInterval(gaps[i]) : "…"}
              </small>
            </button>
          ))}
        </div>
      ) : (
        <div className="review-answers">
          <button className="primary" onClick={() => setRevealed(true)}>
            {t("cards.show")}
          </button>
        </div>
      )}
      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}

function CardList(props: { settings: Settings; onChanged: () => void }) {
  const [cards, setCards] = useState<Card[] | null>(null);
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<Card | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const refresh = useCallback(() => {
    api
      .listCards(null)
      .then(setCards)
      .catch((e) => setToast(errorText(e)));
  }, []);
  useEffect(refresh, [refresh]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return cards ?? [];
    return (cards ?? []).filter((c) => [c.front, c.back, c.bookTitle].some((s) => s.toLowerCase().includes(q)));
  }, [cards, query]);

  const changed = () => {
    refresh();
    props.onChanged();
  };
  const exportCsv = () =>
    api.exportCards().then(
      (path) => path && setToast(t("reader.exported", { path })),
      (e) => setToast(errorText(e)),
    );

  return (
    <div className="card-list">
      <div className="card-list-bar">
        <input placeholder={t("cards.search")} value={query} onChange={(e) => setQuery(e.target.value)} />
        <button disabled={!cards?.length} onClick={exportCsv}>
          {t("cards.export")}
        </button>
      </div>
      {cards?.length === 0 && <div className="empty muted">{t("cards.empty")}</div>}
      {shown.map((c) => (
        <div key={c.id} className={`card-row${c.suspended ? " suspended" : ""}`}>
          <div className="card-row-main">
            <div className="book-title">{c.front}</div>
            <div className="muted small ellipsis">{c.back}</div>
            <div className="muted small">
              {c.bookTitle} · {t("cards.nextReview", { date: formatDate(c.due) })}
            </div>
          </div>
          <span className="entry-actions always">
            <button
              className="icon"
              title={c.suspended ? t("cards.resume") : t("cards.suspend")}
              aria-label={c.suspended ? t("cards.resume") : t("cards.suspend")}
              onClick={() => api.updateCard(c.id, { suspended: !c.suspended }).then(changed)}
            >
              <Icon name={c.suspended ? "play" : "pause"} size={14} />
            </button>
            <button className="icon" title={t("cards.edit")} aria-label={t("cards.edit")} onClick={() => setEditing(c)}>
              <Icon name="edit" size={14} />
            </button>
            <button className="icon" title={t("common.delete")} aria-label={t("common.delete")} onClick={() => api.deleteCard(c.id).then(changed)}>
              <Icon name="close" size={14} />
            </button>
          </span>
        </div>
      ))}
      {editing && (
        <LookupDialog
          request={editRequest(editing)}
          book={{ id: editing.bookId, title: editing.bookTitle }}
          card={editing}
          settings={props.settings}
          onSaved={changed}
          onClose={() => setEditing(null)}
        />
      )}
      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}

export const editRequest = (c: Card): LookupRequest => ({ word: c.front, context: c.context, location: c.location });
