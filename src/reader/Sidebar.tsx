// Left sidebar: table of contents, bookmarks, and highlights/notes.

import { useEffect, useMemo, useRef, useState } from "react";
import type { TocEntry } from "../types";
import type { Bookmark, Highlight, Location, OpenedBook } from "../types";
import { formatDate, t } from "../i18n";
import { Icon } from "../icons";
import { MOD } from "./shortcuts";
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
  width: number;
  onCollapse: () => void;
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
    ["toc", t("side.toc"), null],
    ["bookmarks", t("side.bookmarks"), props.bookmarks.length],
    ...(flow ? ([["notes", t("side.notes"), props.highlights.length]] as [SidebarTab, string, number][]) : []),
  ];

  return (
    <aside className="toc" style={{ width: props.width }}>
      <div className="side-tabs">
        {tabs.map(([key, label, count]) => (
          <button key={key} className={tab === key ? "on" : ""} onClick={() => props.onTab(key)}>
            {label}
            {count ? <span className="tab-count">{count}</span> : null}
          </button>
        ))}
        <button className="icon tab-icon" title={`${t("reader.collapseSidebar")} (T)`} aria-label={t("reader.collapseSidebar")} onClick={props.onCollapse}>
          <Icon name="panelLeft" />
        </button>
      </div>
      <div className="side-list" ref={list}>
        {tab === "toc" &&
          (book.toc.length === 0 ? (
            <div className="side-empty muted small">{t("side.noToc")}</div>
          ) : (
            <TocTree bookId={book.id} toc={book.toc} current={currentToc} onNavigate={props.onNavigate} />
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
  if (sorted.length === 0) return <div className="side-empty muted small">{t("side.noBookmarks", { key: `${MOD}+D` })}</div>;
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
                title={t("common.rename")}
                onClick={(e) => {
                  e.stopPropagation();
                  setEditing(b.id);
                }}
              >
                <Icon name="edit" size={14} />
              </button>
              <button
                className="icon"
                title={t("side.deleteBookmark")}
                onClick={(e) => {
                  e.stopPropagation();
                  props.onRemoveBookmark(b.id);
                }}
              >
                <Icon name="close" size={14} />
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
  const sorted = [...props.highlights].sort((a, b) => a.chapter - b.chapter || a.startBlock - b.startBlock || a.start - b.start);
  return (
    <>
      <div className="notes-head">
        <span className="muted small">{t("side.highlightCount", { n: sorted.length })}</span>
        <button disabled={sorted.length === 0} onClick={props.onExport} title={t("side.exportTitle")}>
          {t("side.export")}
        </button>
      </div>
      {sorted.length === 0 && <div className="side-empty muted small">{t("side.noNotes")}</div>}
      {sorted.map((h, i) => (
        <div key={h.id}>
          {(i === 0 || sorted[i - 1].chapter !== h.chapter) && (
            <div className="notes-chapter">{book.chapters[h.chapter]?.title || t("loc.chapter", { n: h.chapter + 1 })}</div>
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
                  title={t("side.editNote")}
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onEditNote(h.id);
                  }}
                >
                  <Icon name="edit" size={14} />
                </button>
                <button
                  className="icon"
                  title={t("side.deleteHighlight")}
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onRemoveHighlight(h.id);
                  }}
                >
                  <Icon name="close" size={14} />
                </button>
              </span>
            </div>
          </div>
        </div>
      ))}
    </>
  );
}

/** Entries with more than this many items start with only the top level expanded. */
const COLLAPSE_THRESHOLD = 30;

/** For each entry, the indices of its ancestors (nearest last). */
function ancestorsOf(toc: TocEntry[]): number[][] {
  const stack: number[] = [];
  return toc.map((e, i) => {
    while (stack.length && toc[stack[stack.length - 1]].depth >= e.depth) stack.pop();
    const ancestors = [...stack];
    stack.push(i);
    return ancestors;
  });
}

function loadCollapsed(bookId: string): Set<number> | null {
  try {
    const raw = localStorage.getItem(`toc-collapsed:${bookId}`);
    return raw ? new Set(JSON.parse(raw) as number[]) : null;
  } catch {
    return null;
  }
}

function saveCollapsed(bookId: string, collapsed: Set<number>) {
  try {
    localStorage.setItem(`toc-collapsed:${bookId}`, JSON.stringify([...collapsed]));
  } catch {
    // Storage unavailable: the state just isn't remembered.
  }
}

function TocTree(props: { bookId: string; toc: TocEntry[]; current: number; onNavigate: (loc: TocEntry["location"]) => void }) {
  const { toc, current } = props;
  const ancestors = useMemo(() => ancestorsOf(toc), [toc]);
  const hasChildren = useMemo(() => toc.map((e, i) => (toc[i + 1]?.depth ?? -1) > e.depth), [toc]);
  const parents = useMemo(() => toc.map((_, i) => i).filter((i) => hasChildren[i]), [toc, hasChildren]);
  const [collapsed, setCollapsed] = useState<Set<number>>(() => {
    const saved = loadCollapsed(props.bookId);
    if (saved) return saved;
    // Large tables of contents start with only the top level open.
    return new Set(toc.length > COLLAPSE_THRESHOLD ? parents : []);
  });

  const change = (next: Set<number>) => {
    setCollapsed(next);
    saveCollapsed(props.bookId, next);
  };

  // Reading into a collapsed branch opens it, so the current entry is always visible.
  useEffect(() => {
    if (current < 0) return;
    const hidden = ancestors[current].filter((a) => collapsed.has(a));
    if (hidden.length) {
      const next = new Set(collapsed);
      for (const a of hidden) next.delete(a);
      change(next);
    }
    // Only when the current entry changes.
  }, [current]);

  const toggle = (i: number) => {
    const next = new Set(collapsed);
    if (next.has(i)) next.delete(i);
    else next.add(i);
    change(next);
  };

  return (
    <>
      {parents.length > 0 && (
        <div className="toc-tools">
          <button className="link-button" onClick={() => change(new Set())}>
            {t("side.expandAll")}
          </button>
          <button className="link-button" onClick={() => change(new Set(parents))}>
            {t("side.collapseAll")}
          </button>
        </div>
      )}
      {toc.map((e, i) =>
        ancestors[i].some((a) => collapsed.has(a)) ? null : (
          <div key={i} className={`toc-row${i === current ? " active" : ""}`} style={{ paddingLeft: 4 + e.depth * 14 }}>
            {hasChildren[i] ? (
              <button
                className="toc-toggle"
                aria-label={collapsed.has(i) ? t("side.expand") : t("side.collapse")}
                aria-expanded={!collapsed.has(i)}
                onClick={() => toggle(i)}
              >
                <Icon name={collapsed.has(i) ? "chevronRight" : "chevronDown"} size={13} />
              </button>
            ) : (
              <span className="toc-toggle-space" />
            )}
            <button className={`toc-item${i === current ? " active" : ""}`} onClick={() => props.onNavigate(e.location)} title={e.title}>
              {e.title}
            </button>
          </div>
        ),
      )}
    </>
  );
}
