// The AI tab of the Settings dialog: service, model, key, endpoint and answer language.

import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import { t } from "../i18n";
import type { AiConfig, AiStatus } from "../types";
import { ANSWER_LANGUAGES, PROVIDERS, baseUrlPlaceholder, presetOf, providerLabel } from "./presets";

export default function AiSettingsForm(props: { status: AiStatus; onSaved: (s: AiStatus) => void }) {
  const [config, setConfig] = useState<AiConfig>(() => ({
    ...props.status.config,
    provider: props.status.config.provider || "anthropic",
    model: props.status.config.model || (presetOf("anthropic")?.models[0] ?? ""),
  }));
  // undefined: keep the stored key; "": remove it.
  const [apiKey, setApiKey] = useState<string | undefined>(undefined);
  const [hasKey, setHasKey] = useState(props.status.hasKey);
  const [savedProvider, setSavedProvider] = useState(props.status.config.provider);
  const [localModels, setLocalModels] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const preset = presetOf(config.provider);
  const providerChanged = config.provider !== savedProvider;
  const models = [...new Set([...(preset?.models ?? []), ...localModels])];

  const loadLocal = () => {
    setMessage(null);
    api
      .aiListModels(config)
      .then((list) => {
        setLocalModels(list);
        if (list.length && !config.model) setConfig((c) => ({ ...c, model: list[0] }));
        setMessage(list.length ? { ok: true, text: t("ai.localFound", { n: list.length }) } : { ok: false, text: t("ai.localNone") });
      })
      .catch((e) => setMessage({ ok: false, text: errorText(e) }));
  };
  useEffect(() => {
    if (config.provider === "ollama") loadLocal();
    // Only when switching to Ollama.
  }, [config.provider]);

  const pickProvider = (id: string) => {
    const p = presetOf(id);
    setConfig((c) => ({ ...c, provider: id, model: p?.models[0] ?? "", baseUrl: null }));
    setApiKey(undefined);
    setLocalModels([]);
    setMessage(null);
    // Keys are stored per service; another service's key is unknown until saved.
    setHasKey(id === props.status.config.provider ? props.status.hasKey : false);
  };

  const save = async () => {
    const saved = await api.aiSetConfig(config, apiKey);
    props.onSaved(saved);
    setHasKey(saved.hasKey);
    setSavedProvider(saved.config.provider);
    setApiKey(undefined);
    return saved;
  };

  const run = async (test: boolean) => {
    setBusy(true);
    setMessage(null);
    try {
      // The test runs against the saved settings.
      await save();
      const text = test ? t("ai.testOk", { reply: await api.aiTest() }) : t("ai.saved");
      setMessage({ ok: true, text });
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  const needsKey = preset?.needsKey ?? false;
  const needsUrl = config.provider === "custom";
  const canSave = !!config.provider && !!config.model.trim() && (!needsUrl || !!config.baseUrl?.trim());

  return (
    <div className="settings-form">
      <label className="field">
        <span>{t("ai.provider")}</span>
        <select value={config.provider} onChange={(e) => pickProvider(e.target.value)}>
          {PROVIDERS.map((p) => (
            <option key={p.id} value={p.id}>
              {providerLabel(p)}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>{t("ai.model")}</span>
        <div className="inline-add">
          <input
            list="ai-models"
            value={config.model}
            placeholder={t("ai.modelPlaceholder")}
            onChange={(e) => setConfig((c) => ({ ...c, model: e.target.value }))}
          />
          {config.provider === "ollama" && <button onClick={loadLocal}>{t("common.refresh")}</button>}
        </div>
        <datalist id="ai-models">
          {models.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
      </label>
      {(needsKey || needsUrl) && (
        <label className="field">
          <span>
            {t("ai.apiKey")}
            {!needsKey && t("common.optional")}
            {hasKey && apiKey === undefined && !providerChanged && <span className="key-saved">{t("ai.keySaved")}</span>}
          </span>
          <div className="inline-add">
            <input
              type="password"
              autoComplete="off"
              value={apiKey ?? ""}
              placeholder={hasKey && !providerChanged ? t("ai.keyKeep") : (preset?.keyHint ?? "")}
              onChange={(e) => setApiKey(e.target.value)}
            />
            {hasKey && !providerChanged && (
              <button onClick={() => (setApiKey(""), setHasKey(false))} title={t("ai.keyClear")}>
                {t("common.clear")}
              </button>
            )}
          </div>
        </label>
      )}
      {(needsUrl || preset?.local || config.baseUrl) && (
        <label className="field">
          <span>
            {t("ai.baseUrl")}
            {!needsUrl && t("common.optional")}
          </span>
          <input
            value={config.baseUrl ?? ""}
            placeholder={baseUrlPlaceholder(preset)}
            onChange={(e) => setConfig((c) => ({ ...c, baseUrl: e.target.value || null }))}
          />
        </label>
      )}
      <label className="field">
        <span>{t("ai.language")}</span>
        <select value={config.language} onChange={(e) => setConfig((c) => ({ ...c, language: e.target.value }))}>
          {[...new Set([...ANSWER_LANGUAGES, config.language])].map((l) => (
            <option key={l} value={l}>
              {l === "auto" ? t("ai.languageAuto") : l}
            </option>
          ))}
        </select>
      </label>
      <p className="muted small">{preset?.local ? t("ai.localNote") : t("ai.cloudNote")}</p>
      <div className="form-actions">
        {message && <span className={message.ok ? "ok-text small" : "error small"}>{message.text}</span>}
        <span className="spacer" />
        <button onClick={() => run(true)} disabled={!canSave || busy}>
          {busy ? t("ai.testing") : t("ai.test")}
        </button>
        <button className="primary" disabled={!canSave || busy} onClick={() => run(false)}>
          {t("common.save")}
        </button>
      </div>
    </div>
  );
}
