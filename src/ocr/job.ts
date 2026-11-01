// Recognising the text of a scanned PDF, in the background. Runs one book at a time, whatever
// view is open, and saves each page as it goes: stopping loses nothing, and a later run carries
// on with the pages that are left.

import { api } from "../api";
import { onBeforeExit } from "../lifecycle";
import { toRuns } from "./layout";
import { OCR_MODELS } from "./models";
import type { OcrPage, OcrRequest } from "./worker";

/** Rendered width of a page for recognition. */
const PAGE_WIDTH = 1200;
/** Each engine is a single-threaded worker: threads within one would need cross-origin
 * isolation, and page by page work spreads evenly. Each holds its own models, hence the limit. */
const MAX_ENGINES = 3;

export interface OcrProgress {
  bookId: string;
  title: string;
  done: number;
  total: number;
  /** Models are loading; no page has started. */
  loading: boolean;
}

/** A recogniser in a worker of its own. */
class Engine {
  private worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  private calls = new Map<number, { resolve: (value: never) => void; reject: (error: Error) => void }>();
  private nextId = 0;

  constructor() {
    this.worker.onmessage = ({ data }) => {
      const call = this.calls.get(data.id);
      this.calls.delete(data.id);
      if (data.error) call?.reject(new Error(data.error));
      else call?.resolve(data.result as never);
    };
    this.worker.onerror = (e) => this.fail(new Error(e.message || "The recogniser failed to start"));
  }

  private fail(error: Error) {
    this.calls.forEach((call) => call.reject(error));
    this.calls.clear();
  }

  private call<T>(request: DistributiveOmit<OcrRequest, "id">): Promise<T> {
    return new Promise((resolve, reject) => {
      const id = this.nextId++;
      this.calls.set(id, { resolve: resolve as (value: never) => void, reject });
      this.worker.postMessage({ ...request, id });
    });
  }

  init = (detection: string, recognition: string) => this.call<void>({ type: "init", detection, recognition });
  recognise = (image: Blob) => this.call<OcrPage>({ type: "recognise", image });

  stop() {
    this.worker.terminate();
    this.fail(new Error("stopped"));
  }
}

type DistributiveOmit<T, K extends keyof never> = T extends unknown ? Omit<T, K> : never;

let progress: OcrProgress | null = null;
let stopping = false;
let engines: Engine[] = [];
const watchers = new Set<() => void>();
const pageWatchers = new Set<(bookId: string, page: number) => void>();

function publish(next: OcrProgress | null) {
  progress = next;
  watchers.forEach((w) => w());
}

export const ocrProgress = () => progress;

/** Calls `watch` whenever the progress changes; returns an unsubscribe. */
export function watchOcr(watch: () => void): () => void {
  watchers.add(watch);
  return () => {
    watchers.delete(watch);
  };
}

/** Calls `watch` for each page recognised; returns an unsubscribe. */
export function watchOcrPages(watch: (bookId: string, page: number) => void): () => void {
  pageWatchers.add(watch);
  return () => {
    pageWatchers.delete(watch);
  };
}

async function createEngines(): Promise<Engine[]> {
  // In a blob each: workers can read those, and only the page can call the backend.
  const model = async (name: string) => URL.createObjectURL(new Blob([await api.ocrModel(name)]));
  const detection = await model(OCR_MODELS.detection);
  const recognition = await model(OCR_MODELS.recognition);
  const count = Math.min(MAX_ENGINES, Math.max(1, Math.floor((navigator.hardwareConcurrency || 2) / 2)));
  const created = Array.from({ length: count }, () => new Engine());
  engines = created;
  try {
    await Promise.all(created.map((e) => e.init(detection, recognition)));
  } finally {
    URL.revokeObjectURL(detection);
    URL.revokeObjectURL(recognition);
  }
  return created;
}

async function recognise(engine: Engine, bookId: string, page: number) {
  const bytes = await api.ocrPageImage(bookId, page, PAGE_WIDTH);
  const result = await engine.recognise(new Blob([bytes], { type: "image/jpeg" }));
  await api.ocrSavePage(bookId, page, toRuns(result.lines, result.width, result.height));
}

/** Recognises a book's pages without text. Rejects if the engine can't run or a page fails. */
export async function startOcr(bookId: string, title: string, pages: number[]): Promise<void> {
  if (progress || pages.length === 0) return;
  stopping = false;
  let done = 0;
  const report = (loading: boolean) => publish({ bookId, title, done, total: pages.length, loading });
  report(true);
  const queue = [...pages];
  try {
    const created = await createEngines();
    report(false);
    await Promise.all(
      created.map(async (engine) => {
        for (let page = queue.shift(); page !== undefined && !stopping; page = queue.shift()) {
          await recognise(engine, bookId, page);
          done++;
          report(false);
          pageWatchers.forEach((w) => w(bookId, page!));
        }
      }),
    );
  } catch (e) {
    // Stopping tears down the engines under pages still in progress.
    if (!stopping) throw e;
  } finally {
    queue.length = 0;
    engines.forEach((e) => e.stop());
    engines = [];
    if (done > 0) await api.ocrFinish(bookId).catch(() => {});
    publish(null);
  }
}

export function stopOcr() {
  if (!progress) return;
  stopping = true;
  engines.forEach((e) => e.stop());
  engines = [];
}

// Whatever was recognised still goes into the search index when the app quits midway.
onBeforeExit(() => {
  if (!progress) return;
  stopping = true;
  return api.ocrFinish(progress.bookId);
});
