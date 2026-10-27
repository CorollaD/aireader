// Reader preferences and the colour theme derived from the chosen background.

import type { MessageKey } from "./i18n";

export type FontFamily = "sans" | "serif" | "kai";
export type ShelfSort = "recent" | "added" | "title" | "author" | "progress";

export interface Settings {
  background: string;
  /** `null` picks a readable colour automatically. */
  textColor: string | null;
  fontFamily: FontFamily;
  fontSize: number;
  lineHeight: number;
  maxWidth: number;
  pageLayout: "continuous" | "single";
  /** How paged books are scaled before zoom. */
  pageFit: "width" | "page";
  /** Reflowable books: vertical scrolling or horizontal pages. */
  flowMode: "scroll" | "paged";
  columns: 1 | 2;
  /** Space after paragraphs, in em. */
  paragraphSpacing: number;
  indent: boolean;
  justify: boolean;
  /** Horizontal page margin in px. */
  margin: number;
  showToc: boolean;
  shelfView: "grid" | "list";
  shelfSort: ShelfSort;
  /** "auto" follows the system language. */
  uiLanguage: string;
  /** Interface zoom (one of UI_SCALES). */
  uiScale: number;
  shelfSidebar: boolean;
}

export const DEFAULT_SETTINGS: Settings = {
  background: "#faf7ef",
  textColor: null,
  fontFamily: "sans",
  fontSize: 19,
  lineHeight: 1.8,
  maxWidth: 760,
  pageLayout: "continuous",
  pageFit: "width",
  flowMode: "scroll",
  columns: 1,
  paragraphSpacing: 0.75,
  indent: false,
  justify: true,
  margin: 32,
  showToc: true,
  shelfView: "grid",
  shelfSort: "recent",
  uiLanguage: "auto",
  uiScale: 1,
  shelfSidebar: true,
};

export const UI_SCALES: { value: number; label: MessageKey }[] = [
  { value: 0.9, label: "settings.scaleSmall" },
  { value: 1, label: "settings.scaleNormal" },
  { value: 1.15, label: "settings.scaleLarge" },
  { value: 1.3, label: "settings.scaleLarger" },
];

export const BACKGROUNDS: { name: MessageKey; color: string }[] = [
  { name: "bg.white", color: "#ffffff" },
  { name: "bg.cream", color: "#faf7ef" },
  { name: "bg.parchment", color: "#f4ecd8" },
  { name: "bg.green", color: "#cce8cf" },
  { name: "bg.mint", color: "#e2f1e8" },
  { name: "bg.sky", color: "#dfeaf5" },
  { name: "bg.blossom", color: "#f8e5e8" },
  { name: "bg.lightGray", color: "#e6e6e6" },
  { name: "bg.darkGray", color: "#3a3b3e" },
  { name: "bg.night", color: "#1f2023" },
  { name: "bg.navy", color: "#17212f" },
  { name: "bg.black", color: "#000000" },
];

export const FONT_STACKS: Record<FontFamily, { name: MessageKey; css: string }> = {
  sans: {
    name: "font.sans",
    css: '-apple-system, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Noto Sans CJK SC", sans-serif',
  },
  serif: {
    name: "font.serif",
    css: '"Songti SC", STSong, "Noto Serif CJK SC", "Source Han Serif SC", SimSun, Georgia, serif',
  },
  kai: { name: "font.kai", css: '"Kaiti SC", STKaiti, KaiTi, "AR PL UKai CN", serif' },
};

export function normalizeSettings(raw: unknown): Settings {
  const s = { ...DEFAULT_SETTINGS, ...(raw && typeof raw === "object" ? (raw as Partial<Settings>) : {}) };
  if (!/^#[0-9a-f]{6}$/i.test(s.background)) s.background = DEFAULT_SETTINGS.background;
  if (s.textColor !== null && !/^#[0-9a-f]{6}$/i.test(s.textColor)) s.textColor = null;
  if (!(s.fontFamily in FONT_STACKS)) s.fontFamily = DEFAULT_SETTINGS.fontFamily;
  if (!UI_SCALES.some((u) => u.value === s.uiScale)) s.uiScale = DEFAULT_SETTINGS.uiScale;
  return s;
}

function rgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function hex([r, g, b]: number[]): string {
  return (
    "#" +
    [r, g, b]
      .map((v) =>
        Math.round(Math.max(0, Math.min(255, v)))
          .toString(16)
          .padStart(2, "0"),
      )
      .join("")
  );
}

/** Mixes `a` towards `b` by `t` (0..1). */
function mix(a: string, b: string, t: number): string {
  const [x, y] = [rgb(a), rgb(b)];
  return hex(x.map((v, i) => v + (y[i] - v) * t));
}

/** WCAG relative luminance. */
function luminance(color: string): number {
  const [r, g, b] = rgb(color).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export const isDark = (bg: string) => luminance(bg) < 0.25;

/** CSS variables for the whole app, derived from the reading background. */
export function themeVars(s: Settings): Record<string, string> {
  const bg = s.background;
  const dark = isDark(bg);
  // Text is the background pushed strongly towards black/white, which keeps a hint of its hue.
  const fg = s.textColor ?? mix(bg, dark ? "#ffffff" : "#000000", dark ? 0.8 : 0.85);
  const toward = dark ? "#ffffff" : "#000000";
  return {
    "--bg": bg,
    "--fg": fg,
    "--muted": mix(fg, bg, 0.45),
    "--panel": mix(bg, toward, dark ? 0.06 : 0.035),
    "--hover": mix(bg, toward, dark ? 0.12 : 0.07),
    "--border": mix(bg, toward, dark ? 0.18 : 0.12),
    "--accent": dark ? "#6ea8fe" : "#2f6fde",
    "--accent-soft": dark ? "rgba(110,168,254,0.22)" : "rgba(47,111,222,0.14)",
    "--danger": dark ? "#ff8a80" : "#c62828",
    "--ok": dark ? "#81c784" : "#2e7d32",
    "--shadow": dark ? "rgba(0,0,0,0.5)" : "rgba(0,0,0,0.12)",
    "--reader-font": FONT_STACKS[s.fontFamily].css,
    "color-scheme": dark ? "dark" : "light",
  };
}
