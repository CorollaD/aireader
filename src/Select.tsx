// A dropdown that matches the app, built on Radix (keyboard use and screen readers included).

import { useState } from "react";
import * as Radix from "@radix-ui/react-select";
import { Icon } from "./icons";

/** Radix keeps the empty string for "nothing chosen", and forms here use it as a real choice. */
const EMPTY = "\u0000empty";
const wrap = (value: string) => value || EMPTY;
const unwrap = (value: string) => (value === EMPTY ? "" : value);

export interface SelectOption<T extends string> {
  value: T;
  label: string;
  /** A subgroup, shown under the option before it. */
  indent?: boolean;
}

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
  const classes = ["select-trigger", props.ghost && "ghost", props.full && "full", props.className].filter(Boolean).join(" ");
  // The menu opens outside the page's layout, so inside a darkened reader it has to stay in there.
  const [host, setHost] = useState<HTMLElement | null>(null);
  return (
    <Radix.Root value={wrap(props.value)} onValueChange={(v) => props.onChange(unwrap(v) as T)} disabled={props.disabled}>
      <Radix.Trigger
        ref={(el) => setHost(el?.closest<HTMLElement>(".reader[data-theme]") ?? null)}
        className={classes}
        aria-label={props.label}
        title={props.label}
      >
        <Radix.Value />
        <Radix.Icon className="select-chevron">
          <Icon name="chevronDown" size={13} />
        </Radix.Icon>
      </Radix.Trigger>
      <Radix.Portal container={host ?? undefined}>
        <Radix.Content className="select-menu" position="popper" sideOffset={4} collisionPadding={8}>
          <Radix.Viewport>
            {props.options.map((o) => (
              <Radix.Item key={wrap(o.value)} value={wrap(o.value)} className={`select-item${o.indent ? " indent" : ""}`}>
                <Radix.ItemText>{o.label}</Radix.ItemText>
                <Radix.ItemIndicator className="select-check">
                  <Icon name="check" size={13} />
                </Radix.ItemIndicator>
              </Radix.Item>
            ))}
          </Radix.Viewport>
        </Radix.Content>
      </Radix.Portal>
    </Radix.Root>
  );
}
