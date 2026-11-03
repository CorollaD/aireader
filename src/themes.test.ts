import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ACCENTS, luminance, UI_THEMES } from "./settings";

const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

/** The custom properties set in the rule whose selector list includes `selector`. */
function block(selector: string): Record<string, string> {
  const rule = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].find(([, sel]) => sel.split(",").some((s) => s.trim() === selector));
  if (!rule) throw new Error(`no rule for ${selector}`);
  return Object.fromEntries([...rule[2].matchAll(/(--[\w-]+):\s*([^;]+);/g)].map(([, k, v]) => [k, v.trim()]));
}

const contrast = (a: string, b: string) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

describe("interface themes", () => {
  for (const name of UI_THEMES.filter((n) => n !== "system")) {
    it(`${name} keeps its text readable`, () => {
      const v = block(`[data-theme="${name}"]`);
      for (const surface of ["--bg", "--surface", "--panel"]) {
        expect(contrast(v["--fg"], v[surface]), `${name} fg on ${surface}`).toBeGreaterThanOrEqual(12);
        expect(contrast(v["--muted"], v[surface]), `${name} muted on ${surface}`).toBeGreaterThanOrEqual(7);
        expect(contrast(v["--faint"], v[surface]), `${name} faint on ${surface}`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }

  it("gives every accent readable text on it", () => {
    for (const accent of ACCENTS) {
      const v = block(`[data-accent="${accent}"]`);
      expect(contrast(v["--accent-l"], "#ffffff"), `${accent} light`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(v["--accent-d"], "#111114"), `${accent} dark`).toBeGreaterThanOrEqual(4.5);
    }
  });
});
