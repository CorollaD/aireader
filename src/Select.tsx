// A dropdown: the system's own <select>, styled by daisyUI. Its menu is the operating system's,
// with keyboard use and screen readers as they are everywhere else.

export interface SelectOption<T extends string> {
  value: T;
  label: string;
  /** A subgroup, shown under the option before it. */
  indent?: boolean;
}

/** System menus show options as plain text, so a subgroup is set in with spaces. */
const INDENT = "    ";

export function Select<T extends string>(props: {
  value: T;
  options: SelectOption<T>[];
  onChange: (value: T) => void;
  label: string;
  disabled?: boolean;
  /** Toolbars use the borderless form. */
  ghost?: boolean;
  /** Fills the width of its form field. */
  full?: boolean;
  className?: string;
}) {
  const classes = ["d-select d-select-sm", props.ghost ? "d-select-ghost" : "", props.full ? "w-full" : "w-auto", props.className].filter(Boolean).join(" ");
  return (
    <select
      className={classes}
      value={props.value}
      aria-label={props.label}
      title={props.label}
      disabled={props.disabled}
      onChange={(e) => props.onChange(e.target.value as T)}
    >
      {props.options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.indent ? INDENT + o.label : o.label}
        </option>
      ))}
    </select>
  );
}
