import { useState, type CSSProperties, type KeyboardEvent, type MouseEvent } from "react";
import { coverUrl } from "../api";
import type { Book } from "../types";
import { formatDate, formatDuration, formatSize, t } from "../i18n";
import { Icon } from "../icons";

/** Deterministic hue for books without a cover. */
function hue(text: string): number {
  let h = 0;
  for (const c of text) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
}

/** The cover image, or one set from the title on a tint of the book's own hue. */
export function Cover({ book, small }: { book: Book; small?: boolean }) {
  const [failed, setFailed] = useState(false);
  if (book.hasCover && !failed) {
    return (
      <img
        className="block size-full object-cover"
        src={coverUrl(book)}
        alt=""
        loading="lazy"
        decoding="async"
        draggable={false}
        onError={() => setFailed(true)}
      />
    );
  }
  const h = hue(book.title);
  return (
    <div
      className="flex size-full flex-col justify-end gap-1.5 p-[12%] pl-[16%] text-left"
      style={
        {
          background: `linear-gradient(160deg, hsl(${h} 32% var(--cover-a)), hsl(${h} 26% var(--cover-b)))`,
          color: `hsl(${h} 40% var(--cover-ink))`,
        } as CSSProperties
      }
    >
      {!small && <span className="line-clamp-6 text-[15px] leading-snug font-bold tracking-tight [overflow-wrap:anywhere]">{book.title}</span>}
      {!small && book.author && <span className="text-[11px] font-medium opacity-75">{book.author}</span>}
    </div>
  );
}

export const percent = (p: number) => `${Math.round(p * 100)}%`;
const missingLabel = (book: Book) => t(book.synced ? "book.notDownloaded" : "book.missing");

interface CardProps {
  book: Book;
  selected: boolean;
  selecting: boolean;
  onOpen: () => void;
  onToggle: () => void;
  onMenu: (e: MouseEvent) => void;
}

/** Enter/Space activate a card or row like a button (they contain buttons, so they can't be one). */
const activate = (run: () => void) => (e: KeyboardEvent) => {
  if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
  e.preventDefault();
  run();
};

/** A book on the shelf: the cover standing like a real one, its title and progress below. */
export function BookCard({ book, selected, selecting, onOpen, onToggle, onMenu }: CardProps) {
  const status = book.missing ? missingLabel(book) : book.progress >= 1 ? t("book.finished") : book.progress > 0 ? percent(book.progress) : t("book.unread");
  return (
    <div
      className={`book-tile group relative cursor-pointer rounded-md outline-offset-4${selected ? " selected" : ""}${book.missing ? " missing" : ""}`}
      role="button"
      tabIndex={0}
      aria-label={book.title}
      onKeyDown={activate(selecting ? onToggle : onOpen)}
      onClick={selecting ? onToggle : onOpen}
      onContextMenu={onMenu}
      title={
        book.missing
          ? t(book.synced ? "book.notDownloadedTitle" : "book.missingTitle")
          : [
              book.title,
              t("book.size", { size: formatSize(book.size) }),
              book.readSeconds > 0 && t("book.readTime", { time: formatDuration(book.readSeconds) }),
            ]
              .filter(Boolean)
              .join("\n")
      }
    >
      <div className="book3d">
        <div className="book3d-face">
          <Cover book={book} />
          {book.progress > 0 && book.progress < 1 && (
            <div className="absolute inset-x-0 bottom-0 h-[3px] bg-black/20">
              <div className="h-full bg-primary" style={{ width: percent(book.progress) }} />
            </div>
          )}
        </div>
        <span className="absolute top-2 left-3 rounded px-1.5 py-px text-[9px] font-bold tracking-wider text-white backdrop-blur-sm bg-black/45">
          {book.format}
        </span>
        <button
          className={`d-btn d-btn-circle d-btn-xs absolute top-2 right-2 border-black/20 shadow transition-opacity ${
            selected ? "d-btn-primary opacity-100" : "bg-white/90 text-transparent opacity-0 group-hover:opacity-100"
          }${selecting ? " opacity-100" : ""}`}
          aria-label={t("common.select")}
          onClick={(e) => {
            e.stopPropagation();
            onToggle();
          }}
        >
          <Icon name="check" size={12} />
        </button>
      </div>
      <div className="mt-3 pr-6">
        <div className="line-clamp-2 text-[13px] leading-snug font-semibold">{book.title}</div>
        <div className="mt-0.5 flex items-baseline justify-between gap-2 text-xs text-[var(--muted)]">
          <span className="truncate">{book.author ?? t("common.unknownAuthor")}</span>
          <span className={`shrink-0 tabular-nums${book.progress > 0 && !book.missing ? " font-semibold text-[var(--fg)]" : ""}`}>{status}</span>
        </div>
      </div>
      <button
        className="d-btn d-btn-ghost d-btn-xs d-btn-square absolute right-0 bottom-0 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
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

/** A book as a row of the list view. */
export function BookRow({ book, selected, selecting, onOpen, onToggle, onMenu }: CardProps) {
  const cell = "w-[88px] shrink-0 text-[13px] text-[var(--muted)] tabular-nums";
  return (
    <div
      className={`group flex cursor-pointer items-center gap-3 rounded-lg px-2 py-2 hover:bg-[var(--hover)]${selected ? " bg-[var(--accent-soft)]" : ""}${
        book.missing ? " opacity-55" : ""
      }`}
      role="button"
      tabIndex={0}
      aria-label={book.title}
      onKeyDown={activate(selecting ? onToggle : onOpen)}
      onClick={selecting ? onToggle : onOpen}
      onContextMenu={onMenu}
    >
      <input
        type="checkbox"
        className={`d-checkbox d-checkbox-sm d-checkbox-primary transition-opacity ${selecting || selected ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}
        checked={selected}
        aria-label={t("common.select")}
        onClick={(e) => e.stopPropagation()}
        onChange={onToggle}
      />
      <div className="book3d flat w-9 shrink-0">
        <div className="book3d-face">
          <Cover book={book} small />
        </div>
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-semibold">{book.title}</div>
        <div className="truncate text-xs text-[var(--muted)]">
          {book.author ?? t("common.unknownAuthor")}
          {book.group && <span className="text-[var(--faint)]"> · {book.group}</span>}
        </div>
      </div>
      <span className={cell}>{book.format}</span>
      <span className={cell}>{formatSize(book.size)}</span>
      <span className={cell}>{book.missing ? missingLabel(book) : percent(book.progress)}</span>
      <span className={cell}>{book.readSeconds > 0 ? formatDuration(book.readSeconds) : "—"}</span>
      <span className={cell}>{formatDate(book.lastRead)}</span>
      <span className={cell}>{formatDate(book.added)}</span>
      <button
        className="d-btn d-btn-ghost d-btn-xs d-btn-square opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
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
