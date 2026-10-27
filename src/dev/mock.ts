// Development only: an in-memory backend so the UI can run in a plain browser
// (`npm run dev`, then open http://localhost:1420). Never included in production builds.

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { AiConfig, AiEvent, AiTask, Annotations, Block, Book, Location, Source } from "../types";

const now = () => Math.floor(Date.now() / 1000);

function book(id: string, title: string, author: string | null, format: string, group: string | null, progress = 0): Book {
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
    group,
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
      book("0001", "三国演义", "罗贯中", "TXT", "小说", 0.42),
      book("0002", "Alice's Adventures in Wonderland", "Lewis Carroll", "EPUB", "小说", 0.1),
      book("0003", "Attention Is All You Need", "Vaswani et al.", "PDF", "技术"),
      book("0004", "Rust 程序设计语言", null, "MD", "技术", 1),
      book("0005", "漫画样例", null, "CBZ", null),
    ],
    groups: ["小说", "技术"],
    settings: {} as unknown,
    annotations: new Map<string, Annotations>(),
    ai: { provider: "", model: "", baseUrl: null, language: "auto", consented: false, embedding: { provider: "", model: "", baseUrl: null } } as AiConfig,
    aiKey: false,
    indexed: new Set<string>(),
  };
  const aiStatus = () => ({
    config: db.ai,
    hasKey: db.aiKey,
    hasEmbeddingKey: false,
    local: db.ai.provider === "ollama",
    configured: !!db.ai.provider && !!db.ai.model,
  });
  /** Streams `text` to a Channel in small pieces, then finishes. */
  const stream = (channel: { id: number }, text: string, progress = false, sources?: Source[]) => {
    let index = 0;
    const send = (message: AiEvent) =>
      (window as unknown as { __TAURI_INTERNALS__: { runCallback: (id: number, d: unknown) => void } }).__TAURI_INTERNALS__.runCallback(channel.id, {
        index: index++,
        message,
      });
    const pieces = text.match(/.{1,6}/gs) ?? [];
    let t = 0;
    if (sources) setTimeout(() => send({ type: "sources", sources }), (t += 80));
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
            const b = book(String(nextId++).padStart(4, "0"), name.replace(/\.[^.]+$/, ""), null, "EPUB", (a.group as string | undefined) ?? null);
            b.added = now();
            db.books.push(b);
            return name;
          });
          return { added, duplicates: [], failed: [], possibleDuplicates: [] };
        }
        case "update_book": {
          const b = find(a.id);
          const patch = a.patch as Partial<Book>;
          if (patch.title !== undefined) b.title = patch.title;
          if (patch.author !== undefined) b.author = patch.author || null;
          if (patch.group !== undefined) {
            b.group = patch.group || null;
            if (b.group && !db.groups.includes(b.group)) db.groups.push(b.group);
          }
          return b;
        }
        case "ai_semantic_status":
          return {
            available: !!db.ai.embedding?.provider,
            local: db.ai.embedding?.provider === "ollama",
            passages: db.indexed.has(a.bookId as string) ? 120 : 0,
            stale: false,
          };
        case "find_duplicates":
          return [];
        case "merge_duplicates":
          return { removed: 0, highlights: 0, bookmarks: 0 };
        case "remove_books":
          db.books = db.books.filter((b) => !(a.ids as string[]).includes(b.id));
          return null;
        case "create_group":
          if (db.groups.includes(a.name as string)) throw `分组「${a.name}」已存在`;
          db.groups.push(a.name as string);
          return null;
        case "rename_group":
          db.groups = db.groups.map((g) => (g === a.old ? (a.new as string) : g));
          for (const b of db.books) if (b.group === a.old) b.group = a.new as string;
          return null;
        case "delete_group":
          db.groups = db.groups.filter((g) => g !== a.name);
          for (const b of db.books) if (b.group === a.name) b.group = null;
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
              : [0, 1, 2].flatMap((c) => [
                  { title: `第${c + 1}回 示例章节`, depth: 0, location: { kind: "flow", chapter: c, block: 0 } },
                  { title: `${c + 1}.1 小节`, depth: 1, location: { kind: "flow", chapter: c, block: 5 } },
                  { title: `${c + 1}.1.1 细目`, depth: 2, location: { kind: "flow", chapter: c, block: 6 } },
                  { title: `${c + 1}.2 小节`, depth: 1, location: { kind: "flow", chapter: c, block: 12 } },
                ]),
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
        case "get_page_text":
          return [
            { text: "Attention Is All You Need", x: 0.25, y: 0.2, w: 0.5, h: 0.03 },
            { text: "The dominant sequence transduction models are based on", x: 0.15, y: 0.4, w: 0.7, h: 0.015 },
            { text: "complex recurrent or convolutional neural networks.", x: 0.15, y: 0.42, w: 0.65, h: 0.015 },
          ];
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
          if (task.kind === "buildIndex") {
            let index = 0;
            const send = (message: AiEvent) =>
              (window as unknown as { __TAURI_INTERNALS__: { runCallback: (id: number, d: unknown) => void } }).__TAURI_INTERNALS__.runCallback(
                (a.onEvent as { id: number }).id,
                { index: index++, message },
              );
            [0, 32, 64, 96, 120].forEach((done, i) => setTimeout(() => send({ type: "progress", done, total: 120, label: "" }), 200 * (i + 1)));
            setTimeout(() => {
              db.indexed.add(task.bookId);
              send({ type: "done", text: "120", cached: false });
            }, 1300);
            return null;
          }
          if (task.kind === "mindMap") {
            const outline =
              "- 第一回 宴桃园豪杰三结义\n  - 背景\n    - 天下大势 分久必合 合久必分\n    - 黄巾起义\n  - 人物\n    - 刘备：汉室宗亲\n    - 关羽：红脸长髯\n    - 张飞：豹头环眼\n  - 事件\n    - 桃园结义\n    - 招募乡勇\n    - 破黄巾初立战功\n  - 主旨\n    - 忠义\n    - 乱世英雄";
            stream(a.onEvent as { id: number }, outline, task.scope.kind === "book");
            return null;
          }
          if (task.kind === "ask") {
            const sources: Source[] = [0, 1, 2].map((c) => ({
              n: c + 1,
              title: `第${c + 1}回 示例章节`,
              location: { kind: "flow", chapter: c, block: 3 },
              excerpt: "话说天下大势，分久必合，合久必分。周末七国分争，并入于秦。",
            }));
            const answer =
              task.history.length > 1
                ? "继续说明：后来**楚汉相争**，又并入于汉 [2]。"
                : "开篇点明了全书主旨：**天下大势，分久必合，合久必分** [1]。\n\n- 周末七国分争，并入于秦 [1]\n- 楚汉分争，又并入于汉 [2][3]";
            stream(a.onEvent as { id: number }, answer, false, sources);
            return null;
          }
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
            group: ["古典名著", "计算机", "漫画"][i] ?? "其他",
            author: i === 2 ? "某作者" : null,
          }));
        case "get_settings":
          return db.settings;
        case "set_settings":
          db.settings = a.settings;
          return null;
        case "plugin:webview|set_webview_zoom":
          // The browser has no webview zoom; CSS zoom lays out the same way.
          document.documentElement.style.setProperty("zoom", String(a.value));
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
