// Typed wrappers around the Tauri commands and `book://` URLs.

import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AiConfig,
  AiEvent,
  AiStatus,
  AiTask,
  Annotations,
  Block,
  Book,
  BookEstimate,
  BookPatch,
  Hit,
  ImportReport,
  Location,
  OpenedBook,
  Shelf,
  Suggestion,
  TextRun,
} from "./types";

// Custom protocols are exposed as http://<scheme>.localhost on Windows.
const BASE = navigator.userAgent.includes("Windows") ? "http://book.localhost/" : "book://localhost/";

export const coverUrl = (book: Book) => `${BASE}cover/${book.id}?v=${book.added}`;
export const imageUrl = (bookId: string, imageId: number) => `${BASE}img/${bookId}/${imageId}`;
export const pageUrl = (bookId: string, page: number, width: number, rotation = 0) =>
  `${BASE}page/${bookId}/${page}/${width}${rotation ? `/${rotation}` : ""}`;

export const api = {
  listBooks: () => invoke<Shelf>("list_books"),
  importBooks: (paths: string[], group?: string) => invoke<ImportReport>("import_books", { paths, group }),
  takeStartupBook: () => invoke<string | null>("take_startup_book"),
  updateBook: (id: string, patch: BookPatch) => invoke<Book>("update_book", { id, patch }),
  removeBooks: (ids: string[]) => invoke<void>("remove_books", { ids }),
  createGroup: (name: string) => invoke<void>("create_group", { name }),
  renameGroup: (old: string, name: string) => invoke<void>("rename_group", { old, new: name }),
  deleteGroup: (name: string) => invoke<void>("delete_group", { name }),
  openBook: (id: string) => invoke<OpenedBook>("open_book", { id }),
  getChapter: (id: string, index: number) => invoke<Block[]>("get_chapter", { id, index }),
  getPageText: (id: string, page: number) => invoke<TextRun[]>("get_page_text", { id, page }),
  searchBook: (id: string, query: string) => invoke<Hit[]>("search_book", { id, query }),
  saveProgress: (id: string, position: Location, progress: number) =>
    invoke<void>("save_progress", { id, position, progress }),
  getAnnotations: (id: string) => invoke<Annotations>("get_annotations", { id }),
  saveAnnotations: (id: string, annotations: Annotations) => invoke<void>("save_annotations", { id, annotations }),
  /** Opens a save dialog; resolves to the written path, or null if cancelled. */
  exportNotes: (id: string) => invoke<string | null>("export_notes", { id }),
  /** Saves base64 data through a save dialog; resolves to the path, or null if cancelled. */
  saveExport: (fileName: string, extension: string, data: string) =>
    invoke<string | null>("save_export", { fileName, extension, data }),
  addReadingTime: (id: string, seconds: number, speed: number | null) =>
    invoke<Book>("add_reading_time", { id, seconds, speed }),
  aiGetConfig: () => invoke<AiStatus>("ai_get_config"),
  /** `apiKey`: undefined keeps the stored key, "" removes it. */
  aiSetConfig: (config: AiConfig, apiKey?: string) => invoke<AiStatus>("ai_set_config", { config, apiKey }),
  aiTest: () => invoke<string>("ai_test"),
  aiListModels: (config: AiConfig) => invoke<string[]>("ai_list_models", { config }),
  aiEstimateBook: (bookId: string) => invoke<BookEstimate>("ai_estimate_book", { bookId }),
  aiCancel: (taskId: string) => invoke<void>("ai_cancel", { taskId }),
  aiOrganize: (bookIds: string[]) => invoke<Suggestion[]>("ai_organize", { bookIds }),
  /** Language for backend messages (errors, generated titles, AI answers set to "auto"). */
  setLocale: (locale: string) => invoke<void>("set_locale", { locale }),
  getSettings: () => invoke<unknown>("get_settings"),
  setSettings: (settings: unknown) => invoke<void>("set_settings", { settings }),
};

/** Error text from a rejected command. */
export const errorText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

/** Starts an AI task; `onEvent` receives text, progress and the final result. Returns the task id. */
export function runAiTask(task: AiTask, onEvent: (e: AiEvent) => void): string {
  const taskId = crypto.randomUUID();
  const channel = new Channel<AiEvent>();
  channel.onmessage = onEvent;
  invoke("ai_run", { taskId, task, onEvent: channel }).catch((e) => onEvent({ type: "error", message: errorText(e) }));
  return taskId;
}
