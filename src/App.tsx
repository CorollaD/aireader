import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { api, fontUrl } from "./api";
import { onBeforeExit, saveAll } from "./lifecycle";
import { resolveLanguage, setLanguage, t } from "./i18n";
import { CUSTOM_FONT, DEFAULT_SETTINGS, normalizeSettings, themeVars, UI_SCALES, type Settings } from "./settings";
import SettingsDialog, { SettingsOpener, type SettingsTab } from "./SettingsDialog";
import ShelfView from "./shelf/ShelfView";
import Reader from "./reader/Reader";
import { AiProvider } from "./ai/AiContext";
import { applyUiScale } from "./zoom";
import ErrorBoundary from "./ErrorBoundary";

export type UpdateSettings = (patch: Partial<Settings>) => void;

export default function App() {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [loaded, setLoaded] = useState(false);
  const [reading, setReading] = useState<string | null>(null);
  const [settingsTab, setSettingsTab] = useState<SettingsTab | null>(null);
  const saveTimer = useRef<number>();
  const pendingSettings = useRef<Settings | null>(null);

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

  // Closing or quitting: save what is pending, then exit.
  useEffect(() => {
    const closing = listen("app-closing", async () => {
      await saveAll();
      api.exitApp().catch(() => {});
    });
    // A book opened from the file manager while the app runs.
    const opened = listen("open-file", () => {
      api
        .takeStartupBook()
        .then((id) => id && setReading(id))
        .catch(() => {});
    });
    return () => {
      closing.then((un) => un());
      opened.then((un) => un());
    };
  }, []);

  // Messages from the backend (errors, generated titles) follow the interface language.
  useEffect(() => {
    api.setLocale(lang).catch(() => {});
  }, [lang]);

  useEffect(() => applyUiScale(settings.uiScale), [settings.uiScale]);

  // The user's added reading font, registered under one family name.
  useEffect(() => {
    if (!settings.customFont) return;
    const face = new FontFace(CUSTOM_FONT, `url("${fontUrl(settings.customFont)}")`);
    face
      .load()
      .then(() => document.fonts.add(face))
      .catch(() => {});
    return () => {
      document.fonts.delete(face);
    };
  }, [settings.customFont]);

  // Apply theme variables to the document root so dialogs and scrollbars follow it.
  useEffect(() => {
    const root = document.documentElement;
    for (const [k, v] of Object.entries(themeVars(settings))) root.style.setProperty(k, v);
  }, [settings]);

  const flushSettings = useCallback(() => {
    window.clearTimeout(saveTimer.current);
    const next = pendingSettings.current;
    pendingSettings.current = null;
    return next ? api.setSettings(next).catch(() => {}) : undefined;
  }, []);
  useEffect(() => onBeforeExit(flushSettings), [flushSettings]);

  const update: UpdateSettings = useCallback(
    (patch) => {
      setSettings((s) => {
        const next = { ...s, ...patch };
        pendingSettings.current = next;
        window.clearTimeout(saveTimer.current);
        saveTimer.current = window.setTimeout(flushSettings, 400);
        return next;
      });
    },
    [flushSettings],
  );

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

  // A plain frame in the theme's colours until settings arrive, instead of a blank window.
  if (!loaded) return <div className="app-loading" aria-busy="true" />;
  return (
    <SettingsOpener.Provider value={openSettings}>
      <AiProvider openSettings={openAiSettings}>
        <ErrorBoundary key={reading ?? "shelf"} onReset={reading ? close : undefined} resetLabel={t("error.backToShelf")}>
          {reading ? (
            <Reader bookId={reading} settings={settings} update={update} onClose={close} />
          ) : (
            <ShelfView settings={settings} update={update} onOpen={setReading} />
          )}
        </ErrorBoundary>
        {settingsTab && <SettingsDialog settings={settings} update={update} tab={settingsTab} onTab={setSettingsTab} onClose={() => setSettingsTab(null)} />}
      </AiProvider>
    </SettingsOpener.Provider>
  );
}
