// Reader preferences and the colour theme derived from the chosen background.

import type { MessageKey } from "./i18n";

export type FontFamily = "sans" | "serif" | "kai";
export const FONT_WEIGHTS = [400, 500, 600] as const;
export type FontWeight = (typeof FONT_WEIGHTS)[number];
/** The interface's colours: following the system's light or dark, or one theme (styles.css). */
export const UI_THEMES = ["system", "light", "warm", "dark", "slate", "oled"] as const;
export type UiTheme = (typeof UI_THEMES)[number];
const DARK_THEMES: string[] = ["dark", "slate", "oled"];
/** Colours for buttons, links and selection; each has a light- and a dark-theme shade in styles.css. */
export const ACCENTS = ["red", "orange", "amber", "green", "teal", "blue", "indigo", "purple", "pink", "graphite"] as const;
/** Accents of earlier versions, by the one now closest to them. */
const OLD_ACCENTS: Record<string, Accent> = { rose: "pink", cherry: "red", violet: "purple", mint: "green", navy: "blue" };
export type Accent = (typeof ACCENTS)[number];
/** Who reads aloud: the system's voices, or an online service. */
export type TtsProvider = "system" | "openai" | "azure" | "volcengine" | "elevenlabs";
export const TTS_PROVIDERS: TtsProvider[] = ["system", "openai", "azure", "volcengine", "elevenlabs"];

/** An online voice. Each service uses some of the fields; keys are kept in the keychain. */
export interface TtsSettings {
  provider: TtsProvider;
  /** OpenAI-compatible services: the API root, e.g. https://api.openai.com/v1. */
  baseUrl: string;
  model: string;
  voice: string;
  /** Azure: the resource's region. */
  region: string;
  /** Volcengine: the app id and cluster from its console. */
  appId: string;
  cluster: string;
}

/** What a service starts with when chosen. */
export const TTS_DEFAULTS: Record<Exclude<TtsProvider, "system">, Partial<TtsSettings>> = {
  openai: { baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini-tts", voice: "alloy" },
  azure: { region: "eastasia", voice: "zh-CN-XiaoxiaoNeural" },
  volcengine: { cluster: "volcano_tts", voice: "BV001_streaming" },
  elevenlabs: { model: "eleven_multilingual_v2", voice: "JBFqnCBsd6RMkjVDRZzb" },
};

export type ShelfSort = "recent" | "added" | "title" | "author" | "progress";

export interface Settings {
  /** The interface's colours; the reading page has its own, from `background`. */
  uiTheme: UiTheme;
  accent: Accent;
  background: string;
  /** `null` picks a readable colour automatically. */
  textColor: string | null;
  fontFamily: FontFamily;
  /** How heavy book text is drawn. */
  fontWeight: FontWeight;
  /** A font file the user added (see `api.listFonts`), used before `fontFamily`. */
  customFont: string | null;
  fontSize: number;
  lineHeight: number;
  maxWidth: number;
  /** Paged books: one long scroll, one page at a time, or two pages side by side. */
  pageLayout: "continuous" | "single" | "spread";
  /** How paged books are scaled before zoom. */
  pageFit: "width" | "page" | "height";
  /** Reading aloud: a system voice (`voiceURI`; `null` picks one for the text's language). */
  ttsVoice: string | null;
  ttsRate: number;
  tts: TtsSettings;
  /** Comics turn right to left (manga): reversed arrow keys, and spreads read right to left. */
  comicRtl: boolean;
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
  /** Groups with their subgroups folded away. */
  collapsedGroups: string[];
}

export const DEFAULT_SETTINGS: Settings = {
  uiTheme: "system",
  accent: "indigo",
  background: "#f7f0e0",
  textColor: null,
  fontFamily: "sans",
  fontWeight: 400,
  customFont: null,
  fontSize: 19,
  lineHeight: 1.8,
  maxWidth: 760,
  pageLayout: "continuous",
  pageFit: "width",
  comicRtl: false,
  ttsVoice: null,
  ttsRate: 1,
  tts: { provider: "system", baseUrl: "", model: "", voice: "", region: "", appId: "", cluster: "" },
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
  collapsedGroups: [],
};

export const UI_SCALES: { value: number; label: MessageKey }[] = [
  { value: 0.9, label: "settings.scaleSmall" },
  { value: 1, label: "settings.scaleNormal" },
  { value: 1.15, label: "settings.scaleLarge" },
  { value: 1.3, label: "settings.scaleLarger" },
];

export const BACKGROUNDS: { name: MessageKey; color: string }[] = [
  { name: "bg.white", color: "#ffffff" },
  { name: "bg.cream", color: "#f7f0e0" },
  { name: "bg.parchment", color: "#f3e7cf" },
  { name: "bg.green", color: "#d6e8cf" },
  { name: "bg.sky", color: "#d8e6f5" },
  { name: "bg.rose", color: "#f4dfe2" },
  { name: "bg.darkGray", color: "#2f3035" },
  { name: "bg.night", color: "#18202e" },
  { name: "bg.black", color: "#000000" },
];
/** Paper colours of earlier versions, by the preset that replaced them. */
const OLD_BACKGROUNDS: Record<string, string> = {
  "#fbf8f1": "#f7f0e0",
  "#f3ead9": "#f3e7cf",
  "#e4ebe0": "#d6e8cf",
  "#e6ebf2": "#d8e6f5",
  "#2a2b2f": "#2f3035",
  "#1a1b1e": "#18202e",
};

/** Reading themes after Apple Books': paper, ink, typeface and weight chosen together. */
export const READING_THEMES: {
  name: MessageKey;
  background: string;
  textColor: string | null;
  fontFamily: FontFamily;
  fontWeight: FontWeight;
}[] = [
  { name: "theme.original", background: "#ffffff", textColor: null, fontFamily: "sans", fontWeight: 400 },
  { name: "theme.quiet", background: "#48484c", textColor: "#d6d6d8", fontFamily: "sans", fontWeight: 400 },
  { name: "theme.paper", background: "#f4eee2", textColor: "#2b2620", fontFamily: "serif", fontWeight: 400 },
  { name: "theme.bold", background: "#ffffff", textColor: "#000000", fontFamily: "sans", fontWeight: 600 },
  { name: "theme.calm", background: "#ebdcc2", textColor: "#3b2c1b", fontFamily: "serif", fontWeight: 400 },
  { name: "theme.focus", background: "#fbf8ef", textColor: "#161616", fontFamily: "sans", fontWeight: 500 },
];

/** CSS family name under which the user's added font is registered. */
export const CUSTOM_FONT = "AiReader Custom";

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
  if (!UI_THEMES.includes(s.uiTheme)) s.uiTheme = DEFAULT_SETTINGS.uiTheme;
  if (!ACCENTS.includes(s.accent)) s.accent = OLD_ACCENTS[s.accent] ?? DEFAULT_SETTINGS.accent;
  if (!/^#[0-9a-f]{6}$/i.test(s.background)) s.background = DEFAULT_SETTINGS.background;
  s.background = OLD_BACKGROUNDS[s.background.toLowerCase()] ?? s.background;
  if (s.textColor !== null && !/^#[0-9a-f]{6}$/i.test(s.textColor)) s.textColor = null;
  if (!(s.fontFamily in FONT_STACKS)) s.fontFamily = DEFAULT_SETTINGS.fontFamily;
  if (!FONT_WEIGHTS.includes(s.fontWeight)) s.fontWeight = DEFAULT_SETTINGS.fontWeight;
  if (typeof s.ttsVoice !== "string" || !s.ttsVoice) s.ttsVoice = null;
  if (typeof s.ttsRate !== "number" || !(s.ttsRate >= 0.5 && s.ttsRate <= 2)) s.ttsRate = DEFAULT_SETTINGS.ttsRate;
  s.tts = { ...DEFAULT_SETTINGS.tts, ...(s.tts && typeof s.tts === "object" ? s.tts : {}) };
  if (!TTS_PROVIDERS.includes(s.tts.provider)) s.tts.provider = "system";
  if (typeof s.customFont !== "string" || !s.customFont) s.customFont = null;
  if (!UI_SCALES.some((u) => u.value === s.uiScale)) s.uiScale = DEFAULT_SETTINGS.uiScale;
  if (!["continuous", "single", "spread"].includes(s.pageLayout)) s.pageLayout = DEFAULT_SETTINGS.pageLayout;
  if (!["width", "page", "height"].includes(s.pageFit)) s.pageFit = DEFAULT_SETTINGS.pageFit;
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
export function luminance(color: string): number {
  const [r, g, b] = rgb(color).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export const isDark = (bg: string) => luminance(bg) < 0.25;

/** The interface theme in use, with "system" resolved. */
export function uiThemeName(pref: UiTheme, systemDark: boolean): Exclude<UiTheme, "system"> {
  return pref === "system" ? (systemDark ? "dark" : "light") : pref;
}

export const isDarkTheme = (name: string) => DARK_THEMES.includes(name);

/** CSS variables of the reading page, derived from its paper colour. The interface's own colours
 * are in styles.css. */
export function themeVars(s: Settings): Record<string, string> {
  const bg = s.background;
  const dark = isDark(bg);
  // Text is the background pushed nearly to black, or to white short of glaring on a dark
  // page: about as crisp as a printed page or the system's document viewers, with a hint of hue.
  const fg = s.textColor ?? mix(bg, dark ? "#ffffff" : "#000000", dark ? 0.92 : 0.97);
  const toward = dark ? "#ffffff" : "#000000";
  return {
    "--reading-bg": bg,
    "--reading-fg": fg,
    "--reading-muted": mix(fg, bg, 0.45),
    "--reading-panel": mix(bg, toward, dark ? 0.06 : 0.035),
    "--reading-hover": mix(bg, toward, dark ? 0.12 : 0.07),
    "--reading-border": mix(bg, toward, dark ? 0.18 : 0.12),
    "--reading-scheme": dark ? "dark" : "light",
    // An added font comes first; the chosen family covers characters it lacks.
    "--reader-weight": String(s.fontWeight),
    "--reader-font": s.customFont ? `"${CUSTOM_FONT}", ${FONT_STACKS[s.fontFamily].css}` : FONT_STACKS[s.fontFamily].css,
  };
}
