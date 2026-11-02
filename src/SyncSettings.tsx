// The Sync tab of the Settings dialog: where to sync, and how the last sync went.

import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, errorText } from "./api";
import { formatDateTime, t } from "./i18n";
import type { ServerInfo, SyncConfig, SyncStatus } from "./types";

const BLANK: SyncConfig = { backend: "", url: "", username: "", folder: "", files: false };

export default function SyncSettings() {
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [form, setForm] = useState<SyncConfig>(BLANK);
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  // What the AiReader server at the typed address allows; null while unknown.
  const [info, setInfo] = useState<ServerInfo | null>(null);

  const reload = useCallback(() => {
    api
      .syncStatus()
      .then(setStatus)
      .catch(() => {});
  }, []);

  useEffect(() => {
    api
      .syncStatus()
      .then((s) => {
        setStatus(s);
        setForm(s.config);
      })
      .catch(() => {});
    const un = listen("sync-status", reload);
    return () => {
      un.then((f) => f());
    };
  }, [reload]);

  const serverUrl = form.backend === "server" ? form.url.trim() : "";
  useEffect(() => {
    setInfo(null);
    if (!/^https?:\/\/./.test(serverUrl)) return;
    const timer = window.setTimeout(() => {
      api
        .syncServerInfo(serverUrl)
        .then(setInfo)
        .catch(() => setInfo(null));
    }, 500);
    return () => window.clearTimeout(timer);
  }, [serverUrl]);

  const patch = (p: Partial<SyncConfig>) => {
    setForm((f) => ({ ...f, ...p }));
    setMessage(null);
  };
  const cleaned = (): SyncConfig => ({ ...form, url: form.url.trim(), username: form.username.trim() });
  const typed = () => password || null;

  const run = async (work: () => Promise<string | null>) => {
    setBusy(true);
    setMessage(null);
    try {
      const text = await work();
      if (text) setMessage({ ok: true, text });
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  const test = () =>
    run(async () => {
      await api.syncTest(cleaned(), typed());
      return t("sync.testOk");
    });

  const save = () =>
    run(async () => {
      setStatus(await api.syncSave(cleaned(), typed()));
      setPassword("");
      if (!form.backend) return t("sync.saved");
      await api.syncNow(false);
      return t("sync.done");
    });

  const syncNow = () =>
    run(async () => {
      await api.syncNow(false);
      return t("sync.done");
    });

  const signIn = (register: boolean) =>
    run(async () => {
      const next = await api.syncServerSignIn(cleaned(), password, register);
      setStatus(next);
      setForm(next.config);
      setPassword("");
      await api.syncNow(false);
      return t("sync.done");
    });

  const signOut = () =>
    run(async () => {
      const next = await api.syncServerSignOut();
      setStatus(next);
      setForm(next.config);
      return t("sync.signedOut");
    });

  const chooseFolder = async () => {
    const folder = await openDialog({ directory: true });
    if (typeof folder === "string") patch({ folder });
  };

  if (!status) return null;
  const saved = status.config;
  const changed = JSON.stringify(saved) !== JSON.stringify(cleaned()) || password !== "";
  const samePassword = status.hasPassword && saved.url === form.url.trim() && saved.username === form.username.trim();
  // Signed in to the server this form shows: then only the options can change.
  const signedIn = form.backend === "server" && status.signedIn && saved.backend === "server";
  const ready =
    form.backend === "server" ? signedIn : form.backend === "webdav" ? form.url.trim() !== "" : form.backend === "folder" ? form.folder !== "" : false;
  const canSignIn = form.url.trim() !== "" && form.username.trim() !== "" && password !== "";
  const backends = [
    ["", t("sync.off")],
    ["server", t("sync.server")],
    ["webdav", "WebDAV"],
    ["folder", t("sync.folder")],
  ];

  return (
    <>
      <div className="panel-section">
        <div className="panel-label">{t("sync.service")}</div>
        <div className="segmented">
          {backends.map(([value, label]) => (
            <button key={value} className={form.backend === value ? "on" : ""} onClick={() => patch({ backend: value })}>
              {label}
            </button>
          ))}
        </div>
        <p className="muted small hint">{t("sync.intro")}</p>
      </div>

      {form.backend === "server" && signedIn && (
        <div className="panel-section">
          <div className="signed-in">
            <div className="grow">
              <div>{t("sync.signedInAs", { name: saved.username })}</div>
              <div className="muted small ellipsis">{saved.url}</div>
            </div>
            <button disabled={busy} onClick={signOut}>
              {t("sync.signOut")}
            </button>
          </div>
        </div>
      )}

      {form.backend === "server" && !signedIn && (
        <div className="panel-section">
          <label className="field">
            <span>{t("sync.url")}</span>
            <input value={form.url} placeholder="https://sync.example.com" onChange={(e) => patch({ url: e.target.value })} />
          </label>
          <label className="field">
            <span>{t("sync.username")}</span>
            <input value={form.username} autoComplete="username" onChange={(e) => patch({ username: e.target.value })} />
          </label>
          <label className="field">
            <span>{t("sync.password")}</span>
            <input
              type="password"
              value={password}
              autoComplete="current-password"
              onChange={(e) => (setPassword(e.target.value), setMessage(null))}
              onKeyDown={(e) => e.key === "Enter" && canSignIn && !busy && signIn(false)}
            />
          </label>
          <p className="muted small hint">{t("sync.serverHint")}</p>
          <p className="muted small hint">{t("sync.serverPasswordHint")}</p>
          {form.url.trim().startsWith("http://") && <p className="error small">{t("sync.insecure")}</p>}
          <div className="inline-add form-actions">
            <button className="primary" disabled={busy || !canSignIn} onClick={() => signIn(false)}>
              {t("sync.signIn")}
            </button>
            {info?.registration && (
              <button disabled={busy || !canSignIn || password.length < info.minPassword} onClick={() => signIn(true)}>
                {t("sync.register")}
              </button>
            )}
          </div>
          {message && <p className={message.ok ? "ok-text small" : "error small"}>{message.text}</p>}
        </div>
      )}

      {form.backend === "webdav" && (
        <div className="panel-section">
          <label className="field">
            <span>{t("sync.url")}</span>
            <input value={form.url} placeholder="https://dav.jianguoyun.com/dav/" onChange={(e) => patch({ url: e.target.value })} />
          </label>
          <label className="field">
            <span>{t("sync.username")}</span>
            <input value={form.username} autoComplete="off" onChange={(e) => patch({ username: e.target.value })} />
          </label>
          <label className="field">
            <span>{t("sync.password")}</span>
            <input
              type="password"
              value={password}
              autoComplete="off"
              placeholder={samePassword ? t("sync.passwordSaved") : ""}
              onChange={(e) => (setPassword(e.target.value), setMessage(null))}
            />
          </label>
          <p className="muted small hint">{t("sync.passwordHint")}</p>
          {form.url.trim().startsWith("http://") && <p className="error small">{t("sync.insecure")}</p>}
        </div>
      )}

      {form.backend === "folder" && (
        <div className="panel-section">
          <div className="panel-label">{t("sync.chosenFolder")}</div>
          <div className="inline-add">
            <span className="ellipsis grow small">{form.folder || t("sync.noFolder")}</span>
            <button onClick={chooseFolder}>{t("sync.chooseFolder")}</button>
          </div>
          <p className="muted small hint">{t("sync.folderHint")}</p>
        </div>
      )}

      {form.backend && (form.backend !== "server" || signedIn) && (
        <div className="panel-section">
          <label className="toggle">
            <input type="checkbox" checked={form.files} onChange={(e) => patch({ files: e.target.checked })} />
            <span>{t("sync.files")}</span>
          </label>
          <p className="muted small hint">{t("sync.filesHint")}</p>
          <p className="muted small hint">{t("sync.notes")}</p>
        </div>
      )}

      <div className="panel-section" hidden={form.backend === "server" && !signedIn}>
        <div className="inline-add">
          {form.backend && (
            <button disabled={busy || !ready} onClick={test}>
              {t("sync.test")}
            </button>
          )}
          <button className="primary" disabled={busy || !changed || (!!form.backend && !ready)} onClick={save}>
            {t("common.save")}
          </button>
          {saved.backend && !changed && (
            <button disabled={busy || status.running} onClick={syncNow}>
              {t("sync.now")}
            </button>
          )}
        </div>
        {saved.backend && (
          <p className="muted small hint">
            {status.running ? t("sync.running") : status.lastSync ? t("sync.last", { time: formatDateTime(status.lastSync) }) : t("sync.never")}
          </p>
        )}
        {message && <p className={message.ok ? "ok-text small" : "error small"}>{message.text}</p>}
        {!message && status.error && <p className="error small">{status.error}</p>}
      </div>
    </>
  );
}
