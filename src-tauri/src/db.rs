//! The library database, `aireader.db` in the library folder: book records, groups, settings,
//! annotations, AI caches, duplicate-check profiles and the semantic search index.
//!
//! Older libraries kept these in JSON files (`library.json`, `annotations/`, `ai/<book>/`,
//! `profiles.json`). They are imported when the database is first created and then moved to
//! `backup-json/`, so an older version of the app can still be pointed at them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Once};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub const FILE: &str = "aireader.db";
const SCHEMA_VERSION: i64 = 1;

/// Files and folders of the JSON era, moved to `backup-json/` after import.
const LEGACY: &[&str] = &["library.json", "annotations", "ai", "profiles.json"];

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS books (
    id TEXT PRIMARY KEY,
    position INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    title TEXT NOT NULL,
    grp TEXT,
    data TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS books_sha256 ON books (sha256);
CREATE TABLE IF NOT EXISTS groups (name TEXT PRIMARY KEY, position INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS bookmarks (
    id TEXT PRIMARY KEY,
    book_id TEXT NOT NULL,
    location TEXT NOT NULL,
    title TEXT NOT NULL,
    excerpt TEXT NOT NULL,
    created INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS bookmarks_book ON bookmarks (book_id);
CREATE TABLE IF NOT EXISTS highlights (
    id TEXT PRIMARY KEY,
    book_id TEXT NOT NULL,
    chapter INTEGER NOT NULL,
    start_block INTEGER NOT NULL,
    start_offset INTEGER NOT NULL,
    end_block INTEGER NOT NULL,
    end_offset INTEGER NOT NULL,
    text TEXT NOT NULL,
    color TEXT NOT NULL,
    note TEXT,
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS highlights_book ON highlights (book_id);
CREATE TABLE IF NOT EXISTS ai_cache (
    book_id TEXT NOT NULL,
    model TEXT NOT NULL,
    unit TEXT NOT NULL,
    text TEXT NOT NULL,
    PRIMARY KEY (book_id, model, unit)
);
CREATE TABLE IF NOT EXISTS profiles (sha256 TEXT PRIMARY KEY, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS chats (
    id TEXT PRIMARY KEY,
    book_id TEXT NOT NULL,
    title TEXT NOT NULL,
    data TEXT NOT NULL,
    updated INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS chats_book ON chats (book_id);
CREATE TABLE IF NOT EXISTS semantic_books (
    book_id TEXT PRIMARY KEY,
    model TEXT NOT NULL,
    passages INTEGER NOT NULL,
    updated INTEGER NOT NULL
);
";

/// Makes sqlite-vec available to every connection opened afterwards (the vector index needs
/// its `vec0` tables).
pub fn register_sqlite_vec() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        type Init = unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *mut std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int;
        // SAFETY: sqlite3_vec_init has the extension entry point signature SQLite expects;
        // sqlite-vec declares it without parameters, hence the cast.
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<*const (), Init>(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

/// A shared connection to the library database.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        register_sqlite_vec();
        let conn = Connection::open(path).with_context(|| crate::t!("library_corrupt"))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: path.to_owned(),
        })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The database file, for opening further connections (the vector index uses its own).
    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn kv_get<T: DeserializeOwned>(conn: &Connection, key: &str) -> Result<Option<T>> {
    let value: Option<String> = conn
        .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(value.map(|v| serde_json::from_str(&v)).transpose()?)
}

pub fn kv_set<T: Serialize>(conn: &Connection, key: &str, value: &T) -> Result<()> {
    conn.execute(
        "INSERT INTO kv (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, serde_json::to_string(value)?],
    )?;
    Ok(())
}

/// Removes everything stored about a book (its record is handled by the library).
pub fn delete_book_data(conn: &Connection, book_id: &str) -> Result<()> {
    for table in [
        "bookmarks",
        "highlights",
        "ai_cache",
        "chats",
        "semantic_books",
    ] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE book_id = ?1"),
            params![book_id],
        )?;
    }
    crate::ai::semantic::delete_book_passages(conn, book_id)?;
    Ok(())
}

/// Writes a consistent copy of the database to `dest` (which must not exist), even while it is
/// in use.
pub fn snapshot(db: &Db, dest: &Path) -> Result<()> {
    db.conn()
        .execute("VACUUM INTO ?1", params![dest.to_string_lossy()])?;
    Ok(())
}

/// A saved AI conversation about a book. `data` is the frontend's own record (turns, quote).
#[derive(Debug, Serialize, serde::Deserialize, PartialEq)]
pub struct ChatRecord {
    pub id: String,
    pub title: String,
    pub updated: u64,
    pub data: serde_json::Value,
}

/// A book's conversations, most recent first.
pub fn list_chats(conn: &Connection, book_id: &str) -> Result<Vec<ChatRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, updated, data FROM chats WHERE book_id = ?1 ORDER BY updated DESC",
    )?;
    let rows = stmt.query_map(params![book_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, title, updated, data) = row?;
        out.push(ChatRecord {
            id,
            title,
            updated: updated as u64,
            data: serde_json::from_str(&data).unwrap_or_default(),
        });
    }
    Ok(out)
}

pub fn save_chat(conn: &Connection, book_id: &str, chat: &ChatRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO chats (id, book_id, title, data, updated) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET title = excluded.title, data = excluded.data,
           updated = excluded.updated",
        params![
            chat.id,
            book_id,
            chat.title,
            chat.data.to_string(),
            chat.updated as i64
        ],
    )?;
    Ok(())
}

pub fn delete_chat(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM chats WHERE id = ?1", params![id])?;
    Ok(())
}

// --- import of JSON-era libraries --------------------------------------------------------------

/// Whether `root` holds data from before the database.
pub fn has_legacy(root: &Path) -> bool {
    root.join("library.json").exists()
}

/// Imports annotations, AI caches and profiles of a JSON-era library (the library record
/// itself is imported by [`crate::library::Library`]), inside the caller's transaction.
pub fn import_legacy_extras(conn: &Connection, root: &Path) -> Result<()> {
    if let Ok(entries) = std::fs::read_dir(root.join("annotations")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path
                .extension()
                .filter(|e| *e == "json")
                .and_then(|_| path.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
            else {
                continue;
            };
            let Ok(ann) =
                serde_json::from_slice::<crate::annotations::Annotations>(&std::fs::read(&path)?)
            else {
                continue;
            };
            crate::annotations::write(conn, &id, &ann)?;
        }
    }
    if let Ok(entries) = std::fs::read_dir(root.join("ai")) {
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            let file = entry.path().join("summaries.json");
            let Ok(bytes) = std::fs::read(&file) else {
                continue;
            };
            let Ok(data) = serde_json::from_slice::<
                std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
            >(&bytes) else {
                continue;
            };
            for (model, units) in data {
                for (unit, text) in units {
                    conn.execute(
                        "INSERT OR REPLACE INTO ai_cache (book_id, model, unit, text) VALUES (?1, ?2, ?3, ?4)",
                        params![id, model, unit, text],
                    )?;
                }
            }
        }
    }
    if let Ok(bytes) = std::fs::read(root.join("profiles.json"))
        && let Ok(profiles) =
            serde_json::from_slice::<std::collections::HashMap<String, serde_json::Value>>(&bytes)
    {
        for (sha, profile) in profiles {
            conn.execute(
                "INSERT OR REPLACE INTO profiles (sha256, data) VALUES (?1, ?2)",
                params![sha, profile.to_string()],
            )?;
        }
    }
    Ok(())
}

/// Moves the imported JSON files into `backup-json/`.
pub fn archive_legacy(root: &Path) -> Result<()> {
    let backup = root.join("backup-json");
    std::fs::create_dir_all(&backup)?;
    for name in LEGACY {
        let from = root.join(name);
        if from.exists() {
            std::fs::rename(&from, backup.join(name))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chats_are_saved_listed_and_removed_with_the_book() {
        let dir = std::env::temp_dir().join(format!("aireader-chats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("t.db")).unwrap();
        let conn = db.conn();
        let chat = |id: &str, updated: u64| ChatRecord {
            id: id.into(),
            title: format!("chat {id}"),
            updated,
            data: serde_json::json!({ "turns": [{ "role": "user", "text": id }] }),
        };
        save_chat(&conn, "b", &chat("old", 1)).unwrap();
        save_chat(&conn, "b", &chat("new", 2)).unwrap();
        save_chat(&conn, "other", &chat("x", 3)).unwrap();
        let mut updated = chat("old", 5);
        updated.title = "renamed".into();
        save_chat(&conn, "b", &updated).unwrap();
        let list = list_chats(&conn, "b").unwrap();
        assert_eq!(
            list.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["old", "new"]
        );
        assert_eq!(list[0], updated);
        delete_chat(&conn, "new").unwrap();
        assert_eq!(list_chats(&conn, "b").unwrap().len(), 1);
        drop(conn);
        delete_book_data(&db.conn(), "b").unwrap();
        assert!(list_chats(&db.conn(), "b").unwrap().is_empty());
        assert_eq!(list_chats(&db.conn(), "other").unwrap().len(), 1);
    }
}
