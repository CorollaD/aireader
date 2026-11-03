import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { api, fontUrl } from "./api";
import { onBeforeExit, saveAll } from "./lifecycle";
import OcrProgressBar from "./ocr/OcrProgressBar";
import { resolveLanguage, setLanguage, t } from "./i18n";
import { CUSTOM_FONT, DEFAULT_SETTINGS, isDarkTheme, normalizeSettings, themeVars, UI_SCALES, uiThemeName, type Settings } from "./settings";
import SettingsDialog, { SettingsOpener, type SettingsTab } from "./SettingsDialog";
import ShelfView from "./shelf/ShelfView";
import ReviewView from "./study/ReviewView";
import StatsView from "./stats/StatsView";
import OpdsView from "./opds/OpdsView";
import Reader, { type OpenAt } from "./reader/Reader";
import { AiProvider } from "./ai/AiContext";
import { applyUiScale } from "./zoom";
import ErrorBoundary from "./ErrorBoundary";
import { watchTitleBar } from "./titlebar";

export type UpdateSettings = (patch: Partial<Settings>) => void;

/** How long the last sync may hold up quitting (the backend gives up a little later). */
const LAST_SYNC_MS = 5000;

export default function App() {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [loaded, setLoaded] = useState(false);
  const [reading, setReading] = useState<string | null>(null);
  /** Where to open the book: a search result rather than the saved position. */
  const [openAt, setOpenAt] = useState<OpenAt | null>(null);
  const [page, setPage] = useState<"review" | "stats" | "opds" | null>(null);
  const [settingsTab, setSettingsTab] = useState<SettingsTab | null>(null);
  const saveTimer = useRef<number | undefined>(undefined);
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
      // A last sync, if it is on: it mustn't hold up quitting for long.
      await Promise.race([api.syncNow(true).catch(() => {}), new Promise((done) => setTimeout(done, LAST_SYNC_MS))]);
      api.exitApp().catch(() => {});
    });
    // A book opened from the file manager while the app runs.
    const opened = listen("open-file", () => {
      api
        .takeStartupBook()
        .then((id) => {
          if (!id) return;
          setOpenAt(null);
          setPage(null);
          setReading(id);
        })
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

  // The reading page's colours, on the root so that everything inside it can use them.
  useEffect(() => {
    const root = document.documentElement;
    for (const [k, v] of Object.entries(themeVars(settings))) root.style.setProperty(k, v);
  }, [settings]);

  // The interface's theme, following the system's if asked to.
  useEffect(() => {
    const dark = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const name = uiThemeName(settings.uiTheme, dark.matches);
      document.documentElement.dataset.theme = name;
      document.documentElement.dataset.scheme = isDarkTheme(name) ? "dark" : "light";
    };
    apply();
    dark.addEventListener("change", apply);
    return () => dark.removeEventListener("change", apply);
  }, [settings.uiTheme]);
  useEffect(() => {
    document.documentElement.dataset.accent = settings.accent;
  }, [settings.accent]);

  useEffect(() => watchTitleBar(), []);

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

  const open = useCallback((id: string, at?: OpenAt) => {
    setOpenAt(at ?? null);
    setPage(null);
    setReading(id);
  }, []);

  const close = useCallback(() => {
    setOpenAt(null);
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
        <ErrorBoundary key={page ?? reading ?? "shelf"} onReset={reading ? close : undefined} resetLabel={t("error.backToShelf")}>
          {page === "review" ? (
            <ReviewView settings={settings} onBack={() => setPage(null)} onOpen={(id, location) => open(id, { location })} />
          ) : page === "stats" ? (
            <StatsView onBack={() => setPage(null)} onOpen={(id) => open(id)} />
          ) : page === "opds" ? (
            <OpdsView onBack={() => setPage(null)} onOpen={(id) => open(id)} />
          ) : reading ? (
            <Reader bookId={reading} openAt={openAt} settings={settings} update={update} onClose={close} />
          ) : (
            <ShelfView
              settings={settings}
              update={update}
              onOpen={open}
              onReview={() => setPage("review")}
              onStats={() => setPage("stats")}
              onOpds={() => setPage("opds")}
            />
          )}
        </ErrorBoundary>
        {settingsTab && <SettingsDialog settings={settings} update={update} tab={settingsTab} onTab={setSettingsTab} onClose={() => setSettingsTab(null)} />}
        <OcrProgressBar />
      </AiProvider>
    </SettingsOpener.Provider>
  );
}
