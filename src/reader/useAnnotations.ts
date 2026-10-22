// A book's bookmarks and highlights, loaded once and saved shortly after each change.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import type { Annotations, Bookmark, Highlight } from "../types";

const now = () => Math.floor(Date.now() / 1000);
const newId = () => crypto.randomUUID();

export interface AnnotationsApi {
  data: Annotations;
  addBookmark: (b: Omit<Bookmark, "id" | "created">) => void;
  updateBookmark: (id: string, patch: Partial<Pick<Bookmark, "title">>) => void;
  removeBookmark: (id: string) => void;
  addHighlight: (h: Omit<Highlight, "id" | "created" | "updated">) => Highlight;
  updateHighlight: (id: string, patch: Partial<Omit<Highlight, "id" | "created">>) => void;
  removeHighlight: (id: string) => void;
}

export function useAnnotations(bookId: string): AnnotationsApi {
  const [data, setData] = useState<Annotations>({ bookmarks: [], highlights: [] });
  const loaded = useRef(false);
  const timer = useRef<number>();
  const latest = useRef(data);

  useEffect(() => {
    loaded.current = false;
    api
      .getAnnotations(bookId)
      .then((a) => {
        latest.current = a;
        setData(a);
      })
      .catch(() => {})
      .finally(() => (loaded.current = true));
  }, [bookId]);

  const flush = useCallback(() => {
    window.clearTimeout(timer.current);
    timer.current = undefined;
    api.saveAnnotations(bookId, latest.current).catch(() => {});
  }, [bookId]);

  // Save pending changes when leaving the book.
  useEffect(
    () => () => {
      if (timer.current !== undefined) flush();
    },
    [flush],
  );

  const change = useCallback(
    (f: (a: Annotations) => Annotations) => {
      setData((prev) => {
        const next = f(prev);
        latest.current = next;
        return next;
      });
      if (!loaded.current) return;
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(flush, 400);
    },
    [flush],
  );

  const methods = useMemo(
    () => ({
      addBookmark: (b: Omit<Bookmark, "id" | "created">) =>
        change((a) => ({ ...a, bookmarks: [...a.bookmarks, { ...b, id: newId(), created: now() }] })),
      updateBookmark: (id: string, patch: Partial<Pick<Bookmark, "title">>) =>
        change((a) => ({ ...a, bookmarks: a.bookmarks.map((b) => (b.id === id ? { ...b, ...patch } : b)) })),
      removeBookmark: (id: string) => change((a) => ({ ...a, bookmarks: a.bookmarks.filter((b) => b.id !== id) })),
      addHighlight: (h: Omit<Highlight, "id" | "created" | "updated">) => {
        const full: Highlight = { ...h, id: newId(), created: now(), updated: now() };
        change((a) => ({ ...a, highlights: [...a.highlights, full] }));
        return full;
      },
      updateHighlight: (id: string, patch: Partial<Omit<Highlight, "id" | "created">>) =>
        change((a) => ({
          ...a,
          highlights: a.highlights.map((h) => (h.id === id ? { ...h, ...patch, updated: now() } : h)),
        })),
      removeHighlight: (id: string) => change((a) => ({ ...a, highlights: a.highlights.filter((h) => h.id !== id) })),
    }),
    [change],
  );

  return { data, ...methods };
}
