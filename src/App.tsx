import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { DEFAULT_SETTINGS, normalizeSettings, themeVars, type Settings } from "./settings";
import ShelfView from "./shelf/ShelfView";
import Reader from "./reader/Reader";
import { AiProvider } from "./ai/AiContext";

export type UpdateSettings = (patch: Partial<Settings>) => void;

export default function App() {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [loaded, setLoaded] = useState(false);
  const [reading, setReading] = useState<string | null>(null);
  const saveTimer = useRef<number>();

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

  const close = useCallback(() => {
    setReading(null);
    getCurrentWindow().setTitle("AiReader").catch(() => {});
  }, []);

  if (!loaded) return null;
  return (
    <AiProvider>
      {reading ? (
        <Reader bookId={reading} settings={settings} update={update} onClose={close} />
      ) : (
        <ShelfView settings={settings} update={update} onOpen={setReading} />
      )}
    </AiProvider>
  );
}
