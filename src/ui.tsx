// Small shared UI pieces: modal dialog, confirmation, toast.

import { useEffect, useRef, type ReactNode } from "react";
import { t } from "./i18n";
import { Icon } from "./icons";

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function Modal(props: { title: string; onClose: () => void; children: ReactNode; footer?: ReactNode; width?: number }) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && props.onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [props.onClose]);
  // Focus moves into the dialog and returns to where it was when the dialog closes.
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    const el = box.current;
    if (el && !el.contains(document.activeElement)) {
      const first = [...el.querySelectorAll<HTMLElement>(FOCUSABLE)].find((f) => !f.closest(".modal-title"));
      (first ?? el).focus();
    }
    return () => before?.focus?.();
  }, []);
  // Tab cycles within the dialog.
  const trap = (e: React.KeyboardEvent) => {
    if (e.key !== "Tab" || !box.current) return;
    const items = [...box.current.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((f) => f.offsetParent !== null);
    if (!items.length) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div
        ref={box}
        className="modal"
        style={{ width: props.width ?? 420 }}
        role="dialog"
        aria-modal="true"
        aria-label={props.title}
        tabIndex={-1}
        onKeyDown={trap}
      >
        <div className="modal-title">
          <span className="ellipsis">{props.title}</span>
          <button className="icon" aria-label={t("common.close")} title={t("common.close")} onClick={props.onClose}>
            <Icon name="close" />
          </button>
        </div>
        <div className="modal-body">{props.children}</div>
        {props.footer && <div className="modal-footer">{props.footer}</div>}
      </div>
    </div>
  );
}

export function Confirm(props: { title: string; message: ReactNode; confirmText: string; danger?: boolean; onConfirm: () => void; onClose: () => void }) {
  return (
    <Modal
      title={props.title}
      onClose={props.onClose}
      footer={
        <>
          <button onClick={props.onClose}>{t("common.cancel")}</button>
          <button
            className={props.danger ? "danger" : "primary"}
            autoFocus
            onClick={() => {
              props.onConfirm();
              props.onClose();
            }}
          >
            {props.confirmText}
          </button>
        </>
      }
    >
      {props.message}
    </Modal>
  );
}

export function Toast(props: { message: string; onDone: () => void }) {
  useEffect(() => {
    const t = window.setTimeout(props.onDone, 4000);
    return () => window.clearTimeout(t);
  }, [props.message, props.onDone]);
  return (
    <div className="toast" onClick={props.onDone}>
      {props.message}
    </div>
  );
}

/** Whether a key event comes from a text field (shortcuts should not fire then). */
export function isTyping(e: KeyboardEvent): boolean {
  const t = e.target;
  return t instanceof Element && !!t.closest("input, textarea, select");
}
