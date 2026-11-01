// Reading the book aloud with the system's voices, one sentence at a time, on into the next
// chapter or page. Pausing cancels the sentence and starts it again on resuming, which works
// the same on every system's speech engine.

import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { Settings } from "../settings";
import type { Location, OpenedBook } from "../types";
import { pageText } from "./PagedView";
import { chapterSentences, speechLang, splitSentences, type Sentence } from "./speech";

/** How long to wait for the voice list, which arrives late on some systems. */
const VOICES_MS = 1500;

export type SpeechState = "idle" | "playing" | "paused";

/** The sentence being read. */
export type SpeechPosition = { kind: "flow"; chapter: number; block: number; start: number; end: number } | { kind: "page"; page: number };

/** Where to start: in a chapter at a block and offset, or on a page. */
export type SpeechStart = { kind: "flow"; chapter: number; block: number; offset?: number } | { kind: "page"; page: number };

const supported = () => typeof window !== "undefined" && "speechSynthesis" in window;

/** The system's voices; empty where there are none (often on Linux). */
export function useVoices(): SpeechSynthesisVoice[] {
  const [voices, setVoices] = useState<SpeechSynthesisVoice[]>([]);
  useEffect(() => {
    if (!supported()) return;
    const load = () => setVoices(window.speechSynthesis.getVoices());
    load();
    window.speechSynthesis.addEventListener("voiceschanged", load);
    const timer = window.setTimeout(load, VOICES_MS);
    return () => {
      window.speechSynthesis.removeEventListener("voiceschanged", load);
      window.clearTimeout(timer);
    };
  }, []);
  return voices;
}

/** Says `text` once (e.g. a word on a flashcard), interrupting anything being read. */
export function speakOnce(text: string, settings: Pick<Settings, "ttsVoice" | "ttsRate">) {
  if (!supported()) return;
  const synth = window.speechSynthesis;
  synth.cancel();
  const u = new SpeechSynthesisUtterance(text);
  u.lang = speechLang(text);
  u.rate = settings.ttsRate;
  const voice = synth.getVoices().find((v) => v.voiceURI === settings.ttsVoice);
  if (voice) u.voice = voice;
  synth.speak(u);
}

export function useSpeech(book: OpenedBook | null, settings: Settings, onError: (message: string) => void, onPage: (loc: Location) => void) {
  const [state, setState] = useState<SpeechState>("idle");
  const [position, setPosition] = useState<SpeechPosition | null>(null);
  const run = useRef({ unit: 0, queue: [] as Sentence[], index: 0, token: 0 });
  // The utterance is kept referenced: some engines drop its events once it is collected.
  const utterance = useRef<SpeechSynthesisUtterance | null>(null);
  const shownPage = useRef<number | null>(null);
  const live = useRef({ book, settings, onError, onPage });
  live.current = { book, settings, onError, onPage };

  const stop = useCallback(() => {
    run.current.token++;
    if (supported()) window.speechSynthesis.cancel();
    shownPage.current = null;
    setState("idle");
    setPosition(null);
  }, []);

  /** The sentences of a chapter or page. */
  const load = useCallback(async (unit: number): Promise<Sentence[]> => {
    const b = live.current.book!;
    if (b.kind === "flow") return chapterSentences(await api.getChapter(b.id, unit));
    const text = (await pageText(b.id, unit)).map((r) => r.text).join(" ");
    return splitSentences(text).map(([start, end]) => ({ block: 0, start, end, text: text.slice(start, end) }));
  }, []);

  const say = useCallback(
    (index: number) => {
      const r = run.current;
      const b = live.current.book;
      if (!b) return;
      const token = ++r.token;
      const sentence = r.queue[index];
      if (!sentence) {
        // End of the chapter or page: on to the next one that has text.
        const units = b.kind === "flow" ? b.chapters.length : b.pages.length;
        const next = async (unit: number) => {
          if (unit >= units) return stop();
          const queue = await load(unit);
          if (token !== r.token) return;
          if (!queue.length) return next(unit + 1);
          r.unit = unit;
          r.queue = queue;
          say(0);
        };
        next(r.unit + 1).catch((e) => {
          stop();
          live.current.onError(String(e));
        });
        return;
      }
      r.index = index;
      if (b.kind === "flow") {
        setPosition({ kind: "flow", chapter: r.unit, block: sentence.block, start: sentence.start, end: sentence.end });
      } else {
        if (shownPage.current !== r.unit) live.current.onPage({ kind: "page", page: r.unit });
        shownPage.current = r.unit;
        setPosition({ kind: "page", page: r.unit });
      }
      const { ttsRate, ttsVoice } = live.current.settings;
      const synth = window.speechSynthesis;
      const u = new SpeechSynthesisUtterance(sentence.text);
      u.lang = speechLang(sentence.text);
      u.rate = ttsRate;
      const voice = synth.getVoices().find((v) => v.voiceURI === ttsVoice);
      if (voice) u.voice = voice;
      u.onend = () => {
        if (token === r.token) say(index + 1);
      };
      u.onerror = (e) => {
        // Cancelling ends an utterance with an error too.
        if (token !== r.token || e.error === "canceled" || e.error === "interrupted") return;
        stop();
        live.current.onError(e.error);
      };
      utterance.current = u;
      synth.cancel();
      synth.speak(u);
      setState("playing");
    },
    [load, stop],
  );

  const start = useCallback(
    async (from: SpeechStart) => {
      const r = run.current;
      const token = ++r.token;
      const unit = from.kind === "flow" ? from.chapter : from.page;
      try {
        const queue = await load(unit);
        if (token !== r.token) return;
        r.unit = unit;
        r.queue = queue;
        const i = from.kind === "flow" ? queue.findIndex((s) => s.block > from.block || (s.block === from.block && s.end > (from.offset ?? 0))) : 0;
        say(i < 0 ? queue.length : i);
      } catch (e) {
        live.current.onError(String(e));
      }
    },
    [load, say],
  );

  const pause = useCallback(() => {
    run.current.token++;
    window.speechSynthesis.cancel();
    setState("paused");
  }, []);

  const resume = useCallback(() => say(run.current.index), [say]);

  /** Goes a sentence forward or back and reads from there. */
  const step = useCallback(
    (delta: 1 | -1) => {
      const r = run.current;
      say(Math.max(0, r.index + delta));
    },
    [say],
  );

  // A different book, or leaving the reader, ends the reading.
  const bookId = book?.id;
  useEffect(() => stop, [bookId, stop]);

  return { state, position, start, pause, resume, step, stop };
}
