// Bottom bar: draggable progress track, remaining-time estimate, and a jump-to field.

import { useRef, useState } from "react";
import type { Location, OpenedBook } from "../types";
import { formatDuration, formatMinutes, t } from "../i18n";
import { describe, locationAt, progressOf, remainingMinutes } from "./locations";
import type { ReadingStats } from "./useReadingStats";

interface Props {
  book: OpenedBook;
  location: Location | null;
  stats: ReadingStats;
  onJump: (loc: Location) => void;
}

/** Parses "45%" / "45" (percent for text books) or a page number (paged books). */
function parseTarget(book: OpenedBook, input: string): Location | null {
  const text = input.trim();
  const value = parseFloat(text);
  if (!Number.isFinite(value)) return null;
  if (book.kind === "paged" && !text.endsWith("%")) {
    return { kind: "page", page: Math.max(0, Math.min(book.pages.length - 1, Math.round(value) - 1)) };
  }
  return locationAt(book, value / 100);
}

export default function ProgressBar({ book, location, stats, onJump }: Props) {
  const track = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const [editing, setEditing] = useState(false);
  const progress = location ? progressOf(book, location) : 0;

  const fractionAt = (clientX: number) => {
    const r = track.current!.getBoundingClientRect();
    return Math.max(0, Math.min(1, (clientX - r.left) / r.width));
  };

  const remaining = location ? remainingMinutes(book, location, stats.readingSpeed) : null;
  const shown = drag ?? progress;

  // Arrow keys step a page (paged books) or 1% (text); Page Up/Down 10%; Home/End the ends.
  const onKey = (e: React.KeyboardEvent) => {
    const pages = book.pages.length;
    const step = book.kind === "paged" && pages > 0 ? 1 / pages : 0.01;
    const moves: Record<string, number> = {
      ArrowRight: step,
      ArrowUp: step,
      ArrowLeft: -step,
      ArrowDown: -step,
      PageUp: 0.1,
      PageDown: -0.1,
    };
    let target: number;
    if (e.key === "Home") target = 0;
    else if (e.key === "End") target = 1;
    else if (e.key in moves) target = progress + moves[e.key];
    else return;
    e.preventDefault();
    e.stopPropagation();
    onJump(locationAt(book, Math.max(0, Math.min(1, target))));
  };

  return (
    <div className="progress-bar-area">
      <div
        className={`progress-track${drag !== null ? " dragging" : ""}`}
        ref={track}
        role="slider"
        tabIndex={0}
        aria-label={t("progress.jumpTitle")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(progress * 100)}
        aria-valuetext={location ? `${describe(book, location)} · ${Math.round(progress * 100)}%` : undefined}
        onKeyDown={onKey}
        onPointerDown={(e) => {
          try {
            (e.target as HTMLElement).setPointerCapture(e.pointerId);
          } catch {
            // Capture only keeps the drag alive outside the bar; dragging works without it.
          }
          setDrag(fractionAt(e.clientX));
        }}
        onPointerMove={(e) => drag !== null && setDrag(fractionAt(e.clientX))}
        onPointerUp={(e) => {
          if (drag === null) return;
          onJump(locationAt(book, fractionAt(e.clientX)));
          setDrag(null);
        }}
        onPointerCancel={() => setDrag(null)}
      >
        <div className="progress-fill" style={{ width: `${shown * 100}%` }} />
        <div className="progress-thumb" style={{ left: `${shown * 100}%` }} />
        {drag !== null && (
          <div className="progress-tip" style={{ left: `${drag * 100}%` }}>
            {describe(book, locationAt(book, drag))} · {Math.round(drag * 100)}%
          </div>
        )}
      </div>
      <div className="progress-info muted small">
        <span className="ellipsis">
          {remaining && (
            <>
              {remaining.chapter !== null && <>{t("progress.chapterLeft", { time: formatMinutes(remaining.chapter) })} · </>}
              {t("progress.bookLeft", { time: formatMinutes(remaining.book) })}
            </>
          )}
          {stats.readSeconds > 0 && <> · {t("progress.readTime", { time: formatDuration(stats.readSeconds) })}</>}
        </span>
        {editing ? (
          <input
            className="jump-input"
            autoFocus
            placeholder={book.kind === "paged" ? t("progress.jumpPaged") : t("progress.jumpPercent")}
            onBlur={() => setEditing(false)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                const loc = parseTarget(book, (e.target as HTMLInputElement).value);
                if (loc) onJump(loc);
                setEditing(false);
              } else if (e.key === "Escape") setEditing(false);
            }}
          />
        ) : (
          <button className="link-button" title={t("progress.jumpTitle")} onClick={() => setEditing(true)}>
            {location?.kind === "page" ? `${location.page + 1} / ${book.pages.length} · ` : ""}
            {Math.round(progress * 100)}%
          </button>
        )}
      </div>
    </div>
  );
}
