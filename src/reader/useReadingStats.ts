// Counts active reading time and measures reading speed, saving both to the library.
//
// Time counts only while the window is visible and focused and the reader has interacted
// (key, wheel, click, scroll) in the last two minutes. Speed is how far the position moved
// forward during that time; big jumps (navigation) are ignored.

import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { Location, OpenedBook } from "../types";
import { unitsAt } from "./locations";

const TICK_MS = 15_000;
const IDLE_MS = 120_000;
const FLUSH_SECONDS = 60;
/** Largest forward move per tick still counted as reading (chars or pages). */
const MAX_PER_TICK = { flow: 2_000, paged: 4 };

export interface ReadingStats {
  readSeconds: number;
  readingSpeed: number | null;
}

export function useReadingStats(book: OpenedBook | null, location: Location | null): ReadingStats {
  const [stats, setStats] = useState<ReadingStats>({ readSeconds: 0, readingSpeed: null });
  const lastActivity = useRef(Date.now());
  const locRef = useRef(location);
  locRef.current = location;
  const acc = useRef({ seconds: 0, units: 0, lastUnits: null as number | null });

  useEffect(() => {
    if (book) setStats({ readSeconds: book.readSeconds, readingSpeed: book.readingSpeed });
  }, [book]);

  useEffect(() => {
    const mark = () => (lastActivity.current = Date.now());
    const events = ["keydown", "wheel", "mousedown", "touchstart"] as const;
    for (const e of events) window.addEventListener(e, mark, { passive: true });
    document.addEventListener("scroll", mark, { capture: true, passive: true });
    return () => {
      for (const e of events) window.removeEventListener(e, mark);
      document.removeEventListener("scroll", mark, { capture: true });
    };
  }, []);

  useEffect(() => {
    if (!book) return;
    const a = acc.current;
    a.seconds = 0;
    a.units = 0;
    a.lastUnits = null;

    const flush = () => {
      if (a.seconds < 1) return;
      const minutes = a.seconds / 60;
      const speed = a.seconds >= FLUSH_SECONDS && a.units > 0 ? a.units / minutes : null;
      const seconds = Math.round(a.seconds);
      a.seconds = 0;
      a.units = 0;
      setStats((s) => ({ ...s, readSeconds: s.readSeconds + seconds }));
      api
        .addReadingTime(book.id, seconds, speed)
        .then((b) => setStats({ readSeconds: b.readSeconds, readingSpeed: b.readingSpeed }))
        .catch(() => {});
    };

    const tick = () => {
      const loc = locRef.current;
      const units = loc ? unitsAt(book, loc) : null;
      const active =
        document.visibilityState === "visible" && document.hasFocus() && Date.now() - lastActivity.current < IDLE_MS;
      if (active) {
        a.seconds += TICK_MS / 1000;
        if (units !== null && a.lastUnits !== null) {
          const moved = units - a.lastUnits;
          if (moved > 0 && moved <= MAX_PER_TICK[book.kind]) a.units += moved;
        }
      }
      a.lastUnits = units;
      if (a.seconds >= FLUSH_SECONDS) flush();
    };

    const timer = window.setInterval(tick, TICK_MS);
    return () => {
      window.clearInterval(timer);
      flush();
    };
  }, [book]);

  return stats;
}
