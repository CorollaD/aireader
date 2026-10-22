// AI status shared by the shelf and the reader, plus the privacy dialog.

import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { api } from "../api";
import { t } from "../i18n";
import type { AiStatus } from "../types";
import { Modal } from "../ui";
import { presetOf, providerLabel } from "./presets";

interface AiContextValue {
  status: AiStatus | null;
  /** Configured and usable (key present when the service needs one). */
  ready: boolean;
  openSettings: () => void;
  refresh: () => void;
  /** Replaces the status after the settings were saved. */
  setStatus: (s: AiStatus) => void;
  /** Resolves true once the user has agreed to send text to a cloud service (asks if needed). */
  ensureConsent: () => Promise<boolean>;
}

function providerName(id: string) {
  const p = presetOf(id);
  return p ? providerLabel(p) : id;
}

const Ctx = createContext<AiContextValue | null>(null);

export function useAi(): AiContextValue {
  const v = useContext(Ctx);
  if (!v) throw new Error("useAi outside AiProvider");
  return v;
}

/** `openSettings` shows the AI tab of the Settings dialog, which lives in App. */
export function AiProvider({ children, openSettings }: { children: ReactNode; openSettings: () => void }) {
  const [status, setStatus] = useState<AiStatus | null>(null);
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
    <Ctx.Provider value={{ status, ready, openSettings, refresh, setStatus, ensureConsent }}>
      {children}
      {consentAsk && (
        <Modal
          title={t("ai.consentTitle")}
          width={440}
          onClose={() => answerConsent(false)}
          footer={
            <>
              <button onClick={() => answerConsent(false)}>{t("common.cancel")}</button>
              <button className="primary" autoFocus onClick={() => answerConsent(true)}>
                {t("ai.consentAgree")}
              </button>
            </>
          }
        >
          <p>{t("ai.consentBody", { provider: providerName(status?.config.provider ?? "") })}</p>
          <p className="muted small" style={{ marginTop: 8 }}>
            {t("ai.consentHint")}
          </p>
        </Modal>
      )}
    </Ctx.Provider>
  );
}
