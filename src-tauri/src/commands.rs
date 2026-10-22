//! Tauri commands exposed to the frontend, plus the shared application state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::annotations::{self, AnnotationStore, Annotations};
use crate::formats;
use crate::library::{self, Book, BookPatch, Library};
use crate::model::{Block, Content, Document, Location, TocEntry};
use crate::search::{self, Hit};

type CmdResult<T> = Result<T, String>;

const MAX_OPEN: usize = 4;
const MAX_HITS: usize = 1000;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

/// A parsed book kept in memory while it is being read.
pub struct OpenBook {
    pub doc: Document,
    /// Image id → encoded bytes, for the `book://` protocol.
    pub images: HashMap<u64, Arc<[u8]>>,
}

pub struct AppState {
    library: Mutex<Library>,
    /// File passed on the command line, consumed by `take_startup_book`.
    startup_file: Mutex<Option<PathBuf>>,
    /// Most recently used last.
    open: Mutex<Vec<(String, Arc<OpenBook>)>>,
}

impl AppState {
    pub fn new(library: Library, startup_file: Option<PathBuf>) -> Self {
        Self {
            library: Mutex::new(library),
            startup_file: Mutex::new(startup_file),
            open: Mutex::new(Vec::new()),
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
        if let Content::Flow(chapters) = &doc.content {
            for block in chapters.iter().flat_map(|c| &c.blocks) {
                if let Block::Image(img) = block {
                    images.insert(img.id, img.bytes.clone());
                }
            }
        }
        let book = Arc::new(OpenBook { doc, images });
        let mut open = self.open.lock().unwrap();
        open.retain(|(k, _)| k != id);
        open.push((id.to_owned(), book.clone()));
        if open.len() > MAX_OPEN {
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
pub struct ImportReport {
    added: Vec<String>,
    duplicates: Vec<String>,
    failed: Vec<(String, String)>,
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
    let hash = library::hash_file(file)?;
    if let Some(existing) = state(app).library().find_hash(&hash) {
        return Ok(Imported::Existing(existing.id.clone()));
    }
    let root = state(app).library().root().to_owned();
    let prepared = Library::prepare(&root, file, &hash)?;
    let mut lib = state(app).library();
    if lib.add(prepared, group) {
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
                Ok(Imported::Added(_)) => report.added.push(name),
                Ok(Imported::Existing(_)) => report.duplicates.push(name),
                Err(e) => report.failed.push((name, err(e))),
            }
        }
        state(&app).library().save().map_err(err)?;
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
                    .map(|c| ChapterInfo {
                        title: c.title.clone(),
                        blocks: c.blocks.len(),
                        chars: c.blocks.iter().map(|b| b.text().chars().count()).sum(),
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
        match &book.doc.content {
            Content::Flow(chs) => Ok(search::search(chs, &query, MAX_HITS)),
            Content::Paged(_) => Ok(Vec::new()),
        }
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
        lib.save().map_err(err)
    })
    .await
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
    AnnotationStore::new(state(app).library().root())
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

/// Writes the book's highlights and notes as Markdown to a file the user picks.
/// Returns the chosen path, or `None` if the dialog was cancelled.
#[tauri::command]
pub async fn export_notes(app: AppHandle, id: String) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    blocking(move || {
        let entry = state(&app)
            .library()
            .get(&id)
            .cloned()
            .ok_or_else(|| crate::t!("book_not_found"))?;
        let book = state(&app).open_book(&id).map_err(err)?;
        let titles: Vec<String> = match &book.doc.content {
            Content::Flow(chs) => chs.iter().map(|c| c.title.clone()).collect(),
            Content::Paged(_) => Vec::new(),
        };
        let ann = annotation_store(&app).load(&id).map_err(err)?;
        let markdown =
            annotations::to_markdown(&entry.title, entry.author.as_deref(), &titles, &ann);
        let Some(path) = app
            .dialog()
            .file()
            .add_filter("Markdown", &["md"])
            .set_file_name(crate::t!("notes_file", title = entry.title))
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(err)?;
        std::fs::write(&path, markdown).map_err(err)?;
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
        lib.save().map_err(err)?;
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
