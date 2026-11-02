//! Syncing reading data between devices.
//!
//! Every device publishes its data as files under `devices/<device>/` and only ever writes its
//! own folder, so two devices can't overwrite each other and nothing needs locking. A device
//! reads the others' files and merges them record by record, newest first; deletions travel as
//! tombstones. The database file itself is never copied.
//!
//! Layout under the sync root:
//! - `devices/<device>/library.json`: groups, book records and tombstones
//! - `devices/<device>/book-<id>.json`: a book's bookmarks, highlights and conversations
//! - `devices/<device>/cards.json`: flashcards and review history
//! - `devices/<device>/reading.json`: this device's reading time per day
//! - `covers/<id>.jpg`, `files/<id>.book`: shared; named by content, so they never conflict
//!
//! Settings and API keys are not synced.

pub mod commands;
pub mod remote;

use std::collections::HashSet;
use std::sync::Mutex;

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::annotations::{self, Bookmark, Highlight};
use crate::cards::{self, SyncCard, SyncReview};
use crate::db::{self, ChatRecord};
use crate::library::{self, Book, Library};
use crate::{recall, stats};
use remote::Remote;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sync_seen (
    device TEXT NOT NULL,
    file TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (device, file)
);
CREATE TABLE IF NOT EXISTS sync_sent (file TEXT PRIMARY KEY, hash TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS remote_reading (
    device TEXT NOT NULL,
    book_id TEXT NOT NULL,
    seconds INTEGER NOT NULL,
    PRIMARY KEY (device, book_id)
);
";

/// The folder made inside the server or chosen folder.
pub const ROOT: &str = "AiReader";
const VERSION: u32 = 1;
const LIBRARY: &str = "library.json";
const CARDS: &str = "cards.json";
const READING: &str = "reading.json";
const CONFIG: &str = "sync";
const LAST_SYNC: &str = "sync_last";
const MAX_COVER_BYTES: usize = 2 << 20;

/// Sync settings. The WebDAV password is in the system keychain.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncConfig {
    /// "" (off), "server" (an AiReader server), "webdav" or "folder".
    pub backend: String,
    pub url: String,
    pub username: String,
    pub folder: String,
    /// Also upload book files, for other devices to fetch.
    pub files: bool,
}

pub fn config(conn: &Connection) -> Result<SyncConfig> {
    Ok(db::kv_get(conn, CONFIG)?.unwrap_or_default())
}

pub fn set_config(conn: &Connection, config: &SyncConfig) -> Result<()> {
    db::kv_set(conn, CONFIG, config)
}

/// When the last sync finished (Unix seconds).
pub fn last_sync(conn: &Connection) -> Result<Option<u64>> {
    db::kv_get(conn, LAST_SYNC)
}

/// Whether two configurations talk to the same place, so the saved password belongs to both.
pub fn same_server(a: &SyncConfig, b: &SyncConfig) -> bool {
    a.url.trim() == b.url.trim() && a.username.trim() == b.username.trim()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tombstone {
    pub kind: String,
    pub id: String,
    pub at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncBook {
    #[serde(flatten)]
    book: Book,
    /// Reading time on the publishing device alone.
    own_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryFile {
    version: u32,
    groups: Vec<String>,
    groups_updated: u64,
    books: Vec<SyncBook>,
    tombstones: Vec<Tombstone>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct BookFile {
    bookmarks: Vec<Bookmark>,
    highlights: Vec<Highlight>,
    chats: Vec<ChatRecord>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CardsFile {
    cards: Vec<SyncCard>,
    reviews: Vec<SyncReview>,
    highlight_reviews: Vec<recall::SyncReview>,
}

fn book_file(id: &str) -> String {
    format!("book-{id}.json")
}

/// Book ids come from the sync folder and end up in local paths, so only well-formed ones are
/// taken: 16 hex digits, the start of the content hash.
fn valid_book(b: &Book) -> bool {
    let hex = |s: &str| {
        s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    };
    let name = b.file_name.as_str();
    b.id.len() == 16
        && b.sha256.len() == 64
        && hex(&b.sha256)
        && b.sha256.starts_with(&b.id)
        && !name.is_empty()
        && name.len() < 256
        && !name.contains(['/', '\\', '\0'])
        && !name.starts_with('.')
}

/// Whether `incoming` should replace `local`. The tie-break on content makes every device pick
/// the same winner when two changes happen within a second.
fn newer<T: Serialize>(incoming: (u64, &T), local: (u64, &T)) -> bool {
    if incoming.0 != local.0 {
        return incoming.0 > local.0;
    }
    let text = |v: &T| serde_json::to_string(v).unwrap_or_default();
    text(incoming.1) > text(local.1)
}

fn tombstones(conn: &Connection) -> Result<Vec<Tombstone>> {
    let mut stmt = conn.prepare("SELECT kind, id, at FROM tombstones ORDER BY kind, id")?;
    let list = stmt
        .query_map([], |r| {
            Ok(Tombstone {
                kind: r.get(0)?,
                id: r.get(1)?,
                at: r.get::<_, i64>(2)? as u64,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(list)
}

/// When `id` was deleted, if it was.
fn deleted_at(conn: &Connection, kind: &str, id: &str) -> Result<Option<u64>> {
    Ok(conn
        .query_row(
            "SELECT at FROM tombstones WHERE kind = ?1 AND id = ?2",
            params![kind, id],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .map(|at| at as u64))
}

fn remote_seconds(conn: &Connection, book_id: &str) -> Result<u64> {
    let total: Option<i64> = conn.query_row(
        "SELECT sum(seconds) FROM remote_reading WHERE book_id = ?1",
        params![book_id],
        |r| r.get(0),
    )?;
    Ok(total.unwrap_or(0) as u64)
}

/// This device's data as files: (name, contents).
pub fn export(lib: &Library) -> Result<Vec<(String, Vec<u8>)>> {
    let db = lib.db().clone();
    let conn = db.conn();
    let mut files = Vec::new();
    let mut books = Vec::new();
    for book in lib.books() {
        let own_seconds = book
            .read_seconds
            .saturating_sub(remote_seconds(&conn, &book.id)?);
        let mut bookmarks = annotations::read(&conn, &book.id)?;
        bookmarks.bookmarks.sort_by(|a, b| a.id.cmp(&b.id));
        bookmarks.highlights.sort_by(|a, b| a.id.cmp(&b.id));
        let mut chats = db::list_chats(&conn, &book.id)?;
        chats.sort_by(|a, b| a.id.cmp(&b.id));
        if !(bookmarks.bookmarks.is_empty() && bookmarks.highlights.is_empty() && chats.is_empty())
        {
            let file = BookFile {
                bookmarks: bookmarks.bookmarks,
                highlights: bookmarks.highlights,
                chats,
            };
            files.push((book_file(&book.id), serde_json::to_vec(&file)?));
        }
        books.push(SyncBook { book, own_seconds });
    }
    books.sort_by(|a, b| a.book.id.cmp(&b.book.id));
    let library = LibraryFile {
        version: VERSION,
        groups: lib.groups().to_vec(),
        groups_updated: lib.groups_updated(),
        books,
        tombstones: tombstones(&conn)?,
    };
    files.push((LIBRARY.to_owned(), serde_json::to_vec(&library)?));
    let cards = CardsFile {
        cards: cards::sync_cards(&conn)?,
        reviews: cards::sync_reviews(&conn)?,
        highlight_reviews: recall::sync_reviews(&conn)?,
    };
    files.push((CARDS.to_owned(), serde_json::to_vec(&cards)?));
    files.push((
        READING.to_owned(),
        serde_json::to_vec(&stats::own_days(&conn)?)?,
    ));
    Ok(files)
}

/// A file read from another device.
pub struct Incoming {
    pub device: String,
    pub name: String,
    pub tag: String,
    pub bytes: Vec<u8>,
}

/// What a merge changed.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Merged {
    /// Books whose bookmarks or highlights changed.
    pub books: Vec<String>,
    /// Books, groups or reading time changed.
    pub shelf: bool,
    /// Flashcards changed.
    pub cards: bool,
}

impl Merged {
    pub fn is_empty(&self) -> bool {
        self.books.is_empty() && !self.shelf && !self.cards
    }
}

fn merge_library(
    lib: &mut Library,
    device: &str,
    file: LibraryFile,
    merged: &mut Merged,
) -> Result<()> {
    if file.version > VERSION {
        bail!(crate::t!("sync_newer_version"));
    }
    let db = lib.db().clone();
    let mut forgotten = Vec::new();
    {
        let conn = db.conn();
        for t in &file.tombstones {
            db::tombstone(&conn, &t.kind, &t.id, t.at)?;
            let at = t.at as i64;
            match t.kind.as_str() {
                "bookmark" | "highlight" => {
                    let sql = format!(
                        "DELETE FROM {}s WHERE id = ?1 AND updated <= ?2 RETURNING book_id",
                        t.kind
                    );
                    let book: Option<String> = conn
                        .query_row(&sql, params![t.id, at], |r| r.get(0))
                        .optional()?;
                    if let Some(book) = book.filter(|b| !merged.books.contains(b)) {
                        merged.books.push(book);
                    }
                }
                "chat" => {
                    conn.execute(
                        "DELETE FROM chats WHERE id = ?1 AND updated <= ?2",
                        params![t.id, at],
                    )?;
                }
                "card" => {
                    if conn.execute(
                        "DELETE FROM cards WHERE id = ?1 AND updated <= ?2",
                        params![t.id, at],
                    )? > 0
                    {
                        merged.cards = true;
                    }
                }
                "book" if lib.get(&t.id).is_some_and(|b| activity(b) <= t.at) => {
                    forgotten.push(t.id.clone());
                }
                _ => {}
            }
        }
    }
    if !forgotten.is_empty() {
        lib.forget(&forgotten);
        merged.shelf = true;
    }
    if lib.merge_groups(&file.groups, file.groups_updated) {
        merged.shelf = true;
    }
    for incoming in file.books {
        let book = &incoming.book;
        if !valid_book(book) {
            log::warn!("sync: ignoring a malformed book record");
            continue;
        }
        let conn = db.conn();
        if deleted_at(&conn, "book", &book.id)?.is_some_and(|at| activity(book) <= at) {
            continue;
        }
        let before: i64 = conn
            .query_row(
                "SELECT seconds FROM remote_reading WHERE device = ?1 AND book_id = ?2",
                params![device, book.id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        conn.execute(
            "INSERT OR REPLACE INTO remote_reading (device, book_id, seconds) VALUES (?1, ?2, ?3)",
            params![device, book.id, incoming.own_seconds as i64],
        )?;
        drop(conn);
        if lib.merge_book(book) {
            merged.shelf = true;
        }
        let delta = incoming.own_seconds as i64 - before;
        if delta != 0 {
            lib.add_seconds(&book.id, delta);
            merged.shelf = true;
        }
    }
    lib.save()
}

/// The last time anything happened to a book.
fn activity(b: &Book) -> u64 {
    b.added.max(b.last_read.unwrap_or(0)).max(b.meta_updated)
}

/// Merges one kind of record: newer ones win, and deleted ones stay deleted unless changed since.
fn merge_records<T: Serialize + Clone>(
    conn: &Connection,
    kind: &str,
    local: &[T],
    incoming: &[T],
    id: impl Fn(&T) -> &str,
    updated: impl Fn(&T) -> u64,
) -> Result<Vec<T>> {
    let mut out: Vec<T> = local.to_vec();
    for record in incoming {
        if deleted_at(conn, kind, id(record))?.is_some_and(|at| updated(record) <= at) {
            continue;
        }
        match out.iter_mut().find(|l| id(l) == id(record)) {
            Some(l) => {
                if newer((updated(record), record), (updated(l), &*l)) {
                    *l = record.clone();
                }
            }
            None => out.push(record.clone()),
        }
    }
    Ok(out)
}

fn merge_book_file(lib: &Library, id: &str, file: BookFile, merged: &mut Merged) -> Result<()> {
    if lib.get(id).is_none() {
        return Ok(());
    }
    let db = lib.db().clone();
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    let local = annotations::read(&tx, id)?;
    let ann = annotations::Annotations {
        bookmarks: merge_records(
            &tx,
            "bookmark",
            &local.bookmarks,
            &file.bookmarks,
            |b| &b.id,
            |b| b.updated,
        )?,
        highlights: merge_records(
            &tx,
            "highlight",
            &local.highlights,
            &file.highlights,
            |h| &h.id,
            |h| h.updated,
        )?,
    };
    if ann != local {
        annotations::store(&tx, id, &ann)?;
        merged.books.push(id.to_owned());
    }
    let chats = db::list_chats(&tx, id)?;
    for chat in merge_records(&tx, "chat", &chats, &file.chats, |c| &c.id, |c| c.updated)? {
        if !chats.contains(&chat) {
            db::save_chat(&tx, id, &chat)?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn merge_cards(conn: &Connection, file: CardsFile, merged: &mut Merged) -> Result<()> {
    let local = cards::sync_cards(conn)?;
    let all = merge_records(
        conn,
        "card",
        &local,
        &file.cards,
        |c| &c.id,
        |c| c.updated as u64,
    )?;
    for card in all.iter().filter(|c| !local.contains(c)) {
        cards::store_synced(conn, card)?;
        merged.cards = true;
    }
    for review in &file.reviews {
        cards::store_review(conn, review)?;
    }
    for review in &file.highlight_reviews {
        recall::merge_review(conn, review)?;
    }
    Ok(())
}

/// Merges files from other devices. Each device's library file goes first: it carries the
/// tombstones and the books the other files belong to.
pub fn merge(lib: &mut Library, files: &[Incoming]) -> Result<Merged> {
    let mut merged = Merged::default();
    let mut order: Vec<&Incoming> = files.iter().collect();
    order.sort_by_key(|f| f.name != LIBRARY);
    for file in order {
        let result = (|| -> Result<()> {
            if file.name == LIBRARY {
                merge_library(
                    lib,
                    &file.device,
                    serde_json::from_slice(&file.bytes)?,
                    &mut merged,
                )?;
            } else if file.name == CARDS {
                merge_cards(
                    &lib.db().conn(),
                    serde_json::from_slice(&file.bytes)?,
                    &mut merged,
                )?;
            } else if file.name == READING {
                let days: Vec<stats::SyncDay> = serde_json::from_slice(&file.bytes)?;
                stats::store_device(&lib.db().conn(), &file.device, &days)?;
            } else if let Some(id) = file
                .name
                .strip_prefix("book-")
                .and_then(|n| n.strip_suffix(".json"))
            {
                merge_book_file(lib, id, serde_json::from_slice(&file.bytes)?, &mut merged)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                lib.db().conn().execute(
                    "INSERT OR REPLACE INTO sync_seen (device, file, tag) VALUES (?1, ?2, ?3)",
                    params![file.device, file.name, file.tag],
                )?;
            }
            // A file being written right now reads as garbage: it is tried again next time.
            Err(e) => log::warn!("sync: {} from {}: {e:#}", file.name, file.device),
        }
    }
    Ok(merged)
}

fn seen_tag(conn: &Connection, device: &str, file: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT tag FROM sync_seen WHERE device = ?1 AND file = ?2",
            params![device, file],
            |r| r.get(0),
        )
        .optional()?)
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// What a sync did.
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub merged: Merged,
    /// Other devices seen in the sync folder.
    pub devices: usize,
    pub uploaded: usize,
}

/// Device folders are named by [`stats::device_id`]; anything else in there isn't ours.
fn valid_device(name: &str) -> bool {
    name.len() == 16 && name.bytes().all(|c| c.is_ascii_hexdigit())
}

/// Runs one sync. `quick` skips covers and book files, for when the app is closing.
/// `on_merged` hears of changes as soon as they are in: uploads can take a while after that.
pub async fn run(
    library: &Mutex<Library>,
    remote: &Remote,
    upload_files: bool,
    quick: bool,
    on_merged: impl Fn(&Merged),
) -> Result<Outcome> {
    let lock = || library.lock().unwrap_or_else(|e| e.into_inner());
    let device = stats::device_id(&lock().db().conn())?;
    let top = remote.list("").await?;
    if top.is_empty() {
        remote.mkdir("").await?;
    }
    for dir in ["devices", "covers", "files"] {
        if !top.iter().any(|f| f.name == dir) {
            remote.mkdir(dir).await?;
        }
    }
    let own = format!("devices/{device}");
    let devices = remote.list("devices").await?;
    if !devices.iter().any(|f| f.name == device) {
        remote.mkdir(&own).await?;
    }

    // Read what changed on other devices.
    let others: Vec<String> = devices
        .into_iter()
        .filter(|f| f.dir && f.name != device && valid_device(&f.name))
        .map(|f| f.name)
        .collect();
    let mut incoming = Vec::new();
    for other in &others {
        for file in remote.list(&format!("devices/{other}")).await? {
            if file.dir || !file.name.ends_with(".json") {
                continue;
            }
            let seen = seen_tag(&lock().db().conn(), other, &file.name)?;
            if seen.as_deref() == Some(file.tag.as_str()) {
                continue;
            }
            match remote.read(&format!("devices/{other}/{}", file.name)).await {
                Ok(bytes) => incoming.push(Incoming {
                    device: other.clone(),
                    name: file.name,
                    tag: file.tag,
                    bytes,
                }),
                Err(e) => log::warn!("sync: reading {}: {e:#}", file.name),
            }
        }
    }
    let mut merged = merge(&mut lock(), &incoming)?;
    if !merged.is_empty() {
        on_merged(&merged);
    }

    // Publish what changed here. What is on the server is checked too, in case its contents
    // were cleared or this is a different server now.
    let files = export(&lock())?;
    let there: HashSet<String> = remote
        .list(&own)
        .await?
        .into_iter()
        .map(|f| f.name)
        .collect();
    for name in there.iter().filter(|n| !files.iter().any(|(f, _)| f == *n)) {
        // A book left with no notes, or gone.
        if name.ends_with(".json") {
            remote.remove(&format!("{own}/{name}")).await?;
        }
    }
    let mut uploaded = 0;
    for (name, bytes) in files {
        let digest = hash(&bytes);
        let sent: Option<String> = lock()
            .db()
            .conn()
            .query_row(
                "SELECT hash FROM sync_sent WHERE file = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()?;
        if there.contains(&name) && sent.as_deref() == Some(digest.as_str()) {
            continue;
        }
        remote.write(&format!("{own}/{name}"), bytes).await?;
        uploaded += 1;
        lock().db().conn().execute(
            "INSERT OR REPLACE INTO sync_sent (file, hash) VALUES (?1, ?2)",
            params![name, digest],
        )?;
    }

    if !quick {
        if sync_covers(library, remote).await? {
            merged.shelf = true;
            on_merged(&Merged {
                shelf: true,
                ..Merged::default()
            });
        }
        if upload_files {
            uploaded += upload_books(library, remote).await?;
        }
    }
    db::kv_set(&lock().db().conn(), LAST_SYNC, &library::now())?;
    Ok(Outcome {
        merged,
        devices: others.len(),
        uploaded,
    })
}

/// Shares covers with the other devices. Returns whether any arrived.
async fn sync_covers(library: &Mutex<Library>, remote: &Remote) -> Result<bool> {
    let lock = || library.lock().unwrap_or_else(|e| e.into_inner());
    let there: HashSet<String> = remote
        .list("covers")
        .await?
        .into_iter()
        .map(|f| f.name)
        .collect();
    let books = lock().books();
    let mut arrived = false;
    for book in books {
        let name = format!("{}.jpg", book.id);
        let path = lock().cover_path(&book.id);
        if book.has_cover && !there.contains(&name) && path.exists() {
            remote
                .write(&format!("covers/{name}"), std::fs::read(&path)?)
                .await?;
        } else if !book.has_cover && there.contains(&name) {
            let bytes = remote.read(&format!("covers/{name}")).await?;
            if bytes.len() <= MAX_COVER_BYTES && image::load_from_memory(&bytes).is_ok() {
                std::fs::write(&path, bytes)?;
                let mut lib = lock();
                lib.cover_fetched(&book.id);
                lib.save_book(&book.id)?;
                arrived = true;
            }
        }
    }
    Ok(arrived)
}

async fn upload_books(library: &Mutex<Library>, remote: &Remote) -> Result<usize> {
    let lock = || library.lock().unwrap_or_else(|e| e.into_inner());
    let there: HashSet<String> = remote
        .list("files")
        .await?
        .into_iter()
        .map(|f| f.name)
        .collect();
    let local: Vec<(String, std::path::PathBuf)> = {
        let lib = lock();
        lib.books()
            .iter()
            .filter(|b| !b.missing && !there.contains(&format!("{}.book", b.id)))
            .map(|b| (b.id.clone(), lib.book_path(b)))
            .collect()
    };
    let mut uploaded = 0;
    for (id, path) in local {
        remote.upload(&format!("files/{id}.book"), &path).await?;
        uploaded += 1;
    }
    Ok(uploaded)
}

/// Fetches a book's file from the sync folder. The content must match the book's hash: what
/// arrives from the folder is never trusted to be what it says.
pub async fn fetch_book(library: &Mutex<Library>, remote: &Remote, id: &str) -> Result<()> {
    let lock = || library.lock().unwrap_or_else(|e| e.into_inner());
    let (book, root) = {
        let lib = lock();
        let book = lib.get(id).cloned();
        (book, lib.root().to_owned())
    };
    let Some(book) = book else {
        bail!(crate::t!("book_not_found"));
    };
    if !valid_book(&book) {
        bail!(crate::t!("book_not_found"));
    }
    if !remote
        .list("files")
        .await?
        .iter()
        .any(|f| f.name == format!("{id}.book"))
    {
        bail!(crate::t!("sync_no_file"));
    }
    let dir = std::env::temp_dir().join(format!("aireader-fetch-{:x}", library::now_millis()));
    std::fs::create_dir_all(&dir)?;
    let temp = dir.join(&book.file_name);
    let result = async {
        remote.download(&format!("files/{id}.book"), &temp).await?;
        let staged = library::stage(&root, &temp)?;
        if staged.sha256 != book.sha256 {
            bail!(crate::t!("sync_bad_file"));
        }
        lock().fill_missing(staged)
    }
    .await;
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests;
