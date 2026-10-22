// Development only: an in-memory backend so the UI can run in a plain browser
// (`npm run dev`, then open http://localhost:1420). Never included in production builds.

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { AiConfig, AiEvent, AiTask, Annotations, Block, Book, Location } from "../types";

const now = () => Math.floor(Date.now() / 1000);

function book(id: string, title: string, author: string | null, format: string, groups: string[], progress = 0): Book {
  return {
    id,
    title,
    author,
    format,
    fileName: `${title}.${format.toLowerCase()}`,
    size: 1_234_567,
    sha256: id.padEnd(64, "0"),
    hasCover: false,
    added: now() - Number.parseInt(id, 16) * 3600,
    lastRead: progress > 0 ? now() - Number.parseInt(id, 16) * 600 : null,
    progress,
    position: null,
    groups,
    readSeconds: progress > 0 ? 3600 * progress : 0,
    readingSpeed: null,
    rotation: 0,
    missing: false,
  };
}

const PARAGRAPH =
  "话说天下大势，分久必合，合久必分。周末七国分争，并入于秦。及秦灭之后，楚、汉分争，又并入于汉。汉朝自高祖斩白蛇而起义，一统天下，后来光武中兴，传至献帝，遂分为三国。";

function chapter(i: number): Block[] {
  const blocks: Block[] = [{ kind: "heading", value: { level: 2, spans: [{ text: `第${i + 1}回 示例章节` }] } }];
  for (let p = 0; p < 30; p++) {
    blocks.push({ kind: "paragraph", value: [{ text: `　　${PARAGRAPH}` }, ...(p === 2 ? [{ text: "加粗文字", bold: true }] : [])] });
    if (p === 5) blocks.push({ kind: "quote", value: [{ text: "滚滚长江东逝水，浪花淘尽英雄。" }] });
    if (p === 8) blocks.push({ kind: "listItem", value: { marker: "•", depth: 0, spans: [{ text: "列表项" }] } });
  }
  return blocks;
}

export function installMock() {
  mockWindows("main");
  const db = {
    books: [
      book("0001", "三国演义", "罗贯中", "TXT", ["小说"], 0.42),
      book("0002", "Alice's Adventures in Wonderland", "Lewis Carroll", "EPUB", ["小说"], 0.1),
      book("0003", "Attention Is All You Need", "Vaswani et al.", "PDF", ["技术"]),
      book("0004", "Rust 程序设计语言", null, "MD", ["技术"], 1),
      book("0005", "漫画样例", null, "CBZ", []),
    ],
    groups: ["小说", "技术"],
    settings: {} as unknown,
    annotations: new Map<string, Annotations>(),
    ai: { provider: "", model: "", baseUrl: null, language: "中文", consented: false } as AiConfig,
    aiKey: false,
  };
  const aiStatus = () => ({
    config: db.ai,
    hasKey: db.aiKey,
    local: db.ai.provider === "ollama",
    configured: !!db.ai.provider && !!db.ai.model,
  });
  /** Streams `text` to a Channel in small pieces, then finishes. */
  const stream = (channel: { id: number }, text: string, progress = false) => {
    let index = 0;
    const send = (message: AiEvent) =>
      (window as unknown as { __TAURI_INTERNALS__: { runCallback: (id: number, d: unknown) => void } }).__TAURI_INTERNALS__.runCallback(
        channel.id,
        { index: index++, message },
      );
    const pieces = text.match(/.{1,6}/gs) ?? [];
    let t = 0;
    if (progress) [0, 1, 2].forEach((i) => setTimeout(() => send({ type: "progress", done: i, total: 3, label: `正在总结「第${i + 1}回」` }), (t += 150)));
    pieces.forEach((p) => setTimeout(() => send({ type: "delta", text: p }), (t += 40)));
    setTimeout(() => send({ type: "done", text, cached: false }), (t += 60));
  };
  let nextId = 6;

  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      const find = (id: unknown) => db.books.find((b) => b.id === id)!;
      switch (cmd) {
        case "list_books":
          return { books: db.books, groups: db.groups };
        case "take_startup_book":
          return null;
        case "import_books": {
          const added = (a.paths as string[]).map((p) => {
            const name = p.split("/").pop()!;
            const b = book(String(nextId++).padStart(4, "0"), name.replace(/\.[^.]+$/, ""), null, "EPUB", a.group ? [a.group as string] : []);
            b.added = now();
            db.books.push(b);
            return name;
          });
          return { added, duplicates: [], failed: [] };
        }
        case "update_book": {
          const b = find(a.id);
          const patch = a.patch as Partial<Book>;
          if (patch.title !== undefined) b.title = patch.title;
          if (patch.author !== undefined) b.author = patch.author || null;
          if (patch.groups) {
            b.groups = patch.groups;
            for (const g of patch.groups) if (!db.groups.includes(g)) db.groups.push(g);
          }
          return b;
        }
        case "remove_books":
          db.books = db.books.filter((b) => !(a.ids as string[]).includes(b.id));
          return null;
        case "create_group":
          if (db.groups.includes(a.name as string)) throw `分组「${a.name}」已存在`;
          db.groups.push(a.name as string);
          return null;
        case "rename_group":
          db.groups = db.groups.map((g) => (g === a.old ? (a.new as string) : g));
          for (const b of db.books) b.groups = b.groups.map((g) => (g === a.old ? (a.new as string) : g));
          return null;
        case "delete_group":
          db.groups = db.groups.filter((g) => g !== a.name);
          for (const b of db.books) b.groups = b.groups.filter((g) => g !== a.name);
          return null;
        case "open_book": {
          const b = find(a.id);
          const paged = b.format === "PDF" || b.format === "CBZ";
          return {
            id: b.id,
            title: b.title,
            author: b.author,
            format: b.format,
            kind: paged ? "paged" : "flow",
            toc: paged
              ? [0, 2, 4].map((p) => ({ title: `第 ${p + 1} 页`, depth: 0, location: { kind: "page", page: p } }))
              : [0, 1, 2].map((c) => ({ title: `第${c + 1}回 示例章节`, depth: 0, location: { kind: "flow", chapter: c, block: 0 } })),
            chapters: paged
              ? []
              : [0, 1, 2].map((c) => ({
                  title: `第${c + 1}回 示例章节`,
                  blocks: chapter(c).length,
                  chars: chapter(c).reduce((n, bl) => n + JSON.stringify(bl).length, 0),
                })),
            pages: paged ? [1.414, 1.414, 1.414, 1.414, 1.414] : [],
            position: b.position,
            readSeconds: b.readSeconds,
            readingSpeed: b.readingSpeed,
            rotation: b.rotation,
          };
        }
        case "get_chapter":
          return chapter(a.index as number);
        case "search_book":
          return [0, 1, 2].map((c) => ({ chapter: c, block: 3, snippet: `…${String(a.query)}…示例片段` }));
        case "save_progress": {
          const b = find(a.id);
          b.position = a.position as Location;
          b.progress = a.progress as number;
          b.lastRead = now();
          return null;
        }
        case "get_annotations":
          return db.annotations.get(a.id as string) ?? { bookmarks: [], highlights: [] };
        case "save_annotations":
          db.annotations.set(a.id as string, a.annotations as Annotations);
          return null;
        case "export_notes":
          return "/Users/demo/Documents/笔记.md";
        case "add_reading_time": {
          const b = find(a.id);
          b.readSeconds += a.seconds as number;
          if (a.speed) b.readingSpeed = a.speed as number;
          return b;
        }
        case "ai_get_config":
          return aiStatus();
        case "ai_set_config":
          db.ai = a.config as AiConfig;
          if (typeof a.apiKey === "string") db.aiKey = a.apiKey !== "";
          return aiStatus();
        case "ai_test":
          return "连接成功";
        case "ai_list_models":
          return ["qwen3:8b", "llama3.2"];
        case "ai_estimate_book":
          return { units: 3, cachedUnits: 1, bookCached: false, inputTokens: 52000, outputTokens: 2700, cost: 0.26 };
        case "ai_cancel":
          return null;
        case "ai_run": {
          const task = a.task as AiTask;
          const text =
            task.kind === "chat"
              ? task.action === "translate"
                ? "**译文**：The world under heaven, after a long period of division, tends to unite."
                : `这句话出自《三国演义》开篇，意思是：\n\n- 天下**分裂久了**必然统一\n- 统一久了又会分裂\n\n${task.history.length ? "（追问回答）" : ""}`
              : `## 概述\n本章讲述了天下大势分合的规律。\n\n- 要点一：周末七国分争\n- 要点二：楚汉相争\n- 要点三：三国鼎立`;
          stream(a.onEvent as { id: number }, text, task.kind === "summarizeBook");
          return null;
        }
        case "ai_organize":
          return (a.bookIds as string[]).slice(0, 3).map((id, i) => ({
            bookId: id,
            groups: [["古典", "小说"], ["计算机"], ["漫画"]][i] ?? ["其他"],
            author: i === 2 ? "某作者" : null,
          }));
        case "get_settings":
          return db.settings;
        case "set_settings":
          db.settings = a.settings;
          return null;
        case "plugin:dialog|open":
          return ["/Users/demo/Books/新导入的书.epub"];
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
