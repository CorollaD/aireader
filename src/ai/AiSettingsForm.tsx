// The AI tab of the Settings dialog: service, model, key, endpoint and answer language.

import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import { t } from "../i18n";
import type { AiConfig, AiStatus, EmbeddingConfig } from "../types";

/** Suggested embedding models; bge-m3 handles Chinese and other languages well. */
const EMBED_MODELS: Record<string, string[]> = {
  ollama: ["bge-m3", "nomic-embed-text", "mxbai-embed-large"],
  openai: ["text-embedding-3-small", "text-embedding-3-large"],
  custom: ["BAAI/bge-m3", "text-embedding-v3"],
};
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
  // undefined: keep the stored key; "": remove it.
  const [embedKey, setEmbedKey] = useState<string | undefined>(undefined);
  const [hasEmbedKey, setHasEmbedKey] = useState(props.status.hasEmbeddingKey);
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
    const saved = await api.aiSetConfig(config, apiKey, embedKey);
    props.onSaved(saved);
    setHasKey(saved.hasKey);
    setHasEmbedKey(saved.hasEmbeddingKey);
    setEmbedKey(undefined);
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
  const embed = config.embedding;
  const setEmbed = (patch: Partial<EmbeddingConfig>) => setConfig((c) => ({ ...c, embedding: { ...c.embedding, ...patch } }));
  const embedOk = !embed.provider || (!!embed.model.trim() && (embed.provider !== "custom" || !!embed.baseUrl?.trim()));
  const canSave = !!config.provider && !!config.model.trim() && (!needsUrl || !!config.baseUrl?.trim()) && embedOk;

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

      <div className="panel-heading">{t("ai.embedTitle")}</div>
      <p className="muted small hint">{t("ai.embedHint")}</p>
      <label className="field">
        <span>{t("ai.provider")}</span>
        <select
          value={embed.provider}
          onChange={(e) => {
            const provider = e.target.value;
            setEmbed({ provider, model: EMBED_MODELS[provider]?.[0] ?? "", baseUrl: null });
            setEmbedKey(undefined);
          }}
        >
          <option value="">{t("ai.embedNone")}</option>
          <option value="ollama">{t("ai.local", { name: "Ollama" })}</option>
          <option value="openai">OpenAI</option>
          <option value="custom">{t("ai.custom")}</option>
        </select>
      </label>
      {embed.provider && (
        <>
          <label className="field">
            <span>{t("ai.model")}</span>
            <input list="embed-models" value={embed.model} placeholder={t("ai.modelPlaceholder")} onChange={(e) => setEmbed({ model: e.target.value })} />
            <datalist id="embed-models">
              {(EMBED_MODELS[embed.provider] ?? []).map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
          </label>
          {embed.provider !== "openai" && (
            <label className="field">
              <span>
                {t("ai.baseUrl")}
                {embed.provider !== "custom" && t("common.optional")}
              </span>
              <input
                value={embed.baseUrl ?? ""}
                placeholder={embed.provider === "ollama" ? "http://localhost:11434" : t("ai.customUrlHint", { url: "https://api.siliconflow.cn/v1" })}
                onChange={(e) => setEmbed({ baseUrl: e.target.value || null })}
              />
            </label>
          )}
          {embed.provider !== "ollama" && (
            <label className="field">
              <span>
                {t("ai.apiKey")}
                {embed.provider === "custom" && t("common.optional")}
                {hasEmbedKey && embedKey === undefined && <span className="key-saved">{t("ai.keySaved")}</span>}
              </span>
              <div className="inline-add">
                <input
                  type="password"
                  autoComplete="off"
                  value={embedKey ?? ""}
                  placeholder={hasEmbedKey ? t("ai.keyKeep") : embed.provider === "openai" ? "sk-…" : ""}
                  onChange={(e) => setEmbedKey(e.target.value)}
                />
                {hasEmbedKey && (
                  <button onClick={() => (setEmbedKey(""), setHasEmbedKey(false))} title={t("ai.keyClear")}>
                    {t("common.clear")}
                  </button>
                )}
              </div>
            </label>
          )}
        </>
      )}
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
