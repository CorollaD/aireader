import { describe, expect, it } from "vitest";
import { BACKGROUNDS, CUSTOM_FONT, DEFAULT_SETTINGS, isDark, luminance, normalizeSettings, themeVars } from "./settings";

describe("settings", () => {
  it("falls back to defaults for bad values", () => {
    const s = normalizeSettings({ background: "red", textColor: "#12", fontFamily: "comic", uiScale: 7, fontSize: 20 });
    expect(s.background).toBe(DEFAULT_SETTINGS.background);
    expect(s.textColor).toBeNull();
    expect(s.fontFamily).toBe(DEFAULT_SETTINGS.fontFamily);
    expect(s.uiScale).toBe(DEFAULT_SETTINGS.uiScale);
    expect(s.fontSize).toBe(20);
    expect(normalizeSettings(null)).toEqual(DEFAULT_SETTINGS);
  });

  it("derives a readable theme from the background", () => {
    expect(isDark("#000000")).toBe(true);
    expect(isDark("#ffffff")).toBe(false);
    const light = themeVars({ ...DEFAULT_SETTINGS, background: "#ffffff" });
    const dark = themeVars({ ...DEFAULT_SETTINGS, background: "#1e1e1e" });
    expect(light["--reading-scheme"]).toBe("light");
    expect(dark["--reading-scheme"]).toBe("dark");
    // Text contrasts with the background.
    expect(isDark(light["--reading-fg"])).toBe(true);
    expect(isDark(dark["--reading-fg"])).toBe(false);
    expect(themeVars({ ...DEFAULT_SETTINGS, textColor: "#123456" })["--reading-fg"]).toBe("#123456");
    // An added font comes first, the chosen family after it.
    const custom = themeVars({ ...DEFAULT_SETTINGS, customFont: "Kai.ttf" })["--reader-font"];
    expect(custom.startsWith(`"${CUSTOM_FONT}", `)).toBe(true);
    expect(normalizeSettings({ customFont: 3 }).customFont).toBeNull();
    expect(normalizeSettings({ uiTheme: "neon" }).uiTheme).toBe("system");
  });

  it("gives every paper text as crisp as print", () => {
    const contrast = (a: string, b: string) => {
      const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
      return (hi + 0.05) / (lo + 0.05);
    };
    for (const { color } of BACKGROUNDS) {
      const fg = themeVars({ ...DEFAULT_SETTINGS, background: color })["--reading-fg"];
      // Light papers come close to black on white (21:1); dark ones stop short of glare.
      expect(contrast(fg, color), color).toBeGreaterThanOrEqual(isDark(color) ? 11 : 15.5);
    }
  });
});
