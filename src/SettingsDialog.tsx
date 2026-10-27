// The Settings dialog (⌘/Ctrl+,): interface language and size, reading appearance, AI service,
// and the shortcut list.

import { createContext, useContext, useState } from "react";
import { api, errorText } from "./api";
import type { UpdateSettings } from "./App";
import AiSettingsForm from "./ai/AiSettingsForm";
import { useAi } from "./ai/AiContext";
import { LANGUAGES, systemLanguage, t } from "./i18n";
import { Icon, type IconName } from "./icons";
import { ReadingSettings } from "./reader/SettingsPanel";
import { shortcuts } from "./reader/shortcuts";
import { UI_SCALES, type Settings } from "./settings";
import { Modal } from "./ui";

export type SettingsTab = "general" | "reading" | "ai" | "shortcuts";

/** Opens the Settings dialog (provided by App). */
export const SettingsOpener = createContext<(tab?: SettingsTab) => void>(() => {});
export const useOpenSettings = () => useContext(SettingsOpener);

const TABS: { key: SettingsTab; icon: IconName; label: () => string }[] = [
  { key: "general", icon: "globe", label: () => t("settings.tabGeneral") },
  { key: "reading", icon: "type", label: () => t("settings.tabReading") },
  { key: "ai", icon: "sparkles", label: () => t("settings.tabAi") },
  { key: "shortcuts", icon: "keyboard", label: () => t("settings.tabShortcuts") },
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
                <select value={settings.uiLanguage} onChange={(e) => update({ uiLanguage: e.target.value })}>
                  <option value="auto">{t("settings.languageAuto", { name: system.name })}</option>
                  {LANGUAGES.map((l) => (
                    <option key={l.code} value={l.code}>
                      {l.name}
                    </option>
                  ))}
                </select>
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
            </>
          )}
          {tab === "reading" && <ReadingSettings settings={settings} update={update} text paged />}
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
