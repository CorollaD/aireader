import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { resolveLanguage, setLanguage } from "./i18n";
import { DEFAULT_SETTINGS, normalizeSettings, themeVars, UI_SCALES, type Settings } from "./settings";
import SettingsDialog, { SettingsOpener, type SettingsTab } from "./SettingsDialog";
import ShelfView from "./shelf/ShelfView";
import Reader from "./reader/Reader";
import { AiProvider } from "./ai/AiContext";
import { applyUiScale } from "./zoom";

export type UpdateSettings = (patch: Partial<Settings>) => void;

export default function App() {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [loaded, setLoaded] = useState(false);
  const [reading, setReading] = useState<string | null>(null);
  const [settingsTab, setSettingsTab] = useState<SettingsTab | null>(null);
  const saveTimer = useRef<number>();

  // Set before the tree renders, so every t() call below uses it.
  const lang = resolveLanguage(settings.uiLanguage);
  setLanguage(lang);

  useEffect(() => {
    const settingsLoaded = api
      .getSettings()
      .then((raw) => setSettings(normalizeSettings(raw)))
      .catch(() => {});
    // A file passed on the command line opens straight into the reader.
    const startup = api.takeStartupBook().catch(() => null);
    Promise.all([settingsLoaded, startup]).then(([, id]) => {
      if (id) setReading(id);
      setLoaded(true);
    });
  }, []);

  // Messages from the backend (errors, generated titles) follow the interface language.
  useEffect(() => {
    api.setLocale(lang).catch(() => {});
  }, [lang]);

  useEffect(() => applyUiScale(settings.uiScale), [settings.uiScale]);

  // Apply theme variables to the document root so dialogs and scrollbars follow it.
  useEffect(() => {
    const root = document.documentElement;
    for (const [k, v] of Object.entries(themeVars(settings))) root.style.setProperty(k, v);
  }, [settings]);

  const update: UpdateSettings = useCallback((patch) => {
    setSettings((s) => {
      const next = { ...s, ...patch };
      window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => api.setSettings(next).catch(() => {}), 400);
      return next;
    });
  }, []);

  // App-wide keys: ⌘/Ctrl+, opens Settings; on the shelf, ⌘/Ctrl +/-/0 change the interface
  // size (in the reader they change the text size instead).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
      if (e.key === ",") {
        e.preventDefault();
        setSettingsTab((t) => t ?? "general");
        return;
      }
      if (reading) return;
      const i = UI_SCALES.findIndex((u) => u.value === settings.uiScale);
      const step = e.key === "=" || e.key === "+" ? 1 : e.key === "-" ? -1 : 0;
      if (step) {
        e.preventDefault();
        const next = UI_SCALES[Math.max(0, Math.min(UI_SCALES.length - 1, i + step))];
        update({ uiScale: next.value });
      } else if (e.key === "0") {
        e.preventDefault();
        update({ uiScale: 1 });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [reading, settings.uiScale, update]);

  const close = useCallback(() => {
    setReading(null);
    getCurrentWindow()
      .setTitle("AiReader")
      .catch(() => {});
  }, []);

  const openSettings = useCallback((tab: SettingsTab = "general") => setSettingsTab(tab), []);
  const openAiSettings = useCallback(() => setSettingsTab("ai"), []);

  if (!loaded) return null;
  return (
    <SettingsOpener.Provider value={openSettings}>
      <AiProvider openSettings={openAiSettings}>
        {reading ? (
          <Reader bookId={reading} settings={settings} update={update} onClose={close} />
        ) : (
          <ShelfView settings={settings} update={update} onOpen={setReading} />
        )}
        {settingsTab && <SettingsDialog settings={settings} update={update} tab={settingsTab} onTab={setSettingsTab} onClose={() => setSettingsTab(null)} />}
      </AiProvider>
    </SettingsOpener.Provider>
  );
}
