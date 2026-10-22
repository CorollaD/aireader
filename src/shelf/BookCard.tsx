import { useState, type MouseEvent } from "react";
import { coverUrl } from "../api";
import type { Book } from "../types";
import { formatDate, formatDuration, t } from "../i18n";
import { Icon } from "../icons";

/** Deterministic hue for books without a cover. */
function hue(text: string): number {
  let h = 0;
  for (const c of text) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
}

export function Cover({ book, small }: { book: Book; small?: boolean }) {
  const [failed, setFailed] = useState(false);
  if (book.hasCover && !failed) {
    return <img className="cover-img" src={coverUrl(book)} alt="" draggable={false} onError={() => setFailed(true)} />;
  }
  const h = hue(book.title);
  return (
    <div
      className={small ? "cover-placeholder small" : "cover-placeholder"}
      style={{ background: `linear-gradient(160deg, hsl(${h} 45% 52%), hsl(${(h + 40) % 360} 50% 34%))` }}
    >
      {!small && <span className="cover-title">{book.title}</span>}
      {!small && book.author && <span className="cover-author">{book.author}</span>}
    </div>
  );
}

export const percent = (p: number) => `${Math.round(p * 100)}%`;

interface CardProps {
  book: Book;
  selected: boolean;
  selecting: boolean;
  onOpen: () => void;
  onToggle: () => void;
  onMenu: (e: MouseEvent) => void;
}

export function BookCard({ book, selected, selecting, onOpen, onToggle, onMenu }: CardProps) {
  return (
    <div
      className={`book-card${selected ? " selected" : ""}${book.missing ? " missing" : ""}`}
      onClick={selecting ? onToggle : onOpen}
      onContextMenu={onMenu}
      title={
        book.missing
          ? t("book.missingTitle")
          : `${book.title}${book.readSeconds > 0 ? `\n${t("book.readTime", { time: formatDuration(book.readSeconds) })}` : ""}`
      }
    >
      <div className="cover">
        <Cover book={book} />
        <span className="badge">{book.format}</span>
        <button
          className={`check${selected ? " on" : ""}`}
          aria-label={t("common.select")}
          onClick={(e) => {
            e.stopPropagation();
            onToggle();
          }}
        >
          ✓
        </button>
        {book.progress > 0 && (
          <div className="progress-bar">
            <div style={{ width: percent(book.progress) }} />
          </div>
        )}
      </div>
      <div className="book-meta">
        <div className="book-title">{book.title}</div>
        <div className="book-sub">
          <span className="ellipsis">{book.author ?? t("common.unknownAuthor")}</span>
          <span>{book.missing ? t("book.missing") : book.progress > 0 ? percent(book.progress) : t("book.unread")}</span>
        </div>
      </div>
      <button
        className="more"
        aria-label={t("common.more")}
        onClick={(e) => {
          e.stopPropagation();
          onMenu(e);
        }}
      >
        <Icon name="more" />
      </button>
    </div>
  );
}

export function BookRow({ book, selected, selecting, onOpen, onToggle, onMenu }: CardProps) {
  return (
    <div
      className={`book-row${selected ? " selected" : ""}${book.missing ? " missing" : ""}`}
      onClick={selecting ? onToggle : onOpen}
      onContextMenu={onMenu}
    >
      <button
        className={`check inline${selected ? " on" : ""}`}
        aria-label={t("common.select")}
        onClick={(e) => {
          e.stopPropagation();
          onToggle();
        }}
      >
        ✓
      </button>
      <div className="row-cover">
        <Cover book={book} small />
      </div>
      <div className="row-main">
        <div className="book-title">{book.title}</div>
        <div className="book-sub">
          {book.author ?? t("common.unknownAuthor")}
          {book.group && <span className="row-groups"> · {book.group}</span>}
        </div>
      </div>
      <span className="row-cell">{book.format}</span>
      <span className="row-cell">{book.missing ? t("book.missing") : percent(book.progress)}</span>
      <span className="row-cell">{book.readSeconds > 0 ? formatDuration(book.readSeconds) : "—"}</span>
      <span className="row-cell">{formatDate(book.lastRead)}</span>
      <span className="row-cell">{formatDate(book.added)}</span>
      <button
        className="more inline"
        aria-label={t("common.more")}
        onClick={(e) => {
          e.stopPropagation();
          onMenu(e);
        }}
      >
        <Icon name="more" />
      </button>
    </div>
  );
}
