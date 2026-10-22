import { useCallback, useEffect, useMemo, useState, type MouseEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, errorText } from "../api";
import type { UpdateSettings } from "../App";
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

const SORTS: { key: ShelfSort; label: string }[] = [
  { key: "recent", label: "最近阅读" },
  { key: "added", label: "添加时间" },
  { key: "title", label: "书名" },
  { key: "author", label: "作者" },
  { key: "progress", label: "阅读进度" },
];

function sortBooks(books: Book[], sort: ShelfSort): Book[] {
  const byText = (a: string, b: string) => a.localeCompare(b, "zh-Hans-CN");
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
  const ai = useAi();

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
        const parts = [`已导入 ${r.added.length} 本`];
        if (r.duplicates.length) parts.push(`${r.duplicates.length} 本已在书架中`);
        if (r.failed.length) parts.push(`${r.failed.length} 本失败（${r.failed.map(([n]) => n).slice(0, 3).join("、")}）`);
        setToast(parts.join("，"));
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
    const r = await openDialog({ multiple: true, filters: [{ name: "电子书与文档", extensions: EXTENSIONS }] });
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
      setToast(`已删除 ${books.length} 本书`);
    } catch (e) {
      fail(e);
    }
  };

  const count = (pred: (b: Book) => boolean) => shelf.books.filter(pred).length;
  const sideItem = (key: string, label: string, n: number) => (
    <button key={key} className={`side-item${filter === key ? " active" : ""}`} onClick={() => setFilter(key)}>
      <span className="ellipsis">{label}</span>
      <span className="count">{n}</span>
    </button>
  );

  return (
    <div className="shelf" onClick={() => setMenu(null)}>
      <aside className="sidebar">
        <div className="brand">AiReader</div>
        {sideItem(ALL, "全部书籍", shelf.books.length)}
        {sideItem(RECENT, "最近阅读", count((b) => !!b.lastRead))}
        {sideItem(UNGROUPED, "未分组", count((b) => b.groups.length === 0))}
        <div className="side-heading">
          <span>分组</span>
          <button className="icon" title="新建分组" onClick={() => setGroupDialog({ old: null })}>
            ＋
          </button>
        </div>
        {shelf.groups.map((g) => (
          <div key={g} className="side-group">
            {sideItem(g, g, count((b) => b.groups.includes(g)))}
            <span className="side-actions">
              <button className="icon" title="重命名" onClick={() => setGroupDialog({ old: g })}>
                ✎
              </button>
              <button className="icon" title="删除分组" onClick={() => setGroupDeleting(g)}>
                ×
              </button>
            </span>
          </div>
        ))}
        {shelf.groups.length === 0 && <div className="muted small side-empty">用分组整理你的书</div>}
        <div className="side-footer">
          <button className="side-item" onClick={ai.openSettings}>
            <span>AI 设置</span>
            <span className="count">{ai.ready ? "已配置" : "未配置"}</span>
          </button>
        </div>
      </aside>

      <main className="shelf-main">
        <header className="shelf-bar">
          <input className="search" placeholder="搜索书名或作者" value={query} onChange={(e) => setQuery(e.target.value)} />
          <select
            value={settings.shelfSort}
            onChange={(e) => update({ shelfSort: e.target.value as ShelfSort })}
            disabled={filter === RECENT}
            title="排序"
          >
            {SORTS.map((s) => (
              <option key={s.key} value={s.key}>
                按{s.label}
              </option>
            ))}
          </select>
          <div className="segmented">
            <button className={settings.shelfView === "grid" ? "on" : ""} onClick={() => update({ shelfView: "grid" })}>
              网格
            </button>
            <button className={settings.shelfView === "list" ? "on" : ""} onClick={() => update({ shelfView: "list" })}>
              列表
            </button>
          </div>
          <span className="spacer" />
          {shelf.books.length > 0 && (
            <button
              onClick={() => (ai.ready ? setOrganizing(selecting ? selectedBooks : visible) : ai.openSettings())}
              title={selecting ? "为选中的书建议分组" : "为当前列表中的书建议分组"}
            >
              AI 整理
            </button>
          )}
          <button onClick={pickFolder}>导入文件夹</button>
          <button className="primary" onClick={pickFiles}>
            导入书籍
          </button>
        </header>

        {selecting && (
          <div className="select-bar">
            <span>已选择 {selected.size} 本</span>
            <button onClick={() => setSelected(new Set(visible.map((b) => b.id)))}>全选</button>
            <button onClick={() => setEditing(selectedBooks)}>设置分组</button>
            <button className="danger" onClick={() => setDeleting(selectedBooks)}>
              删除
            </button>
            <span className="spacer" />
            <button onClick={() => setSelected(new Set())}>取消选择</button>
          </div>
        )}

        <div className="shelf-content">
          {shelf.books.length === 0 ? (
            <div className="empty">
              <div className="empty-title">书架还是空的</div>
              <div className="muted">支持 EPUB · PDF · TXT · Markdown · MOBI/AZW3 · FB2 · DOCX · CBZ</div>
              <div className="empty-actions">
                <button className="primary" onClick={pickFiles}>
                  导入书籍
                </button>
                <button onClick={pickFolder}>导入文件夹</button>
              </div>
              <div className="muted small">也可以把文件或文件夹拖到窗口中</div>
            </div>
          ) : visible.length === 0 ? (
            <div className="empty muted">没有符合条件的书</div>
          ) : settings.shelfView === "grid" ? (
            <div className="book-grid">
              {visible.map((b) => (
                <BookCard
                  key={b.id}
                  book={b}
                  selected={selected.has(b.id)}
                  selecting={selecting}
                  onOpen={() => (b.missing ? setToast("书籍文件已丢失，请删除后重新导入") : props.onOpen(b.id))}
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
                <span className="row-main">书名</span>
                <span className="row-cell">格式</span>
                <span className="row-cell">进度</span>
                <span className="row-cell">阅读时长</span>
                <span className="row-cell">最近阅读</span>
                <span className="row-cell">添加时间</span>
                <span className="more-space" />
              </div>
              {visible.map((b) => (
                <BookRow
                  key={b.id}
                  book={b}
                  selected={selected.has(b.id)}
                  selecting={selecting}
                  onOpen={() => (b.missing ? setToast("书籍文件已丢失，请删除后重新导入") : props.onOpen(b.id))}
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
            打开
          </button>
          <button onClick={() => (setMenu(null), setEditing([menu.book]))}>编辑信息与分组</button>
          <button onClick={() => (setMenu(null), toggle(menu.book.id))}>{selected.has(menu.book.id) ? "取消选择" : "选择"}</button>
          <hr />
          <button className="danger" onClick={() => (setMenu(null), setDeleting([menu.book]))}>
            从书架删除
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
          title="从书架删除"
          danger
          confirmText="删除"
          message={
            <p>
              将删除{deleting.length === 1 ? `《${deleting[0].title}》` : ` ${deleting.length} 本书`}
              及其阅读进度。书库中保存的副本会一并删除，你导入时选择的原文件不受影响。
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
          title="删除分组"
          danger
          confirmText="删除分组"
          message={<p>删除分组「{groupDeleting}」？分组里的书会保留在书架上。</p>}
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
            <div className="modal-title">正在导入…</div>
            <div className="bar">
              <div style={{ width: progress.total ? `${(progress.done / progress.total) * 100}%` : "0%" }} />
            </div>
            <div className="muted small ellipsis">
              {progress.total ? `${progress.done} / ${progress.total}　${progress.name}` : "正在扫描文件…"}
            </div>
          </div>
        </div>
      )}
      {dragging && <div className="drop-overlay">松开以导入到{currentGroup ? `「${currentGroup}」` : "书架"}</div>}
      {organizing && (
        <OrganizeDialog
          books={organizing}
          onClose={() => setOrganizing(null)}
          onApply={async (changes) => {
            for (const c of changes) await api.updateBook(c.id, c.patch);
            refresh();
            setToast(`已更新 ${changes.length} 本书`);
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
      title={props.old === null ? "新建分组" : "重命名分组"}
      onClose={props.onClose}
      width={340}
      footer={
        <>
          <button onClick={props.onClose}>取消</button>
          <button className="primary" disabled={!name.trim()} onClick={() => props.onSave(name)}>
            确定
          </button>
        </>
      }
    >
      <input
        className="full"
        value={name}
        autoFocus
        placeholder="分组名称"
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && name.trim() && props.onSave(name)}
      />
    </Modal>
  );
}
