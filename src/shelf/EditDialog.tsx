import { useState } from "react";
import { Modal } from "../ui";
import { BTN, BTN_PRIMARY, FIELD, INPUT } from "../tw";
import type { Book, BookPatch } from "../types";
import { formatSize, t } from "../i18n";
import { childName, groupTree, labelOf } from "./groups";
import { Select } from "../Select";

// Values of the group picker besides group names.
const NONE = "\u0000none";
const KEEP = "\u0000keep";
const NEW = "\u0000new";

/**
 * Edits one book's title, author and group, or moves several books to one group. A book is in
 * at most one group.
 */
export function EditDialog(props: { books: Book[]; groups: string[]; onSave: (patch: BookPatch) => void; onClose: () => void }) {
  const single = props.books.length === 1 ? props.books[0] : null;
  const [title, setTitle] = useState(single?.title ?? "");
  const [author, setAuthor] = useState(single?.author ?? "");
  // Several books in different groups start at "keep"; otherwise at their common group.
  const [choice, setChoice] = useState(() => {
    const groups = new Set(props.books.map((b) => b.group ?? NONE));
    return groups.size === 1 ? [...groups][0] : KEEP;
  });
  const [newGroup, setNewGroup] = useState("");
  const [newParent, setNewParent] = useState("");
  const mixed = props.books.length > 1 && new Set(props.books.map((b) => b.group ?? NONE)).size > 1;

  const save = () => {
    const patch: BookPatch = {};
    if (choice === NEW) patch.group = childName(newParent || null, newGroup);
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
          <button className={BTN} onClick={props.onClose}>
            {t("common.cancel")}
          </button>
          <button className={BTN_PRIMARY} onClick={save} disabled={invalid}>
            {t("common.save")}
          </button>
        </>
      }
    >
      {single && (
        <>
          <label className={FIELD}>
            <span>{t("edit.bookTitle")}</span>
            <input className={INPUT} value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
          </label>
          <label className={FIELD}>
            <span>{t("edit.author")}</span>
            <input className={INPUT} value={author} onChange={(e) => setAuthor(e.target.value)} placeholder={t("edit.unknown")} />
          </label>
        </>
      )}
      <label className={FIELD}>
        <span>{t("edit.groups")}</span>
        <Select
          full
          value={choice}
          label={t("edit.groups")}
          options={[
            ...(mixed ? [{ value: KEEP, label: t("edit.keepGroup") }] : []),
            { value: NONE, label: t("shelf.ungrouped") },
            ...groupTree(props.groups).flatMap(({ name, children }) => [
              { value: name, label: name },
              ...children.map((child) => ({ value: child, label: labelOf(child, name), indent: true })),
            ]),
            { value: NEW, label: t("edit.newGroupOption") },
          ]}
          onChange={setChoice}
        />
        {choice === NEW && (
          <input
            className={INPUT}
            value={newGroup}
            autoFocus
            placeholder={t("edit.newGroup")}
            onChange={(e) => setNewGroup(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && !invalid && save()}
          />
        )}
        {choice === NEW && (
          <Select
            full
            value={newParent}
            label={t("shelf.parentGroup")}
            options={[{ value: "", label: t("shelf.topLevel") }, ...groupTree(props.groups).map(({ name }) => ({ value: name, label: name }))]}
            onChange={setNewParent}
          />
        )}
      </label>
      {single && (
        <div className="text-xs text-[var(--muted)]">
          {single.fileName} · {t("book.size", { size: formatSize(single.size) })}
        </div>
      )}
    </Modal>
  );
}
