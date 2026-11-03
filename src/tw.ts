// Class names for the controls used everywhere (daisyUI components, Tailwind utilities), so forms
// and toolbars look the same in every dialog and panel.

/** A labelled form field: a <label> whose first <span> is the label. */
export const FIELD = "mb-4 flex flex-col gap-1.5 [&>span:first-child]:text-[13px] [&>span:first-child]:font-semibold";
export const INPUT = "d-input d-input-sm w-full";
export const TEXTAREA = "d-textarea w-full text-[13px] leading-relaxed";
export const BTN = "d-btn d-btn-sm";
export const BTN_PRIMARY = "d-btn d-btn-sm d-btn-primary";
export const BTN_DANGER = "d-btn d-btn-sm d-btn-error";
export const BTN_GHOST = "d-btn d-btn-sm d-btn-ghost";
/** A square button with only an icon, quiet until pointed at. */
export const BTN_ICON = "d-btn d-btn-sm d-btn-ghost d-btn-square text-[var(--muted)] hover:text-[var(--fg)]";
/** A small explanatory paragraph under a control. */
export const HINT = "mt-1.5 text-xs leading-relaxed text-[var(--muted)]";
/** A titled block in a settings page or panel. */
export const SECTION = "mb-5 [&>h3]:mb-2 [&>h3]:text-[13px] [&>h3]:font-semibold";
