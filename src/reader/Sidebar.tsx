// Left sidebar: table of contents, bookmarks, highlights/notes and notes on the whole book.

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { TocEntry } from "../types";
import type { BookNote, Bookmark, Card, Highlight, Location, OpenedBook } from "../types";
import Markdown from "../ai/Markdown";
import { formatDate, t } from "../i18n";
import { Icon, type IconName } from "../icons";
import { MOD } from "./shortcuts";
import { atOrBefore } from "./locations";

export type SidebarTab = "toc" | "bookmarks" | "notes" | "cards";

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
  notes: BookNote[];
  onUpdateNote: (id: string, patch: Pick<BookNote, "title" | "text">) => void;
  onRemoveNote: (id: string) => void;
  onExport: () => void;
  cards: Card[];
  onCardFromHighlight: (highlight: Highlight) => void;
  onEditCard: (card: Card) => void;
  onDeleteCard: (card: Card) => void;
  width: number;
  onCollapse: () => void;
}

export default function Sidebar(props: Props) {
  const { book, tab, location } = props;
  const list = useRef<HTMLDivElement>(null);

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

  const tabs: [SidebarTab, string, IconName, number | null][] = [
    ["toc", t("side.toc"), "list", null],
    ["bookmarks", t("side.bookmarks"), "bookmark", props.bookmarks.length],
    ["notes", t("side.notes"), "note", props.highlights.length + props.notes.length],
    ["cards", t("side.cards"), "layers", props.cards.length],
  ];

  return (
    <aside className="toc" style={{ width: props.width }}>
      <div className="side-tabs">
        <div className="ai-switch" role="tablist">
          {tabs.map(([key, label, icon, count]) => (
            <button key={key} role="tab" aria-selected={tab === key} className={tab === key ? "on" : ""} title={label} onClick={() => props.onTab(key)}>
              <Icon name={icon} size={14} />
              <span className="tab-label">{label}</span>
              {count ? <span className="tab-count">{count}</span> : null}
            </button>
          ))}
        </div>
        <span className="spacer" />
        <button className="icon-button" title={`${t("reader.collapseSidebar")} (T)`} aria-label={t("reader.collapseSidebar")} onClick={props.onCollapse}>
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
        {tab === "notes" && <Notes {...props} />}
        {tab === "cards" && <Cards {...props} />}
      </div>
    </aside>
  );
}

function Cards(props: Props) {
  if (props.cards.length === 0) return <div className="side-empty muted small">{t("side.noCards")}</div>;
  return (
    <>
      {props.cards.map((c) => (
        <div key={c.id} className="side-entry" onClick={() => c.location && props.onNavigate(c.location, true)}>
          <div className="entry-title">{c.front}</div>
          {c.back && <div className="entry-text">{c.back}</div>}
          <div className="entry-meta">
            <span>{formatDate(c.created)}</span>
            <span className="entry-actions">
              <button
                className="icon"
                title={t("cards.edit")}
                onClick={(e) => {
                  e.stopPropagation();
                  props.onEditCard(c);
                }}
              >
                <Icon name="edit" size={14} />
              </button>
              <button
                className="icon"
                title={t("common.delete")}
                onClick={(e) => {
                  e.stopPropagation();
                  props.onDeleteCard(c);
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
  const flow = book.kind === "flow";
  const sorted = [...props.highlights].sort((a, b) => a.chapter - b.chapter || a.startBlock - b.startBlock || a.start - b.start);
  const counts = [
    ...(flow && (sorted.length > 0 || props.notes.length === 0) ? [t("side.highlightCount", { n: sorted.length })] : []),
    ...(props.notes.length > 0 ? [t("side.bookNoteCount", { n: props.notes.length })] : []),
  ];
  return (
    <>
      <div className="notes-head">
        <span className="muted small">{counts.join(" · ")}</span>
        <button disabled={sorted.length === 0 && props.notes.length === 0} onClick={props.onExport} title={t("side.exportTitle")}>
          {t("side.export")}
        </button>
      </div>
      {props.notes.length > 0 && <div className="notes-chapter">{t("side.bookNotes")}</div>}
      {props.notes.map((n) => (
        <BookNoteEntry key={n.id} note={n} onUpdate={props.onUpdateNote} onRemove={props.onRemoveNote} />
      ))}
      {sorted.length === 0 && props.notes.length === 0 && <div className="side-empty muted small">{flow ? t("side.noNotes") : t("side.noBookNotes")}</div>}
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
                  title={t("daily.makeCard")}
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onCardFromHighlight(h);
                  }}
                >
                  <Icon name="layers" size={14} />
                </button>
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

/** A note on the whole book: shortened until clicked, edited in place. */
function BookNoteEntry(props: { note: BookNote; onUpdate: (id: string, patch: Pick<BookNote, "title" | "text">) => void; onRemove: (id: string) => void }) {
  const { note } = props;
  const [open, setOpen] = useState(false);
  const [clipped, setClipped] = useState(false);
  const [draft, setDraft] = useState<{ title: string; text: string } | null>(null);
  const body = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = body.current;
    if (el && !open) setClipped(el.scrollHeight > el.clientHeight + 1);
  }, [note.text, open, draft]);
  if (draft) {
    const save = () => {
      if (draft.text.trim()) props.onUpdate(note.id, { title: draft.title.trim(), text: draft.text.trim() });
      setDraft(null);
    };
    return (
      <div className="side-entry book-note editing">
        <input className="full" value={draft.title} placeholder={t("side.noteTitle")} onChange={(e) => setDraft({ ...draft, title: e.target.value })} />
        <textarea
          className="full"
          autoFocus
          rows={8}
          value={draft.text}
          onChange={(e) => setDraft({ ...draft, text: e.target.value })}
          onKeyDown={(e) => {
            if (e.key === "Escape") setDraft(null);
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) save();
          }}
        />
        <div className="form-actions inline-add">
          <button onClick={() => setDraft(null)}>{t("common.cancel")}</button>
          <button className="primary" disabled={!draft.text.trim()} onClick={save}>
            {t("common.save")}
          </button>
        </div>
      </div>
    );
  }
  return (
    <div
      className={`side-entry book-note${open ? " open" : ""}${clipped ? " clipped" : ""}`}
      onClick={() => clipped && setOpen((o) => !o)}
      title={clipped && !open ? t("side.expandNote") : undefined}
    >
      {note.title && <div className="entry-title">{note.title}</div>}
      <div className="book-note-text" ref={body}>
        <Markdown text={note.text} />
      </div>
      <div className="entry-meta">
        <span>{formatDate(note.updated)}</span>
        <span className="entry-actions">
          <button
            className="icon"
            title={t("side.editNote")}
            onClick={(e) => {
              e.stopPropagation();
              setDraft({ title: note.title, text: note.text });
            }}
          >
            <Icon name="edit" size={14} />
          </button>
          <button
            className="icon"
            title={t("side.deleteNote")}
            onClick={(e) => {
              e.stopPropagation();
              props.onRemove(note.id);
            }}
          >
            <Icon name="close" size={14} />
          </button>
        </span>
      </div>
    </div>
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
