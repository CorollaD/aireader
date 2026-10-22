import { useState } from "react";
import { Modal } from "../ui";
import type { Book, BookPatch } from "../types";
import { t } from "../i18n";

// Values of the group picker besides group names.
const NONE = "\u0000none";
const KEEP = "\u0000keep";
const NEW = "\u0000new";

/**
 * Edits one book's title, author and group, or moves several books to one group. A book is in
 * at most one group.
 */
export function EditDialog(props: {
  books: Book[];
  groups: string[];
  onSave: (patch: BookPatch) => void;
  onClose: () => void;
}) {
  const single = props.books.length === 1 ? props.books[0] : null;
  const [title, setTitle] = useState(single?.title ?? "");
  const [author, setAuthor] = useState(single?.author ?? "");
  // Several books in different groups start at "keep"; otherwise at their common group.
  const [choice, setChoice] = useState(() => {
    const groups = new Set(props.books.map((b) => b.group ?? NONE));
    return groups.size === 1 ? [...groups][0] : KEEP;
  });
  const [newGroup, setNewGroup] = useState("");
  const mixed = props.books.length > 1 && new Set(props.books.map((b) => b.group ?? NONE)).size > 1;

  const save = () => {
    const patch: BookPatch = {};
    if (choice === NEW) patch.group = newGroup.trim();
    else if (choice !== KEEP) patch.group = choice === NONE ? null : choice;
    if (single) {
      patch.title = title;
      patch.author = author;
    }
    props.onSave(patch);
  };

  const invalid = (!!single && !title.trim()) || (choice === NEW && !newGroup.trim());

  return (
    <Modal
      title={single ? t("edit.titleOne") : t("edit.titleMany", { n: props.books.length })}
      onClose={props.onClose}
      footer={
        <>
          <button onClick={props.onClose}>{t("common.cancel")}</button>
          <button className="primary" onClick={save} disabled={invalid}>
            {t("common.save")}
          </button>
        </>
      }
    >
      {single && (
        <>
          <label className="field">
            <span>{t("edit.bookTitle")}</span>
            <input value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
          </label>
          <label className="field">
            <span>{t("edit.author")}</span>
            <input value={author} onChange={(e) => setAuthor(e.target.value)} placeholder={t("edit.unknown")} />
          </label>
        </>
      )}
      <label className="field">
        <span>{t("edit.groups")}</span>
        <select value={choice} onChange={(e) => setChoice(e.target.value)} autoFocus={!single}>
          {mixed && <option value={KEEP}>{t("edit.keepGroup")}</option>}
          <option value={NONE}>{t("shelf.ungrouped")}</option>
          {props.groups.map((g) => (
            <option key={g} value={g}>
              {g}
            </option>
          ))}
          <option value={NEW}>{t("edit.newGroupOption")}</option>
        </select>
        {choice === NEW && (
          <input
            className="inline-add"
            value={newGroup}
            autoFocus
            placeholder={t("edit.newGroup")}
            onChange={(e) => setNewGroup(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && !invalid && save()}
          />
        )}
      </label>
      {single && (
        <div className="muted small">
          {single.fileName} · {(single.size / 1024 / 1024).toFixed(2)} MB
        </div>
      )}
    </Modal>
  );
}
