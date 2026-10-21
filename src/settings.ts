// Reader preferences and the colour theme derived from the chosen background.

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
  showToc: boolean;
  shelfView: "grid" | "list";
  shelfSort: ShelfSort;
}

export const DEFAULT_SETTINGS: Settings = {
  background: "#faf7ef",
  textColor: null,
  fontFamily: "sans",
  fontSize: 19,
  lineHeight: 1.8,
  maxWidth: 760,
  pageLayout: "continuous",
  showToc: true,
  shelfView: "grid",
  shelfSort: "recent",
};

export const BACKGROUNDS: { name: string; color: string }[] = [
  { name: "纯白", color: "#ffffff" },
  { name: "米白", color: "#faf7ef" },
  { name: "羊皮纸", color: "#f4ecd8" },
  { name: "护眼绿", color: "#cce8cf" },
  { name: "薄荷", color: "#e2f1e8" },
  { name: "天青", color: "#dfeaf5" },
  { name: "樱粉", color: "#f8e5e8" },
  { name: "浅灰", color: "#e6e6e6" },
  { name: "深灰", color: "#3a3b3e" },
  { name: "夜间", color: "#1f2023" },
  { name: "深蓝", color: "#17212f" },
  { name: "墨黑", color: "#000000" },
];

export const FONT_STACKS: Record<FontFamily, { name: string; css: string }> = {
  sans: {
    name: "黑体",
    css: '-apple-system, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Noto Sans CJK SC", sans-serif',
  },
  serif: {
    name: "宋体",
    css: '"Songti SC", STSong, "Noto Serif CJK SC", "Source Han Serif SC", SimSun, Georgia, serif',
  },
  kai: { name: "楷体", css: '"Kaiti SC", STKaiti, KaiTi, "AR PL UKai CN", serif' },
};

export function normalizeSettings(raw: unknown): Settings {
  const s = { ...DEFAULT_SETTINGS, ...(raw && typeof raw === "object" ? (raw as Partial<Settings>) : {}) };
  if (!/^#[0-9a-f]{6}$/i.test(s.background)) s.background = DEFAULT_SETTINGS.background;
  if (s.textColor !== null && !/^#[0-9a-f]{6}$/i.test(s.textColor)) s.textColor = null;
  if (!(s.fontFamily in FONT_STACKS)) s.fontFamily = DEFAULT_SETTINGS.fontFamily;
  return s;
}

function rgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function hex([r, g, b]: number[]): string {
  return "#" + [r, g, b].map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0")).join("");
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
    "--shadow": dark ? "rgba(0,0,0,0.5)" : "rgba(0,0,0,0.12)",
    "--reader-font": FONT_STACKS[s.fontFamily].css,
    "color-scheme": dark ? "dark" : "light",
  };
}
