//! Tauri commands exposed to the frontend, plus the shared application state.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::annotations::{self, AnnotationStore, Annotations};
use crate::cards::{self, Card, CardPatch, NewCard, Stats};
use crate::dedupe::{self, DuplicateGroup, ProfileCache};
use crate::dictionary::{self, Definition, DictRecord, Dictionaries};
use crate::export::{self, BookNotes, NotesFormat};
use crate::formats;
use crate::library::{self, Book, BookPatch, Library};
use crate::model::{Block, Content, Document, Location, TocEntry};
use crate::search::{Hit, SearchText};

type CmdResult<T> = Result<T, String>;

const MAX_OPEN: usize = 4;
/// Approximate memory the open books may take together; the least recently used ones are
/// dropped beyond it (the current book always stays).
const MAX_OPEN_BYTES: usize = 384 << 20;
const MAX_HITS: usize = 1000;

fn err(e: impl std::fmt::Display) -> String {
    let message = format!("{e:#}");
    log::warn!("{message}");
    message
}

/// A parsed book kept in memory while it is being read.
pub struct OpenBook {
    pub doc: Document,
    /// Image id → encoded bytes, for the `book://` protocol.
    pub images: HashMap<u64, Arc<[u8]>>,
    /// Characters in each chapter (reflowable books).
    pub chars: Vec<usize>,
    /// Built on the first search.
    search: std::sync::OnceLock<SearchText>,
    /// Approximate size in memory, for limiting the cache.
    bytes: usize,
}

pub struct AppState {
    library: Mutex<Library>,
    /// A file to open: from the command line, a second launch or the OS. Consumed by
    /// `take_startup_book`.
    startup_file: Mutex<Option<PathBuf>>,
    /// Most recently used last.
    open: Mutex<Vec<(String, Arc<OpenBook>)>>,
    indexer: Indexer,
    dictionaries: Dictionaries,
}

/// The background job that adds books to the full-text index.
#[derive(Default)]
struct Indexer {
    running: AtomicBool,
    /// More books may have arrived; look again before finishing.
    again: AtomicBool,
    /// Stopped by the user, until the next start of the app.
    stopped: AtomicBool,
    progress: Mutex<Option<IndexProgress>>,
}

#[derive(Clone, Serialize)]
pub struct IndexProgress {
    done: usize,
    total: usize,
}

impl AppState {
    pub fn new(library: Library, startup_file: Option<PathBuf>) -> Self {
        let dictionaries = Dictionaries::new(library.root().join("dictionaries"));
        Self {
            library: Mutex::new(library),
            startup_file: Mutex::new(startup_file),
            open: Mutex::new(Vec::new()),
            indexer: Indexer::default(),
            dictionaries,
        }
    }

    pub fn library(&self) -> MutexGuard<'_, Library> {
        self.library.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Returns the parsed book, parsing it (outside any lock) if it is not cached.
    pub fn open_book(&self, id: &str) -> anyhow::Result<Arc<OpenBook>> {
        {
            let mut open = self.open.lock().unwrap();
            if let Some(i) = open.iter().position(|(k, _)| k == id) {
                let entry = open.remove(i);
                let book = entry.1.clone();
                open.push(entry);
                return Ok(book);
            }
        }
        let path = self
            .library()
            .file_of(id)
            .ok_or_else(|| anyhow::anyhow!(crate::t!("book_not_found")))?;
        if !path.exists() {
            anyhow::bail!(crate::t!("book_file_missing", path = path.display()));
        }
        let doc = formats::open(&path)?;
        let mut images = HashMap::new();
        let mut chars = Vec::new();
        if let Content::Flow(chapters) = &doc.content {
            for block in chapters.iter().flat_map(|c| &c.blocks) {
                if let Block::Image(img) = block {
                    images.insert(img.id, img.bytes.clone());
                }
            }
            chars = chapters
                .iter()
                .map(|c| c.blocks.iter().map(|b| b.text().chars().count()).sum())
                .collect();
        }
        // Images plus text (UTF-8 and a lowercased copy for search); a PDF's pages live in
        // PDFium, roughly the size of the file.
        let bytes = images.values().map(|b| b.len()).sum::<usize>()
            + chars.iter().sum::<usize>() * 6
            + match &doc.content {
                Content::Paged(_) => std::fs::metadata(&path).map_or(0, |m| m.len() as usize),
                Content::Flow(_) => 0,
            };
        let book = Arc::new(OpenBook {
            doc,
            images,
            chars,
            search: std::sync::OnceLock::new(),
            bytes,
        });
        let mut open = self.open.lock().unwrap();
        open.retain(|(k, _)| k != id);
        open.push((id.to_owned(), book.clone()));
        while open.len() > 1
            && (open.len() > MAX_OPEN
                || open.iter().map(|(_, b)| b.bytes).sum::<usize>() > MAX_OPEN_BYTES)
        {
            open.remove(0);
        }
        Ok(book)
    }

    fn forget(&self, ids: &[String]) {
        self.open.lock().unwrap().retain(|(k, _)| !ids.contains(k));
    }
}

fn state(app: &AppHandle) -> &AppState {
    app.state::<AppState>().inner()
}

/// Asks the frontend to open `file` (it calls `take_startup_book` on the `open-file` event,
/// or at startup if the event came too early).
pub fn request_open(app: &AppHandle, file: PathBuf) {
    if let Some(st) = app.try_state::<AppState>() {
        *st.startup_file.lock().unwrap() = Some(file);
        let _ = app.emit("open-file", ());
    }
}

/// Adds books missing from the full-text index, one at a time, on a thread of its own. It has
/// its own database connection, so the app isn't held up. Emits `fulltext-progress` (null when
/// finished).
pub fn start_indexing(app: &AppHandle) {
    let ix = &state(app).indexer;
    ix.again.store(true, Ordering::SeqCst);
    if ix.stopped.load(Ordering::SeqCst) || ix.running.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let ix = &state(&app).indexer;
        while ix.again.swap(false, Ordering::SeqCst) {
            if let Err(e) = index_pending(&app) {
                log::warn!("full-text indexing failed: {e:#}");
            }
        }
        *ix.progress.lock().unwrap() = None;
        ix.running.store(false, Ordering::SeqCst);
        let _ = app.emit("fulltext-progress", None::<IndexProgress>);
    });
}

fn index_pending(app: &AppHandle) -> anyhow::Result<()> {
    let ix = &state(app).indexer;
    let (books, mut conn) = {
        let lib = state(app).library();
        let conn = rusqlite::Connection::open(lib.db().path())?;
        (lib.books(), conn)
    };
    conn.busy_timeout(std::time::Duration::from_secs(30))?;
    let hashes: Vec<(String, String)> = books
        .iter()
        .filter(|b| !b.missing)
        .map(|b| (b.id.clone(), b.sha256.clone()))
        .collect();
    let todo = crate::fulltext::pending(&conn, &hashes)?;
    for (done, id) in todo.iter().enumerate() {
        if ix.stopped.load(Ordering::SeqCst) {
            break;
        }
        let progress = IndexProgress {
            done,
            total: todo.len(),
        };
        *ix.progress.lock().unwrap() = Some(progress.clone());
        let _ = app.emit("fulltext-progress", Some(progress));
        let Some(book) = books.iter().find(|b| b.id == *id) else {
            continue;
        };
        let path = state(app).library().book_path(book);
        // A file that can't be parsed (or breaks a parser) is left out, not retried each start.
        let doc = std::panic::catch_unwind(|| formats::open(&path))
            .ok()
            .and_then(Result::ok);
        crate::fulltext::index_book(&mut conn, id, &book.sha256, doc.as_ref())?;
        // Gives the reader's own page rendering room on big libraries.
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Ok(())
}

/// Seconds the frontend gets to save before the app quits anyway.
const CLOSE_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// Starts quitting: the frontend is told (`app-closing`) so it can save pending progress and
/// settings, then calls `exit_app`. Quits after [`CLOSE_GRACE`] if it doesn't.
pub fn begin_close(app: &AppHandle) {
    static CLOSING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if CLOSING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let _ = app.emit("app-closing", ());
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(CLOSE_GRACE);
        app.exit(0);
    });
}

#[tauri::command]
pub fn exit_app(app: AppHandle) {
    app.exit(0);
}

/// Records an error from the frontend (e.g. a view that failed to render) in the app log.
#[tauri::command]
pub fn log_error(message: String) {
    log::error!("frontend: {message}");
}

/// Runs blocking work off the main thread.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

#[derive(Serialize)]
pub struct Shelf {
    books: Vec<Book>,
    groups: Vec<String>,
}

#[tauri::command]
pub async fn list_books(app: AppHandle) -> CmdResult<Shelf> {
    blocking(move || {
        let lib = state(&app).library();
        Ok(Shelf {
            books: lib.books(),
            groups: lib.groups().to_vec(),
        })
    })
    .await
}

#[derive(Clone, Serialize)]
struct ImportProgress {
    done: usize,
    total: usize,
    name: String,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    added: Vec<String>,
    duplicates: Vec<String>,
    failed: Vec<(String, String)>,
    /// Ids of added books that may be another file of a book already on the shelf.
    possible_duplicates: Vec<String>,
}

enum Imported {
    Added(String),
    Existing(String),
}

/// Imports one file: returns the existing book if the same content is already on the shelf.
fn import_file(
    app: &AppHandle,
    file: &std::path::Path,
    group: Option<&str>,
) -> anyhow::Result<Imported> {
    let root = state(app).library().root().to_owned();
    let staged = library::stage(&root, file)?;
    let hash = staged.sha256.clone();
    if let Some(existing) = state(app).library().find_hash(&hash) {
        return Ok(Imported::Existing(existing.id.clone()));
    }
    let prepared = Library::prepare(staged)?;
    let mut lib = state(app).library();
    if lib.add(prepared, group)? {
        Ok(Imported::Added(hash[..16].to_owned()))
    } else {
        Ok(Imported::Existing(
            lib.find_hash(&hash)
                .map(|b| b.id.clone())
                .unwrap_or_default(),
        ))
    }
}

/// Imports files and folders (recursively), emitting `import-progress` events.
#[tauri::command]
pub async fn import_books(
    app: AppHandle,
    paths: Vec<String>,
    group: Option<String>,
) -> CmdResult<ImportReport> {
    blocking(move || {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let files = library::collect_files(&paths);
        let mut report = ImportReport::default();
        let mut added_ids = Vec::new();
        for (i, file) in files.iter().enumerate() {
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let _ = app.emit(
                "import-progress",
                ImportProgress {
                    done: i,
                    total: files.len(),
                    name: name.clone(),
                },
            );
            match import_file(&app, file, group.as_deref()) {
                Ok(Imported::Added(id)) => {
                    report.added.push(name);
                    added_ids.push(id);
                }
                Ok(Imported::Existing(_)) => report.duplicates.push(name),
                Err(e) => report.failed.push((name, err(e))),
            }
        }
        state(&app).library().save().map_err(err)?;
        start_indexing(&app);
        // Titles alone are enough to suggest a check; the dialog compares contents.
        let books = state(&app).library().books();
        report.possible_duplicates = dedupe::candidate_groups(&books)
            .into_iter()
            .flatten()
            .map(|i| books[i].id.clone())
            .filter(|id| added_ids.contains(id))
            .collect();
        let _ = app.emit(
            "import-progress",
            ImportProgress {
                done: files.len(),
                total: files.len(),
                name: String::new(),
            },
        );
        Ok(report)
    })
    .await
}

#[derive(Clone, Serialize)]
struct DedupeProgress {
    done: usize,
    total: usize,
    name: String,
}

/// Books on the shelf that are the same book in several files, best copy first. With `only`,
/// just the groups containing one of those books (e.g. ones just imported). Emits
/// `dedupe-progress` while files are analysed (analyses are cached by content hash).
#[tauri::command]
pub async fn find_duplicates(
    app: AppHandle,
    only: Option<Vec<String>>,
) -> CmdResult<Vec<DuplicateGroup>> {
    blocking(move || {
        let (books, db) = {
            let lib = state(&app).library();
            (lib.books(), lib.db().clone())
        };
        let mut wanted: Vec<usize> = dedupe::candidate_groups(&books)
            .into_iter()
            .filter(|g| {
                only.as_ref()
                    .is_none_or(|ids| g.iter().any(|&i| ids.contains(&books[i].id)))
            })
            .flatten()
            .filter(|&i| !books[i].missing)
            .collect();
        wanted.sort_unstable();
        let mut cache = ProfileCache::open(&db);
        let mut profiles = HashMap::new();
        for (k, &i) in wanted.iter().enumerate() {
            let book = &books[i];
            let _ = app.emit(
                "dedupe-progress",
                DedupeProgress {
                    done: k,
                    total: wanted.len(),
                    name: book.title.clone(),
                },
            );
            let profile = match cache.get(&book.sha256) {
                Some(p) => p.clone(),
                None => {
                    let path = state(&app).library().book_path(book);
                    // A file that can't be read is left out rather than failing the search.
                    let Ok(doc) = formats::open(&path) else {
                        continue;
                    };
                    let p = dedupe::profile(&doc);
                    cache.put(&book.sha256, p.clone());
                    p
                }
            };
            profiles.insert(book.id.clone(), profile);
        }
        let hashes: Vec<String> = books.iter().map(|b| b.sha256.clone()).collect();
        let _ = cache.save(&hashes);
        let _ = app.emit(
            "dedupe-progress",
            DedupeProgress {
                done: wanted.len(),
                total: wanted.len(),
                name: String::new(),
            },
        );
        let subset: Vec<Book> = wanted.iter().map(|&i| books[i].clone()).collect();
        Ok(dedupe::group_duplicates(&subset, &profiles))
    })
    .await
}

#[derive(Deserialize)]
pub struct Merge {
    keep: String,
    remove: Vec<String>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeReport {
    removed: usize,
    highlights: usize,
    bookmarks: usize,
}

/// Keeps one copy of each duplicate group: the others' reading time, furthest position,
/// bookmarks and highlights move onto it, then they are removed from the shelf.
#[tauri::command]
pub async fn merge_duplicates(app: AppHandle, merges: Vec<Merge>) -> CmdResult<MergeReport> {
    blocking(move || {
        let store = annotation_store(&app);
        let mut report = MergeReport::default();
        for m in merges {
            let remove: Vec<String> = m.remove.into_iter().filter(|id| *id != m.keep).collect();
            let (keep_path, copies) = {
                let lib = state(&app).library();
                let Some(keep) = lib.get(&m.keep) else {
                    continue;
                };
                let copies: Vec<(Book, PathBuf)> = remove
                    .iter()
                    .filter_map(|id| lib.get(id).map(|b| (b.clone(), lib.book_path(b))))
                    .collect();
                (lib.book_path(keep), copies)
            };
            let keep_doc = formats::open(&keep_path).map_err(err)?;
            let mut keep_ann = store.load(&m.keep).map_err(err)?;
            let mut furthest: Option<(Location, f32)> = None;
            for (book, path) in &copies {
                // A copy that can't be opened still goes; its notes can't be placed.
                if let Ok(doc) = formats::open(path) {
                    let ann = store.load(&book.id).map_err(err)?;
                    let moved = dedupe::transfer_annotations(&doc, &ann, &keep_doc, &mut keep_ann);
                    report.highlights += moved.highlights;
                    report.bookmarks += moved.bookmarks;
                }
                if book.position.is_some() && furthest.is_none_or(|(_, p)| book.progress > p) {
                    furthest = Some((dedupe::location_at(&keep_doc, book.progress), book.progress));
                }
            }
            store.save(&m.keep, &keep_ann).map_err(err)?;
            let ids: Vec<String> = copies.iter().map(|(b, _)| b.id.clone()).collect();
            let mut lib = state(&app).library();
            // Conversations about a copy stay with the book.
            for id in &ids {
                lib.db()
                    .conn()
                    .execute(
                        "UPDATE chats SET book_id = ?1 WHERE book_id = ?2",
                        rusqlite::params![m.keep, id],
                    )
                    .map_err(err)?;
                cards::move_book(&lib.db().conn(), id, &m.keep).map_err(err)?;
            }
            lib.absorb(&m.keep, &ids, furthest).map_err(err)?;
            lib.remove(&ids);
            lib.save().map_err(err)?;
            report.removed += ids.len();
        }
        Ok(report)
    })
    .await
}

/// The book to open at startup when a file was passed on the command line (`aireader book.epub`).
/// It is imported first if needed. Returns `None` after the first call.
#[tauri::command]
pub async fn take_startup_book(app: AppHandle) -> CmdResult<Option<String>> {
    blocking(move || {
        let Some(file) = state(&app).startup_file.lock().unwrap().take() else {
            return Ok(None);
        };
        let id = match import_file(&app, &file, None).map_err(err)? {
            Imported::Added(id) | Imported::Existing(id) => id,
        };
        state(&app).library().save().map_err(err)?;
        start_indexing(&app);
        Ok(Some(id))
    })
    .await
}

#[tauri::command]
pub async fn update_book(app: AppHandle, id: String, patch: BookPatch) -> CmdResult<Book> {
    blocking(move || {
        let mut lib = state(&app).library();
        let book = lib.update(&id, patch).map_err(err)?;
        lib.save().map_err(err)?;
        Ok(book)
    })
    .await
}

#[tauri::command]
pub async fn remove_books(app: AppHandle, ids: Vec<String>) -> CmdResult<()> {
    blocking(move || {
        state(&app).forget(&ids);
        let mut lib = state(&app).library();
        lib.remove(&ids);
        lib.save().map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn create_group(app: AppHandle, name: String) -> CmdResult<()> {
    blocking(move || {
        let mut lib = state(&app).library();
        lib.create_group(&name).map_err(err)?;
        lib.save().map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn rename_group(app: AppHandle, old: String, new: String) -> CmdResult<()> {
    blocking(move || {
        let mut lib = state(&app).library();
        lib.rename_group(&old, &new).map_err(err)?;
        lib.save().map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn delete_group(app: AppHandle, name: String) -> CmdResult<()> {
    blocking(move || {
        let mut lib = state(&app).library();
        lib.delete_group(&name);
        lib.save().map_err(err)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterInfo {
    title: String,
    blocks: usize,
    /// Text length in characters, for reading-time estimates.
    chars: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedBook {
    id: String,
    title: String,
    author: Option<String>,
    format: String,
    /// "flow" or "paged".
    kind: &'static str,
    toc: Vec<TocEntry>,
    chapters: Vec<ChapterInfo>,
    /// Height/width of each page (paged books only).
    pages: Vec<f32>,
    position: Option<Location>,
    read_seconds: u64,
    reading_speed: Option<f32>,
    rotation: u16,
}

#[tauri::command]
pub async fn open_book(app: AppHandle, id: String) -> CmdResult<OpenedBook> {
    blocking(move || {
        let st = state(&app);
        let book = st.open_book(&id).map_err(err)?;
        let entry = st
            .library()
            .get(&id)
            .cloned()
            .ok_or_else(|| crate::t!("book_not_found"))?;
        let (kind, chapters, pages) = match &book.doc.content {
            Content::Flow(chs) => (
                "flow",
                chs.iter()
                    .zip(&book.chars)
                    .map(|(c, &chars)| ChapterInfo {
                        title: c.title.clone(),
                        blocks: c.blocks.len(),
                        chars,
                    })
                    .collect(),
                Vec::new(),
            ),
            Content::Paged(src) => (
                "paged",
                Vec::new(),
                (0..src.page_count()).map(|i| src.page_aspect(i)).collect(),
            ),
        };
        Ok(OpenedBook {
            id,
            title: entry.title,
            author: entry.author,
            format: entry.format,
            kind,
            toc: book.doc.toc.clone(),
            chapters,
            pages,
            position: entry.position,
            read_seconds: entry.read_seconds,
            reading_speed: entry.reading_speed,
            rotation: entry.rotation,
        })
    })
    .await
}

#[tauri::command]
pub async fn get_chapter(app: AppHandle, id: String, index: usize) -> CmdResult<Vec<Block>> {
    blocking(move || {
        let book = state(&app).open_book(&id).map_err(err)?;
        match &book.doc.content {
            Content::Flow(chs) => chs
                .get(index)
                .map(|c| c.blocks.clone())
                .ok_or_else(|| crate::t!("chapter_not_found")),
            Content::Paged(_) => Err(crate::t!("not_text_book")),
        }
    })
    .await
}

#[tauri::command]
pub async fn search_book(app: AppHandle, id: String, query: String) -> CmdResult<Vec<Hit>> {
    blocking(move || {
        let book = state(&app).open_book(&id).map_err(err)?;
        let text = book
            .search
            .get_or_init(|| SearchText::build(&book.doc.content));
        Ok(text.search(&query, MAX_HITS))
    })
    .await
}

#[tauri::command]
pub async fn save_progress(
    app: AppHandle,
    id: String,
    position: Location,
    progress: f32,
) -> CmdResult<()> {
    blocking(move || {
        let mut lib = state(&app).library();
        lib.set_progress(&id, position, progress).map_err(err)?;
        lib.save_book(&id).map_err(err)
    })
    .await
}

/// Passages across the library matching `query`, for books still on the shelf.
#[tauri::command]
pub async fn search_library(
    app: AppHandle,
    query: String,
) -> CmdResult<Vec<crate::fulltext::BookHits>> {
    blocking(move || {
        let (db, ids): (_, HashSet<String>) = {
            let lib = state(&app).library();
            (
                lib.db().clone(),
                lib.books().into_iter().map(|b| b.id).collect(),
            )
        };
        let mut hits = crate::fulltext::search(&db.conn(), &query).map_err(err)?;
        hits.retain(|b| ids.contains(&b.book_id));
        Ok(hits)
    })
    .await
}

/// How far the full-text index has got, while books are being added to it.
#[tauri::command]
pub fn fulltext_status(app: AppHandle) -> Option<IndexProgress> {
    state(&app).indexer.progress.lock().unwrap().clone()
}

/// Stops adding books to the full-text index until the app is next started.
#[tauri::command]
pub fn stop_indexing(app: AppHandle) {
    state(&app).indexer.stopped.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> CmdResult<serde_json::Value> {
    blocking(move || Ok(state(&app).library().settings().clone())).await
}

/// Language for messages from the backend (errors, generated titles, AI answers on "auto").
#[tauri::command]
pub fn set_locale(locale: String) {
    crate::i18n::set_locale(&locale);
}

#[tauri::command]
pub async fn set_settings(app: AppHandle, settings: serde_json::Value) -> CmdResult<()> {
    blocking(move || {
        let mut lib = state(&app).library();
        lib.set_settings(settings);
        lib.save().map_err(err)
    })
    .await
}

fn annotation_store(app: &AppHandle) -> AnnotationStore {
    AnnotationStore::new(state(app).library().db())
}

#[tauri::command]
pub async fn get_annotations(app: AppHandle, id: String) -> CmdResult<Annotations> {
    blocking(move || annotation_store(&app).load(&id).map_err(err)).await
}

#[tauri::command]
pub async fn save_annotations(
    app: AppHandle,
    id: String,
    annotations: Annotations,
) -> CmdResult<()> {
    blocking(move || annotation_store(&app).save(&id, &annotations).map_err(err)).await
}

#[tauri::command]
pub async fn list_chats(app: AppHandle, book_id: String) -> CmdResult<Vec<crate::db::ChatRecord>> {
    blocking(move || {
        let db = state(&app).library().db().clone();
        crate::db::list_chats(&db.conn(), &book_id).map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn save_chat(
    app: AppHandle,
    book_id: String,
    chat: crate::db::ChatRecord,
) -> CmdResult<()> {
    blocking(move || {
        let db = state(&app).library().db().clone();
        crate::db::save_chat(&db.conn(), &book_id, &chat).map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn delete_chat(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(move || {
        let db = state(&app).library().db().clone();
        crate::db::delete_chat(&db.conn(), &id).map_err(err)
    })
    .await
}

/// A book's notes for exporting. Chapter names come from the book, which is parsed only if it
/// has highlights (and left out if it can't be read: they get generated names).
fn book_notes(app: &AppHandle, id: &str) -> anyhow::Result<Option<BookNotes>> {
    let st = state(app);
    let Some(entry) = st.library().get(id).cloned() else {
        return Ok(None);
    };
    let annotations = annotation_store(app).load(id)?;
    let chapters = if annotations.highlights.is_empty() {
        Vec::new()
    } else {
        match st.open_book(id).as_ref().map(|b| &b.doc.content) {
            Ok(Content::Flow(chs)) => chs.iter().map(|c| c.title.clone()).collect(),
            _ => Vec::new(),
        }
    };
    Ok(Some(BookNotes {
        title: entry.title,
        author: entry.author,
        chapters,
        annotations,
    }))
}

/// Exports the notes of one book (`id`), or of every book, to where the user picks: a file, or
/// a folder for all books in a one-file-per-book format. Returns that path, or `None` if the
/// dialog was cancelled.
#[tauri::command]
pub async fn export_notes(
    app: AppHandle,
    id: Option<String>,
    format: NotesFormat,
) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    blocking(move || {
        let ids = match &id {
            Some(id) => vec![id.clone()],
            None => {
                let db = state(&app).library().db().clone();
                let conn = db.conn();
                annotations::books_with_notes(&conn).map_err(err)?
            }
        };
        let mut books = Vec::new();
        for id in &ids {
            books.extend(book_notes(&app, id).map_err(err)?);
        }
        if books.is_empty() {
            return Err(crate::t!("notes_none"));
        }
        let ext = format.extension();
        if id.is_none() && !format.single_file() {
            let Some(dir) = app.dialog().file().blocking_pick_folder() else {
                return Ok(None);
            };
            let dir = dir.into_path().map_err(err)?;
            let mut taken = HashSet::new();
            for b in &books {
                let name = export::file_name(&b.title, ext, &mut taken);
                std::fs::write(dir.join(name), export::book_markdown(b, format)).map_err(err)?;
            }
            return Ok(Some(dir.display().to_string()));
        }
        let name = match &books[..] {
            [b] if id.is_some() => crate::t!("notes_file", title = b.title),
            _ => format!("AiReader-notes-{}.{ext}", library::today()),
        };
        let content = match format {
            NotesFormat::Csv => export::to_csv(&books),
            NotesFormat::Json => export::to_json(&books).map_err(err)?,
            _ => export::book_markdown(&books[0], format),
        };
        let name = match name.rsplit_once('.') {
            Some((stem, _)) => format!("{stem}.{ext}"),
            None => name,
        };
        let Some(path) = app
            .dialog()
            .file()
            .add_filter(ext.to_uppercase(), &[ext])
            .set_file_name(name)
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(err)?;
        std::fs::write(&path, content).map_err(err)?;
        Ok(Some(path.display().to_string()))
    })
    .await
}

// --- flashcards --------------------------------------------------------------------------------

/// A blocking command that works on the library database.
async fn with_db<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&rusqlite::Connection) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    blocking(move || {
        let db = state(&app).library().db().clone();
        let conn = db.conn();
        f(&conn).map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn add_card(app: AppHandle, card: NewCard) -> CmdResult<Card> {
    with_db(app, move |c| cards::add(c, card, library::now())).await
}

#[tauri::command]
pub async fn find_card(
    app: AppHandle,
    book_id: Option<String>,
    front: String,
) -> CmdResult<Option<Card>> {
    with_db(app, move |c| cards::find(c, book_id.as_deref(), &front)).await
}

#[tauri::command]
pub async fn list_cards(app: AppHandle, book_id: Option<String>) -> CmdResult<Vec<Card>> {
    with_db(app, move |c| cards::list(c, book_id.as_deref())).await
}

#[tauri::command]
pub async fn due_cards(app: AppHandle) -> CmdResult<Vec<Card>> {
    with_db(app, move |c| cards::due(c, library::now())).await
}

/// `today`: the Unix time the user's local day began.
#[tauri::command]
pub async fn card_stats(app: AppHandle, today: u64) -> CmdResult<Stats> {
    with_db(app, move |c| cards::stats(c, library::now(), today)).await
}

#[tauri::command]
pub async fn update_card(app: AppHandle, id: String, patch: CardPatch) -> CmdResult<Card> {
    with_db(app, move |c| cards::update(c, &id, patch)).await
}

#[tauri::command]
pub async fn delete_card(app: AppHandle, id: String) -> CmdResult<()> {
    with_db(app, move |c| cards::delete(c, &id)).await
}

/// Seconds until the card returns after each answer, from Again to Easy.
#[tauri::command]
pub async fn card_intervals(app: AppHandle, id: String) -> CmdResult<[u64; 4]> {
    with_db(app, move |c| cards::intervals(c, &id, library::now())).await
}

#[tauri::command]
pub async fn review_card(app: AppHandle, id: String, rating: u8) -> CmdResult<Card> {
    with_db(app, move |c| cards::review(c, &id, rating, library::now())).await
}

/// Exports every card to a CSV file the user picks; returns the path, or `None` if cancelled.
#[tauri::command]
pub async fn export_cards(app: AppHandle) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    blocking(move || {
        let db = state(&app).library().db().clone();
        let list = cards::list(&db.conn(), None).map_err(err)?;
        let Some(path) = app
            .dialog()
            .file()
            .add_filter("CSV", &["csv"])
            .set_file_name(format!("AiReader-cards-{}.csv", library::today()))
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(err)?;
        std::fs::write(&path, export::cards_csv(&list)).map_err(err)?;
        Ok(Some(path.display().to_string()))
    })
    .await
}

// --- dictionaries ------------------------------------------------------------------------------

const DICTIONARIES: &str = "dictionaries";

fn dict_records(app: &AppHandle) -> anyhow::Result<Vec<DictRecord>> {
    let db = state(app).library().db().clone();
    let conn = db.conn();
    Ok(crate::db::kv_get(&conn, DICTIONARIES)?.unwrap_or_default())
}

fn save_dict_records(app: &AppHandle, records: &[DictRecord]) -> anyhow::Result<()> {
    let db = state(app).library().db().clone();
    let conn = db.conn();
    crate::db::kv_set(&conn, DICTIONARIES, &records)
}

#[tauri::command]
pub async fn list_dictionaries(app: AppHandle) -> CmdResult<Vec<DictRecord>> {
    blocking(move || dict_records(&app).map_err(err)).await
}

/// Adds a dictionary the user picks; returns the list, or `None` if the dialog was cancelled.
#[tauri::command]
pub async fn import_dictionary(app: AppHandle) -> CmdResult<Option<Vec<DictRecord>>> {
    use tauri_plugin_dialog::DialogExt;
    blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter(crate::t!("dict_filter"), dictionary::EXTENSIONS)
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let file = file.into_path().map_err(err)?;
        let record = state(&app).dictionaries.import(&file).map_err(err)?;
        let mut records = dict_records(&app).map_err(err)?;
        records.push(record);
        save_dict_records(&app, &records).map_err(err)?;
        Ok(Some(records))
    })
    .await
}

/// Saves the list's order and which dictionaries are on; ones left out are deleted.
#[tauri::command]
pub async fn set_dictionaries(app: AppHandle, records: Vec<DictRecord>) -> CmdResult<()> {
    blocking(move || {
        for old in dict_records(&app).map_err(err)? {
            if !records.iter().any(|r| r.id == old.id) {
                state(&app).dictionaries.remove(&old.id).map_err(err)?;
            }
        }
        save_dict_records(&app, &records).map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn lookup_word(app: AppHandle, word: String) -> CmdResult<Vec<Definition>> {
    blocking(move || {
        let records = dict_records(&app).map_err(err)?;
        Ok(state(&app).dictionaries.lookup(&records, &word))
    })
    .await
}

#[tauri::command]
pub async fn list_fonts(app: AppHandle) -> CmdResult<Vec<String>> {
    blocking(move || Ok(state(&app).library().fonts())).await
}

/// Adds a font file the user picks; returns its name, or `None` if the dialog was cancelled.
#[tauri::command]
pub async fn import_font(app: AppHandle) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .add_filter(crate::t!("font_filter"), library::FONT_EXTENSIONS)
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(err)?;
        state(&app).library().add_font(&path).map(Some).map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn remove_font(app: AppHandle, name: String) -> CmdResult<()> {
    blocking(move || state(&app).library().remove_font(&name).map_err(err)).await
}

/// Backs up the library to a zip the user picks (`books`: include the book files). Returns the
/// path, or `None` if the dialog was cancelled.
#[tauri::command]
pub async fn backup_library(app: AppHandle, books: bool) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .add_filter("ZIP", &["zip"])
            .set_file_name(format!("AiReader-backup-{}.zip", library::today()))
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(err)?;
        // Not under the library lock: copying book files can take a while. The database
        // snapshot is consistent on its own.
        let lib = state(&app).library();
        let (root, db) = (lib.root().to_owned(), lib.db().clone());
        drop(lib);
        Library::backup_at(&root, &db, &path, books).map_err(err)?;
        Ok(Some(path.display().to_string()))
    })
    .await
}

/// Saves exported data (base64) through a save dialog; returns the path, or None if cancelled.
#[tauri::command]
pub async fn save_export(
    app: AppHandle,
    file_name: String,
    extension: String,
    data: String,
) -> CmdResult<Option<String>> {
    use base64::Engine;
    use tauri_plugin_dialog::DialogExt;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(err)?;
    blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .add_filter(extension.to_uppercase(), &[extension.as_str()])
            .set_file_name(file_name)
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(err)?;
        std::fs::write(&path, bytes).map_err(err)?;
        Ok(Some(path.display().to_string()))
    })
    .await
}

#[tauri::command]
pub async fn add_reading_time(
    app: AppHandle,
    id: String,
    seconds: u64,
    speed: Option<f32>,
) -> CmdResult<Book> {
    blocking(move || {
        let mut lib = state(&app).library();
        lib.add_reading_time(&id, seconds, speed).map_err(err)?;
        lib.save_book(&id).map_err(err)?;
        lib.get(&id)
            .cloned()
            .ok_or_else(|| crate::t!("book_not_found"))
    })
    .await
}

/// Text runs of a PDF page for the selectable text layer (empty for scans and comics).
#[tauri::command]
pub async fn get_page_text(
    app: AppHandle,
    id: String,
    page: usize,
) -> CmdResult<Vec<crate::model::TextRun>> {
    blocking(move || {
        let book = state(&app).open_book(&id).map_err(err)?;
        Ok(match &book.doc.content {
            Content::Paged(src) if page < src.page_count() => src.text_runs(page),
            _ => Vec::new(),
        })
    })
    .await
}
