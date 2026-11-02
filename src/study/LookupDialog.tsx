// Looking up a selected word or phrase, and keeping it as a flashcard: the answer can come from
// the added dictionaries, from the AI, or from the reader's own words.

import { useEffect, useRef, useState } from "react";
import { api, errorText, runAiTask } from "../api";
import { useAi } from "../ai/AiContext";
import { t } from "../i18n";
import { Icon } from "../icons";
import { useOpenSettings } from "../SettingsDialog";
import type { Settings } from "../settings";
import { speakOnce } from "../reader/useSpeech";
import type { Card, Definition, Location } from "../types";
import { Modal } from "../ui";

/** The longest text worth looking up in a dictionary. */
const MAX_WORD = 30;

/** What is being looked up. */
export interface LookupRequest {
  word: string;
  context: string;
  location: Location | null;
  /** An answer to start from, e.g. the note on a highlight. */
  back?: string;
}

export default function LookupDialog(props: {
  request: LookupRequest;
  /** Saved under this book. */
  book: { id: string | null; title: string };
  /** The card being edited, if it is known already. */
  card?: Card;
  settings: Settings;
  onSaved: (card: Card) => void;
  onClose: () => void;
}) {
  const { request, book, settings } = props;
  const ai = useAi();
  const openSettings = useOpenSettings();
  const [front, setFront] = useState(request.word);
  const [back, setBack] = useState(request.back ?? "");
  const [definitions, setDefinitions] = useState<Definition[] | null>(null);
  const [existing, setExisting] = useState<Card | null>(props.card ?? null);
  const [asking, setAsking] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const edited = useRef(false);
  const shortText = request.word.length <= MAX_WORD;

  useEffect(() => {
    let alive = true;
    // Passages aren't in dictionaries.
    const lookedUp = shortText ? api.lookupWord(request.word) : Promise.resolve([]);
    Promise.all([props.card ?? api.findCard(book.id, request.word), lookedUp])
      .then(([card, defs]) => {
        if (!alive) return;
        setExisting(card);
        setDefinitions(defs);
        if (!edited.current) setBack(card?.back || request.back || defs[0]?.text || "");
      })
      .catch((e) => {
        setDefinitions([]);
        setError(errorText(e));
      });
    return () => {
      alive = false;
    };
  }, [book.id, request.word, request.back, props.card, shortText]);

  const askAi = async () => {
    if (!(await ai.ensureConsent())) return;
    setError(null);
    edited.current = true;
    setBack("");
    const taskId = runAiTask({ kind: "define", bookTitle: book.title, word: front, context: request.context }, (e) => {
      if (e.type === "delta") setBack((b) => b + e.text);
      else if (e.type === "done") {
        setBack(e.text);
        setAsking(null);
      } else if (e.type === "error") {
        setError(e.message);
        setAsking(null);
      }
    });
    setAsking(taskId);
  };

  const save = async () => {
    try {
      const card = existing
        ? await api.updateCard(existing.id, { front, back })
        : await api.addCard({
            id: crypto.randomUUID(),
            bookId: book.id,
            bookTitle: book.title,
            front,
            back,
            context: request.context,
            location: request.location,
          });
      props.onSaved(card);
      props.onClose();
    } catch (e) {
      setError(errorText(e));
    }
  };

  const close = () => {
    if (asking) api.aiCancel(asking).catch(() => {});
    props.onClose();
  };

  return (
    <Modal
      title={existing ? t("cards.edit") : t("cards.lookup")}
      width={520}
      onClose={close}
      footer={
        <>
          <button onClick={close}>{t("common.cancel")}</button>
          <button className="primary" disabled={!front.trim() || !!asking} onClick={save}>
            {existing ? t("common.save") : t("cards.add")}
          </button>
        </>
      }
    >
      <div className="lookup">
        <div className="lookup-word">
          <input value={front} aria-label={t("cards.front")} onChange={(e) => setFront(e.target.value)} />
          <button className="icon-button" title={t("cards.speak")} aria-label={t("cards.speak")} onClick={() => speakOnce(front, settings).catch(() => {})}>
            <Icon name="volume" size={16} />
          </button>
        </div>
        {request.context && request.context !== request.word && <blockquote className="note-quote">{request.context}</blockquote>}
        {definitions && definitions.length > 1 && (
          <div className="lookup-sources">
            {definitions.map((d) => (
              <button
                key={d.dictionary}
                onClick={() => {
                  edited.current = true;
                  setBack(d.text);
                }}
              >
                {d.dictionary}
              </button>
            ))}
          </div>
        )}
        <textarea
          className="note-text"
          value={back}
          placeholder={definitions === null ? t("common.loading") : t("cards.backPlaceholder")}
          aria-label={t("cards.back")}
          onChange={(e) => {
            edited.current = true;
            setBack(e.target.value);
          }}
        />
        <div className="lookup-foot">
          {ai.ready && (
            <button disabled={!!asking} onClick={askAi}>
              <Icon name="sparkles" size={14} /> {asking ? t("cards.asking") : t("cards.askAi")}
            </button>
          )}
          {definitions?.length === 0 && shortText && (
            <span className="muted small">
              {t("cards.noDefinition")}{" "}
              <button className="link-button" onClick={() => openSettings("dictionary")}>
                {t("cards.addDictionary")}
              </button>
            </span>
          )}
        </div>
        {definitions && definitions[0] && <div className="muted small">{t("cards.source", { name: definitions[0].dictionary })}</div>}
        {error && <div className="error small">{error}</div>}
      </div>
    </Modal>
  );
}
