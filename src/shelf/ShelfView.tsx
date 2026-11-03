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
import { Confirm, Menu, MenuDivider, MenuItem, Modal, Notice, Toast } from "../ui";
import { BookCard, BookRow } from "./BookCard";
import { EditDialog } from "./EditDialog";
import { childName, groupTree, inGroup, labelOf } from "./groups";
import { DedupeDialog, DedupeProgress } from "./DedupeDialog";
import { useAi } from "../ai/AiContext";
import OrganizeDialog from "../ai/OrganizeDialog";
import CiteDialog from "../paper/CiteDialog";
import PaperDialog from "../paper/PaperDialog";
import { dragRegion } from "../titlebar";
import brandMark from "../../src-tauri/icons/icon.svg?url";
import { mobile } from "../platform";

const EXTENSIONS = [
  "epub",
  "pdf",
  "txt",
  "md",
  "markdown",
  "mdown",
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

/** Rows of the sidebar, after the Apple Books sidebar. */
const SIDE_ITEM =
  "tw group/side flex h-8 w-full items-center gap-2.5 rounded-lg px-2 text-left text-[13px] text-[var(--muted)] hover:bg-[var(--hover)] hover:text-[var(--fg)] [&>svg]:text-[var(--faint)]";
const SIDE_ACTIVE = " bg-[var(--selected)] font-semibold text-[var(--fg)]! [&>svg]:text-[var(--accent)]!";
const SIDE_HEADING = "px-2 pt-2 pb-1 text-[11px] font-semibold tracking-wide text-[var(--faint)] uppercase";
const SIDE_ICON = "d-btn d-btn-ghost d-btn-xs d-btn-square text-[var(--muted)]";
/** Toolbar buttons: quiet until pointed at. */
const BAR_ICON = "d-btn d-btn-ghost d-btn-sm d-btn-square text-[var(--muted)]";
const BAR_BUTTON = "d-btn d-btn-ghost d-btn-sm font-medium text-[var(--muted)] hover:text-[var(--fg)]";

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
  // Phones show the sidebar as a drawer over the shelf, opened on demand.
  const [drawer, setDrawer] = useState(false);
  const sidebarShown = mobile ? drawer : settings.shelfSidebar;
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
    <button key={key} className={`${SIDE_ITEM}${filter === key ? SIDE_ACTIVE : ""}`} onClick={() => (setFilter(key), setDrawer(false))}>
      <Icon name={icon} />
      <span className="grow truncate">{label}</span>
      <span className="count text-xs font-medium text-[var(--faint)] tabular-nums group-hover/side:invisible">{n}</span>
    </button>
  );
  const toggleFold = (name: string) =>
    update({
      collapsedGroups: settings.collapsedGroups.includes(name) ? settings.collapsedGroups.filter((g) => g !== name) : [...settings.collapsedGroups, name],
    });

  const groupRow = (name: string, parent: string | null, children: string[], folded: boolean) => (
    <div key={name} className={`group/side relative${parent ? " [&>button]:pl-8" : ""}${children.length > 0 ? " [&>button>svg]:invisible" : ""}`}>
      {children.length > 0 && (
        <button
          className="side-fold d-btn d-btn-ghost d-btn-xs absolute top-1/2 left-1 z-10 h-6 min-h-0 w-6 -translate-y-1/2 p-0"
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
      <span className="absolute top-1/2 right-1 hidden -translate-y-1/2 rounded-md bg-[var(--panel)] group-hover/side:flex">
        {!parent && (
          <button
            className={SIDE_ICON}
            title={t("shelf.newSubgroup")}
            aria-label={t("shelf.newSubgroup")}
            onClick={() => setGroupDialog({ old: null, parent: name })}
          >
            <Icon name="plus" size={14} />
          </button>
        )}
        <button className={SIDE_ICON} title={t("shelf.renameGroup")} aria-label={t("shelf.renameGroup")} onClick={() => setGroupDialog({ old: name, parent })}>
          <Icon name="edit" size={14} />
        </button>
        {children.length === 0 && (
          <button className={SIDE_ICON} title={t("shelf.moveGroup")} aria-label={t("shelf.moveGroup")} onClick={() => setGroupMoving({ name, parent })}>
            <Icon name="folder" size={14} />
          </button>
        )}
        <button className={SIDE_ICON} title={t("shelf.deleteGroup")} aria-label={t("shelf.deleteGroup")} onClick={() => setGroupDeleting(name)}>
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
    <div className="shelf flex h-full" onClick={() => (setMenu(null), setImportMenu(false))}>
      {mobile && drawer && <div className="drawer-backdrop" onClick={() => setDrawer(false)} />}
      {sidebarShown && (
        <aside
          className="sidebar flex w-[232px] shrink-0 flex-col gap-px overflow-y-auto border-r border-[var(--line)] bg-[var(--panel)] px-2.5 py-3"
          // On phones, going anywhere from the drawer closes it; folding a group doesn't.
          onClick={(e) => mobile && (e.target as HTMLElement).closest("button:not(.side-fold)") && setDrawer(false)}
        >
          <div className="brand flex items-center gap-2 px-2 pt-0.5 pb-3 text-[15px] font-bold tracking-tight" {...dragRegion}>
            <img className="-my-0.5 -ml-1 size-6" src={brandMark} alt="" draggable={false} />
            AiReader
          </div>
          <div className={SIDE_HEADING}>{t("shelf.library")}</div>
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
          <button className={SIDE_ITEM} onClick={props.onReview}>
            <Icon name="layers" />
            <span className="grow truncate">{t("cards.review")}</span>
            {!!cardStats?.due && <span className="d-badge d-badge-primary d-badge-xs font-semibold">{cardStats.due}</span>}
          </button>
          <button className={SIDE_ITEM} onClick={props.onStats}>
            <Icon name="chart" />
            <span className="truncate">{t("stats.title")}</span>
          </button>
          <button className={SIDE_ITEM} onClick={props.onOpds}>
            <Icon name="globe" />
            <span className="truncate">{t("opds.title")}</span>
          </button>
          <div className={`${SIDE_HEADING} mt-3 flex items-center justify-between`}>
            <span>{t("shelf.groups")}</span>
            <button
              className={SIDE_ICON}
              title={t("shelf.newGroup")}
              aria-label={t("shelf.newGroup")}
              onClick={() => setGroupDialog({ old: null, parent: null })}
            >
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
          {shelf.groups.length === 0 && <div className="px-2 py-1.5 text-xs text-[var(--muted)]">{t("shelf.groupsHint")}</div>}
          <div className="side-footer mt-auto flex flex-col gap-px border-t border-[var(--line)] pt-2">
            <button className={SIDE_ITEM} onClick={ai.openSettings} title={`${t("ai.settings")}: ${ai.ready ? t("ai.configured") : t("ai.notConfigured")}`}>
              <Icon name="sparkles" />
              <span className="grow truncate">{t("ai.settings")}</span>
              <span
                className={`d-status ${ai.ready ? "d-status-success" : ""}`}
                title={ai.ready ? t("ai.configured") : t("ai.notConfigured")}
                aria-label={ai.ready ? t("ai.configured") : t("ai.notConfigured")}
              />
            </button>
            <button className={SIDE_ITEM} onClick={() => openSettings()}>
              <Icon name="settings" />
              <span className="grow truncate">{t("settings.title")}</span>
            </button>
          </div>
        </aside>
      )}

      <main className="shelf-main flex min-w-0 flex-1 flex-col">
        <header className="shelf-bar flex min-h-[var(--bar-h)] items-center gap-1.5 px-4" {...dragRegion}>
          <button
            className={`icon-button${sidebarShown ? " on" : ""}`}
            title={t("shelf.toggleSidebar")}
            aria-label={t("shelf.toggleSidebar")}
            onClick={() => (mobile ? setDrawer((d) => !d) : update({ shelfSidebar: !settings.shelfSidebar }))}
          >
            <Icon name="panelLeft" />
          </button>
          <label className="search d-input d-input-sm w-72 min-w-32 shrink rounded-lg border-transparent bg-[var(--input)] text-[13px] shadow-none focus-within:border-[var(--accent)] focus-within:bg-[var(--bg)] focus-within:outline-none">
            <Icon name="search" />
            <input className="tw grow" placeholder={t("shelf.searchPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} />
          </label>
          <Select
            ghost
            value={settings.shelfSort}
            label={t("shelf.sort")}
            disabled={filter === RECENT}
            options={SORTS.map((o) => ({ value: o.key, label: t(o.label) }))}
            onChange={(shelfSort) => update({ shelfSort })}
          />
          <div className="d-join rounded-lg bg-[var(--input)] p-0.5">
            {(["grid", "list"] as const).map((view) => (
              <button
                key={view}
                className={`d-join-item d-btn d-btn-xs h-7 border-0 px-2 shadow-none ${
                  settings.shelfView === view ? "rounded-md! bg-[var(--bg)] shadow-sm" : "d-btn-ghost rounded-md! text-[var(--muted)]"
                }`}
                title={t(view === "grid" ? "shelf.gridView" : "shelf.listView")}
                aria-label={t(view === "grid" ? "shelf.gridView" : "shelf.listView")}
                aria-pressed={settings.shelfView === view}
                onClick={() => update({ shelfView: view })}
              >
                <Icon name={view} />
              </button>
            ))}
          </div>
          <span className="flex-1" />
          {shelf.books.length > 1 && (
            <button className={BAR_BUTTON} onClick={() => findDuplicates()} title={t("dd.findTitle")}>
              <Icon name="layers" />
              <span className="label">{t("dd.find")}</span>
            </button>
          )}
          {shelf.books.length > 0 && (
            <button
              className={BAR_BUTTON}
              onClick={() => (ai.ready ? setOrganizing(selecting ? selectedBooks : visible) : ai.openSettings())}
              title={selecting ? t("shelf.organizeSelected") : t("shelf.organizeVisible")}
            >
              <Icon name="sparkles" />
              <span className="label">{t("shelf.organize")}</span>
            </button>
          )}
          <div className="menu-anchor relative">
            <div className="d-join">
              <button className="d-join-item d-btn d-btn-primary d-btn-sm" onClick={pickFiles}>
                <Icon name="filePlus" />
                <span className="label">{t("shelf.importBooks")}</span>
              </button>
              <button
                className="d-join-item d-btn d-btn-primary d-btn-sm border-l border-l-white/25 px-2"
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
              <Menu className="absolute top-full right-0 mt-1.5" onClick={(e) => e.stopPropagation()}>
                <MenuItem icon="filePlus" onClick={() => (setImportMenu(false), pickFiles())}>
                  {t("shelf.importBooks")}
                </MenuItem>
                <MenuItem icon="folderPlus" onClick={() => (setImportMenu(false), pickFolder())}>
                  {t("shelf.importFolder")}
                </MenuItem>
                <MenuItem icon="note" onClick={() => (setImportMenu(false), setPaperFor({}))}>
                  {t("paper.importTitle")}
                </MenuItem>
              </Menu>
            )}
          </div>
          {!sidebarShown && (
            <button className={BAR_ICON} title={t("settings.title")} aria-label={t("settings.title")} onClick={() => openSettings()}>
              <Icon name="settings" />
            </button>
          )}
        </header>

        {selecting && (
          <div className="mx-4 mb-1 flex items-center gap-1 rounded-xl bg-[var(--accent-soft)] px-3 py-1.5 text-[13px]">
            <span className="mr-2 font-semibold">{t("shelf.selectedCount", { n: selected.size })}</span>
            <button className="d-btn d-btn-ghost d-btn-sm" onClick={() => setSelected(new Set(visible.map((b) => b.id)))}>
              {t("shelf.selectAll")}
            </button>
            <button className="d-btn d-btn-ghost d-btn-sm" onClick={() => setEditing(selectedBooks)}>
              {t("shelf.setGroups")}
            </button>
            <button className="d-btn d-btn-ghost d-btn-sm" onClick={() => setCiting(selectedBooks.map((b) => b.id))}>
              {t("cite.export")}
            </button>
            <button className="d-btn d-btn-ghost d-btn-sm text-[var(--danger)]" onClick={() => setDeleting(selectedBooks)}>
              {t("common.delete")}
            </button>
            <span className="flex-1" />
            <button className="d-btn d-btn-ghost d-btn-sm" onClick={() => setSelected(new Set())}>
              {t("shelf.clearSelection")}
            </button>
          </div>
        )}

        <div className="shelf-content flex-1 overflow-y-auto px-7 pt-4 pb-10 max-sm:px-5">
          {shelf.books.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
              <div className="text-xl font-bold tracking-tight">{t("shelf.emptyTitle")}</div>
              <div className="text-[var(--muted)]">{t("shelf.emptyFormats", { formats: FORMATS })}</div>
              <div className="mt-2 flex gap-2.5">
                <button className="d-btn d-btn-primary d-btn-sm" onClick={pickFiles}>
                  <Icon name="filePlus" /> {t("shelf.importBooks")}
                </button>
                <button className="d-btn d-btn-sm" onClick={pickFolder}>
                  <Icon name="folderPlus" /> {t("shelf.importFolder")}
                </button>
              </div>
              <div className="text-xs text-[var(--muted)]">{t("shelf.dropHint")}</div>
            </div>
          ) : visible.length === 0 ? (
            !foundInBooks && <div className="flex h-full items-center justify-center text-[var(--muted)]">{t("shelf.noMatch")}</div>
          ) : settings.shelfView === "grid" ? (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-x-7 gap-y-9 max-sm:grid-cols-[repeat(auto-fill,minmax(108px,1fr))] max-sm:gap-x-4">
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
            <div className="flex flex-col">
              <div className="flex items-center gap-3 border-b border-[var(--line)] px-2 pb-2 text-[11px] font-semibold tracking-wide text-[var(--faint)] uppercase">
                <span className="w-4 shrink-0" />
                <span className="w-9 shrink-0" />
                <span className="flex-1">{t("shelf.colTitle")}</span>
                {(["shelf.colFormat", "shelf.colSize", "shelf.colProgress", "shelf.colReadTime", "shelf.colLastRead", "shelf.colAdded"] as const).map((k) => (
                  <span key={k} className="w-[88px] shrink-0">
                    {t(k)}
                  </span>
                ))}
                <span className="w-6 shrink-0" />
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
        <Menu className="fixed" style={{ left: menu.x, top: menu.y }} onClick={(e) => e.stopPropagation()}>
          <MenuItem disabled={menu.book.missing && !menu.book.synced} onClick={() => (setMenu(null), openBook(menu.book))}>
            {t("shelf.open")}
          </MenuItem>
          <MenuItem onClick={() => (setMenu(null), setEditing([menu.book]))}>{t("shelf.editInfo")}</MenuItem>
          <MenuItem onClick={() => (setMenu(null), setPaperFor({ bookId: menu.book.id }))}>{t("paper.details")}</MenuItem>
          <MenuItem onClick={() => (setMenu(null), setCiting([menu.book.id]))}>{t("cite.action")}</MenuItem>
          <MenuItem onClick={() => (setMenu(null), toggle(menu.book.id))}>
            {selected.has(menu.book.id) ? t("shelf.clearSelection") : t("common.select")}
          </MenuItem>
          <MenuDivider />
          <MenuItem danger onClick={() => (setMenu(null), setDeleting([menu.book]))}>
            {t("shelf.remove")}
          </MenuItem>
        </Menu>
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
        <div className="d-modal d-modal-open">
          <div className="d-modal-box flex w-[420px] flex-col gap-3" role="dialog" aria-modal="true" aria-label={t("shelf.importing")}>
            <h2 className="text-[17px] font-semibold">{t("shelf.importing")}</h2>
            <progress className="d-progress d-progress-primary w-full" value={progress.total ? progress.done : undefined} max={progress.total || 1} />
            <div className="truncate text-xs text-[var(--muted)]">
              {progress.total ? `${progress.done} / ${progress.total}　${progress.name}` : t("shelf.scanning")}
            </div>
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
      {downloading !== null && <Notice message={t("shelf.downloading", { title: downloading })} />}
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
          <button className="d-btn d-btn-sm" onClick={props.onClose}>
            {t("common.cancel")}
          </button>
          <button className="d-btn d-btn-sm d-btn-primary" disabled={target === (props.parent ?? "")} onClick={() => props.onMove(target || null)}>
            {t("common.ok")}
          </button>
        </>
      }
    >
      <label className="flex flex-col gap-1.5">
        <span className="text-[13px] font-semibold">{t("shelf.parentGroup")}</span>
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
          <button className="d-btn d-btn-sm" onClick={props.onClose}>
            {t("common.cancel")}
          </button>
          <button className="d-btn d-btn-sm d-btn-primary" disabled={!name.trim()} onClick={() => props.onSave(name)}>
            {t("common.ok")}
          </button>
        </>
      }
    >
      <input
        className="d-input w-full"
        value={name}
        autoFocus
        placeholder={t("shelf.groupName")}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && name.trim() && props.onSave(name)}
      />
    </Modal>
  );
}
