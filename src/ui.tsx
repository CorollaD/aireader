// Small shared UI pieces: modal dialog, confirmation, toast, menus.

import { useEffect, useRef, type CSSProperties, type ReactNode } from "react";
import { t } from "./i18n";
import { Icon, type IconName } from "./icons";
import { mobile } from "./platform";

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
      const first = [...el.querySelectorAll<HTMLElement>(FOCUSABLE)].find((f) => !f.closest("[data-modal-title]"));
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
    <div className={`d-modal d-modal-open${mobile ? " d-modal-bottom" : ""}`} onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div
        ref={box}
        className="d-modal-box flex max-h-[calc(100vh-48px)] flex-col gap-0 p-0 shadow-2xl ring-1 ring-[var(--line)] max-sm:max-w-full max-sm:pb-[env(safe-area-inset-bottom)]"
        style={{ width: props.width ?? 420, maxWidth: mobile ? "100%" : "calc(100vw - 40px)" }}
        role="dialog"
        aria-modal="true"
        aria-label={props.title}
        tabIndex={-1}
        onKeyDown={trap}
      >
        <div data-modal-title className="flex items-center justify-between gap-3 px-6 pt-5 pb-3">
          <h2 className="truncate text-[17px] font-semibold tracking-tight">{props.title}</h2>
          <button className="d-btn d-btn-ghost d-btn-sm d-btn-circle -mr-2" aria-label={t("common.close")} title={t("common.close")} onClick={props.onClose}>
            <Icon name="close" />
          </button>
        </div>
        <div className="modal-body min-h-0 overflow-y-auto px-6 pb-5">{props.children}</div>
        {props.footer && <div className="flex justify-end gap-2 border-t border-[var(--line)] bg-[var(--panel)] px-6 py-3.5">{props.footer}</div>}
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
          <button className="d-btn d-btn-sm" onClick={props.onClose}>
            {t("common.cancel")}
          </button>
          <button
            className={`d-btn d-btn-sm ${props.danger ? "d-btn-error" : "d-btn-primary"}`}
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
  return <Notice message={props.message} onClick={props.onDone} />;
}

/** A short message floating at the bottom of the window. */
export function Notice(props: { message: string; onClick?: () => void }) {
  return (
    <div className="d-toast d-toast-center z-[80] mb-[env(safe-area-inset-bottom)]" role="status">
      <div className="d-alert max-w-md border-[var(--line)] bg-[var(--fg)] py-2.5 text-[13px] text-[var(--bg)] shadow-xl" onClick={props.onClick}>
        {props.message}
      </div>
    </div>
  );
}

/** The look shared by menus: a frosted panel of rows. */
const MENU =
  "d-menu d-menu-sm z-[75] min-w-52 gap-px rounded-xl border border-[var(--line)] bg-[var(--glass)] p-1.5 text-[13px] shadow-xl backdrop-blur-xl backdrop-saturate-150";

/** A menu (dropdown or context menu) of `MenuItem`s. */
export function Menu(props: { children: ReactNode; className?: string; style?: CSSProperties; onClick?: (e: React.MouseEvent) => void }) {
  return (
    <ul role="menu" className={`${MENU} ${props.className ?? ""}`} style={props.style} onClick={props.onClick}>
      {props.children}
    </ul>
  );
}

export function MenuItem(props: {
  icon?: IconName;
  danger?: boolean;
  disabled?: boolean;
  /** A choice that is on, marked with a tick. */
  checked?: boolean;
  /** Its keyboard shortcut, shown at the end. */
  kbd?: string;
  className?: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <li className={`${props.disabled ? "d-menu-disabled" : ""} ${props.className ?? ""}`}>
      <button
        role={props.checked === undefined ? "menuitem" : "menuitemcheckbox"}
        aria-checked={props.checked}
        disabled={props.disabled}
        className={`tw gap-2.5 rounded-lg py-1.5${props.danger ? " text-[var(--danger)]" : ""}${props.checked ? " font-semibold [&>svg]:text-[var(--accent)]" : ""}`}
        onClick={props.onClick}
      >
        {props.icon && <Icon name={props.icon} />}
        <span className="flex-1">{props.children}</span>
        {props.checked && <Icon name="check" size={14} />}
        {props.kbd && <kbd className="ml-3">{props.kbd}</kbd>}
      </button>
    </li>
  );
}

export function MenuDivider(props: { className?: string }) {
  return <li role="separator" className={`pointer-events-none mx-2 my-1 h-px bg-[var(--line)] ${props.className ?? ""}`} />;
}

/** Whether a key event comes from a text field (shortcuts should not fire then). */
export function isTyping(e: KeyboardEvent): boolean {
  const t = e.target;
  return t instanceof Element && !!t.closest("input, textarea, select");
}
