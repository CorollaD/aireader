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
  ChatRecord,
  DuplicateGroup,
  Hit,
  ImportReport,
  Location,
  MergeReport,
  OpenedBook,
  SemanticStatus,
  Shelf,
  Suggestion,
  TextRun,
} from "./types";

// Custom protocols are exposed as http://<scheme>.localhost on Windows.
const BASE = navigator.userAgent.includes("Windows") ? "http://book.localhost/" : "book://localhost/";

export const coverUrl = (book: Book) => `${BASE}cover/${book.id}?v=${book.added}`;
export const fontUrl = (name: string) => `${BASE}font/${encodeURIComponent(name)}`;
export const imageUrl = (bookId: string, imageId: number) => `${BASE}img/${bookId}/${imageId}`;
export const pageUrl = (bookId: string, page: number, width: number, rotation = 0) => `${BASE}page/${bookId}/${page}/${width}${rotation ? `/${rotation}` : ""}`;

export const api = {
  listBooks: () => invoke<Shelf>("list_books"),
  importBooks: (paths: string[], group?: string) => invoke<ImportReport>("import_books", { paths, group }),
  /** The book to open from the command line, a second launch or the OS (imported if new). */
  takeStartupBook: () => invoke<string | null>("take_startup_book"),
  /** Writes a frontend error to the app log. */
  logError: (message: string) => invoke<void>("log_error", { message }),
  /** Quits the app (after pending work was saved). */
  exitApp: () => invoke<void>("exit_app"),
  updateBook: (id: string, patch: BookPatch) => invoke<Book>("update_book", { id, patch }),
  removeBooks: (ids: string[]) => invoke<void>("remove_books", { ids }),
  /** Same books stored in several files; `only` limits it to groups with those books. */
  findDuplicates: (only?: string[]) => invoke<DuplicateGroup[]>("find_duplicates", { only: only ?? null }),
  /** Keeps one copy per group; the others' reading data moves onto it before they go. */
  mergeDuplicates: (merges: { keep: string; remove: string[] }[]) => invoke<MergeReport>("merge_duplicates", { merges }),
  createGroup: (name: string) => invoke<void>("create_group", { name }),
  renameGroup: (old: string, name: string) => invoke<void>("rename_group", { old, new: name }),
  deleteGroup: (name: string) => invoke<void>("delete_group", { name }),
  openBook: (id: string) => invoke<OpenedBook>("open_book", { id }),
  getChapter: (id: string, index: number) => invoke<Block[]>("get_chapter", { id, index }),
  getPageText: (id: string, page: number) => invoke<TextRun[]>("get_page_text", { id, page }),
  searchBook: (id: string, query: string) => invoke<Hit[]>("search_book", { id, query }),
  saveProgress: (id: string, position: Location, progress: number) => invoke<void>("save_progress", { id, position, progress }),
  getAnnotations: (id: string) => invoke<Annotations>("get_annotations", { id }),
  saveAnnotations: (id: string, annotations: Annotations) => invoke<void>("save_annotations", { id, annotations }),
  /** Saved AI conversations about a book, most recent first. */
  listChats: (bookId: string) => invoke<ChatRecord[]>("list_chats", { bookId }),
  saveChat: (bookId: string, chat: ChatRecord) => invoke<void>("save_chat", { bookId, chat }),
  deleteChat: (id: string) => invoke<void>("delete_chat", { id }),
  /** Opens a save dialog; resolves to the written path, or null if cancelled. */
  exportNotes: (id: string) => invoke<string | null>("export_notes", { id }),
  /** Reading fonts the user added (file names). */
  listFonts: () => invoke<string[]>("list_fonts"),
  /** Opens a file dialog and adds the font; resolves to its name, or null if cancelled. */
  importFont: () => invoke<string | null>("import_font"),
  removeFont: (name: string) => invoke<void>("remove_font", { name }),
  /** Backs up the library to a zip the user picks; resolves to the path, or null if cancelled. */
  backupLibrary: (books: boolean) => invoke<string | null>("backup_library", { books }),
  /** Saves base64 data through a save dialog; resolves to the path, or null if cancelled. */
  saveExport: (fileName: string, extension: string, data: string) => invoke<string | null>("save_export", { fileName, extension, data }),
  addReadingTime: (id: string, seconds: number, speed: number | null) => invoke<Book>("add_reading_time", { id, seconds, speed }),
  aiGetConfig: () => invoke<AiStatus>("ai_get_config"),
  /** `apiKey`: undefined keeps the stored key, "" removes it. */
  /** Keys: undefined keeps the stored one, "" removes it. */
  aiSetConfig: (config: AiConfig, apiKey?: string, embeddingKey?: string) => invoke<AiStatus>("ai_set_config", { config, apiKey, embeddingKey }),
  aiSemanticStatus: (bookId: string) => invoke<SemanticStatus>("ai_semantic_status", { bookId }),
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
  // Text arrives in many small pieces; they are passed on at most once per frame, so a long
  // answer doesn't re-render (and re-parse its Markdown) for every few characters.
  let pending = "";
  let frame = 0;
  const flush = () => {
    cancelAnimationFrame(frame);
    frame = 0;
    if (pending) {
      const text = pending;
      pending = "";
      onEvent({ type: "delta", text });
    }
  };
  channel.onmessage = (e) => {
    if (e.type === "delta") {
      pending += e.text;
      frame ||= requestAnimationFrame(flush);
      return;
    }
    flush();
    onEvent(e);
  };
  invoke("ai_run", { taskId, task, onEvent: channel }).catch((e) => onEvent({ type: "error", message: errorText(e) }));
  return taskId;
}
