// Settings → Reading: which voice reads aloud. The system's voices need nothing; an online service
// needs its address or region, a voice, and a key (kept in the keychain).

import { useEffect, useState } from "react";
import type { UpdateSettings } from "../App";
import { api, errorText } from "../api";
import { t, type MessageKey } from "../i18n";
import { Select } from "../Select";
import { TTS_DEFAULTS, TTS_PROVIDERS, type Settings, type TtsProvider, type TtsSettings as Tts } from "../settings";
import { speakOnce } from "./useSpeech";

const PROVIDER_NAMES: Record<TtsProvider, MessageKey> = {
  system: "tts.system",
  openai: "tts.openai",
  azure: "tts.azure",
  volcengine: "tts.volcengine",
  elevenlabs: "tts.elevenlabs",
};

/** OpenAI-compatible services worth a shortcut. */
const PRESETS: { name: MessageKey; baseUrl: string; model: string; voice: string }[] = [
  { name: "tts.presetOpenai", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini-tts", voice: "alloy" },
  {
    name: "tts.presetSiliconflow",
    baseUrl: "https://api.siliconflow.cn/v1",
    model: "FunAudioLLM/CosyVoice2-0.5B",
    voice: "FunAudioLLM/CosyVoice2-0.5B:alex",
  },
  { name: "tts.presetKokoro", baseUrl: "http://localhost:8880/v1", model: "kokoro", voice: "zf_xiaobei" },
];

export default function TtsSettings({ settings, update }: { settings: Settings; update: UpdateSettings }) {
  const tts = settings.tts;
  const online = tts.provider !== "system";
  const [key, setKey] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    setKey("");
    setMessage(null);
  }, [tts.provider]);

  // An OpenAI-compatible key belongs to its address, so another address needs its own.
  const keyFor = tts.provider === "openai" ? tts.baseUrl.trim().replace(/\/+$/, "").toLowerCase() : "";
  useEffect(() => {
    if (!online) return;
    let current = true;
    api
      .ttsHasKey(tts)
      .then((has) => current && setHasKey(has))
      .catch(() => current && setHasKey(false));
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only the provider and address pick the key
  }, [tts.provider, keyFor, online]);

  const set = (patch: Partial<Tts>) => {
    update({ tts: { ...tts, ...patch } });
    setMessage(null);
  };
  const choose = (provider: TtsProvider) => {
    const blank = { baseUrl: "", model: "", voice: "", region: "", appId: "", cluster: "" };
    update({ tts: { ...blank, ...(provider === "system" ? {} : TTS_DEFAULTS[provider]), provider } });
  };

  const saveKey = async () => {
    if (!key.trim()) return;
    await api.ttsSetKey(tts, key);
    setKey("");
    setHasKey(true);
  };

  const listen = async () => {
    setBusy(true);
    setMessage(null);
    try {
      await saveKey();
      await speakOnce(t("tts.sample"), settings);
      setMessage({ ok: true, text: t("tts.testOk") });
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  const field = (label: MessageKey, name: keyof Tts, placeholder?: string) => (
    <label className="field">
      <span>{t(label)}</span>
      <input value={tts[name]} placeholder={placeholder} spellCheck={false} onChange={(e) => set({ [name]: e.target.value })} />
    </label>
  );
  const preset = PRESETS.findIndex((p) => p.baseUrl === tts.baseUrl);
  const keyLabel: MessageKey = tts.provider === "volcengine" ? "tts.token" : "tts.key";
  const keyOptional = tts.provider === "openai" && /^http:\/\/(localhost|127\.0\.0\.1)/.test(tts.baseUrl);

  return (
    <div className="panel-section tts-settings">
      <div className="panel-label">{t("tts.title")}</div>
      <Select value={tts.provider} label={t("tts.title")} options={TTS_PROVIDERS.map((p) => ({ value: p, label: t(PROVIDER_NAMES[p]) }))} onChange={choose} />
      {!online && <p className="muted small hint">{t("tts.systemHint")}</p>}
      {online && (
        <>
          {tts.provider === "openai" && (
            <>
              <label className="field">
                <span>{t("tts.preset")}</span>
                <Select
                  full
                  value={preset < 0 ? "" : String(preset)}
                  label={t("tts.preset")}
                  options={[...PRESETS.map((p, i) => ({ value: String(i), label: t(p.name) })), { value: "", label: t("tts.presetCustom") }]}
                  onChange={(v) => {
                    const p = PRESETS[Number(v)];
                    if (v !== "" && p) set({ baseUrl: p.baseUrl, model: p.model, voice: p.voice });
                  }}
                />
              </label>
              {field("tts.baseUrl", "baseUrl", "https://api.openai.com/v1")}
              {field("tts.model", "model")}
            </>
          )}
          {tts.provider === "azure" && field("tts.region", "region", "eastasia")}
          {tts.provider === "volcengine" && (
            <>
              {field("tts.appId", "appId")}
              {field("tts.cluster", "cluster", "volcano_tts")}
            </>
          )}
          {tts.provider === "elevenlabs" && field("tts.model", "model", "eleven_multilingual_v2")}
          {field("tts.voice", "voice")}
          <label className="field">
            <span>{t(keyLabel)}</span>
            <input
              type="password"
              value={key}
              autoComplete="off"
              placeholder={hasKey ? t("tts.keySaved") : keyOptional ? t("tts.keyOptional") : ""}
              onChange={(e) => (setKey(e.target.value), setMessage(null))}
              onBlur={() => saveKey().catch((e) => setMessage({ ok: false, text: errorText(e) }))}
            />
          </label>
          <p className="muted small hint">{t("tts.cloudHint")}</p>
          <div className="inline-add form-actions">
            <button disabled={busy} onClick={listen}>
              {t("tts.test")}
            </button>
          </div>
          {message && <p className={message.ok ? "ok-text small" : "error small"}>{message.text}</p>}
        </>
      )}
    </div>
  );
}
