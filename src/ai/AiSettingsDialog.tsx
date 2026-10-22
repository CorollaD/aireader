import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import type { AiConfig, AiStatus } from "../types";
import { Modal } from "../ui";
import { LANGUAGES, PROVIDERS, presetOf } from "./presets";

export default function AiSettingsDialog(props: { status: AiStatus; onSaved: (s: AiStatus) => void; onClose: () => void }) {
  const [config, setConfig] = useState<AiConfig>(() => ({
    ...props.status.config,
    provider: props.status.config.provider || "anthropic",
    model: props.status.config.model || (presetOf("anthropic")?.models[0] ?? ""),
  }));
  // undefined: keep the stored key; "": remove it.
  const [apiKey, setApiKey] = useState<string | undefined>(undefined);
  const [hasKey, setHasKey] = useState(props.status.hasKey);
  const [localModels, setLocalModels] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const preset = presetOf(config.provider);
  const providerChanged = config.provider !== props.status.config.provider;
  const models = [...new Set([...(preset?.models ?? []), ...localModels])];

  const loadLocal = () => {
    setMessage(null);
    api
      .aiListModels(config)
      .then((list) => {
        setLocalModels(list);
        if (list.length && !config.model) setConfig((c) => ({ ...c, model: list[0] }));
        setMessage(list.length ? { ok: true, text: `找到 ${list.length} 个本地模型` } : { ok: false, text: "没有找到已安装的模型" });
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
    setApiKey(undefined);
    return saved;
  };

  const test = async () => {
    setBusy(true);
    setMessage(null);
    try {
      // The test runs against the saved settings.
      await save();
      const reply = await api.aiTest();
      setMessage({ ok: true, text: `连接成功：${reply}` });
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
    <Modal
      title="AI 设置"
      width={500}
      onClose={props.onClose}
      footer={
        <>
          <button onClick={test} disabled={!canSave || busy}>
            {busy ? "测试中…" : "测试连接"}
          </button>
          <span className="spacer" />
          <button onClick={props.onClose}>取消</button>
          <button
            className="primary"
            disabled={!canSave || busy}
            onClick={() =>
              save()
                .then(props.onClose)
                .catch((e) => setMessage({ ok: false, text: errorText(e) }))
            }
          >
            保存
          </button>
        </>
      }
    >
      <label className="field">
        <span>服务</span>
        <select value={config.provider} onChange={(e) => pickProvider(e.target.value)}>
          {PROVIDERS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>模型</span>
        <div className="inline-add">
          <input
            list="ai-models"
            value={config.model}
            placeholder="模型名称"
            onChange={(e) => setConfig((c) => ({ ...c, model: e.target.value }))}
          />
          {config.provider === "ollama" && <button onClick={loadLocal}>刷新</button>}
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
            API Key{!needsKey && "（可选）"}
            {hasKey && apiKey === undefined && !providerChanged && <span className="key-saved">已保存在系统钥匙串</span>}
          </span>
          <div className="inline-add">
            <input
              type="password"
              autoComplete="off"
              value={apiKey ?? ""}
              placeholder={hasKey && !providerChanged ? "留空则保持不变" : preset?.keyHint ?? ""}
              onChange={(e) => setApiKey(e.target.value)}
            />
            {hasKey && !providerChanged && (
              <button onClick={() => (setApiKey(""), setHasKey(false))} title="从钥匙串删除">
                清除
              </button>
            )}
          </div>
        </label>
      )}
      {(needsUrl || preset?.local || config.baseUrl) && (
        <label className="field">
          <span>接口地址{!needsUrl && "（可选）"}</span>
          <input
            value={config.baseUrl ?? ""}
            placeholder={preset?.baseUrlHint ?? "使用默认地址"}
            onChange={(e) => setConfig((c) => ({ ...c, baseUrl: e.target.value || null }))}
          />
        </label>
      )}
      <label className="field">
        <span>回答语言</span>
        <select value={config.language} onChange={(e) => setConfig((c) => ({ ...c, language: e.target.value }))}>
          {[...new Set([config.language, ...LANGUAGES])].map((l) => (
            <option key={l}>{l}</option>
          ))}
        </select>
      </label>
      <p className="muted small">
        {preset?.local
          ? "本地模型在你的电脑上运行，书籍内容不会离开本机。"
          : "使用云端服务时，所需的书籍文字会发送给该服务处理，并按其价格计费。API Key 只保存在系统钥匙串中。"}
      </p>
      {message && <p className={message.ok ? "ok-text small" : "error small"}>{message.text}</p>}
    </Modal>
  );
}
