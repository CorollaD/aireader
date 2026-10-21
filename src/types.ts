// Mirrors the Rust types serialized by src-tauri (model.rs, library.rs, commands.rs).

export type Location =
  | { kind: "flow"; chapter: number; block: number }
  | { kind: "page"; page: number };

export interface Book {
  id: string;
  title: string;
  author: string | null;
  format: string;
  fileName: string;
  size: number;
  sha256: string;
  hasCover: boolean;
  added: number;
  lastRead: number | null;
  progress: number;
  position: Location | null;
  groups: string[];
  missing: boolean;
}

export interface Shelf {
  books: Book[];
  groups: string[];
}

export interface BookPatch {
  title?: string;
  author?: string;
  groups?: string[];
}

export interface ImportProgress {
  done: number;
  total: number;
  name: string;
}

export interface ImportReport {
  added: string[];
  duplicates: string[];
  failed: [string, string][];
}

export interface TocEntry {
  title: string;
  depth: number;
  location: Location;
}

export interface OpenedBook {
  id: string;
  title: string;
  author: string | null;
  format: string;
  kind: "flow" | "paged";
  toc: TocEntry[];
  chapters: { title: string; blocks: number }[];
  /** Height/width ratio of each page (paged books). */
  pages: number[];
  position: Location | null;
}

export interface Span {
  text: string;
  bold?: boolean;
  italic?: boolean;
  code?: boolean;
  link?: boolean;
}

export type Block =
  | { kind: "heading"; value: { level: number; spans: Span[] } }
  | { kind: "paragraph"; value: Span[] }
  | { kind: "quote"; value: Span[] }
  | { kind: "listItem"; value: { marker: string; depth: number; spans: Span[] } }
  | { kind: "code"; value: string }
  | { kind: "image"; value: number }
  | { kind: "rule" };

export interface Hit {
  chapter: number;
  block: number;
  snippet: string;
}
