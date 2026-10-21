import { useState } from "react";
import { Modal } from "../ui";
import type { Book, BookPatch } from "../types";

/**
 * Edits one book's title/author/groups, or only the groups of several books. `onSave` gets the
 * checked groups and the groups that were unchecked; groups only some books had stay untouched.
 */
export function EditDialog(props: {
  books: Book[];
  groups: string[];
  onSave: (patch: BookPatch, removedGroups: string[]) => void;
  onClose: () => void;
}) {
  const single = props.books.length === 1 ? props.books[0] : null;
  const [title, setTitle] = useState(single?.title ?? "");
  const [author, setAuthor] = useState(single?.author ?? "");
  // For several books, a group is pre-checked only if every book is in it.
  const [initial] = useState(() => props.groups.filter((g) => props.books.every((b) => b.groups.includes(g))));
  const [checked, setChecked] = useState<Set<string>>(() => new Set(initial));
  const [newGroup, setNewGroup] = useState("");
  const allGroups = [...props.groups, ...[...checked].filter((g) => !props.groups.includes(g))];

  const toggle = (g: string) =>
    setChecked((s) => {
      const n = new Set(s);
      if (n.has(g)) n.delete(g);
      else n.add(g);
      return n;
    });

  const addGroup = () => {
    const g = newGroup.trim();
    if (g) setChecked((s) => new Set(s).add(g));
    setNewGroup("");
  };

  const save = () => {
    const patch: BookPatch = { groups: [...checked] };
    if (single) {
      patch.title = title;
      patch.author = author;
    }
    props.onSave(
      patch,
      initial.filter((g) => !checked.has(g)),
    );
  };

  return (
    <Modal
      title={single ? "编辑书籍信息" : `设置 ${props.books.length} 本书的分组`}
      onClose={props.onClose}
      footer={
        <>
          <button onClick={props.onClose}>取消</button>
          <button className="primary" onClick={save} disabled={!!single && !title.trim()}>
            保存
          </button>
        </>
      }
    >
      {single && (
        <>
          <label className="field">
            <span>书名</span>
            <input value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
          </label>
          <label className="field">
            <span>作者</span>
            <input value={author} onChange={(e) => setAuthor(e.target.value)} placeholder="未知" />
          </label>
        </>
      )}
      <div className="field">
        <span>分组</span>
        <div className="group-checks">
          {allGroups.length === 0 && <span className="muted">还没有分组</span>}
          {allGroups.map((g) => (
            <label key={g} className="chip-check">
              <input type="checkbox" checked={checked.has(g)} onChange={() => toggle(g)} />
              {g}
            </label>
          ))}
        </div>
        <div className="inline-add">
          <input
            value={newGroup}
            placeholder="新分组名称"
            onChange={(e) => setNewGroup(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addGroup()}
          />
          <button onClick={addGroup} disabled={!newGroup.trim()}>
            添加
          </button>
        </div>
      </div>
      {single && (
        <div className="muted small">
          {single.fileName} · {(single.size / 1024 / 1024).toFixed(2)} MB
        </div>
      )}
    </Modal>
  );
}
