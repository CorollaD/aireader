// How long until a flashcard comes back, in words.

import { t } from "../i18n";

const MINUTE = 60;
const HOUR = 3600;
const DAY = 86_400;

/** `seconds` from now, in the largest unit that fits. */
export function formatInterval(seconds: number): string {
  if (seconds < HOUR) return t("cards.inMinutes", { n: Math.max(1, Math.round(seconds / MINUTE)) });
  if (seconds < DAY) return t("cards.inHours", { n: Math.round(seconds / HOUR) });
  if (seconds < 30 * DAY) return t("cards.inDays", { n: Math.round(seconds / DAY) });
  if (seconds < 365 * DAY) return t("cards.inMonths", { n: Math.round(seconds / (30 * DAY)) });
  return t("cards.inYears", { n: Math.round((seconds / (365 * DAY)) * 10) / 10 });
}

/** The Unix time the local day began. */
export const startOfToday = () => Math.floor(new Date().setHours(0, 0, 0, 0) / 1000);
