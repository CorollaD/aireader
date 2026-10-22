// Floating toolbars for a text selection and for an existing highlight, and the note editor.

import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Modal } from "../ui";
import type { HighlightColor } from "../types";
import { t } from "../i18n";
import { MOD } from "./shortcuts";
import { Icon, type IconName } from "../icons";

export const COLORS: HighlightColor[] = ["yellow", "green", "blue", "pink", "purple"];

/** Positions a toolbar above `rect` (or below it near the top of the window). */
function placement(rect: DOMRect, width: number): React.CSSProperties {
  const left = Math.max(8, Math.min(window.innerWidth - width - 8, rect.left + rect.width / 2 - width / 2));
  return rect.top > 70 ? { left, top: rect.top - 50 } : { left, top: rect.bottom + 10 };
}

/** Buttons keep the text selection alive by not taking focus on mousedown. */
const keepSelection = (e: React.MouseEvent) => e.preventDefault();

export interface ToolbarAction {
  label: string;
  title?: string;
  icon?: IconName;
  run: () => void;
}

export function SelectionToolbar(props: {
  rect: DOMRect;
  current?: HighlightColor;
  /** Without these (PDF text), the toolbar offers only copy and extra actions. */
  onColor?: (color: HighlightColor) => void;
  onNote?: () => void;
  onCopy: () => void;
  onRemove?: () => void;
  /** More actions after the built-in ones (e.g. AI). */
  extra?: ToolbarAction[];
}) {
  const el = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(300);
  // Centre on the selection using the toolbar's real width (it depends on the buttons shown).
  useLayoutEffect(() => {
    if (el.current) setWidth(el.current.offsetWidth);
  }, [props.extra?.length, !!props.onColor, !!props.onRemove]);
  return (
    <div ref={el} className="float-toolbar" style={placement(props.rect, width)} onMouseDown={keepSelection} onClick={(e) => e.stopPropagation()}>
      {props.onColor &&
        COLORS.map((c) => (
          <button
            key={c}
            className={`color-dot hl-${c}${props.current === c ? " on" : ""}`}
            title={t("annotate.highlight", { color: t(`color.${c}`) })}
            aria-label={t("annotate.highlight", { color: t(`color.${c}`) })}
            onClick={() => props.onColor?.(c)}
          />
        ))}
      {props.onColor && <span className="sep" />}
      {props.onNote && <button onClick={props.onNote}>{t("annotate.note")}</button>}
      <button onClick={props.onCopy}>{t("common.copy")}</button>
      {props.onRemove && (
        <button className="danger" onClick={props.onRemove}>
          {t("common.delete")}
        </button>
      )}
      {props.extra && props.extra.length > 0 && (
        <span className="toolbar-ai">
          {props.extra.map((a) => (
            <button key={a.label} title={a.title} onClick={a.run}>
              {a.icon && <Icon name={a.icon} size={14} />}
              {a.label}
            </button>
          ))}
        </span>
      )}
    </div>
  );
}

export function NoteEditor(props: { quote: string; note: string; onSave: (note: string) => void; onClose: () => void; footer?: ReactNode }) {
  const [text, setText] = useState(props.note);
  return (
    <Modal
      title={t("annotate.note")}
      width={460}
      onClose={props.onClose}
      footer={
        <>
          <button onClick={props.onClose}>{t("common.cancel")}</button>
          <button
            className="primary"
            onClick={() => {
              props.onSave(text);
              props.onClose();
            }}
          >
            {t("common.save")}
          </button>
        </>
      }
    >
      <blockquote className="note-quote">{props.quote}</blockquote>
      <textarea
        className="note-text"
        autoFocus
        value={text}
        placeholder={t("annotate.notePlaceholder")}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            props.onSave(text);
            props.onClose();
          }
        }}
      />
      <div className="muted small">{t("annotate.saveHint", { key: MOD })}</div>
    </Modal>
  );
}
