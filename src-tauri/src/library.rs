//! The bookshelf: imported books are copied into the library directory, de-duplicated by
//! content hash, and described in `library.json` together with groups and reader settings.
//!
//! Layout under the library root:
//! - `library.json`
//! - `books/<id>/<original file name>`
//! - `covers/<id>.jpg`

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::formats;
use crate::model::Location;

const COVER_WIDTH: u32 = 360;
const COVER_HEIGHT: u32 = 540;

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
    pub groups: Vec<String>,
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
}

/// Editable book fields; `None` leaves a field unchanged.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookPatch {
    pub title: Option<String>,
    pub author: Option<String>,
    pub groups: Option<Vec<String>>,
}

pub struct Library {
    root: PathBuf,
    data: LibraryData,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
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
}

impl Library {
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root.join("books"))?;
        std::fs::create_dir_all(root.join("covers"))?;
        let file = root.join("library.json");
        let data = match std::fs::read(&file) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("library.json 已损坏")?,
            Err(_) => LibraryData::default(),
        };
        Ok(Self {
            root: root.to_owned(),
            data,
        })
    }

    pub fn save(&self) -> Result<()> {
        let file = self.root.join("library.json");
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.data)?)?;
        std::fs::rename(tmp, file)?;
        Ok(())
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
            .context("书籍不存在")
    }

    pub fn book_path(&self, book: &Book) -> PathBuf {
        self.root.join("books").join(&book.id).join(&book.file_name)
    }

    pub fn file_of(&self, id: &str) -> Option<PathBuf> {
        self.get(id).map(|b| self.book_path(b))
    }

    pub fn cover_path(&self, id: &str) -> PathBuf {
        self.root.join("covers").join(format!("{id}.jpg"))
    }

    /// The id of an existing book with this content hash.
    pub fn find_hash(&self, sha256: &str) -> Option<&Book> {
        self.data.books.iter().find(|b| b.sha256 == sha256)
    }

    /// Copies `src` into the library and parses it. Runs without holding the library lock;
    /// the result is committed with [`Library::add`]. Fails on unreadable or unsupported files.
    pub fn prepare(root: &Path, src: &Path, sha256: &str) -> Result<Prepared> {
        let file_name = src
            .file_name()
            .context("无效的文件名")?
            .to_string_lossy()
            .into_owned();
        let id = sha256[..16].to_owned();
        let dir = root.join("books").join(&id);
        std::fs::create_dir_all(&dir)?;
        let dest = dir.join(&file_name);
        let result = (|| {
            std::fs::copy(src, &dest).context("复制文件失败")?;
            let format = formats::detect(&dest)?.label().to_owned();
            let doc = formats::open(&dest)?;
            let has_cover = formats::cover_image(&dest, &doc)
                .and_then(|bytes| thumbnail(&bytes).ok())
                .is_some_and(|jpg| {
                    std::fs::write(root.join("covers").join(format!("{id}.jpg")), jpg).is_ok()
                });
            Ok(Book {
                id: id.clone(),
                title: doc.title.clone(),
                author: doc.author.clone().filter(|a| !a.trim().is_empty()),
                format,
                file_name: file_name.clone(),
                size: std::fs::metadata(&dest)?.len(),
                sha256: sha256.to_owned(),
                has_cover,
                added: now(),
                last_read: None,
                progress: 0.0,
                position: None,
                groups: Vec::new(),
                missing: false,
            })
        })();
        match result {
            Ok(book) => Ok(Prepared { book }),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                Err(e)
            }
        }
    }

    /// Adds a prepared book (into `groups`, if given). Returns `false` if a book with the same
    /// content was added concurrently.
    pub fn add(&mut self, prepared: Prepared, groups: &[String]) -> bool {
        let mut book = prepared.book;
        if self.find_hash(&book.sha256).is_some() {
            return false;
        }
        for g in groups {
            self.ensure_group(g);
        }
        book.groups = groups.to_vec();
        self.data.books.push(book);
        true
    }

    pub fn update(&mut self, id: &str, patch: BookPatch) -> Result<Book> {
        if let Some(groups) = &patch.groups {
            for g in groups {
                self.ensure_group(g);
            }
        }
        let book = self.get_mut(id)?;
        if let Some(title) = patch.title.map(|t| t.trim().to_owned()) {
            if title.is_empty() {
                bail!("书名不能为空");
            }
            book.title = title;
        }
        if let Some(author) = patch.author {
            let author = author.trim();
            book.author = (!author.is_empty()).then(|| author.to_owned());
        }
        if let Some(mut groups) = patch.groups {
            groups.dedup();
            book.groups = groups;
        }
        Ok(book.clone())
    }

    /// Removes books from the shelf and deletes their stored copies and covers.
    pub fn remove(&mut self, ids: &[String]) {
        for id in ids {
            let _ = std::fs::remove_dir_all(self.root.join("books").join(id));
            let _ = std::fs::remove_file(self.cover_path(id));
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

    fn ensure_group(&mut self, name: &str) {
        if !self.data.groups.iter().any(|g| g == name) {
            self.data.groups.push(name.to_owned());
        }
    }

    pub fn create_group(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            bail!("分组名不能为空");
        }
        if self.data.groups.iter().any(|g| g == name) {
            bail!("分组「{name}」已存在");
        }
        self.data.groups.push(name.to_owned());
        Ok(())
    }

    pub fn rename_group(&mut self, old: &str, new: &str) -> Result<()> {
        let new = new.trim();
        if new.is_empty() {
            bail!("分组名不能为空");
        }
        if old == new {
            return Ok(());
        }
        if self.data.groups.iter().any(|g| g == new) {
            bail!("分组「{new}」已存在");
        }
        let g = self
            .data
            .groups
            .iter_mut()
            .find(|g| *g == old)
            .context("分组不存在")?;
        *g = new.to_owned();
        for b in &mut self.data.books {
            for bg in &mut b.groups {
                if bg == old {
                    *bg = new.to_owned();
                }
            }
        }
        Ok(())
    }

    /// Deletes a group; its books stay on the shelf.
    pub fn delete_group(&mut self, name: &str) {
        self.data.groups.retain(|g| g != name);
        for b in &mut self.data.books {
            b.groups.retain(|g| g != name);
        }
    }

    pub fn settings(&self) -> &serde_json::Value {
        &self.data.settings
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
        let hash = hash_file(src)?;
        if lib.find_hash(&hash).is_some() {
            return Ok(false);
        }
        let prepared = Library::prepare(lib.root(), src, &hash)?;
        Ok(lib.add(prepared, &[]))
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
                groups: Some(vec!["小说".into()]),
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
        lib.save().unwrap();

        let mut lib = Library::open(&root).unwrap();
        let b = lib.get(&id).unwrap();
        assert_eq!(b.title, "新书名");
        assert_eq!(b.author.as_deref(), Some("某人"));
        assert_eq!(b.groups, ["文学"]);
        assert_eq!(
            b.position,
            Some(Location::Flow {
                chapter: 1,
                block: 0
            })
        );
        assert!(b.last_read.is_some());
        assert_eq!(lib.groups(), ["文学", "技术"]);

        lib.delete_group("文学");
        assert!(lib.get(&id).unwrap().groups.is_empty());
        lib.remove(std::slice::from_ref(&id));
        assert!(lib.get(&id).is_none());
        assert!(!root.join("books").join(&id).exists());
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
