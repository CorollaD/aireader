// Keyboard shortcuts shown in the help dialog. Keep in sync with the key handlers in
// Reader.tsx, FlowView.tsx, PagedView.tsx and App.tsx.

import { t } from "../i18n";

const mac = navigator.platform.toLowerCase().includes("mac");
export const MOD = mac ? "⌘" : "Ctrl";

/** Built on each call so the descriptions follow the interface language. */
export function shortcuts(): { group: string; items: [string, string][] }[] {
  const space = t("sc.space");
  return [
    {
      group: t("sc.groupMove"),
      items: [
        ["← / →", t("sc.prevNext")],
        [`${space} / PgDn`, t("sc.pageForward")],
        [`Shift+${space} / PgUp`, t("sc.pageBack")],
        ["↑ / ↓", t("sc.scroll")],
        ["Home / End", t("sc.homeEnd")],
        [mac ? "⌥← / ⌥→" : "Alt+← / Alt+→", t("sc.history")],
      ],
    },
    {
      group: t("sc.groupNotes"),
      items: [
        [`${MOD}+D`, t("sc.bookmark")],
        [t("sc.selectText"), t("sc.selectTextDesc")],
      ],
    },
    {
      group: t("sc.groupView"),
      items: [
        [`${MOD} + / ${MOD} -`, t("sc.fontSize")],
        [`${MOD}+F`, t("sc.search")],
        ["T", t("sc.sidebar")],
        [`${MOD}+Shift+A`, t("sc.aiPanel")],
        ["F", t("sc.focus")],
        [mac ? "⌃⌘F / F11" : "F11", t("sc.fullscreen")],
        ["?", t("sc.help")],
        ["Esc", t("sc.escape")],
      ],
    },
    {
      group: t("sc.groupApp"),
      items: [
        [`${MOD}+,`, t("sc.settings")],
        [`${MOD} + / ${MOD} -`, t("sc.uiScale")],
        [`${MOD}+0`, t("sc.uiScaleReset")],
      ],
    },
  ];
}
