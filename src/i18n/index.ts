// Interface translations. `zh-CN.ts` is the source of keys; every other language must provide
// all of them (TypeScript checks this). `t()` reads the current language, which App sets and
// re-renders on change.

import de from "./de";
import en from "./en";
import es from "./es";
import fr from "./fr";
import it from "./it";
import ja from "./ja";
import ko from "./ko";
import pt from "./pt";
import zhCN from "./zh-CN";
import zhTW from "./zh-TW";

export type Messages = Record<keyof typeof zhCN, string>;
export type MessageKey = keyof Messages;

export const LANGUAGES = [
  { code: "zh-CN", name: "简体中文", english: "Simplified Chinese" },
  { code: "zh-TW", name: "繁體中文", english: "Traditional Chinese" },
  { code: "en", name: "English", english: "English" },
  { code: "es", name: "Español", english: "Spanish" },
  { code: "it", name: "Italiano", english: "Italian" },
  { code: "fr", name: "Français", english: "French" },
  { code: "de", name: "Deutsch", english: "German" },
  { code: "ja", name: "日本語", english: "Japanese" },
  { code: "ko", name: "한국어", english: "Korean" },
  { code: "pt", name: "Português", english: "Portuguese" },
] as const;

export type Lang = (typeof LANGUAGES)[number]["code"];

const DICTS: Record<Lang, Messages> = {
  "zh-CN": zhCN,
  "zh-TW": zhTW,
  en,
  es,
  it,
  fr,
  de,
  ja,
  ko,
  pt,
};

/** The best supported language for the system's preferred languages. */
export function systemLanguage(): Lang {
  for (const tag of navigator.languages?.length ? navigator.languages : [navigator.language]) {
    const lower = tag.toLowerCase();
    if (lower.startsWith("zh")) {
      return /tw|hk|mo|hant/.test(lower) ? "zh-TW" : "zh-CN";
    }
    const base = lower.split("-")[0];
    const match = LANGUAGES.find((l) => l.code === base);
    if (match) return match.code;
  }
  return "en";
}

/** Resolves a setting ("auto" or a code) to a supported language. */
export function resolveLanguage(setting: string | undefined): Lang {
  return LANGUAGES.some((l) => l.code === setting) ? (setting as Lang) : systemLanguage();
}

let current: Lang = systemLanguage();

export function setLanguage(lang: Lang) {
  current = lang;
  document.documentElement.lang = lang;
}

export function language(): Lang {
  return current;
}

/** The translated message, with `{name}` placeholders filled from `params`. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  const text = DICTS[current][key] ?? zhCN[key] ?? key;
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (m, name: string) => (name in params ? String(params[name]) : m));
}

export function formatDate(secs: number | null): string {
  if (!secs) return "—";
  return new Intl.DateTimeFormat(current, { year: "numeric", month: "short", day: "numeric" }).format(new Date(secs * 1000));
}

export function formatNumber(n: number): string {
  return new Intl.NumberFormat(current, { maximumFractionDigits: 1, notation: n >= 10_000 ? "compact" : "standard" }).format(n);
}

export function formatMinutes(minutes: number): string {
  if (minutes < 1) return t("time.lessThanMinute");
  if (minutes < 60) return t("time.minutes", { n: Math.round(minutes) });
  const h = Math.floor(minutes / 60);
  const m = Math.round(minutes % 60);
  return m ? t("time.hoursMinutes", { h, m }) : t("time.hours", { h });
}

export function formatDuration(seconds: number): string {
  if (seconds <= 0) return t("time.minutes", { n: 0 });
  return formatMinutes(seconds / 60);
}

/** Joins a short list in the language's style ("a, b and c" / "a、b 和 c"). */
export function formatList(items: string[]): string {
  return new Intl.ListFormat(current, { style: "short", type: "conjunction" }).format(items);
}
