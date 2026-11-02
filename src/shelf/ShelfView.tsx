import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import LibrarySearch from "./LibrarySearch";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, errorText } from "../api";
import type { UpdateSettings } from "../App";
import { formatList, t, type MessageKey } from "../i18n";
import { Icon, type IconName } from "../icons";
import { useOpenSettings } from "../SettingsDialog";
import { Select } from "../Select";
import type { ShelfSort, Settings } from "../settings";
import type { Book, BookPatch, CardStats, DuplicateGroup, ImportProgress, Location, Shelf, SyncChanges } from "../types";
import { startOfToday } from "../study/intervals";
import { Confirm, Modal, Toast } from "../ui";
import { BookCard, BookRow } from "./BookCard";
import { EditDialog } from "./EditDialog";
import { childName, groupTree, inGroup, labelOf } from "./groups";
import { DedupeDialog, DedupeProgress } from "./DedupeDialog";
import { useAi } from "../ai/AiContext";
import OrganizeDialog from "../ai/OrganizeDialog";
import CiteDialog from "../paper/CiteDialog";
import PaperDialog from "../paper/PaperDialog";
import { dragRegion } from "../titlebar";

const EXTENSIONS = [
  "epub",
  "pdf",
  "txt",
  "md",
  "markdown",
  "mobi",
  "azw",
  "azw3",
  "prc",
  "fb2",
  "zip",
  "docx",
  "cbz",
  "djvu",
  "djv",
  "rtf",
  "odt",
  "html",
  "htm",
  "xhtml",
];

// Sidebar filters; any other value is a group name.
const isPaper = (b: Book) => !!b.paper && b.paper.kind !== "book";

const ALL = "\u0000all";
const RECENT = "\u0000recent";
const UNGROUPED = "\u0000none";
const PAPERS = "\u0000papers";

const SORTS: { key: ShelfSort; label: MessageKey }[] = [
  { key: "recent", label: "sort.recent" },
  { key: "added", label: "sort.added" },
  { key: "title", label: "sort.title" },
  { key: "author", label: "sort.author" },
  { key: "progress", label: "sort.progress" },
];

const FORMATS = "EPUB · PDF · TXT · Markdown · MOBI/AZW3 · FB2 · DOCX · ODT · RTF · HTML · CBZ · DjVu";

function sortBooks(books: Book[], sort: ShelfSort): Book[] {
  const byText = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });
  const sorted = [...books];
  switch (sort) {
    case "recent":
      return sorted.sort((a, b) => (b.lastRead ?? 0) - (a.lastRead ?? 0) || b.added - a.added);
    case "added":
      return sorted.sort((a, b) => b.added - a.added);
    case "title":
      return sorted.sort((a, b) => byText(a.title, b.title));
    case "author":
      return sorted.sort((a, b) => byText(a.author ?? "￿", b.author ?? "￿") || byText(a.title, b.title));
    case "progress":
      return sorted.sort((a, b) => b.progress - a.progress);
  }
}

interface Menu {
  book: Book;
  x: number;
  y: number;
}

export default function ShelfView(props: {
  settings: Settings;
  update: UpdateSettings;
  onOpen: (id: string, at?: { location: Location; query: string }) => void;
  onReview: () => void;
  onStats: () => void;
  onOpds: () => void;
}) {
  const { settings, update } = props;
  const [shelf, setShelf] = useState<Shelf>({ books: [], groups: [] });
  const [filter, setFilter] = useState(ALL);
  const [query, setQuery] = useState("");
  const [foundInBooks, setFoundInBooks] = useState(false);
  const [cardStats, setCardStats] = useState<CardStats | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [menu, setMenu] = useState<Menu | null>(null);
  const [editing, setEditing] = useState<Book[] | null>(null);
  const [deleting, setDeleting] = useState<Book[] | null>(null);
  const [groupDialog, setGroupDialog] = useState<{ old: string | null; parent: string | null } | null>(null);
  const [groupMoving, setGroupMoving] = useState<{ name: string; parent: string | null } | null>(null);
  const [groupDeleting, setGroupDeleting] = useState<string | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [dragging, setDragging] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [organizing, setOrganizing] = useState<Book[] | null>(null);
  const [importMenu, setImportMenu] = useState(false);
  const [dedupe, setDedupe] = useState<{ groups: DuplicateGroup[]; intro?: string } | null>(null);
  const [dedupeProgress, setDedupeProgress] = useState<ImportProgress | null>(null);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [paperFor, setPaperFor] = useState<{ bookId?: string } | null>(null);
  const [citing, setCiting] = useState<string[] | null>(null);
  const fetching = useRef(false);
  const ai = useAi();
  const openSettings = useOpenSettings();

  const refresh = useCallback(() => {
    api
      .listBooks()
      .then(setShelf)
      .catch((e) => setToast(errorText(e)));
  }, []);
  useEffect(refresh, [refresh]);

  const refreshCards = useCallback(() => {
    api
      .cardStats(startOfToday())
      .then(setCardStats)
      .catch(() => {});
  }, []);
  useEffect(refreshCards, [refreshCards]);

  // Another device changed something.
  useEffect(() => {
    const un = listen<SyncChanges>("sync-changed", ({ payload }) => {
      if (payload.shelf) refresh();
      if (payload.cards) refreshCards();
    });
    return () => {
      un.then((f) => f());
    };
  }, [refresh, refreshCards]);

  const fail = (e: unknown) => setToast(errorText(e));

  const currentGroup = filter.startsWith("\u0000") ? undefined : filter;

  /** Looks for duplicates (only around `only` when given) and shows them for review. */
  const findDuplicates = useCallback(async (only?: string[], intro?: string) => {
    setDedupeProgress({ done: 0, total: 0, name: "" });
    try {
      const groups = await api.findDuplicates(only);
      // The dialog shows shelf entries, which must include books imported just now.
      setShelf(await api.listBooks());
      // After an import, nothing to review means nothing to say.
      if (!only || groups.length > 0) setDedupe({ groups, intro });
    } catch (e) {
      setToast(errorText(e));
    } finally {
      setDedupeProgress(null);
    }
  }, []);

  const importPaths = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) return;
      setProgress({ done: 0, total: 0, name: "" });
      try {
        const r = await api.importBooks(paths, currentGroup);
        const parts = [t("shelf.imported", { n: r.added.length })];
        if (r.duplicates.length) parts.push(t("shelf.duplicates", { n: r.duplicates.length }));
        if (r.failed.length) {
          parts.push(t("shelf.failed", { n: r.failed.length, names: formatList(r.failed.map(([n]) => n).slice(0, 3)) }));
        }
        setToast(formatList(parts));
        if (r.possibleDuplicates.length) {
          setProgress(null);
          refresh();
          await findDuplicates(r.possibleDuplicates, t("dd.importedHint", { n: r.possibleDuplicates.length }));
        }
      } catch (e) {
        fail(e);
      } finally {
        setProgress(null);
        refresh();
      }
    },
    [currentGroup, refresh, findDuplicates],
  );

  useEffect(() => {
    const unProgress = listen<ImportProgress>("import-progress", (e) => setProgress(e.payload));
    const unDedupe = listen<ImportProgress>("dedupe-progress", (e) => setDedupeProgress((p) => (p ? e.payload : p)));
    const unDrop = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") setDragging(true);
      else if (p.type === "leave") setDragging(false);
      else if (p.type === "drop") {
        setDragging(false);
        importPaths(p.paths);
      }
    });
    return () => {
      unProgress.then((f) => f());
      unDedupe.then((f) => f());
      unDrop.then((f) => f());
    };
  }, [importPaths]);

  const pickFiles = async () => {
    const r = await openDialog({ multiple: true, filters: [{ name: t("shelf.fileFilter"), extensions: EXTENSIONS }] });
    if (r) importPaths(Array.isArray(r) ? r : [r]);
  };
  const pickFolder = async () => {
    const r = await openDialog({ directory: true, multiple: true });
    if (r) importPaths(Array.isArray(r) ? r : [r]);
  };

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    let books = shelf.books;
    if (filter === RECENT) books = books.filter((b) => b.lastRead);
    else if (filter === UNGROUPED) books = books.filter((b) => !b.group);
    else if (filter === PAPERS) books = books.filter(isPaper);
    else if (filter !== ALL) books = books.filter((b) => inGroup(b.group, filter, shelf.groups));
    if (q) books = books.filter((b) => b.title.toLowerCase().includes(q) || (b.author ?? "").toLowerCase().includes(q));
    return sortBooks(books, filter === RECENT ? "recent" : settings.shelfSort);
  }, [shelf.books, shelf.groups, filter, query, settings.shelfSort]);

  const selectedBooks = shelf.books.filter((b) => selected.has(b.id));
  const selecting = selected.size > 0;
  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const openMenu = (book: Book) => (e: MouseEvent) => {
    e.preventDefault();
    setMenu({ book, x: Math.min(e.clientX, window.innerWidth - 180), y: Math.min(e.clientY, window.innerHeight - 160) });
  };

  const saveEdit = async (books: Book[], patch: BookPatch) => {
    try {
      for (const b of books) await api.updateBook(b.id, patch);
      setEditing(null);
      refresh();
    } catch (e) {
      fail(e);
    }
  };

  const remove = async (books: Book[]) => {
    try {
      await api.removeBooks(books.map((b) => b.id));
      setSelected(new Set());
      refresh();
      setToast(t("shelf.removed", { n: books.length }));
    } catch (e) {
      fail(e);
    }
  };

  const count = (pred: (b: Book) => boolean) => shelf.books.filter(pred).length;
  const sideItem = (key: string, icon: IconName, label: string, n: number) => (
    <button key={key} className={`side-item${filter === key ? " active" : ""}`} onClick={() => setFilter(key)}>
      <Icon name={icon} />
      <span className="ellipsis grow">{label}</span>
      <span className="count">{n}</span>
    </button>
  );
  const toggleFold = (name: string) =>
    update({
      collapsedGroups: settings.collapsedGroups.includes(name) ? settings.collapsedGroups.filter((g) => g !== name) : [...settings.collapsedGroups, name],
    });

  const groupRow = (name: string, parent: string | null, children: string[], folded: boolean) => (
    <div key={name} className={`side-group${parent ? " child" : ""}${children.length > 0 ? " parent" : ""}`}>
      {children.length > 0 && (
        <button
          className="icon side-fold"
          title={folded ? t("shelf.expand") : t("shelf.collapse")}
          aria-label={folded ? t("shelf.expand") : t("shelf.collapse")}
          aria-expanded={!folded}
          onClick={() => toggleFold(name)}
        >
          <Icon name={folded ? "chevronRight" : "chevronDown"} size={13} />
        </button>
      )}
      {sideItem(
        name,
        "folder",
        labelOf(name, parent),
        count((b) => inGroup(b.group, name, shelf.groups)),
      )}
      <span className="side-actions">
        {!parent && (
          <button
            className="icon"
            title={t("shelf.newSubgroup")}
            aria-label={t("shelf.newSubgroup")}
            onClick={() => setGroupDialog({ old: null, parent: name })}
          >
            <Icon name="plus" size={14} />
          </button>
        )}
        <button className="icon" title={t("shelf.renameGroup")} aria-label={t("shelf.renameGroup")} onClick={() => setGroupDialog({ old: name, parent })}>
          <Icon name="edit" size={14} />
        </button>
        {children.length === 0 && (
          <button className="icon" title={t("shelf.moveGroup")} aria-label={t("shelf.moveGroup")} onClick={() => setGroupMoving({ name, parent })}>
            <Icon name="folder" size={14} />
          </button>
        )}
        <button className="icon" title={t("shelf.deleteGroup")} aria-label={t("shelf.deleteGroup")} onClick={() => setGroupDeleting(name)}>
          <Icon name="close" size={14} />
        </button>
      </span>
    </div>
  );

  /** Makes sure a book's file is here, fetching it if it can be. */
  const fileReady = async (b: Book) => {
    if (!b.missing) return true;
    if (!b.synced) {
      setToast(t("shelf.fileMissing"));
      return false;
    }
    if (fetching.current) return false;
    fetching.current = true;
    setDownloading(b.title);
    try {
      await api.syncFetchBook(b.id);
      refresh();
      return true;
    } catch (e) {
      fail(e);
      return false;
    } finally {
      fetching.current = false;
      setDownloading(null);
    }
  };
  const openBook = (b: Book) => fileReady(b).then((ok) => ok && props.onOpen(b.id));

  return (
    <div className="shelf" onClick={() => (setMenu(null), setImportMenu(false))}>
      {settings.shelfSidebar && (
        <aside className="sidebar">
          <div className="brand" {...dragRegion}>
            AiReader
          </div>
          {sideItem(ALL, "library", t("shelf.all"), shelf.books.length)}
          {shelf.books.some(isPaper) && sideItem(PAPERS, "note", t("shelf.papers"), count(isPaper))}
          {sideItem(
            RECENT,
            "clock",
            t("shelf.recent"),
            count((b) => !!b.lastRead),
          )}
          {sideItem(
            UNGROUPED,
            "inbox",
            t("shelf.ungrouped"),
            count((b) => !b.group),
          )}
          <button className="side-item" onClick={props.onReview}>
            <Icon name="layers" />
            <span className="ellipsis grow">{t("cards.review")}</span>
            {!!cardStats?.due && <span className="count">{cardStats.due}</span>}
          </button>
          <button className="side-item" onClick={props.onStats}>
            <Icon name="chart" />
            <span className="ellipsis">{t("stats.title")}</span>
          </button>
          <button className="side-item" onClick={props.onOpds}>
            <Icon name="globe" />
            <span className="ellipsis">{t("opds.title")}</span>
          </button>
          <div className="side-heading">
            <span>{t("shelf.groups")}</span>
            <button className="icon" title={t("shelf.newGroup")} aria-label={t("shelf.newGroup")} onClick={() => setGroupDialog({ old: null, parent: null })}>
              <Icon name="plus" size={14} />
            </button>
          </div>
          {groupTree(shelf.groups).map(({ name, children }) => {
            const folded = settings.collapsedGroups.includes(name);
            return (
              <div key={name}>
                {groupRow(name, null, children, folded)}
                {!folded && children.map((child) => groupRow(child, name, [], false))}
              </div>
            );
          })}
          {shelf.groups.length === 0 && <div className="muted small side-empty">{t("shelf.groupsHint")}</div>}
          <div className="side-footer">
            <button className="side-item" onClick={ai.openSettings} title={`${t("ai.settings")}: ${ai.ready ? t("ai.configured") : t("ai.notConfigured")}`}>
              <Icon name="sparkles" />
              <span className="ellipsis grow">{t("ai.settings")}</span>
              <span
                className={`status-dot${ai.ready ? " on" : ""}`}
                title={ai.ready ? t("ai.configured") : t("ai.notConfigured")}
                aria-label={ai.ready ? t("ai.configured") : t("ai.notConfigured")}
              />
            </button>
            <button className="side-item" onClick={() => openSettings()}>
              <Icon name="settings" />
              <span className="ellipsis grow">{t("settings.title")}</span>
            </button>
          </div>
        </aside>
      )}

      <main className="shelf-main">
        <header className="shelf-bar" {...dragRegion}>
          <button
            className={`icon-button${settings.shelfSidebar ? " on" : ""}`}
            title={t("shelf.toggleSidebar")}
            aria-label={t("shelf.toggleSidebar")}
            onClick={() => update({ shelfSidebar: !settings.shelfSidebar })}
          >
            <Icon name="panelLeft" />
          </button>
          <label className="search">
            <Icon name="search" />
            <input placeholder={t("shelf.searchPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} />
          </label>
          <Select
            ghost
            value={settings.shelfSort}
            label={t("shelf.sort")}
            disabled={filter === RECENT}
            options={SORTS.map((o) => ({ value: o.key, label: t(o.label) }))}
            onChange={(shelfSort) => update({ shelfSort })}
          />
          <div className="segmented">
            <button
              className={`icon-button${settings.shelfView === "grid" ? " on" : ""}`}
              title={t("shelf.gridView")}
              aria-label={t("shelf.gridView")}
              onClick={() => update({ shelfView: "grid" })}
            >
              <Icon name="grid" />
            </button>
            <button
              className={`icon-button${settings.shelfView === "list" ? " on" : ""}`}
              title={t("shelf.listView")}
              aria-label={t("shelf.listView")}
              onClick={() => update({ shelfView: "list" })}
            >
              <Icon name="list" />
            </button>
          </div>
          <span className="spacer" />
          {shelf.books.length > 1 && (
            <button onClick={() => findDuplicates()} title={t("dd.findTitle")}>
              <Icon name="layers" />
              <span className="label">{t("dd.find")}</span>
            </button>
          )}
          {shelf.books.length > 0 && (
            <button
              onClick={() => (ai.ready ? setOrganizing(selecting ? selectedBooks : visible) : ai.openSettings())}
              title={selecting ? t("shelf.organizeSelected") : t("shelf.organizeVisible")}
            >
              <Icon name="sparkles" />
              <span className="label">{t("shelf.organize")}</span>
            </button>
          )}
          <div className="menu-anchor">
            <div className="split-button">
              <button className="primary" onClick={pickFiles}>
                <Icon name="filePlus" />
                <span className="label">{t("shelf.importBooks")}</span>
              </button>
              <button
                className="primary"
                aria-label={t("common.more")}
                onClick={(e) => {
                  e.stopPropagation();
                  setImportMenu((m) => !m);
                }}
              >
                <Icon name="chevronDown" size={14} />
              </button>
            </div>
            {importMenu && (
              <div className="menu dropdown" onClick={(e) => e.stopPropagation()}>
                <button onClick={() => (setImportMenu(false), pickFiles())}>
                  <Icon name="filePlus" /> {t("shelf.importBooks")}
                </button>
                <button onClick={() => (setImportMenu(false), pickFolder())}>
                  <Icon name="folderPlus" /> {t("shelf.importFolder")}
                </button>
                <button onClick={() => (setImportMenu(false), setPaperFor({}))}>
                  <Icon name="note" /> {t("paper.importTitle")}
                </button>
              </div>
            )}
          </div>
          {!settings.shelfSidebar && (
            <button className="icon-button" title={t("settings.title")} aria-label={t("settings.title")} onClick={() => openSettings()}>
              <Icon name="settings" />
            </button>
          )}
        </header>

        {selecting && (
          <div className="select-bar">
            <span>{t("shelf.selectedCount", { n: selected.size })}</span>
            <button onClick={() => setSelected(new Set(visible.map((b) => b.id)))}>{t("shelf.selectAll")}</button>
            <button onClick={() => setEditing(selectedBooks)}>{t("shelf.setGroups")}</button>
            <button onClick={() => setCiting(selectedBooks.map((b) => b.id))}>{t("cite.export")}</button>
            <button className="danger" onClick={() => setDeleting(selectedBooks)}>
              {t("common.delete")}
            </button>
            <span className="spacer" />
            <button onClick={() => setSelected(new Set())}>{t("shelf.clearSelection")}</button>
          </div>
        )}

        <div className="shelf-content">
          {shelf.books.length === 0 ? (
            <div className="empty">
              <div className="empty-title">{t("shelf.emptyTitle")}</div>
              <div className="muted">{t("shelf.emptyFormats", { formats: FORMATS })}</div>
              <div className="empty-actions">
                <button className="primary" onClick={pickFiles}>
                  <Icon name="filePlus" /> {t("shelf.importBooks")}
                </button>
                <button onClick={pickFolder}>
                  <Icon name="folderPlus" /> {t("shelf.importFolder")}
                </button>
              </div>
              <div className="muted small">{t("shelf.dropHint")}</div>
            </div>
          ) : visible.length === 0 ? (
            !foundInBooks && <div className="empty muted">{t("shelf.noMatch")}</div>
          ) : settings.shelfView === "grid" ? (
            <div className="book-grid">
              {visible.map((b) => (
                <BookCard
                  key={b.id}
                  book={b}
                  selected={selected.has(b.id)}
                  selecting={selecting}
                  onOpen={() => openBook(b)}
                  onToggle={() => toggle(b.id)}
                  onMenu={openMenu(b)}
                />
              ))}
            </div>
          ) : (
            <div className={`book-list${selecting ? " selecting" : ""}`}>
              <div className="book-row header">
                <span className="check-space" />
                <span className="row-cover" />
                <span className="row-main">{t("shelf.colTitle")}</span>
                <span className="row-cell">{t("shelf.colFormat")}</span>
                <span className="row-cell">{t("shelf.colProgress")}</span>
                <span className="row-cell">{t("shelf.colReadTime")}</span>
                <span className="row-cell">{t("shelf.colLastRead")}</span>
                <span className="row-cell">{t("shelf.colAdded")}</span>
                <span className="more-space" />
              </div>
              {visible.map((b) => (
                <BookRow
                  key={b.id}
                  book={b}
                  selected={selected.has(b.id)}
                  selecting={selecting}
                  onOpen={() => openBook(b)}
                  onToggle={() => toggle(b.id)}
                  onMenu={openMenu(b)}
                />
              ))}
            </div>
          )}
          <LibrarySearch
            query={query}
            books={shelf.books}
            onResults={setFoundInBooks}
            onOpen={(b, location) => fileReady(b).then((ok) => ok && props.onOpen(b.id, { location, query: query.trim() }))}
          />
        </div>
      </main>

      {menu && (
        <div className="menu" style={{ left: menu.x, top: menu.y }} onClick={(e) => e.stopPropagation()}>
          <button onClick={() => (setMenu(null), openBook(menu.book))} disabled={menu.book.missing && !menu.book.synced}>
            {t("shelf.open")}
          </button>
          <button onClick={() => (setMenu(null), setEditing([menu.book]))}>{t("shelf.editInfo")}</button>
          <button onClick={() => (setMenu(null), setPaperFor({ bookId: menu.book.id }))}>{t("paper.details")}</button>
          <button onClick={() => (setMenu(null), setCiting([menu.book.id]))}>{t("cite.action")}</button>
          <button onClick={() => (setMenu(null), toggle(menu.book.id))}>{selected.has(menu.book.id) ? t("shelf.clearSelection") : t("common.select")}</button>
          <hr />
          <button className="danger" onClick={() => (setMenu(null), setDeleting([menu.book]))}>
            {t("shelf.remove")}
          </button>
        </div>
      )}

      {editing && <EditDialog books={editing} groups={shelf.groups} onSave={(patch) => saveEdit(editing, patch)} onClose={() => setEditing(null)} />}
      {deleting && (
        <Confirm
          title={t("shelf.remove")}
          danger
          confirmText={t("common.delete")}
          message={<p>{deleting.length === 1 ? t("shelf.removeOne", { title: deleting[0].title }) : t("shelf.removeMany", { n: deleting.length })}</p>}
          onConfirm={() => remove(deleting)}
          onClose={() => setDeleting(null)}
        />
      )}
      {groupDialog && (
        <GroupDialog
          old={groupDialog.old === null ? null : labelOf(groupDialog.old, groupDialog.parent)}
          parent={groupDialog.parent}
          onClose={() => setGroupDialog(null)}
          onSave={async (label) => {
            const name = childName(groupDialog.parent, label);
            try {
              if (groupDialog.old === null) await api.createGroup(name);
              else {
                await api.renameGroup(groupDialog.old, name);
                if (filter === groupDialog.old) setFilter(name);
              }
              setGroupDialog(null);
              refresh();
            } catch (e) {
              fail(e);
            }
          }}
        />
      )}
      {groupMoving && (
        <GroupMoveDialog
          name={groupMoving.name}
          parent={groupMoving.parent}
          targets={groupTree(shelf.groups)
            .map((g) => g.name)
            .filter((g) => g !== groupMoving.name)}
          onClose={() => setGroupMoving(null)}
          onMove={async (target) => {
            const name = childName(target, labelOf(groupMoving.name, groupMoving.parent));
            try {
              await api.renameGroup(groupMoving.name, name);
              if (filter === groupMoving.name) setFilter(name);
              setGroupMoving(null);
              refresh();
            } catch (e) {
              fail(e);
            }
          }}
        />
      )}
      {groupDeleting && (
        <Confirm
          title={t("shelf.deleteGroup")}
          danger
          confirmText={t("shelf.deleteGroup")}
          message={
            <>
              <p>{t("shelf.deleteGroupConfirm", { name: groupDeleting })}</p>
              {groupTree(shelf.groups).some((g) => g.name === groupDeleting && g.children.length > 0) && (
                <p className="muted small">{t("shelf.deleteGroupChildren")}</p>
              )}
            </>
          }
          onConfirm={async () => {
            try {
              await api.deleteGroup(groupDeleting);
              if (filter === groupDeleting) setFilter(ALL);
              refresh();
            } catch (e) {
              fail(e);
            }
          }}
          onClose={() => setGroupDeleting(null)}
        />
      )}
      {progress && (
        <div className="modal-backdrop">
          <div className="modal import-progress">
            <div className="modal-title">{t("shelf.importing")}</div>
            <div className="bar">
              <div style={{ width: progress.total ? `${(progress.done / progress.total) * 100}%` : "0%" }} />
            </div>
            <div className="muted small ellipsis">{progress.total ? `${progress.done} / ${progress.total}　${progress.name}` : t("shelf.scanning")}</div>
          </div>
        </div>
      )}
      {dragging && <div className="drop-overlay">{currentGroup ? t("shelf.dropToGroup", { group: currentGroup }) : t("shelf.dropToShelf")}</div>}
      {organizing && (
        <OrganizeDialog
          books={organizing}
          onClose={() => setOrganizing(null)}
          onApply={async (changes) => {
            for (const c of changes) await api.updateBook(c.id, c.patch);
            refresh();
            setToast(t("shelf.updated", { n: changes.length }));
          }}
        />
      )}
      {dedupeProgress && <DedupeProgress {...dedupeProgress} />}
      {dedupe && (
        <DedupeDialog
          groups={dedupe.groups}
          intro={dedupe.intro}
          books={new Map(shelf.books.map((b) => [b.id, b]))}
          onClose={() => setDedupe(null)}
          onApply={async (merges) => {
            try {
              const r = await api.mergeDuplicates(merges);
              setDedupe(null);
              setSelected(new Set());
              setToast(t("dd.done", { n: r.removed, h: r.highlights, b: r.bookmarks }));
            } catch (e) {
              fail(e);
            } finally {
              refresh();
            }
          }}
        />
      )}
      {paperFor && <PaperDialog bookId={paperFor.bookId} onClose={() => setPaperFor(null)} onSaved={refresh} onImported={refresh} />}
      {citing && <CiteDialog ids={citing} onClose={() => setCiting(null)} />}
      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
      {downloading !== null && <div className="toast">{t("shelf.downloading", { title: downloading })}</div>}
    </div>
  );
}

function GroupMoveDialog(props: { name: string; parent: string | null; targets: string[]; onMove: (target: string | null) => void; onClose: () => void }) {
  const [target, setTarget] = useState(props.parent ?? "");
  return (
    <Modal
      title={t("shelf.moveGroup")}
      onClose={props.onClose}
      width={340}
      footer={
        <>
          <button onClick={props.onClose}>{t("common.cancel")}</button>
          <button className="primary" disabled={target === (props.parent ?? "")} onClick={() => props.onMove(target || null)}>
            {t("common.ok")}
          </button>
        </>
      }
    >
      <label className="field">
        <span>{t("shelf.parentGroup")}</span>
        <Select
          full
          value={target}
          label={t("shelf.parentGroup")}
          options={[{ value: "", label: t("shelf.topLevel") }, ...props.targets.map((g) => ({ value: g, label: g }))]}
          onChange={setTarget}
        />
      </label>
    </Modal>
  );
}

function GroupDialog(props: { old: string | null; parent: string | null; onSave: (label: string) => void; onClose: () => void }) {
  const [name, setName] = useState(props.old ?? "");
  return (
    <Modal
      title={props.old !== null ? t("shelf.renameGroup") : props.parent ? t("shelf.newSubgroupOf", { name: props.parent }) : t("shelf.newGroup")}
      onClose={props.onClose}
      width={340}
      footer={
        <>
          <button onClick={props.onClose}>{t("common.cancel")}</button>
          <button className="primary" disabled={!name.trim()} onClick={() => props.onSave(name)}>
            {t("common.ok")}
          </button>
        </>
      }
    >
      <input
        className="full"
        value={name}
        autoFocus
        placeholder={t("shelf.groupName")}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && name.trim() && props.onSave(name)}
      />
    </Modal>
  );
}
