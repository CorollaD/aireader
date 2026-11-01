// What a PDF needs recognised, and starting that from the reader.

import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { api, errorText } from "../api";
import { ocrProgress, startOcr, watchOcr } from "./job";

/** A book counts as scanned, and is worth a suggestion, when this share of its pages has no text. */
const SCANNED_SHARE = 0.3;

const NONE: number[] = [];

export interface OcrState {
  /** Pages with no text yet. */
  pending: number[];
  /** Most of the book is pictures of text. */
  scanned: boolean;
  /** This book is being recognised. */
  running: boolean;
  error: string | null;
  start: () => void;
}

export function useOcr(bookId: string, title: string, isPdf: boolean): OcrState {
  const [status, setStatus] = useState<{ pages: number; pending: number[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const progress = useSyncExternalStore(watchOcr, ocrProgress);
  const running = progress?.bookId === bookId;

  useEffect(() => {
    if (!isPdf || running) return;
    let alive = true;
    api
      .ocrStatus(bookId)
      .then((s) => alive && setStatus(s))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [bookId, isPdf, running]);

  const pending = status?.pending ?? NONE;
  const start = useCallback(() => {
    setError(null);
    startOcr(bookId, title, pending).catch((e) => setError(errorText(e)));
  }, [bookId, title, pending]);

  return { pending, scanned: !!status && pending.length >= status.pages * SCANNED_SHARE, running, error, start };
}
