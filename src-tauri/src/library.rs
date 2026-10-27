//! The bookshelf: imported books are copied into the library directory, de-duplicated by
//! content hash, and recorded in the library database together with groups and settings.
//!
//! Layout under the library root:
//! - `aireader.db` (see [`crate::db`])
//! - `books/<id>/<original file name>`
//! - `books/.incoming-*/` (files being imported; left-overs are removed on open)
//! - `covers/<id>.jpg`
//! - `fonts/<file>` (reading fonts the user added)

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::formats;
use crate::model::Location;

const COVER_WIDTH: u32 = 360;
const COVER_HEIGHT: u32 = 540;
const STAGING_PREFIX: &str = ".incoming-";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    /// Display label such as "EPUB".
    pub format: String,
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    pub has_cover: bool,
    /// Unix seconds.
    pub added: u64,
    pub last_read: Option<u64>,
    /// Reading progress in `0.0..=1.0`.
    pub progress: f32,
    pub position: Option<Location>,
    /// The book's group; `None` is "ungrouped". A book is in at most one group.
    #[serde(default)]
    pub group: Option<String>,
    /// Groups from older libraries, where a book could be in several; migrated on load.
    #[serde(default, rename = "groups", skip_serializing)]
    legacy_groups: Vec<String>,
    /// Active reading time in seconds.
    #[serde(default)]
    pub read_seconds: u64,
    /// Measured reading speed: characters per minute for reflowable books, pages per minute
    /// for paged ones.
    #[serde(default)]
    pub reading_speed: Option<f32>,
    /// Page rotation in degrees (0/90/180/270) for paged books.
    #[serde(default)]
    pub rotation: u16,
    /// Set when listing if the stored file has disappeared.
    #[serde(default, skip_deserializing)]
    pub missing: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct LibraryData {
    books: Vec<Book>,
    groups: Vec<String>,
    /// Reader preferences owned by the frontend.
    settings: serde_json::Value,
    /// AI service settings (API keys are kept in the system keychain, not here).
    ai: crate::ai::config::AiConfig,
}

/// Editable book fields; `None` leaves a field unchanged.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookPatch {
    pub title: Option<String>,
    pub author: Option<String>,
    /// `Some(None)` (JSON `null`) or an empty name removes the book from its group.
    #[serde(default, deserialize_with = "present")]
    pub group: Option<Option<String>>,
    pub rotation: Option<u16>,
}

/// Tells a field that is present but `null` (`Some(None)`) from a missing one (`None`).
fn present<'de, D, T>(d: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

pub struct Library {
    root: PathBuf,
    db: crate::db::Db,
    data: LibraryData,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Font files the reader can use.
pub const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "woff", "woff2"];

fn is_font_file(name: &str) -> bool {
    !name.starts_with('.')
        && name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| FONT_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

/// Today's date as `YYYY-MM-DD` (UTC), for file names.
pub fn today() -> String {
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let days = now() as i64 / 86_400 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Expands folders (recursively) into supported book files; plain files are kept as given.
pub fn collect_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            if p.is_dir() {
                walk(&p, out);
            } else if formats::is_supported(&p) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            walk(p, &mut out);
        } else {
            out.push(p.clone());
        }
    }
    out
}

pub fn hash_file(path: &Path) -> Result<String> {
    copy_hashed(path, None)
}

/// Hashes `src`, copying it to `dest` on the way when given, so a file is read only once.
fn copy_hashed(src: &Path, mut dest: Option<&mut std::fs::File>) -> Result<String> {
    use std::io::{Read, Write};
    let mut file = std::fs::File::open(src)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        if let Some(out) = dest.as_mut() {
            out.write_all(&buf[..n])
                .with_context(|| crate::t!("copy_failed"))?;
        }
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// A file copied into the library's staging folder. The folder is removed when this is
/// dropped, unless the book was added.
pub struct Staged {
    dir: PathBuf,
    file_name: String,
    pub sha256: String,
}

impl Staged {
    fn file(&self) -> PathBuf {
        self.dir.join(&self.file_name)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Copies `src` into the staging folder of the library at `root`, hashing it on the way.
pub fn stage(root: &Path, src: &Path) -> Result<Staged> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let file_name = src
        .file_name()
        .with_context(|| crate::t!("invalid_file_name"))?
        .to_string_lossy()
        .into_owned();
    let dir = root.join("books").join(format!(
        "{STAGING_PREFIX}{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir)?;
    let mut staged = Staged {
        dir,
        file_name,
        sha256: String::new(),
    };
    let mut out = std::fs::File::create(staged.file()).with_context(|| crate::t!("copy_failed"))?;
    staged.sha256 = copy_hashed(src, Some(&mut out))?;
    Ok(staged)
}

/// Shrinks a cover to fit `COVER_WIDTH`×`COVER_HEIGHT` and encodes it as JPEG.
fn thumbnail(bytes: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(bytes)?.thumbnail(COVER_WIDTH, COVER_HEIGHT);
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img.to_rgb8()).write_to(&mut out, image::ImageFormat::Jpeg)?;
    Ok(out.into_inner())
}

/// A copied and parsed book that is not yet part of the library.
pub struct Prepared {
    book: Book,
    staged: Staged,
}

impl Library {
    /// Opens the library at `root`, creating its database; a JSON-era library is imported on
    /// first open and its files moved to `backup-json/`.
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root.join("books"))?;
        std::fs::create_dir_all(root.join("covers"))?;
        let db = crate::db::Db::open(&root.join(crate::db::FILE))?;
        // Imports interrupted by a crash or quit.
        if let Ok(entries) = std::fs::read_dir(root.join("books")) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(STAGING_PREFIX)
                {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
        let imported = Self::import_legacy(root, &db)?;
        let data = Self::load(&db)?;
        if imported {
            crate::db::archive_legacy(root)?;
        }
        Ok(Self {
            root: root.to_owned(),
            db,
            data,
        })
    }

    /// Imports `library.json` and the other JSON files into an empty database.
    fn import_legacy(root: &Path, db: &crate::db::Db) -> Result<bool> {
        let mut conn = db.conn();
        let empty: bool = conn.query_row(
            "SELECT NOT EXISTS (SELECT 1 FROM books) AND NOT EXISTS (SELECT 1 FROM kv)",
            [],
            |r| r.get(0),
        )?;
        if !empty || !crate::db::has_legacy(root) {
            return Ok(false);
        }
        let bytes = std::fs::read(root.join("library.json"))?;
        let mut data: LibraryData =
            serde_json::from_slice(&bytes).with_context(|| crate::t!("library_corrupt"))?;
        // Books that were in several groups keep the first one.
        for b in &mut data.books {
            let legacy = std::mem::take(&mut b.legacy_groups);
            if b.group.is_none() {
                b.group = legacy.into_iter().find(|g| !g.trim().is_empty());
            }
        }
        let tx = conn.transaction()?;
        Self::write(&tx, &data)?;
        crate::db::import_legacy_extras(&tx, root)?;
        tx.commit()?;
        Ok(true)
    }

    fn load(db: &crate::db::Db) -> Result<LibraryData> {
        let conn = db.conn();
        let mut books = Vec::new();
        let mut stmt = conn.prepare("SELECT data FROM books ORDER BY position")?;
        for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
            books.push(serde_json::from_str(&row?).with_context(|| crate::t!("library_corrupt"))?);
        }
        let mut stmt = conn.prepare("SELECT name FROM groups ORDER BY position")?;
        let groups = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(LibraryData {
            books,
            groups,
            settings: crate::db::kv_get(&conn, "settings")?.unwrap_or_default(),
            ai: crate::db::kv_get(&conn, "ai")?.unwrap_or_default(),
        })
    }

    fn write(conn: &rusqlite::Connection, data: &LibraryData) -> Result<()> {
        use rusqlite::params;
        let ids: Vec<&str> = data.books.iter().map(|b| b.id.as_str()).collect();
        let mut stale = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT id FROM books")?;
            for id in stmt.query_map([], |r| r.get::<_, String>(0))? {
                let id = id?;
                if !ids.contains(&id.as_str()) {
                    stale.push(id);
                }
            }
        }
        for id in stale {
            conn.execute("DELETE FROM books WHERE id = ?1", params![id])?;
        }
        for (position, b) in data.books.iter().enumerate() {
            conn.execute(
                "INSERT INTO books (id, position, sha256, title, grp, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET position = excluded.position, sha256 = excluded.sha256,
                   title = excluded.title, grp = excluded.grp, data = excluded.data",
                params![b.id, position as i64, b.sha256, b.title, b.group, serde_json::to_string(b)?],
            )?;
        }
        conn.execute("DELETE FROM groups", [])?;
        for (position, g) in data.groups.iter().enumerate() {
            conn.execute(
                "INSERT INTO groups (name, position) VALUES (?1, ?2)",
                params![g, position as i64],
            )?;
        }
        crate::db::kv_set(conn, "settings", &data.settings)?;
        crate::db::kv_set(conn, "ai", &data.ai)?;
        Ok(())
    }

    /// Writes the library's records to the database in one transaction.
    pub fn save(&self) -> Result<()> {
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        Self::write(&tx, &self.data)?;
        tx.commit()?;
        Ok(())
    }

    /// Writes one book's record, for changes that touch nothing else (progress, reading time).
    pub fn save_book(&self, id: &str) -> Result<()> {
        use rusqlite::params;
        let b = self.get(id).with_context(|| crate::t!("book_not_found"))?;
        let changed = self.db.conn().execute(
            "UPDATE books SET sha256 = ?2, title = ?3, grp = ?4, data = ?5 WHERE id = ?1",
            params![b.id, b.sha256, b.title, b.group, serde_json::to_string(b)?],
        )?;
        if changed == 0 {
            // Not stored yet (added but not saved): write everything.
            return self.save();
        }
        Ok(())
    }

    /// Writes a backup of the library at `root` as a zip: the database and covers, and with
    /// `books` the book files too. Restoring is unpacking it into an empty library folder.
    pub fn backup_at(root: &Path, db: &crate::db::Db, dest: &Path, books: bool) -> Result<()> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let snapshot = root.join(".backup.db");
        let _ = std::fs::remove_file(&snapshot);
        crate::db::snapshot(db, &snapshot)?;
        let result = (|| -> Result<()> {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(dest)?);
            let packed = SimpleFileOptions::default().large_file(true);
            // Book files are mostly compressed already.
            let stored = packed.compression_method(zip::CompressionMethod::Stored);
            zip.start_file(crate::db::FILE, packed)?;
            std::io::copy(&mut std::fs::File::open(&snapshot)?, &mut zip)?;
            let add_dir = |zip: &mut zip::ZipWriter<std::fs::File>,
                           dir: &str,
                           options: SimpleFileOptions|
             -> Result<()> {
                let mut files: Vec<PathBuf> = Vec::new();
                if !root.join(dir).is_dir() {
                    return Ok(());
                }
                let mut stack = vec![root.join(dir)];
                while let Some(d) = stack.pop() {
                    for entry in std::fs::read_dir(&d)?.flatten() {
                        let path = entry.path();
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with('.') {
                            continue; // staging folders, .DS_Store
                        }
                        if path.is_dir() {
                            stack.push(path);
                        } else {
                            files.push(path);
                        }
                    }
                }
                files.sort();
                for path in files {
                    let rel = path
                        .strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    zip.start_file(rel, options)?;
                    std::io::copy(&mut std::fs::File::open(&path)?, zip)?;
                }
                Ok(())
            };
            add_dir(&mut zip, "covers", packed)?;
            add_dir(&mut zip, "fonts", stored)?;
            if books {
                add_dir(&mut zip, "books", stored)?;
            }
            zip.finish()?.flush()?;
            Ok(())
        })();
        let _ = std::fs::remove_file(&snapshot);
        if result.is_err() {
            let _ = std::fs::remove_file(dest);
        }
        result
    }

    /// The library database, shared by annotations, AI caches and the search index.
    pub fn db(&self) -> &crate::db::Db {
        &self.db
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn books(&self) -> Vec<Book> {
        let mut books = self.data.books.clone();
        for b in &mut books {
            b.missing = !self.book_path(b).exists();
        }
        books
    }

    pub fn groups(&self) -> &[String] {
        &self.data.groups
    }

    pub fn get(&self, id: &str) -> Option<&Book> {
        self.data.books.iter().find(|b| b.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut Book> {
        self.data
            .books
            .iter_mut()
            .find(|b| b.id == id)
            .with_context(|| crate::t!("book_not_found"))
    }

    pub fn book_path(&self, book: &Book) -> PathBuf {
        self.root.join("books").join(&book.id).join(&book.file_name)
    }

    pub fn file_of(&self, id: &str) -> Option<PathBuf> {
        self.get(id).map(|b| self.book_path(b))
    }

    /// Reading fonts the user added, by file name.
    pub fn fonts(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.root.join("fonts"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| is_font_file(n))
            .collect();
        names.sort();
        names
    }

    /// The stored font `name`, if it is one (names never lead outside the fonts folder).
    pub fn font_path(&self, name: &str) -> Option<PathBuf> {
        let path = self.root.join("fonts").join(name);
        (is_font_file(name) && !name.contains(['/', '\\']) && path.is_file()).then_some(path)
    }

    /// Copies a font file into the library; returns its stored name.
    pub fn add_font(&self, src: &Path) -> Result<String> {
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| is_font_file(n))
            .with_context(|| crate::t!("font_unsupported"))?;
        let dir = self.root.join("fonts");
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(src, dir.join(&name)).with_context(|| crate::t!("copy_failed"))?;
        Ok(name)
    }

    pub fn remove_font(&self, name: &str) -> Result<()> {
        if let Some(path) = self.font_path(name) {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn cover_path(&self, id: &str) -> PathBuf {
        self.root.join("covers").join(format!("{id}.jpg"))
    }

    /// The id of an existing book with this content hash.
    pub fn find_hash(&self, sha256: &str) -> Option<&Book> {
        self.data.books.iter().find(|b| b.sha256 == sha256)
    }

    /// Parses a staged file and makes its cover. Runs without holding the library lock; the
    /// result is committed with [`Library::add`]. Fails on unsupported or broken files.
    pub fn prepare(staged: Staged) -> Result<Prepared> {
        let dest = staged.file();
        let format = formats::detect(&dest)?.label().to_owned();
        let (title, author, cover) = {
            // Dropped before the staged folder is moved (an open PDF can't be moved on Windows).
            let doc = formats::open(&dest)?;
            let cover = formats::cover_image(&doc).and_then(|bytes| thumbnail(&bytes).ok());
            (doc.title, doc.author, cover)
        };
        let has_cover =
            cover.is_some_and(|jpg| std::fs::write(staged.dir.join(".cover.jpg"), jpg).is_ok());
        let book = Book {
            id: staged.sha256[..16].to_owned(),
            title,
            author: author.filter(|a| !a.trim().is_empty()),
            format,
            file_name: staged.file_name.clone(),
            size: std::fs::metadata(&dest)?.len(),
            sha256: staged.sha256.clone(),
            has_cover,
            added: now(),
            last_read: None,
            progress: 0.0,
            position: None,
            group: None,
            legacy_groups: Vec::new(),
            read_seconds: 0,
            reading_speed: None,
            rotation: 0,
            missing: false,
        };
        Ok(Prepared { book, staged })
    }

    /// Moves a prepared book into the library (into `group`, if given). Returns `false` if a
    /// book with the same content was added concurrently.
    pub fn add(&mut self, prepared: Prepared, group: Option<&str>) -> Result<bool> {
        let Prepared { mut book, staged } = prepared;
        if self.find_hash(&book.sha256).is_some() {
            return Ok(false);
        }
        if book.has_cover {
            std::fs::rename(staged.dir.join(".cover.jpg"), self.cover_path(&book.id))?;
        }
        let dir = self.root.join("books").join(&book.id);
        // A folder left by a crash between moving a book in and saving the library.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::rename(&staged.dir, &dir)?;
        let group = group.map(str::trim).filter(|g| !g.is_empty());
        if let Some(g) = group {
            self.ensure_group(g);
        }
        book.group = group.map(str::to_owned);
        self.data.books.push(book);
        Ok(true)
    }

    pub fn update(&mut self, id: &str, patch: BookPatch) -> Result<Book> {
        let group = patch
            .group
            .map(|g| g.map(|g| g.trim().to_owned()).filter(|g| !g.is_empty()));
        if let Some(Some(g)) = &group {
            self.ensure_group(g);
        }
        let book = self.get_mut(id)?;
        if let Some(title) = patch.title.map(|t| t.trim().to_owned()) {
            if title.is_empty() {
                bail!(crate::t!("title_empty"));
            }
            book.title = title;
        }
        if let Some(author) = patch.author {
            let author = author.trim();
            book.author = (!author.is_empty()).then(|| author.to_owned());
        }
        if let Some(group) = group {
            book.group = group;
        }
        if let Some(rotation) = patch.rotation {
            book.rotation = rotation % 360 / 90 * 90;
        }
        Ok(book.clone())
    }

    /// Removes books from the shelf and deletes their stored copies, covers, annotations and
    /// AI caches.
    pub fn remove(&mut self, ids: &[String]) {
        for id in ids {
            let _ = std::fs::remove_dir_all(self.root.join("books").join(id));
            let _ = std::fs::remove_file(self.cover_path(id));
            let _ = crate::db::delete_book_data(&self.db.conn(), id);
        }
        self.data.books.retain(|b| !ids.contains(&b.id));
    }

    pub fn set_progress(&mut self, id: &str, position: Location, progress: f32) -> Result<()> {
        let book = self.get_mut(id)?;
        book.position = Some(position);
        book.progress = progress.clamp(0.0, 1.0);
        book.last_read = Some(now());
        Ok(())
    }

    /// Folds the reading data of duplicate copies into `keep` before they are removed: reading
    /// time adds up, the latest reading date wins, `furthest` (the copies' furthest position,
    /// mapped into the kept book) replaces a position that is less far, and an ungrouped book
    /// takes a copy's group.
    pub fn absorb(
        &mut self,
        keep: &str,
        removed: &[String],
        furthest: Option<(Location, f32)>,
    ) -> Result<()> {
        let others: Vec<Book> = self
            .data
            .books
            .iter()
            .filter(|b| removed.contains(&b.id))
            .cloned()
            .collect();
        let book = self.get_mut(keep)?;
        for o in &others {
            book.read_seconds += o.read_seconds;
            book.last_read = book.last_read.max(o.last_read);
            if book.group.is_none() {
                book.group = o.group.clone();
            }
        }
        if let Some((position, progress)) = furthest
            && progress > book.progress
        {
            book.position = Some(position);
            book.progress = progress.clamp(0.0, 1.0);
        }
        Ok(())
    }

    /// Adds active reading time; `speed` is a measured reading speed sample (characters or
    /// pages per minute) folded into the stored average.
    pub fn add_reading_time(&mut self, id: &str, seconds: u64, speed: Option<f32>) -> Result<()> {
        let book = self.get_mut(id)?;
        book.read_seconds += seconds;
        if let Some(sample) = speed.filter(|s| s.is_finite() && *s > 0.0) {
            // Exponential moving average so the estimate adapts but is not jumpy.
            book.reading_speed = Some(match book.reading_speed {
                Some(avg) => avg * 0.8 + sample * 0.2,
                None => sample,
            });
        }
        Ok(())
    }

    fn ensure_group(&mut self, name: &str) {
        if !self.data.groups.iter().any(|g| g == name) {
            self.data.groups.push(name.to_owned());
        }
    }

    pub fn create_group(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            bail!(crate::t!("group_empty"));
        }
        if self.data.groups.iter().any(|g| g == name) {
            bail!(crate::t!("group_exists", name = name));
        }
        self.data.groups.push(name.to_owned());
        Ok(())
    }

    pub fn rename_group(&mut self, old: &str, new: &str) -> Result<()> {
        let new = new.trim();
        if new.is_empty() {
            bail!(crate::t!("group_empty"));
        }
        if old == new {
            return Ok(());
        }
        if self.data.groups.iter().any(|g| g == new) {
            bail!(crate::t!("group_exists", name = new));
        }
        let g = self
            .data
            .groups
            .iter_mut()
            .find(|g| *g == old)
            .with_context(|| crate::t!("group_not_found"))?;
        *g = new.to_owned();
        for b in &mut self.data.books {
            if b.group.as_deref() == Some(old) {
                b.group = Some(new.to_owned());
            }
        }
        Ok(())
    }

    /// Deletes a group; its books stay on the shelf.
    pub fn delete_group(&mut self, name: &str) {
        self.data.groups.retain(|g| g != name);
        for b in &mut self.data.books {
            if b.group.as_deref() == Some(name) {
                b.group = None;
            }
        }
    }

    pub fn settings(&self) -> &serde_json::Value {
        &self.data.settings
    }

    pub fn ai_config(&self) -> &crate::ai::config::AiConfig {
        &self.data.ai
    }

    pub fn set_ai_config(&mut self, config: crate::ai::config::AiConfig) {
        self.data.ai = config;
    }

    pub fn set_settings(&mut self, settings: serde_json::Value) {
        self.data.settings = settings;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aireader-lib-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn import(lib: &mut Library, src: &Path) -> Result<bool> {
        let staged = stage(lib.root(), src)?;
        assert_eq!(staged.sha256, hash_file(src)?);
        if lib.find_hash(&staged.sha256).is_some() {
            return Ok(false);
        }
        let prepared = Library::prepare(staged)?;
        lib.add(prepared, None)
    }

    #[test]
    fn import_dedupe_edit_groups_remove_and_reload() {
        let dir = temp("flow");
        let src_dir = dir.join("src");
        std::fs::create_dir_all(src_dir.join("nested")).unwrap();
        std::fs::write(
            src_dir.join("一.txt"),
            "第一章 开始\n正文\n第二章 继续\n更多",
        )
        .unwrap();
        std::fs::write(src_dir.join("nested/notes.md"), "# Notes\n\ntext").unwrap();
        std::fs::write(src_dir.join("nested/ignored.bin"), [0u8; 4]).unwrap();
        std::fs::write(
            src_dir.join("copy.txt"),
            "第一章 开始\n正文\n第二章 继续\n更多",
        )
        .unwrap();

        let files = collect_files(std::slice::from_ref(&src_dir));
        assert_eq!(files.len(), 3, "{files:?}");

        let root = dir.join("lib");
        let mut lib = Library::open(&root).unwrap();
        let added: Vec<bool> = files.iter().map(|f| import(&mut lib, f).unwrap()).collect();
        assert_eq!(
            added.iter().filter(|a| **a).count(),
            2,
            "identical content is skipped"
        );

        let books = lib.books();
        let txt = books.iter().find(|b| b.format == "TXT").unwrap();
        assert!(lib.book_path(txt).exists());
        assert!(!txt.missing);
        let id = txt.id.clone();

        lib.update(
            &id,
            BookPatch {
                title: Some("新书名".into()),
                author: Some(" 某人 ".into()),
                group: Some(Some("小说".into())),
                rotation: None,
            },
        )
        .unwrap();
        assert!(
            lib.update(
                &id,
                BookPatch {
                    title: Some("  ".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
        lib.create_group("技术").unwrap();
        assert!(lib.create_group("技术").is_err());
        lib.rename_group("小说", "文学").unwrap();
        lib.set_progress(
            &id,
            Location::Flow {
                chapter: 1,
                block: 0,
            },
            0.5,
        )
        .unwrap();
        lib.add_reading_time(&id, 60, Some(500.0)).unwrap();
        lib.add_reading_time(&id, 30, Some(300.0)).unwrap();
        lib.update(
            &id,
            BookPatch {
                rotation: Some(450),
                ..Default::default()
            },
        )
        .unwrap();
        lib.save().unwrap();

        let mut lib = Library::open(&root).unwrap();
        let b = lib.get(&id).unwrap();
        assert_eq!(b.title, "新书名");
        assert_eq!(b.author.as_deref(), Some("某人"));
        assert_eq!(b.group.as_deref(), Some("文学"));
        assert_eq!(
            b.position,
            Some(Location::Flow {
                chapter: 1,
                block: 0
            })
        );
        assert!(b.last_read.is_some());
        assert_eq!(b.read_seconds, 90);
        assert_eq!(b.reading_speed, Some(460.0));
        assert_eq!(b.rotation, 90);
        assert_eq!(lib.groups(), ["文学", "技术"]);

        lib.delete_group("文学");
        assert_eq!(lib.get(&id).unwrap().group, None);
        lib.remove(std::slice::from_ref(&id));
        assert!(lib.get(&id).is_none());
        assert!(!root.join("books").join(&id).exists());
    }

    #[test]
    fn books_in_several_groups_keep_the_first() {
        let root = temp("legacy");
        std::fs::create_dir_all(&root).unwrap();
        let book = |id: &str, groups: &str| {
            format!(
                r#"{{"id":"{id}","title":"t","author":null,"format":"TXT","fileName":"a.txt","size":1,"sha256":"{id}","hasCover":false,"added":0,"lastRead":null,"progress":0,"position":null,"groups":{groups}}}"#
            )
        };
        let json = format!(
            r#"{{"books":[{},{}],"groups":["小说","历史"]}}"#,
            book("a", r#"["小说","历史"]"#),
            book("b", "[]")
        );
        std::fs::write(root.join("library.json"), json).unwrap();
        let mut lib = Library::open(&root).unwrap();
        assert_eq!(lib.get("a").unwrap().group.as_deref(), Some("小说"));
        assert_eq!(lib.get("b").unwrap().group, None);
        lib.update("a", serde_json::from_str(r#"{"group":null}"#).unwrap())
            .unwrap();
        assert_eq!(lib.get("a").unwrap().group, None, "null removes the group");
        lib.update("a", serde_json::from_str(r#"{"title":"u"}"#).unwrap())
            .unwrap();
        assert_eq!(
            lib.get("a").unwrap().group,
            None,
            "a missing field changes nothing"
        );
        lib.save().unwrap();
        drop(lib);
        // The JSON was imported into the database and moved aside; reopening reads the database.
        assert!(!root.join("library.json").exists());
        assert!(root.join("backup-json/library.json").exists());
        let lib = Library::open(&root).unwrap();
        assert_eq!(lib.get("a").unwrap().title, "u");
        assert_eq!(lib.get("a").unwrap().group, None);
        assert_eq!(lib.groups(), ["小说", "历史"]);
    }

    #[test]
    fn legacy_annotations_and_caches_are_imported() {
        let root = temp("legacy-extras");
        std::fs::write(root.join("library.json"), r#"{"books":[],"groups":["g"]}"#).unwrap();
        std::fs::create_dir_all(root.join("annotations")).unwrap();
        std::fs::write(
            root.join("annotations/b1.json"),
            r#"{"bookmarks":[{"id":"m","location":{"kind":"page","page":2},"title":"t","excerpt":"","created":1}],"highlights":[]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("ai/b1")).unwrap();
        std::fs::write(
            root.join("ai/b1/summaries.json"),
            r#"{"x/m":{"ch:0":"概要"}}"#,
        )
        .unwrap();
        let lib = Library::open(&root).unwrap();
        let ann = crate::annotations::AnnotationStore::new(lib.db())
            .load("b1")
            .unwrap();
        assert_eq!(ann.bookmarks[0].location, Location::Page { page: 2 });
        let cache = crate::ai::tasks::SummaryCache::open(lib.db(), "b1");
        assert_eq!(cache.get("x/m", "ch:0").map(String::as_str), Some("概要"));
        assert!(root.join("backup-json/annotations/b1.json").exists() && !root.join("ai").exists());
    }

    #[test]
    fn absorbing_duplicates_keeps_the_furthest_reading() {
        let root = temp("absorb");
        let book = |id: &str,
                    progress: f32,
                    seconds: u64,
                    last: Option<u64>,
                    group: Option<&str>| {
            format!(
                r#"{{"id":"{id}","title":"t","author":null,"format":"EPUB","fileName":"a","size":1,"sha256":"{id}","hasCover":false,"added":0,"lastRead":{},"progress":{progress},"position":null,"readSeconds":{seconds},"group":{}}}"#,
                last.map_or("null".into(), |v| v.to_string()),
                group.map_or("null".into(), |g| format!("\"{g}\""))
            )
        };
        let json = format!(
            r#"{{"books":[{},{},{}]}}"#,
            book("keep", 0.1, 60, Some(5), None),
            book("a", 0.6, 30, Some(9), Some("小说")),
            book("b", 0.3, 10, None, Some("历史"))
        );
        std::fs::write(root.join("library.json"), json).unwrap();
        let mut lib = Library::open(&root).unwrap();
        let ids = vec!["a".to_owned(), "b".to_owned()];
        let at = Location::Flow {
            chapter: 3,
            block: 2,
        };
        lib.absorb("keep", &ids, Some((at, 0.6))).unwrap();
        let k = lib.get("keep").unwrap();
        assert_eq!(
            (k.read_seconds, k.last_read, k.progress, k.position),
            (100, Some(9), 0.6, Some(at))
        );
        assert_eq!(k.group.as_deref(), Some("小说"), "the first copy's group");
        // A position less far than the kept book's own does not replace it.
        lib.absorb(
            "keep",
            &[],
            Some((
                Location::Flow {
                    chapter: 0,
                    block: 0,
                },
                0.2,
            )),
        )
        .unwrap();
        assert_eq!(lib.get("keep").unwrap().position, Some(at));
    }

    #[test]
    fn failed_import_leaves_nothing_behind() {
        let dir = temp("bad");
        let bad = dir.join("broken.epub");
        std::fs::write(&bad, b"not a zip").unwrap();
        let root = dir.join("lib");
        let mut lib = Library::open(&root).unwrap();
        assert!(import(&mut lib, &bad).is_err());
        assert!(lib.books().is_empty());
        assert_eq!(std::fs::read_dir(root.join("books")).unwrap().count(), 0);
        // A staging folder left by a crash is removed when the library is opened.
        std::fs::create_dir_all(root.join("books/.incoming-1-0")).unwrap();
        drop(lib);
        Library::open(&root).unwrap();
        assert_eq!(std::fs::read_dir(root.join("books")).unwrap().count(), 0);
    }

    #[test]
    fn backup_holds_database_covers_and_optionally_books() {
        let dir = temp("backup");
        std::fs::write(dir.join("a.txt"), "第一章\n甲").unwrap();
        let root = dir.join("lib");
        let mut lib = Library::open(&root).unwrap();
        import(&mut lib, &dir.join("a.txt")).unwrap();
        lib.save().unwrap();
        std::fs::write(lib.cover_path("0123456789abcdef"), b"jpg").unwrap();
        let names = |books: bool| {
            let dest = dir.join(format!("b-{books}.zip"));
            Library::backup_at(lib.root(), lib.db(), &dest, books).unwrap();
            let zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
            zip.file_names()
                .filter_map(|n| n.ok().map(|n| n.into_owned()))
                .collect::<Vec<_>>()
        };
        let small = names(false);
        assert!(small.contains(&"aireader.db".to_owned()));
        assert!(small.contains(&"covers/0123456789abcdef.jpg".to_owned()));
        assert!(!small.iter().any(|n| n.starts_with("books/")));
        assert!(
            names(true)
                .iter()
                .any(|n| n.starts_with("books/") && n.ends_with("a.txt"))
        );
        // The snapshot opens as a library with the book in it.
        let restored = dir.join("restored");
        std::fs::create_dir_all(&restored).unwrap();
        let mut zip =
            zip::ZipArchive::new(std::fs::File::open(dir.join("b-true.zip")).unwrap()).unwrap();
        zip.extract(&restored).unwrap();
        assert_eq!(Library::open(&restored).unwrap().books().len(), 1);
        assert!(!root.join(".backup.db").exists());
        assert_eq!(today().len(), 10);
    }

    #[test]
    fn fonts_are_stored_listed_and_removed() {
        let dir = temp("fonts");
        std::fs::write(dir.join("My Font.TTF"), b"font").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let lib = Library::open(&dir.join("lib")).unwrap();
        assert!(lib.fonts().is_empty());
        assert_eq!(
            lib.add_font(&dir.join("My Font.TTF")).unwrap(),
            "My Font.TTF"
        );
        assert!(lib.add_font(&dir.join("notes.txt")).is_err());
        assert_eq!(lib.fonts(), ["My Font.TTF"]);
        assert!(lib.font_path("My Font.TTF").is_some());
        assert!(lib.font_path("../My Font.TTF").is_none());
        assert!(lib.font_path("missing.ttf").is_none());
        lib.remove_font("My Font.TTF").unwrap();
        assert!(lib.fonts().is_empty());
    }

    #[test]
    fn saving_one_book_keeps_the_others() {
        let dir = temp("save-one");
        std::fs::write(dir.join("a.txt"), "第一章\n甲").unwrap();
        std::fs::write(dir.join("b.txt"), "第一章\n乙").unwrap();
        let root = dir.join("lib");
        let mut lib = Library::open(&root).unwrap();
        import(&mut lib, &dir.join("a.txt")).unwrap();
        import(&mut lib, &dir.join("b.txt")).unwrap();
        lib.save().unwrap();
        let ids: Vec<String> = lib.books().iter().map(|b| b.id.clone()).collect();
        lib.set_progress(
            &ids[1],
            Location::Flow {
                chapter: 0,
                block: 1,
            },
            0.7,
        )
        .unwrap();
        lib.save_book(&ids[1]).unwrap();
        drop(lib);
        let lib = Library::open(&root).unwrap();
        let books = lib.books();
        assert_eq!(books.len(), 2);
        assert_eq!(books[1].id, ids[1], "order is kept");
        assert_eq!(books[1].progress, 0.7);
        assert!(lib.cover_path(&ids[0]).parent().unwrap().exists());
    }

    #[test]
    fn thumbnails_covers_to_jpeg() {
        let img = image::RgbaImage::from_pixel(1000, 1500, image::Rgba([10, 20, 30, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let jpg = thumbnail(png.get_ref()).unwrap();
        let out = image::load_from_memory(&jpg).unwrap();
        assert_eq!((out.width(), out.height()), (COVER_WIDTH, COVER_HEIGHT));
    }
}
