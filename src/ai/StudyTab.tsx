// The AI panel's Study tab: flashcards to keep, and a quiz to take now, from the current chapter
// or the reader's highlights.

import { useState } from "react";
import { api, runAiTask } from "../api";
import { t } from "../i18n";
import { Icon } from "../icons";
import type { Highlight, Location, OpenedBook, QuizQuestion, StudyCard, StudyScope } from "../types";
import { useAi } from "./AiContext";

/** Pages taken together when a paged book has no chapters. */
const PAGE_SPAN = 10;
const CARD_COUNT = 8;
/** Characters of a quote searched for: long enough to be unique, short enough to survive edits. */
const QUOTE_PREFIX = 14;

type Source = "here" | "highlights";

export default function StudyTab(props: {
  book: OpenedBook;
  location: Location | null;
  highlights: Highlight[];
  onCardsSaved: () => void;
  onNavigate: (loc: Location) => void;
  onNotice: (text: string) => void;
}) {
  const { book, location, highlights } = props;
  const ai = useAi();
  const [source, setSource] = useState<Source>("here");
  const [busy, setBusy] = useState<{ taskId: string; label: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [cards, setCards] = useState<(StudyCard & { keep: boolean })[] | null>(null);
  const [quiz, setQuiz] = useState<QuizQuestion[] | null>(null);
  const [answers, setAnswers] = useState<(number | null)[]>([]);

  const scope = (): StudyScope | null => {
    if (source === "highlights") {
      const text = highlights.map((h) => (h.note?.trim() ? `${h.text}\n(${h.note.trim()})` : h.text)).join("\n\n");
      return { kind: "text", title: t("side.notes"), text };
    }
    if (location?.kind === "flow") return { kind: "chapter", chapter: location.chapter };
    if (location?.kind === "page") return { kind: "pages", from: location.page, to: Math.min(location.page + PAGE_SPAN - 1, book.pages.length - 1) };
    return null;
  };

  /** Runs a task whose result is JSON. */
  const run = async (kind: "cards" | "quiz") => {
    const s = scope();
    if (!s || busy || !(await ai.ensureConsent())) return;
    setError(null);
    setCards(null);
    setQuiz(null);
    const task =
      kind === "cards"
        ? { kind: "generateCards" as const, bookId: book.id, scope: s, count: CARD_COUNT }
        : { kind: "quiz" as const, bookId: book.id, scope: s };
    const label = t(kind === "cards" ? "study.makingCards" : "study.makingQuiz");
    const taskId = runAiTask(task, (e) => {
      if (e.type === "progress") setBusy((b) => b && { ...b, label: e.label });
      else if (e.type === "error") {
        setError(e.message);
        setBusy(null);
      } else if (e.type === "done") {
        setBusy(null);
        if (kind === "cards") setCards((JSON.parse(e.text) as StudyCard[]).map((c) => ({ ...c, keep: true })));
        else {
          const questions = JSON.parse(e.text) as QuizQuestion[];
          setQuiz(questions);
          setAnswers(questions.map(() => null));
        }
      }
    });
    setBusy({ taskId, label });
  };

  const stop = () => {
    if (busy) api.aiCancel(busy.taskId).catch(() => {});
    setBusy(null);
  };

  const save = async () => {
    const chosen = (cards ?? []).filter((c) => c.keep);
    for (const c of chosen) {
      await api.addCard({ id: crypto.randomUUID(), bookId: book.id, bookTitle: book.title, front: c.front, back: c.back, context: c.quote, location });
    }
    setCards(null);
    props.onCardsSaved();
    props.onNotice(t("study.saved", { n: chosen.length }));
  };

  /** Finds a quoted passage in the book. */
  const openQuote = async (quote: string) => {
    const hits = await api.searchBook(book.id, quote.trim().slice(0, QUOTE_PREFIX)).catch(() => []);
    if (hits[0]) props.onNavigate(hits[0].location);
    else props.onNotice(t("study.quoteNotFound"));
  };

  const answered = answers.filter((a) => a !== null).length;
  const score = quiz ? quiz.filter((q, i) => answers[i] === q.answer).length : 0;
  const here = book.kind === "flow" ? t("study.thisChapter") : t("study.nextPages", { n: PAGE_SPAN });

  return (
    <div className="ai-scroll">
      <div className="segmented study-source">
        <button className={source === "here" ? "on" : ""} onClick={() => setSource("here")}>
          {here}
        </button>
        <button className={source === "highlights" ? "on" : ""} disabled={highlights.length === 0} onClick={() => setSource("highlights")}>
          {t("study.myHighlights", { n: highlights.length })}
        </button>
      </div>
      <div className="ai-cards">
        <button className="ai-card" disabled={!!busy} onClick={() => run("cards")}>
          <Icon name="layers" size={18} />
          <span className="ai-card-title">{t("study.makeCards")}</span>
          <span className="muted small">{t("study.makeCardsHint")}</span>
        </button>
        <button className="ai-card" disabled={!!busy} onClick={() => run("quiz")}>
          <Icon name="message" size={18} />
          <span className="ai-card-title">{t("study.quiz")}</span>
          <span className="muted small">{t("study.quizHint")}</span>
        </button>
      </div>

      {busy && (
        <div className="study-busy muted small">
          <span className="ellipsis">{busy.label}</span>
          <button className="link-button" onClick={stop}>
            {t("common.stop")}
          </button>
        </div>
      )}
      {error && <div className="error small">{error}</div>}

      {cards && (
        <div className="study-list">
          {cards.map((c, i) => (
            <label key={c.front} className="study-card">
              <input type="checkbox" checked={c.keep} onChange={(e) => setCards(cards.map((x, k) => (k === i ? { ...x, keep: e.target.checked } : x)))} />
              <span>
                <strong>{c.front}</strong>
                <span className="muted">{c.back}</span>
              </span>
            </label>
          ))}
          <button className="primary" disabled={!cards.some((c) => c.keep)} onClick={save}>
            {t("study.saveCards", { n: cards.filter((c) => c.keep).length })}
          </button>
        </div>
      )}

      {quiz && (
        <div className="study-list">
          {quiz.map((q, i) => {
            const picked = answers[i];
            return (
              <section key={i} className="quiz-question">
                <p>
                  {i + 1}. {q.question}
                </p>
                {q.options.map((option, k) => {
                  const state = picked === null ? "" : k === q.answer ? " right" : k === picked ? " wrong" : "";
                  return (
                    <button
                      key={k}
                      className={`quiz-option${state}`}
                      disabled={picked !== null}
                      onClick={() => setAnswers(answers.map((a, x) => (x === i ? k : a)))}
                    >
                      {state === " right" && <Icon name="check" size={13} />}
                      {state === " wrong" && <Icon name="close" size={13} />}
                      <span>{option}</span>
                    </button>
                  );
                })}
                {picked !== null && (
                  <p className="quiz-explain small">
                    {q.explanation}{" "}
                    {q.quote && source === "here" && (
                      <button className="link-button" onClick={() => openQuote(q.quote)}>
                        {t("cards.openSource")}
                      </button>
                    )}
                  </p>
                )}
              </section>
            );
          })}
          {answered === quiz.length && (
            <div className="quiz-score">
              {t("study.score", { right: score, total: quiz.length })}
              <button onClick={() => run("quiz")}>{t("study.again")}</button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
