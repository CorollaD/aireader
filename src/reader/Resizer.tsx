// Draggable dividers between the reader's side panels and the text, with widths remembered
// per book.

import { useState } from "react";
import { t } from "../i18n";

export type Panel = "toc" | "ai" | "search";

const DEFAULTS: Record<Panel, number> = { toc: 270, ai: 360, search: 300 };
const LIMITS: Record<Panel, [number, number]> = { toc: [200, 480], ai: [300, 720], search: [240, 520] };
/** The text keeps at least this much room; panels shrink first. */
const MIN_TEXT = 360;

type Widths = Record<Panel, number>;

function load(bookId: string): Widths {
  try {
    const saved = JSON.parse(localStorage.getItem(`panel-widths:${bookId}`) ?? "{}") as Partial<Widths>;
    return { ...DEFAULTS, ...saved };
  } catch {
    return { ...DEFAULTS };
  }
}

function save(bookId: string, widths: Widths) {
  try {
    localStorage.setItem(`panel-widths:${bookId}`, JSON.stringify(widths));
  } catch {
    // Storage unavailable: widths last until the book is closed.
  }
}

/** The panel's width clamped to its limits and to what leaves the text `MIN_TEXT`. */
export function fitWidth(panel: Panel, wanted: number, room: number): number {
  const [min, max] = LIMITS[panel];
  return Math.max(min, Math.min(wanted, max, room - MIN_TEXT));
}

export const minWidth = (panel: Panel) => LIMITS[panel][0];

/** Widths of the reader's panels for one book, as chosen by dragging. */
export function usePanelWidths(bookId: string) {
  const [widths, setWidths] = useState<Widths>(() => load(bookId));
  const set = (panel: Panel, width: number) =>
    setWidths((w) => {
      const next = { ...w, [panel]: Math.round(width) };
      save(bookId, next);
      return next;
    });
  const reset = (panel: Panel) => set(panel, DEFAULTS[panel]);
  return { widths, set, reset };
}

/**
 * A divider beside a panel. `side` is where the panel is: dragging towards the text makes it
 * wider. `max` is the widest the panel may get right now.
 */
export function Resizer(props: {
  panel: Panel;
  side: "left" | "right";
  width: number;
  max: number;
  onChange: (width: number) => void;
  onReset: () => void;
}) {
  const [dragging, setDragging] = useState(false);
  const [min] = LIMITS[props.panel];

  const start = (e: React.PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = props.width;
    const target = e.currentTarget as HTMLElement;
    try {
      target.setPointerCapture(e.pointerId);
    } catch {
      // Dragging still works while the pointer stays over the window.
    }
    document.body.classList.add("resizing");
    setDragging(true);
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - startX;
      const wanted = startWidth + (props.side === "left" ? dx : -dx);
      props.onChange(Math.max(min, Math.min(props.max, wanted)));
    };
    const end = () => {
      document.body.classList.remove("resizing");
      setDragging(false);
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", end);
      target.removeEventListener("pointercancel", end);
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", end);
    target.addEventListener("pointercancel", end);
  };

  return (
    <div
      className={`resizer${dragging ? " active" : ""}`}
      role="separator"
      aria-orientation="vertical"
      aria-valuenow={props.width}
      aria-valuemin={min}
      aria-valuemax={props.max}
      title={t("reader.resizeHint")}
      onPointerDown={start}
      onDoubleClick={props.onReset}
    />
  );
}
