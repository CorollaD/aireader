// The Settings dialog (⌘/Ctrl+,): interface language and size, reading appearance, AI service,
// and the shortcut list.

import { createContext, useContext, useEffect, useState } from "react";
import { api, errorText } from "./api";
import ExportNotes from "./ExportNotes";
import DictionarySettings from "./study/DictionarySettings";
import SyncSettings from "./SyncSettings";
import type { UpdateSettings } from "./App";
import AiSettingsForm from "./ai/AiSettingsForm";
import { useAi } from "./ai/AiContext";
import { LANGUAGES, systemLanguage, t, type MessageKey } from "./i18n";
import { Icon, type IconName } from "./icons";
import { ReadingSettings } from "./reader/SettingsPanel";
import { shortcuts } from "./reader/shortcuts";
import { ACCENTS, UI_SCALES, type Settings, type UiTheme } from "./settings";
import { Modal } from "./ui";
import { Select } from "./Select";
import { getVersion } from "@tauri-apps/api/app";
import TtsSettings from "./reader/TtsSettings";

export type SettingsTab = "general" | "reading" | "dictionary" | "sync" | "ai" | "shortcuts";

/** Opens the Settings dialog (provided by App). */
export const SettingsOpener = createContext<(tab?: SettingsTab) => void>(() => {});
export const useOpenSettings = () => useContext(SettingsOpener);

const TABS: { key: SettingsTab; icon: IconName; label: () => string }[] = [
  { key: "general", icon: "globe", label: () => t("settings.tabGeneral") },
  { key: "reading", icon: "type", label: () => t("settings.tabReading") },
  { key: "dictionary", icon: "bookOpen", label: () => t("settings.tabDictionary") },
  { key: "sync", icon: "refresh", label: () => t("settings.tabSync") },
  { key: "ai", icon: "sparkles", label: () => t("settings.tabAi") },
  { key: "shortcuts", icon: "keyboard", label: () => t("settings.tabShortcuts") },
];

const THEMES: [UiTheme, MessageKey][] = [
  ["system", "settings.themeSystem"],
  ["light", "settings.themeLight"],
  ["dark", "settings.themeDark"],
];

export default function SettingsDialog(props: {
  settings: Settings;
  update: UpdateSettings;
  tab: SettingsTab;
  onTab: (tab: SettingsTab) => void;
  onClose: () => void;
}) {
  const { settings, update, tab } = props;
  const ai = useAi();
  const system = LANGUAGES.find((l) => l.code === systemLanguage())!;

  return (
    <Modal title={t("settings.title")} width={680} onClose={props.onClose}>
      <div className="settings-dialog">
        <nav className="settings-tabs">
          {TABS.map((tb) => (
            <button key={tb.key} className={`side-item${tab === tb.key ? " active" : ""}`} onClick={() => props.onTab(tb.key)}>
              <Icon name={tb.icon} />
              <span className="ellipsis">{tb.label()}</span>
            </button>
          ))}
        </nav>
        <div className="settings-page">
          {tab === "general" && (
            <>
              <div className="panel-section">
                <div className="panel-label">{t("settings.language")}</div>
                <Select
                  value={settings.uiLanguage}
                  label={t("settings.language")}
                  options={[
                    { value: "auto", label: t("settings.languageAuto", { name: system.name }) },
                    ...LANGUAGES.map((l) => ({ value: l.code as string, label: l.name })),
                  ]}
                  onChange={(uiLanguage) => update({ uiLanguage })}
                />
              </div>
              <div className="panel-section">
                <div className="panel-label">{t("settings.theme")}</div>
                <div className="segmented">
                  {THEMES.map(([value, label]) => (
                    <button key={value} className={settings.uiTheme === value ? "on" : ""} onClick={() => update({ uiTheme: value })}>
                      {t(label)}
                    </button>
                  ))}
                </div>
              </div>
              <div className="panel-section">
                <div className="panel-label">{t("settings.accent")}</div>
                <div className="accent-swatches" role="radiogroup" aria-label={t("settings.accent")}>
                  {ACCENTS.map((a) => (
                    <button
                      key={a}
                      role="radio"
                      aria-checked={settings.accent === a}
                      data-accent={a}
                      className={`accent-swatch${settings.accent === a ? " on" : ""}`}
                      title={t(`accent.${a}`)}
                      aria-label={t(`accent.${a}`)}
                      onClick={() => update({ accent: a })}
                    />
                  ))}
                </div>
              </div>
              <div className="panel-section">
                <div className="panel-label">{t("settings.uiScale")}</div>
                <div className="segmented">
                  {UI_SCALES.map((u) => (
                    <button key={u.value} className={settings.uiScale === u.value ? "on" : ""} onClick={() => update({ uiScale: u.value })}>
                      {t(u.label)}
                    </button>
                  ))}
                </div>
                <p className="muted small hint">{t("settings.uiScaleHint")}</p>
              </div>
              <Backup />
              <div className="panel-section">
                <div className="panel-label">{t("export.allBooks")}</div>
                <ExportNotes />
              </div>
              <AppVersion />
            </>
          )}
          {tab === "reading" && (
            <>
              <ReadingSettings settings={settings} update={update} text paged />
              <TtsSettings settings={settings} update={update} />
            </>
          )}
          {tab === "dictionary" && <DictionarySettings />}
          {tab === "sync" && <SyncSettings />}
          {tab === "ai" && ai.status && <AiSettingsForm status={ai.status} onSaved={ai.setStatus} />}
          {tab === "shortcuts" && (
            <div className="shortcuts">
              {shortcuts().map((g) => (
                <div key={g.group}>
                  <div className="panel-label">{g.group}</div>
                  {g.items.map(([k, d]) => (
                    <div key={g.group + k} className="shortcut-row">
                      <kbd>{k}</kbd>
                      <span>{d}</span>
                    </div>
                  ))}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </Modal>
  );
}

/** Backing up the library (database and covers, optionally the book files) to a zip. */
function Backup() {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const run = async (books: boolean) => {
    setBusy(true);
    setMessage(null);
    try {
      const path = await api.backupLibrary(books);
      if (path) setMessage({ ok: true, text: t("settings.backupDone", { path }) });
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="panel-section">
      <div className="panel-label">{t("settings.backup")}</div>
      <div className="inline-add">
        <button disabled={busy} onClick={() => run(false)}>
          {t("settings.backupData")}
        </button>
        <button disabled={busy} onClick={() => run(true)}>
          {t("settings.backupAll")}
        </button>
      </div>
      <p className="muted small hint">{busy ? t("settings.backingUp") : t("settings.backupHint")}</p>
      {message && <p className={message.ok ? "ok-text small" : "error small"}>{message.text}</p>}
    </div>
  );
}

/** Which version this is, for bug reports and checking for updates. */
function AppVersion() {
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => {});
  }, []);
  if (!version) return null;
  return <p className="app-version muted small">{t("settings.version", { version })}</p>;
}
