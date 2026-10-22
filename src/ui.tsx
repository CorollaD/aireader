// Small shared UI pieces: modal dialog, confirmation, toast.

import { useEffect, type ReactNode } from "react";
import { t } from "./i18n";
import { Icon } from "./icons";

export function Modal(props: { title: string; onClose: () => void; children: ReactNode; footer?: ReactNode; width?: number }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && props.onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [props.onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div className="modal" style={{ width: props.width ?? 420 }} role="dialog" aria-label={props.title}>
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

export function Confirm(props: {
  title: string;
  message: ReactNode;
  confirmText: string;
  danger?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
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
