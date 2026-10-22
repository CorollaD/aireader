import { useCallback, useEffect, useMemo, useState, type MouseEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, errorText } from "../api";
import type { UpdateSettings } from "../App";
import { formatList, t, type MessageKey } from "../i18n";
import { Icon, type IconName } from "../icons";
import { useOpenSettings } from "../SettingsDialog";
import type { ShelfSort, Settings } from "../settings";
import type { Book, BookPatch, ImportProgress, Shelf } from "../types";
import { Confirm, Modal, Toast } from "../ui";
import { BookCard, BookRow } from "./BookCard";
import { EditDialog } from "./EditDialog";
import { useAi } from "../ai/AiContext";
import OrganizeDialog from "../ai/OrganizeDialog";

const EXTENSIONS = ["epub", "pdf", "txt", "md", "markdown", "mobi", "azw", "azw3", "prc", "fb2", "zip", "docx", "cbz"];

// Sidebar filters; any other value is a group name.
const ALL = "\u0000all";
const RECENT = "\u0000recent";
const UNGROUPED = "\u0000none";

const SORTS: { key: ShelfSort; label: MessageKey }[] = [
  { key: "recent", label: "sort.recent" },
  { key: "added", label: "sort.added" },
  { key: "title", label: "sort.title" },
  { key: "author", label: "sort.author" },
  { key: "progress", label: "sort.progress" },
];

const FORMATS = "EPUB · PDF · TXT · Markdown · MOBI/AZW3 · FB2 · DOCX · CBZ";

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

export default function ShelfView(props: { settings: Settings; update: UpdateSettings; onOpen: (id: string) => void }) {
  const { settings, update } = props;
  const [shelf, setShelf] = useState<Shelf>({ books: [], groups: [] });
  const [filter, setFilter] = useState(ALL);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [menu, setMenu] = useState<Menu | null>(null);
  const [editing, setEditing] = useState<Book[] | null>(null);
  const [deleting, setDeleting] = useState<Book[] | null>(null);
  const [groupDialog, setGroupDialog] = useState<{ old: string | null } | null>(null);
  const [groupDeleting, setGroupDeleting] = useState<string | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [dragging, setDragging] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [organizing, setOrganizing] = useState<Book[] | null>(null);
  const [importMenu, setImportMenu] = useState(false);
  const ai = useAi();
  const openSettings = useOpenSettings();

  const refresh = useCallback(() => {
    api.listBooks().then(setShelf).catch((e) => setToast(errorText(e)));
  }, []);
  useEffect(refresh, [refresh]);

  const fail = (e: unknown) => setToast(errorText(e));

  const currentGroup = filter.startsWith("\u0000") ? undefined : filter;

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
      } catch (e) {
        fail(e);
      } finally {
        setProgress(null);
        refresh();
      }
    },
    [currentGroup, refresh],
  );

  useEffect(() => {
    const unProgress = listen<ImportProgress>("import-progress", (e) => setProgress(e.payload));
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
    else if (filter === UNGROUPED) books = books.filter((b) => b.groups.length === 0);
    else if (filter !== ALL) books = books.filter((b) => b.groups.includes(filter));
    if (q) books = books.filter((b) => b.title.toLowerCase().includes(q) || (b.author ?? "").toLowerCase().includes(q));
    return sortBooks(books, filter === RECENT ? "recent" : settings.shelfSort);
  }, [shelf.books, filter, query, settings.shelfSort]);

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

  const saveEdit = async (books: Book[], patch: BookPatch, removed: string[]) => {
    try {
      for (const b of books) {
        const keep = b.groups.filter((g) => !removed.includes(g) && !(patch.groups ?? []).includes(g));
        const groups = books.length === 1 ? patch.groups : [...keep, ...(patch.groups ?? [])];
        await api.updateBook(b.id, { ...patch, groups });
      }
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
  const openBook = (b: Book) => (b.missing ? setToast(t("shelf.fileMissing")) : props.onOpen(b.id));

  return (
    <div className="shelf" onClick={() => (setMenu(null), setImportMenu(false))}>
      {settings.shelfSidebar && (
        <aside className="sidebar">
          <div className="brand">AiReader</div>
          {sideItem(ALL, "library", t("shelf.all"), shelf.books.length)}
          {sideItem(RECENT, "clock", t("shelf.recent"), count((b) => !!b.lastRead))}
          {sideItem(UNGROUPED, "inbox", t("shelf.ungrouped"), count((b) => b.groups.length === 0))}
          <div className="side-heading">
            <span>{t("shelf.groups")}</span>
            <button className="icon" title={t("shelf.newGroup")} aria-label={t("shelf.newGroup")} onClick={() => setGroupDialog({ old: null })}>
              <Icon name="plus" size={14} />
            </button>
          </div>
          {shelf.groups.map((g) => (
            <div key={g} className="side-group">
              {sideItem(g, "folder", g, count((b) => b.groups.includes(g)))}
              <span className="side-actions">
                <button className="icon" title={t("shelf.renameGroup")} onClick={() => setGroupDialog({ old: g })}>
                  <Icon name="edit" size={14} />
                </button>
                <button className="icon" title={t("shelf.deleteGroup")} onClick={() => setGroupDeleting(g)}>
                  <Icon name="close" size={14} />
                </button>
              </span>
            </div>
          ))}
          {shelf.groups.length === 0 && <div className="muted small side-empty">{t("shelf.groupsHint")}</div>}
          <div className="side-footer">
            <button
              className="side-item"
              onClick={ai.openSettings}
              title={`${t("ai.settings")}: ${ai.ready ? t("ai.configured") : t("ai.notConfigured")}`}
            >
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
        <header className="shelf-bar">
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
          <select
            value={settings.shelfSort}
            onChange={(e) => update({ shelfSort: e.target.value as ShelfSort })}
            disabled={filter === RECENT}
            title={t("shelf.sort")}
          >
            {SORTS.map((s) => (
              <option key={s.key} value={s.key}>
                {t(s.label)}
              </option>
            ))}
          </select>
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
            <div className="empty muted">{t("shelf.noMatch")}</div>
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
            <div className="book-list">
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
        </div>
      </main>

      {menu && (
        <div className="menu" style={{ left: menu.x, top: menu.y }} onClick={(e) => e.stopPropagation()}>
          <button onClick={() => (setMenu(null), props.onOpen(menu.book.id))} disabled={menu.book.missing}>
            {t("shelf.open")}
          </button>
          <button onClick={() => (setMenu(null), setEditing([menu.book]))}>{t("shelf.editInfo")}</button>
          <button onClick={() => (setMenu(null), toggle(menu.book.id))}>
            {selected.has(menu.book.id) ? t("shelf.clearSelection") : t("common.select")}
          </button>
          <hr />
          <button className="danger" onClick={() => (setMenu(null), setDeleting([menu.book]))}>
            {t("shelf.remove")}
          </button>
        </div>
      )}

      {editing && (
        <EditDialog
          books={editing}
          groups={shelf.groups}
          onSave={(patch, removed) => saveEdit(editing, patch, removed)}
          onClose={() => setEditing(null)}
        />
      )}
      {deleting && (
        <Confirm
          title={t("shelf.remove")}
          danger
          confirmText={t("common.delete")}
          message={
            <p>
              {deleting.length === 1
                ? t("shelf.removeOne", { title: deleting[0].title })
                : t("shelf.removeMany", { n: deleting.length })}
            </p>
          }
          onConfirm={() => remove(deleting)}
          onClose={() => setDeleting(null)}
        />
      )}
      {groupDialog && (
        <GroupDialog
          old={groupDialog.old}
          onClose={() => setGroupDialog(null)}
          onSave={async (name) => {
            try {
              if (groupDialog.old === null) await api.createGroup(name);
              else {
                await api.renameGroup(groupDialog.old, name);
                if (filter === groupDialog.old) setFilter(name.trim());
              }
              setGroupDialog(null);
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
          message={<p>{t("shelf.deleteGroupConfirm", { name: groupDeleting })}</p>}
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
            <div className="muted small ellipsis">
              {progress.total ? `${progress.done} / ${progress.total}　${progress.name}` : t("shelf.scanning")}
            </div>
          </div>
        </div>
      )}
      {dragging && (
        <div className="drop-overlay">
          {currentGroup ? t("shelf.dropToGroup", { group: currentGroup }) : t("shelf.dropToShelf")}
        </div>
      )}
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
      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}

function GroupDialog(props: { old: string | null; onSave: (name: string) => void; onClose: () => void }) {
  const [name, setName] = useState(props.old ?? "");
  return (
    <Modal
      title={props.old === null ? t("shelf.newGroup") : t("shelf.renameGroup")}
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
