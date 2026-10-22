// Floating toolbars for a text selection and for an existing highlight, and the note editor.

import { useState, type ReactNode } from "react";
import { Modal } from "../ui";
import type { HighlightColor } from "../types";

export const COLORS: { key: HighlightColor; name: string }[] = [
  { key: "yellow", name: "黄色" },
  { key: "green", name: "绿色" },
  { key: "blue", name: "蓝色" },
  { key: "pink", name: "粉色" },
  { key: "purple", name: "紫色" },
];

/** Positions a toolbar above `rect` (or below it near the top of the window). */
function placement(rect: DOMRect): React.CSSProperties {
  const width = 300;
  const left = Math.max(8, Math.min(window.innerWidth - width - 8, rect.left + rect.width / 2 - width / 2));
  return rect.top > 70 ? { left, top: rect.top - 50 } : { left, top: rect.bottom + 10 };
}

/** Buttons keep the text selection alive by not taking focus on mousedown. */
const keepSelection = (e: React.MouseEvent) => e.preventDefault();

export interface ToolbarAction {
  label: string;
  title?: string;
  run: () => void;
}

export function SelectionToolbar(props: {
  rect: DOMRect;
  current?: HighlightColor;
  onColor: (color: HighlightColor) => void;
  onNote: () => void;
  onCopy: () => void;
  onRemove?: () => void;
  /** More actions after the built-in ones (e.g. AI). */
  extra?: ToolbarAction[];
}) {
  return (
    <div className="float-toolbar" style={placement(props.rect)} onMouseDown={keepSelection} onClick={(e) => e.stopPropagation()}>
      {COLORS.map((c) => (
        <button
          key={c.key}
          className={`color-dot hl-${c.key}${props.current === c.key ? " on" : ""}`}
          title={`${c.name}高亮`}
          aria-label={`${c.name}高亮`}
          onClick={() => props.onColor(c.key)}
        />
      ))}
      <span className="sep" />
      <button onClick={props.onNote}>笔记</button>
      <button onClick={props.onCopy}>复制</button>
      {props.onRemove && (
        <button className="danger" onClick={props.onRemove}>
          删除
        </button>
      )}
      {props.extra?.map((a) => (
        <button key={a.label} title={a.title} onClick={a.run}>
          {a.label}
        </button>
      ))}
    </div>
  );
}

export function NoteEditor(props: { quote: string; note: string; onSave: (note: string) => void; onClose: () => void; footer?: ReactNode }) {
  const [text, setText] = useState(props.note);
  return (
    <Modal
      title="笔记"
      width={460}
      onClose={props.onClose}
      footer={
        <>
          <button onClick={props.onClose}>取消</button>
          <button
            className="primary"
            onClick={() => {
              props.onSave(text);
              props.onClose();
            }}
          >
            保存
          </button>
        </>
      }
    >
      <blockquote className="note-quote">{props.quote}</blockquote>
      <textarea
        className="note-text"
        autoFocus
        value={text}
        placeholder="写下你的想法…"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            props.onSave(text);
            props.onClose();
          }
        }}
      />
      <div className="muted small">⌘/Ctrl + Enter 保存</div>
    </Modal>
  );
}
