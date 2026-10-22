// Bottom bar: draggable progress track, remaining-time estimate, and a jump-to field.

import { useRef, useState } from "react";
import type { Location, OpenedBook } from "../types";
import { describe, formatDuration, formatMinutes, locationAt, progressOf, remainingMinutes } from "./locations";
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

  return (
    <div className="progress-bar-area">
      <div
        className={`progress-track${drag !== null ? " dragging" : ""}`}
        ref={track}
        onPointerDown={(e) => {
          (e.target as HTMLElement).setPointerCapture(e.pointerId);
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
              {remaining.chapter !== null && <>本章剩余约 {formatMinutes(remaining.chapter)} · </>}
              全书剩余约 {formatMinutes(remaining.book)}
            </>
          )}
          {stats.readSeconds > 0 && <> · 已读 {formatDuration(stats.readSeconds)}</>}
        </span>
        {editing ? (
          <input
            className="jump-input"
            autoFocus
            placeholder={book.kind === "paged" ? "页码或 50%" : "百分比"}
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
          <button className="link-button" title="跳转到百分比或页码" onClick={() => setEditing(true)}>
            {location?.kind === "page" ? `${location.page + 1} / ${book.pages.length} · ` : ""}
            {Math.round(progress * 100)}%
          </button>
        )}
      </div>
    </div>
  );
}
