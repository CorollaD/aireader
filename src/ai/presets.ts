// Services offered in AI settings. `id` is the rig vendor id ("custom" = any OpenAI-compatible URL).

export interface ProviderPreset {
  id: string;
  name: string;
  models: string[];
  needsKey: boolean;
  /** Shown as the base URL placeholder; required for "custom". */
  baseUrlHint?: string;
  local?: boolean;
  keyHint?: string;
}

export const PROVIDERS: ProviderPreset[] = [
  { id: "anthropic", name: "Claude（Anthropic）", models: ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"], needsKey: true, keyHint: "sk-ant-…" },
  { id: "openai", name: "OpenAI", models: ["gpt-5.5", "gpt-5.4-mini", "gpt-5.4-nano"], needsKey: true, keyHint: "sk-…" },
  { id: "deepseek", name: "DeepSeek", models: ["deepseek-v4-flash", "deepseek-v4-pro"], needsKey: true },
  { id: "moonshot", name: "Kimi（Moonshot）", models: ["kimi-k3", "kimi-k2.6"], needsKey: true },
  { id: "zai", name: "智谱 GLM", models: ["glm-5.3", "glm-5.3-flash"], needsKey: true },
  { id: "gcp.gemini", name: "Gemini", models: ["gemini-3.8-flash", "gemini-flash-latest"], needsKey: true },
  { id: "openrouter", name: "OpenRouter", models: ["anthropic/claude-sonnet-5-5", "deepseek/deepseek-v4-flash"], needsKey: true },
  { id: "ollama", name: "Ollama（本地）", models: [], needsKey: false, local: true, baseUrlHint: "http://localhost:11434" },
  { id: "llamacpp", name: "llama.cpp（本地）", models: ["local"], needsKey: false, local: true, baseUrlHint: "http://localhost:8080" },
  {
    id: "custom",
    name: "自定义 OpenAI 兼容服务",
    models: [],
    needsKey: false,
    baseUrlHint: "例如 https://dashscope.aliyuncs.com/compatible-mode/v1",
  },
];

export const LANGUAGES = ["中文", "English", "日本語"];

export const presetOf = (id: string) => PROVIDERS.find((p) => p.id === id);
