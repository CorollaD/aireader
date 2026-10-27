//! PDF via PDFium, loaded at runtime from a dynamic library.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Result, anyhow};
use pdfium_render::prelude::*;

use crate::model::{Content, Document, Location, PageImage, PageSource, TextRun, TocEntry};

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();
static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Registers the app's bundled resource directory, where installers place `lib/<pdfium>`.
/// Call before the first PDF is opened.
pub fn set_resource_dir(dir: PathBuf) {
    let _ = RESOURCE_DIR.set(dir);
}

/// Directories searched for the PDFium library before falling back to the system path.
fn library_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::var_os("AIREADER_PDFIUM_DIR") {
        dirs.push(PathBuf::from(d));
    }
    if let Some(res) = RESOURCE_DIR.get() {
        dirs.push(res.join("lib"));
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_owned))
    {
        dirs.push(exe_dir.join("lib"));
        dirs.push(exe_dir.join("../Frameworks"));
        dirs.push(exe_dir.join("../Resources/lib"));
        dirs.push(exe_dir.join("../lib"));
        // `target/{debug,release}` during development.
        dirs.push(exe_dir.join("../../lib"));
        dirs.push(exe_dir);
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("lib"));
        dirs.push(cwd);
    }
    dirs
}

fn bind() -> Result<Pdfium, String> {
    for dir in library_dirs() {
        let lib = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if lib.exists()
            && let Ok(bindings) = Pdfium::bind_to_library(&lib)
        {
            return Ok(Pdfium::new(bindings));
        }
    }
    Pdfium::bind_to_system_library()
        .map(Pdfium::new)
        .map_err(|e| e.to_string())
}

pub fn pdfium() -> Result<&'static Pdfium> {
    PDFIUM.get_or_init(bind).as_ref().map_err(|e| {
        anyhow!(
            "The PDFium library {} was not found, so PDFs can't be opened.\n\
             In development, run src-tauri/scripts/fetch-pdfium.sh and rebuild, \
             or point AIREADER_PDFIUM_DIR at the directory that contains it.\n({e})",
            Pdfium::pdfium_platform_library_name().to_string_lossy(),
        )
    })
}

pub struct PdfSource {
    doc: PdfDocument<'static>,
    aspects: Vec<f32>,
}

impl PageSource for PdfSource {
    fn page_count(&self) -> usize {
        self.aspects.len()
    }

    fn page_aspect(&self, index: usize) -> f32 {
        self.aspects.get(index).copied().unwrap_or(1.414)
    }

    fn page_text(&self, index: usize) -> Option<String> {
        let page = self.doc.pages().get(index as PdfPageIndex).ok()?;
        let text = page.text().ok()?.all();
        (!text.trim().is_empty()).then_some(text)
    }

    fn text_runs(&self, index: usize) -> Vec<TextRun> {
        let Ok(page) = self.doc.pages().get(index as PdfPageIndex) else {
            return Vec::new();
        };
        let (pw, ph) = (page.width().value, page.height().value);
        let Ok(text) = page.text() else {
            return Vec::new();
        };
        if pw <= 0.0 || ph <= 0.0 {
            return Vec::new();
        }
        text.segments()
            .iter()
            .filter_map(|seg| {
                let t = seg.text();
                if t.trim().is_empty() {
                    return None;
                }
                let b = seg.bounds();
                let (left, right, top, bottom) = (
                    b.left().value,
                    b.right().value,
                    b.top().value,
                    b.bottom().value,
                );
                // Vertical text (e.g. margin watermarks) would become one huge glyph box; skip it.
                if t.chars().count() > 2 && (top - bottom) > 2.0 * (right - left) {
                    return None;
                }
                Some(TextRun {
                    text: t,
                    x: left / pw,
                    y: (ph - top) / ph,
                    w: (right - left) / pw,
                    h: (top - bottom) / ph,
                })
            })
            .collect()
    }

    fn image_dpi(&self, index: usize) -> Option<f32> {
        let page = self.doc.pages().get(index as PdfPageIndex).ok()?;
        // The largest picture on the page is the scan, if this is a scanned book.
        page.objects()
            .iter()
            .filter_map(|object| {
                let image = object.as_image_object()?;
                let inches = object.bounds().ok()?.width().value / 72.0;
                let pixels = image.width().ok()? as f32;
                (inches > 1.0 && pixels > 0.0).then_some((inches, pixels / inches))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, dpi)| dpi)
    }

    fn photographic(&self, index: usize) -> bool {
        self.page_text(index).is_none() && self.image_dpi(index).is_some()
    }

    fn render(&self, index: usize, width: u32) -> Result<PageImage> {
        let page = self.doc.pages().get(index as PdfPageIndex)?;
        let config = PdfRenderConfig::new()
            .set_target_width(width as Pixels)
            .render_form_data(true);
        let bitmap = page.render_with_config(&config)?;
        Ok(PageImage {
            width: bitmap.width() as u32,
            height: bitmap.height() as u32,
            rgba: bitmap.as_rgba_bytes(),
        })
    }
}

pub fn load(path: &Path) -> Result<Document> {
    let doc = pdfium()?
        .load_pdf_from_file(path, None)
        .map_err(|e| match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                anyhow!(crate::t!("pdf_encrypted"))
            }
            e => anyhow!(crate::t!("pdf_open_failed", error = e)),
        })?;
    let aspects = doc
        .pages()
        .page_sizes()?
        .iter()
        .map(|r| {
            let (w, h) = (r.width().value, r.height().value);
            if w > 0.0 { h / w } else { 1.414 }
        })
        .collect();
    let title = doc
        .metadata()
        .get(PdfDocumentMetadataTagType::Title)
        .map(|t| t.value().trim().to_owned())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| Document::file_stem(path));
    let author = doc
        .metadata()
        .get(PdfDocumentMetadataTagType::Author)
        .map(|t| t.value().trim().to_owned())
        .filter(|t| !t.is_empty());

    let mut toc = Vec::new();
    if let Some(root) = doc.bookmarks().root() {
        let mut budget = 10_000;
        collect_bookmarks(Some(root), 0, &mut toc, &mut budget);
    }

    let source = PdfSource { doc, aspects };
    Ok(Document {
        path: path.to_owned(),
        title,
        author,
        toc,
        cover: None,
        content: Content::Paged(Arc::new(source)),
    })
}

/// Walks a bookmark level via `next_sibling`; `budget` guards against cyclic outlines.
fn collect_bookmarks(
    mut node: Option<PdfBookmark>,
    depth: usize,
    out: &mut Vec<TocEntry>,
    budget: &mut usize,
) {
    while let Some(bm) = node {
        if *budget == 0 || depth > 16 {
            return;
        }
        *budget -= 1;
        let page = bm.destination().and_then(|d| d.page_index().ok());
        if let (Some(title), Some(page)) = (bm.title(), page) {
            out.push(TocEntry {
                title: title.trim().to_owned(),
                depth,
                location: Location::Page {
                    page: page as usize,
                },
            });
        }
        collect_bookmarks(bm.first_child(), depth + 1, out, budget);
        node = bm.next_sibling();
    }
}
