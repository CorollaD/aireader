// Typed wrappers around the Tauri commands and `book://` URLs.

import { invoke } from "@tauri-apps/api/core";
import type { Block, Book, BookPatch, Hit, ImportReport, Location, OpenedBook, Shelf } from "./types";

// Custom protocols are exposed as http://<scheme>.localhost on Windows.
const BASE = navigator.userAgent.includes("Windows") ? "http://book.localhost/" : "book://localhost/";

export const coverUrl = (book: Book) => `${BASE}cover/${book.id}?v=${book.added}`;
export const imageUrl = (bookId: string, imageId: number) => `${BASE}img/${bookId}/${imageId}`;
export const pageUrl = (bookId: string, page: number, width: number) => `${BASE}page/${bookId}/${page}/${width}`;

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
  searchBook: (id: string, query: string) => invoke<Hit[]>("search_book", { id, query }),
  saveProgress: (id: string, position: Location, progress: number) =>
    invoke<void>("save_progress", { id, position, progress }),
  getSettings: () => invoke<unknown>("get_settings"),
  setSettings: (settings: unknown) => invoke<void>("set_settings", { settings }),
};

/** Error text from a rejected command. */
export const errorText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
