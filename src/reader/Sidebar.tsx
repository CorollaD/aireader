// Left sidebar: table of contents, bookmarks, and highlights/notes.

import { useEffect, useMemo, useRef, useState } from "react";
import type { Bookmark, Highlight, Location, OpenedBook } from "../types";
import { formatDate } from "../shelf/BookCard";
import { atOrBefore } from "./locations";

export type SidebarTab = "toc" | "bookmarks" | "notes";

interface Props {
  book: OpenedBook;
  tab: SidebarTab;
  onTab: (tab: SidebarTab) => void;
  location: Location | null;
  bookmarks: Bookmark[];
  highlights: Highlight[];
  onNavigate: (loc: Location, flash?: boolean) => void;
  onRenameBookmark: (id: string, title: string) => void;
  onRemoveBookmark: (id: string) => void;
  onEditNote: (id: string) => void;
  onRemoveHighlight: (id: string) => void;
  onExport: () => void;
}

export default function Sidebar(props: Props) {
  const { book, tab, location } = props;
  const list = useRef<HTMLDivElement>(null);
  const flow = book.kind === "flow";

  const currentToc = useMemo(() => {
    if (!location) return -1;
    let idx = -1;
    book.toc.forEach((e, i) => {
      if (atOrBefore(e.location, location)) idx = i;
    });
    return idx;
  }, [book, location]);

  useEffect(() => {
    if (tab === "toc") list.current?.querySelector(".toc-item.active")?.scrollIntoView({ block: "nearest" });
  }, [currentToc, tab]);

  const tabs: [SidebarTab, string, number | null][] = [
    ["toc", "目录", null],
    ["bookmarks", "书签", props.bookmarks.length],
    ...(flow ? ([["notes", "笔记", props.highlights.length]] as [SidebarTab, string, number][]) : []),
  ];

  return (
    <aside className="toc">
      <div className="side-tabs">
        {tabs.map(([key, label, count]) => (
          <button key={key} className={tab === key ? "on" : ""} onClick={() => props.onTab(key)}>
            {label}
            {count ? <span className="tab-count">{count}</span> : null}
          </button>
        ))}
      </div>
      <div className="side-list" ref={list}>
        {tab === "toc" &&
          (book.toc.length === 0 ? (
            <div className="side-empty muted small">这本书没有目录</div>
          ) : (
            book.toc.map((e, i) => (
              <button
                key={i}
                className={`toc-item${i === currentToc ? " active" : ""}`}
                style={{ paddingLeft: 12 + e.depth * 14 }}
                onClick={() => props.onNavigate(e.location)}
                title={e.title}
              >
                {e.title}
              </button>
            ))
          ))}
        {tab === "bookmarks" && <Bookmarks {...props} />}
        {tab === "notes" && flow && <Notes {...props} />}
      </div>
    </aside>
  );
}

function Bookmarks(props: Props) {
  const [editing, setEditing] = useState<string | null>(null);
  const sorted = [...props.bookmarks].sort((a, b) => (atOrBefore(a.location, b.location) ? -1 : 1));
  if (sorted.length === 0) return <div className="side-empty muted small">按 ⌘/Ctrl+D 或点击工具栏的书签按钮，为当前位置添加书签</div>;
  return (
    <>
      {sorted.map((b) => (
        <div key={b.id} className="side-entry" onClick={() => editing !== b.id && props.onNavigate(b.location)}>
          {editing === b.id ? (
            <input
              className="full"
              autoFocus
              defaultValue={b.title}
              onClick={(e) => e.stopPropagation()}
              onBlur={(e) => {
                props.onRenameBookmark(b.id, e.target.value.trim() || b.title);
                setEditing(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                if (e.key === "Escape") setEditing(null);
              }}
            />
          ) : (
            <div className="entry-title">{b.title}</div>
          )}
          {b.excerpt && <div className="entry-text">{b.excerpt}</div>}
          <div className="entry-meta">
            <span>{formatDate(b.created)}</span>
            <span className="entry-actions">
              <button
                className="icon"
                title="重命名"
                onClick={(e) => {
                  e.stopPropagation();
                  setEditing(b.id);
                }}
              >
                ✎
              </button>
              <button
                className="icon"
                title="删除书签"
                onClick={(e) => {
                  e.stopPropagation();
                  props.onRemoveBookmark(b.id);
                }}
              >
                ×
              </button>
            </span>
          </div>
        </div>
      ))}
    </>
  );
}

function Notes(props: Props) {
  const { book } = props;
  const sorted = [...props.highlights].sort(
    (a, b) => a.chapter - b.chapter || a.startBlock - b.startBlock || a.start - b.start,
  );
  return (
    <>
      <div className="notes-head">
        <span className="muted small">{sorted.length} 条高亮</span>
        <button disabled={sorted.length === 0} onClick={props.onExport} title="导出为 Markdown">
          导出
        </button>
      </div>
      {sorted.length === 0 && <div className="side-empty muted small">选中正文文字，即可添加高亮或笔记</div>}
      {sorted.map((h, i) => (
        <div key={h.id}>
          {(i === 0 || sorted[i - 1].chapter !== h.chapter) && (
            <div className="notes-chapter">{book.chapters[h.chapter]?.title || `第 ${h.chapter + 1} 章`}</div>
          )}
          <div
            className={`side-entry note-entry hl-bar-${h.color}`}
            onClick={() => props.onNavigate({ kind: "flow", chapter: h.chapter, block: h.startBlock }, true)}
          >
            <div className="entry-text quote">{h.text}</div>
            {h.note && <div className="entry-note">{h.note}</div>}
            <div className="entry-meta">
              <span>{formatDate(h.updated)}</span>
              <span className="entry-actions">
                <button
                  className="icon"
                  title="编辑笔记"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onEditNote(h.id);
                  }}
                >
                  ✎
                </button>
                <button
                  className="icon"
                  title="删除高亮"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onRemoveHighlight(h.id);
                  }}
                >
                  ×
                </button>
              </span>
            </div>
          </div>
        </div>
      ))}
    </>
  );
}
