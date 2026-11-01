// Online catalogs (OPDS): browse or search a catalog and download books to the shelf.

import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorText } from "../api";
import { t } from "../i18n";
import { Icon } from "../icons";
import type { OpdsCatalog, OpdsDownload, OpdsEntry, OpdsFeed } from "../types";
import { Confirm, Modal, Toast } from "../ui";
import SourcesDialog from "./SourcesDialog";
import type { Source } from "./sources";

/** A page of a catalog on the way back up. */
interface Crumb {
  title: string;
  feed: OpdsFeed;
}

/** A cover from the catalog, fetched through the backend once it scrolls into view. */
function RemoteCover({ catalogId, url }: { catalogId: string; url: string | null }) {
  const box = useRef<HTMLDivElement>(null);
  const [src, setSrc] = useState<string | null>(url?.startsWith("data:") ? url : null);
  useEffect(() => {
    if (!url || src || !box.current) return;
    const seen = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      seen.disconnect();
      api
        .opdsImage(catalogId, url)
        .then(setSrc)
        .catch(() => {});
    });
    seen.observe(box.current);
    return () => seen.disconnect();
  }, [catalogId, url, src]);
  return (
    <div className="opds-cover" ref={box}>
      {src ? <img src={src} alt="" draggable={false} /> : <Icon name="bookOpen" size={18} />}
    </div>
  );
}

function CatalogDialog(props: { catalog: Partial<OpdsCatalog>; onSave: (c: OpdsCatalog, password?: string) => void; onClose: () => void }) {
  const [title, setTitle] = useState(props.catalog.title ?? "");
  const [url, setUrl] = useState(props.catalog.url ?? "");
  const [username, setUsername] = useState(props.catalog.username ?? "");
  const [password, setPassword] = useState<string | undefined>(undefined);
  const insecure = url.trim().startsWith("http://") && !!username.trim();
  return (
    <Modal
      title={props.catalog.id ? t("opds.edit") : t("opds.add")}
      width={460}
      onClose={props.onClose}
      footer={
        <>
          <button onClick={props.onClose}>{t("common.cancel")}</button>
          <button
            className="primary"
            disabled={!url.trim()}
            onClick={() =>
              props.onSave(
                { id: props.catalog.id ?? "", title: title.trim() || url.trim(), url: url.trim(), username, kind: props.catalog.kind ?? "" },
                password,
              )
            }
          >
            {t("common.save")}
          </button>
        </>
      }
    >
      <div className="settings-form">
        <label className="field">
          <span>{t("opds.address")}</span>
          <input value={url} placeholder="http://192.168.1.2:8080/opds" onChange={(e) => setUrl(e.target.value)} />
        </label>
        <label className="field">
          <span>{t("opds.name")}</span>
          <input value={title} onChange={(e) => setTitle(e.target.value)} />
        </label>
        <label className="field">
          <span>
            {t("opds.username")}
            {t("common.optional")}
          </span>
          <input value={username} autoComplete="off" onChange={(e) => setUsername(e.target.value)} />
        </label>
        <label className="field">
          <span>
            {t("opds.password")}
            {t("common.optional")}
          </span>
          <input
            type="password"
            autoComplete="off"
            value={password ?? ""}
            placeholder={props.catalog.username ? t("ai.keyKeep") : ""}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        {insecure && <p className="error small">{t("opds.insecure")}</p>}
        <p className="muted small hint">{t("opds.hint")}</p>
      </div>
    </Modal>
  );
}

function EntryRow(props: {
  entry: OpdsEntry;
  catalogId: string;
  onFollow: (entry: OpdsEntry) => void;
  onDownload: (entry: OpdsEntry, d: OpdsDownload) => Promise<string>;
  onOpen: (bookId: string) => void;
}) {
  const { entry } = props;
  const [state, setState] = useState<{ busy?: string; bookId?: string }>({});
  const download = async (d: OpdsDownload) => {
    setState({ busy: d.url });
    try {
      setState({ bookId: await props.onDownload(entry, d) });
    } catch {
      setState({});
    }
  };
  return (
    <div className="opds-entry">
      <RemoteCover catalogId={props.catalogId} url={entry.cover} />
      <div className="opds-main">
        {entry.link ? (
          <button className="opds-link" onClick={() => props.onFollow(entry)}>
            <span className="book-title">{entry.title}</span>
            <Icon name="chevronRight" size={14} />
          </button>
        ) : (
          <div className="book-title">{entry.title}</div>
        )}
        {entry.author && <div className="muted small">{entry.author}</div>}
        {entry.summary && <div className="muted small opds-summary">{entry.summary}</div>}
        {entry.downloads.length === 0 && !entry.link && <div className="muted small">{t("opds.noDownload")}</div>}
        {entry.downloads.length > 0 && (
          <div className="opds-downloads">
            {state.bookId ? (
              <button className="primary" onClick={() => props.onOpen(state.bookId!)}>
                {t("shelf.open")}
              </button>
            ) : (
              entry.downloads.map((d) => (
                <button key={d.url} disabled={!!state.busy} title={d.title ?? undefined} onClick={() => download(d)}>
                  {state.busy === d.url ? t("opds.downloading") : d.format}
                </button>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export default function OpdsView(props: { onBack: () => void; onOpen: (bookId: string) => void }) {
  const [catalogs, setCatalogs] = useState<OpdsCatalog[] | null>(null);
  const [current, setCurrent] = useState<string | null>(null);
  const [crumbs, setCrumbs] = useState<Crumb[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [dialog, setDialog] = useState<Partial<OpdsCatalog> | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);
  const [sources, setSources] = useState(false);
  const page = crumbs[crumbs.length - 1] ?? null;

  /** Runs a request for a feed, showing progress and errors. */
  const load = useCallback(async (request: Promise<OpdsFeed>, apply: (feed: OpdsFeed) => void) => {
    setBusy(true);
    setError(null);
    try {
      apply(await request);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const open = useCallback(
    (id: string) => {
      setCurrent(id);
      setCrumbs([]);
      setQuery("");
      load(api.opdsFeed(id, null), (feed) => setCrumbs([{ title: feed.title, feed }]));
    },
    [load],
  );

  useEffect(() => {
    api.opdsCatalogs().then(
      (list) => {
        setCatalogs(list);
        if (list[0]) open(list[0].id);
      },
      (e) => {
        setCatalogs([]);
        setError(errorText(e));
      },
    );
  }, [open]);

  const save = async (catalog: OpdsCatalog, password?: string) => {
    setDialog(null);
    try {
      const list = await api.opdsSaveCatalog(catalog, password);
      setCatalogs(list);
      const saved = list.find((c) => c.id === catalog.id) ?? list[list.length - 1];
      open(saved.id);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const addSource = (source: Source) => {
    setSources(false);
    save({ id: "", username: "", ...source });
  };

  const remove = async () => {
    if (!current) return;
    const list = await api.opdsRemoveCatalog(current);
    setCatalogs(list);
    setCrumbs([]);
    setCurrent(null);
    if (list[0]) open(list[0].id);
  };

  const follow = (entry: OpdsEntry) => {
    if (current && entry.link) load(api.opdsFeed(current, entry.link), (feed) => setCrumbs((c) => [...c, { title: entry.title, feed }]));
  };

  const more = () => {
    if (!current || !page?.feed.next) return;
    load(api.opdsFeed(current, page.feed.next), (feed) =>
      setCrumbs((c) => [...c.slice(0, -1), { ...page, feed: { ...feed, entries: [...page.feed.entries, ...feed.entries] } }]),
    );
  };

  const search = () => {
    const q = query.trim();
    const template = crumbs[0]?.feed.search ?? page?.feed.search;
    if (!current || !q || !template) return;
    load(api.opdsSearch(current, template, q), (feed) => setCrumbs((c) => [...c.slice(0, 1), { title: t("opds.results", { query: q }), feed }]));
  };

  const download = async (entry: OpdsEntry, d: OpdsDownload) => {
    try {
      const book = await api.opdsDownload(current!, d.url, entry.title, d.extension);
      setToast(book.thin ? t("opds.thin") : book.added ? t("opds.added") : t("opds.already"));
      return book.id;
    } catch (e) {
      setToast(errorText(e));
      throw e;
    }
  };

  const catalog = catalogs?.find((c) => c.id === current) ?? null;
  const searchable = !!(crumbs[0]?.feed.search ?? page?.feed.search);

  return (
    <div className="review-view">
      <header className="review-head">
        <button onClick={props.onBack}>
          <Icon name="arrowLeft" size={15} /> {t("reader.shelf")}
        </button>
        <span className="review-title">{t("opds.title")}</span>
        {catalogs && catalogs.length > 0 && (
          <select value={current ?? ""} aria-label={t("opds.title")} onChange={(e) => open(e.target.value)}>
            {catalogs.map((c) => (
              <option key={c.id} value={c.id}>
                {c.title}
              </option>
            ))}
          </select>
        )}
        <span className="spacer" />
        {catalog && (
          <>
            <button className="icon-button" title={t("opds.edit")} aria-label={t("opds.edit")} onClick={() => setDialog(catalog)}>
              <Icon name="edit" size={15} />
            </button>
            <button className="icon-button" title={t("common.delete")} aria-label={t("common.delete")} onClick={() => setRemoving(true)}>
              <Icon name="close" size={15} />
            </button>
          </>
        )}
        <button onClick={() => setSources(true)}>
          <Icon name="library" size={14} /> {t("opds.sources")}
        </button>
        <button onClick={() => setDialog({})}>
          <Icon name="plus" size={14} /> {t("opds.add")}
        </button>
      </header>

      <div className="opds-page">
        {catalogs?.length === 0 && (
          <div className="review-done">
            <div className="ai-welcome-title">{t("opds.emptyTitle")}</div>
            <p className="muted small">{t("opds.emptyHint")}</p>
            <button className="primary" onClick={() => setSources(true)}>
              {t("opds.sources")}
            </button>
          </div>
        )}
        {crumbs.length > 0 && (
          <div className="opds-bar">
            {crumbs.length > 1 && (
              <button className="icon-button" title={t("reader.back")} aria-label={t("reader.back")} onClick={() => setCrumbs((c) => c.slice(0, -1))}>
                <Icon name="arrowLeft" size={15} />
              </button>
            )}
            <span className="review-title ellipsis">{page?.title}</span>
            <span className="spacer" />
            {searchable && (
              <input
                className="opds-search"
                placeholder={t("opds.search")}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && search()}
              />
            )}
          </div>
        )}
        {error && <div className="error small opds-error">{error}</div>}
        {page?.feed.entries.length === 0 && !busy && (
          <div className="empty muted">{crumbs.length === 1 && searchable ? t("opds.searchPrompt") : t("opds.noEntries")}</div>
        )}
        {current &&
          page?.feed.entries.map((entry, i) => (
            <EntryRow key={entry.id || i} entry={entry} catalogId={current} onFollow={follow} onDownload={download} onOpen={props.onOpen} />
          ))}
        {busy && <div className="center-msg muted">{t("common.loading")}</div>}
        {page?.feed.next && !busy && (
          <button className="opds-more" onClick={more}>
            {t("opds.more")}
          </button>
        )}
      </div>

      {removing && catalog && (
        <Confirm
          title={t("opds.remove")}
          danger
          confirmText={t("common.delete")}
          message={<p>{t("opds.removeHint", { name: catalog.title })}</p>}
          onConfirm={remove}
          onClose={() => setRemoving(false)}
        />
      )}
      {dialog && <CatalogDialog catalog={dialog} onSave={save} onClose={() => setDialog(null)} />}
      {sources && <SourcesDialog catalogs={catalogs ?? []} onAdd={addSource} onClose={() => setSources(false)} />}
      {toast && <Toast message={toast} onDone={() => setToast(null)} />}
    </div>
  );
}
