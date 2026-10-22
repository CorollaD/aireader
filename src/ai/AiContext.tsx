// AI status shared by the shelf and the reader, plus the settings and privacy dialogs.

import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { api } from "../api";
import type { AiStatus } from "../types";
import { Modal } from "../ui";
import AiSettingsDialog from "./AiSettingsDialog";
import { presetOf } from "./presets";

interface AiContextValue {
  status: AiStatus | null;
  /** Configured and usable (key present when the service needs one). */
  ready: boolean;
  openSettings: () => void;
  refresh: () => void;
  /** Resolves true once the user has agreed to send text to a cloud service (asks if needed). */
  ensureConsent: () => Promise<boolean>;
}

const Ctx = createContext<AiContextValue | null>(null);

export function useAi(): AiContextValue {
  const v = useContext(Ctx);
  if (!v) throw new Error("useAi outside AiProvider");
  return v;
}

export function AiProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [consentAsk, setConsentAsk] = useState(false);
  const consentResolve = useRef<((ok: boolean) => void) | null>(null);

  const refresh = useCallback(() => {
    api.aiGetConfig().then(setStatus).catch(() => {});
  }, []);
  useEffect(refresh, [refresh]);

  const ready =
    !!status?.configured && (status.hasKey || status.local || !presetOf(status.config.provider)?.needsKey);

  const ensureConsent = useCallback(() => {
    if (!status || status.local || status.config.consented) return Promise.resolve(true);
    setConsentAsk(true);
    return new Promise<boolean>((resolve) => (consentResolve.current = resolve));
  }, [status]);

  const answerConsent = async (ok: boolean) => {
    setConsentAsk(false);
    if (ok && status) {
      const next = await api.aiSetConfig({ ...status.config, consented: true }).catch(() => null);
      if (next) setStatus(next);
    }
    consentResolve.current?.(ok);
    consentResolve.current = null;
  };

  return (
    <Ctx.Provider value={{ status, ready, openSettings: () => setSettingsOpen(true), refresh, ensureConsent }}>
      {children}
      {settingsOpen && status && (
        <AiSettingsDialog
          status={status}
          onSaved={setStatus}
          onClose={() => setSettingsOpen(false)}
        />
      )}
      {consentAsk && (
        <Modal
          title="发送书籍内容给 AI 服务？"
          width={440}
          onClose={() => answerConsent(false)}
          footer={
            <>
              <button onClick={() => answerConsent(false)}>取消</button>
              <button className="primary" autoFocus onClick={() => answerConsent(true)}>
                同意并继续
              </button>
            </>
          }
        >
          <p>
            AI 功能会把所需的书籍文字（章节内容、选中的文字、书名等）发送给「{presetOf(status?.config.provider ?? "")?.name ?? status?.config.provider}」处理，
            会产生该服务的用量费用，并受其隐私政策约束。
          </p>
          <p className="muted small" style={{ marginTop: 8 }}>
            不想上传内容，可以在 AI 设置中改用 Ollama 等本地模型。此确认只需一次。
          </p>
        </Modal>
      )}
    </Ctx.Provider>
  );
}
