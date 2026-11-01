// Results of searching inside every book on the shelf, under the title matches.

import { useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import { t } from "../i18n";
import { describeHit } from "./searchText";
import type { Book, IndexProgress, LibraryHits, Location } from "../types";
import { Cover } from "./BookCard";

/** Waits this long after typing before searching. */
const DEBOUNCE_MS = 250;

/** `text` with the words of `query` marked. */
function marked(text: string, query: string): ReactNode[] {
  const words = query.split(/\s+/).filter(Boolean);
  if (!words.length) return [text];
  const pattern = new RegExp(`(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`, "gi");
  return text.split(pattern).map((part, i) => (i % 2 ? <mark key={i}>{part}</mark> : part));
}

export default function LibrarySearch(props: {
  query: string;
  books: Book[];
  onOpen: (book: Book, location: Location) => void;
  onResults: (found: boolean) => void;
}) {
  const { query, onResults } = props;
  const [results, setResults] = useState<LibraryHits[]>([]);
  const [indexing, setIndexing] = useState<IndexProgress | null>(null);

  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setResults([]);
      onResults(false);
      return;
    }
    let alive = true;
    const timer = window.setTimeout(() => {
      api
        .searchLibrary(q)
        .then((r) => {
          if (!alive) return;
          setResults(r);
          onResults(r.length > 0);
        })
        .catch(() => {});
    }, DEBOUNCE_MS);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [query, onResults]);

  useEffect(() => {
    api
      .fulltextStatus()
      .then(setIndexing)
      .catch(() => {});
    const un = listen<IndexProgress | null>("fulltext-progress", (e) => setIndexing(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);

  if (!query.trim()) return null;
  const shown = results.flatMap((r) => {
    const book = props.books.find((b) => b.id === r.bookId);
    return book ? [{ book, hits: r.hits }] : [];
  });
  return (
    <section className="library-search">
      {shown.length > 0 && <div className="panel-heading">{t("shelf.inBooks", { n: shown.length })}</div>}
      {indexing && (
        <div className="muted small index-note">
          {t("shelf.indexing", { done: indexing.done, total: indexing.total })}{" "}
          <button
            className="link-button"
            onClick={() => {
              api.stopIndexing().catch(() => {});
              setIndexing(null);
            }}
          >
            {t("common.stop")}
          </button>
        </div>
      )}
      {shown.map(({ book, hits }) => (
        <div key={book.id} className="library-hit">
          <div className="row-cover">
            <Cover book={book} small />
          </div>
          <div className="library-hit-main">
            <div className="book-title">{book.title}</div>
            {hits.map((h, i) => (
              <button key={i} className="hit" onClick={() => props.onOpen(book, h.location)}>
                <span className="hit-chapter">{describeHit(h)}</span>
                <span>{marked(h.snippet, query)}</span>
              </button>
            ))}
          </div>
        </div>
      ))}
    </section>
  );
}
