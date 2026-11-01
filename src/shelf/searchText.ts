// Presenting full-text search results.

import { t } from "../i18n";
import type { TextHit } from "../types";

/** Where a hit is: its chapter's title, or the page. */
export function describeHit(h: TextHit): string {
  return h.title || (h.location.kind === "page" ? t("loc.page", { n: h.location.page + 1 }) : t("loc.chapter", { n: h.location.chapter + 1 }));
}
