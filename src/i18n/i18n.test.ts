import { describe, expect, it } from "vitest";
import { LANGUAGES, setLanguage, t } from ".";
import zhCN from "./zh-CN";
import zhTW from "./zh-TW";
import en from "./en";
import es from "./es";
import it_ from "./it";
import fr from "./fr";
import de from "./de";
import ja from "./ja";
import ko from "./ko";
import pt from "./pt";

const dicts: Record<string, Record<string, string>> = { "zh-TW": zhTW, en, es, it: it_, fr, de, ja, ko, pt };
const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("translations", () => {
  it("cover every language offered", () => {
    expect(Object.keys(dicts).sort()).toEqual(
      LANGUAGES.map((l) => l.code)
        .filter((c) => c !== "zh-CN")
        .sort(),
    );
  });

  for (const [lang, dict] of Object.entries(dicts)) {
    it(`${lang} has exactly the source keys`, () => {
      expect(Object.keys(dict).sort()).toEqual(Object.keys(zhCN).sort());
    });

    it(`${lang} keeps every placeholder`, () => {
      const wrong = Object.entries(zhCN)
        .filter(([key, text]) => placeholders(dict[key] ?? "").join() !== placeholders(text).join())
        .map(([key]) => key);
      expect(wrong).toEqual([]);
    });

    it(`${lang} has no empty messages`, () => {
      expect(Object.entries(dict).filter(([, v]) => !v.trim())).toEqual([]);
    });
  }

  it("fills placeholders and leaves unknown ones", () => {
    setLanguage("en");
    expect(t("loc.page", { n: 3 })).toMatch(/3/);
    expect(t("loc.page")).toContain("{n}");
    setLanguage("zh-CN");
    expect(t("loc.page", { n: 3 })).toBe("第 3 页");
  });
});
