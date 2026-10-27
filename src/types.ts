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
  /** A book is in at most one group; null = ungrouped. */
  group: string | null;
  /** Active reading time in seconds. */
  readSeconds: number;
  /** Characters per minute (reflowable) or pages per minute (paged). */
  readingSpeed: number | null;
  /** Page rotation in degrees, for paged books. */
  rotation: number;
  missing: boolean;
}

export interface Shelf {
  books: Book[];
  groups: string[];
}

export interface BookPatch {
  title?: string;
  author?: string;
  /** null removes the book from its group. */
  group?: string | null;
  rotation?: number;
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
  /** Ids of added books that may be another file of a book already on the shelf. */
  possibleDuplicates: string[];
}

/** One file of a book that is on the shelf several times. */
export interface DuplicateCopy {
  id: string;
  score: number;
  chars: number;
  pages: number | null;
  dpi: number | null;
  hasText: boolean;
  tocEntries: number;
  /** Share of the group's most complete copy (0..1). */
  completeness: number;
}

export interface DuplicateGroup {
  /** Best first. */
  copies: DuplicateCopy[];
  /** False when a copy could only be matched by title and page count. */
  confident: boolean;
}

export interface MergeReport {
  removed: number;
  highlights: number;
  bookmarks: number;
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
  chapters: { title: string; blocks: number; chars: number }[];
  /** Height/width ratio of each page (paged books). */
  pages: number[];
  position: Location | null;
  readSeconds: number;
  readingSpeed: number | null;
  rotation: number;
}

export interface Bookmark {
  id: string;
  location: Location;
  title: string;
  excerpt: string;
  created: number;
}

/**
 * A highlighted range in a reflowable book. Offsets index the block's text (concatenated
 * spans) in UTF-16 code units.
 */
export interface Highlight {
  id: string;
  chapter: number;
  startBlock: number;
  start: number;
  endBlock: number;
  end: number;
  text: string;
  color: HighlightColor;
  note?: string | null;
  created: number;
  updated: number;
}

export type HighlightColor = "yellow" | "green" | "blue" | "pink" | "purple";

export interface Annotations {
  bookmarks: Bookmark[];
  highlights: Highlight[];
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

// --- AI (src-tauri/src/ai) ---

export interface AiConfig {
  /** rig vendor id (anthropic, openai, deepseek, ollama, …) or "custom". */
  provider: string;
  model: string;
  baseUrl: string | null;
  language: string;
  consented: boolean;
  /** The model for semantic search; provider "" = none. */
  embedding: EmbeddingConfig;
}

export interface EmbeddingConfig {
  /** "", "ollama", "openai" or "custom" (any OpenAI-compatible URL). */
  provider: string;
  model: string;
  baseUrl: string | null;
}

export interface AiStatus {
  config: AiConfig;
  hasKey: boolean;
  local: boolean;
  configured: boolean;
  hasEmbeddingKey: boolean;
}

/** A book's semantic index. */
export interface SemanticStatus {
  /** An embedding model is set up, so an index can be built. */
  available: boolean;
  /** The embedding model runs locally (no consent needed). */
  local: boolean;
  /** Passages indexed with the current model; 0 = not built. */
  passages: number;
  /** An index exists but was built with another model. */
  stale: boolean;
}

export type AiEvent =
  | { type: "delta"; text: string }
  | { type: "progress"; done: number; total: number; label: string }
  | { type: "done"; text: string; cached: boolean }
  | { type: "error"; message: string }
  | { type: "sources"; sources: Source[] };

/** A passage an answer may cite as [n]. */
export interface Source {
  n: number;
  title: string;
  location: Location;
  excerpt: string;
}

export type ChatAction = "explain" | "translate" | "summarize";

export interface ChatTurn {
  role: "user" | "assistant";
  text: string;
}

export type AiTask =
  | { kind: "summarizeChapter"; bookId: string; chapter: number }
  | { kind: "summarizePages"; bookId: string; from: number; to: number }
  | { kind: "summarizeBook"; bookId: string }
  | {
      kind: "chat";
      bookTitle: string;
      selection: string;
      context: string;
      action: ChatAction | null;
      history: ChatTurn[];
    }
  | { kind: "ask"; bookId: string; location: Location | null; history: ChatTurn[] }
  | { kind: "buildIndex"; bookId: string }
  | { kind: "mindMap"; bookId: string; scope: MindMapScope; refresh?: boolean };

export type MindMapScope =
  | { kind: "chapter"; chapter: number }
  | { kind: "pages"; from: number; to: number }
  | { kind: "book" }
  | { kind: "text"; title: string; text: string };

export interface BookEstimate {
  units: number;
  cachedUnits: number;
  bookCached: boolean;
  inputTokens: number;
  outputTokens: number;
  /** USD, when the model's prices are known. */
  cost: number | null;
}

export interface Suggestion {
  bookId: string;
  /** Only set when it differs from the book's current group. */
  group: string | null;
  author: string | null;
}

/** A run of text on a PDF page; coordinates are fractions of the unrotated page, from top-left. */
export interface TextRun {
  text: string;
  x: number;
  y: number;
  w: number;
  h: number;
}
