// A book's bookmarks and highlights, loaded once and saved shortly after each change.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import { onBeforeExit } from "../lifecycle";
import type { Annotations, Bookmark, Highlight, SyncChanges } from "../types";

const now = () => Math.floor(Date.now() / 1000);
const newId = () => crypto.randomUUID();

export interface AnnotationsApi {
  data: Annotations;
  addBookmark: (b: Omit<Bookmark, "id" | "created" | "updated">) => void;
  updateBookmark: (id: string, patch: Partial<Pick<Bookmark, "title">>) => void;
  removeBookmark: (id: string) => void;
  addHighlight: (h: Omit<Highlight, "id" | "created" | "updated">) => Highlight;
  updateHighlight: (id: string, patch: Partial<Omit<Highlight, "id" | "created">>) => void;
  removeHighlight: (id: string) => void;
}

export function useAnnotations(bookId: string): AnnotationsApi {
  const [data, setData] = useState<Annotations>({ bookmarks: [], highlights: [] });
  const loaded = useRef(false);
  const timer = useRef<number | undefined>(undefined);
  const latest = useRef(data);
  /** Deleted since the last save. Saves name them: anything else this copy lacks came from a sync. */
  const removed = useRef<string[]>([]);

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
    const gone = removed.current;
    removed.current = [];
    return api.saveAnnotations(bookId, latest.current, gone).catch(() => {});
  }, [bookId]);

  // Save pending changes when leaving the book, or quitting.
  useEffect(() => {
    const unregister = onBeforeExit(() => (timer.current !== undefined ? flush() : undefined));
    return () => {
      unregister();
      if (timer.current !== undefined) flush();
    };
  }, [flush]);

  // Another device changed this book's notes.
  useEffect(() => {
    const un = listen<SyncChanges>("sync-changed", async ({ payload }) => {
      if (!payload.books.includes(bookId)) return;
      if (timer.current !== undefined) await flush();
      const fresh = await api.getAnnotations(bookId);
      // A change made meanwhile saves and reloads on its own terms.
      if (timer.current !== undefined) return;
      latest.current = fresh;
      setData(fresh);
    });
    return () => {
      un.then((f) => f());
    };
  }, [bookId, flush]);

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
      addBookmark: (b: Omit<Bookmark, "id" | "created" | "updated">) =>
        change((a) => ({ ...a, bookmarks: [...a.bookmarks, { ...b, id: newId(), created: now(), updated: now() }] })),
      updateBookmark: (id: string, patch: Partial<Pick<Bookmark, "title">>) =>
        change((a) => ({ ...a, bookmarks: a.bookmarks.map((b) => (b.id === id ? { ...b, ...patch, updated: now() } : b)) })),
      removeBookmark: (id: string) => {
        removed.current.push(id);
        change((a) => ({ ...a, bookmarks: a.bookmarks.filter((b) => b.id !== id) }));
      },
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
      removeHighlight: (id: string) => {
        removed.current.push(id);
        change((a) => ({ ...a, highlights: a.highlights.filter((h) => h.id !== id) }));
      },
    }),
    [change],
  );

  return { data, ...methods };
}
