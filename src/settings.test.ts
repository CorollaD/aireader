import { describe, expect, it } from "vitest";
import { CUSTOM_FONT, DEFAULT_SETTINGS, isDark, normalizeSettings, themeVars } from "./settings";

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
    expect(light["color-scheme"]).toBe("light");
    expect(dark["color-scheme"]).toBe("dark");
    // Text contrasts with the background.
    expect(isDark(light["--fg"])).toBe(true);
    expect(isDark(dark["--fg"])).toBe(false);
    expect(themeVars({ ...DEFAULT_SETTINGS, textColor: "#123456" })["--fg"]).toBe("#123456");
    // An added font comes first, the chosen family after it.
    const custom = themeVars({ ...DEFAULT_SETTINGS, customFont: "Kai.ttf" })["--reader-font"];
    expect(custom.startsWith(`"${CUSTOM_FONT}", `)).toBe(true);
    expect(normalizeSettings({ customFont: 3 }).customFont).toBeNull();
  });
});
