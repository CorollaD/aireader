//! PDF via PDFium, loaded at runtime from a dynamic library.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Result, anyhow};
use pdfium_render::prelude::*;

use crate::model::{Content, Document, Location, PageSource, TocEntry};

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

/// Directories searched for the PDFium library before falling back to the system path.
fn library_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::var_os("AIREADER_PDFIUM_DIR") {
        dirs.push(PathBuf::from(d));
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_owned))
    {
        dirs.push(exe_dir.join("lib"));
        dirs.push(exe_dir.join("../Frameworks"));
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
            "未找到 PDFium 动态库（{}）。请运行 scripts/fetch-pdfium.sh，或把 {} 放到程序目录下的 lib/ 中。\n{e}",
            Pdfium::pdfium_platform_library_name().to_string_lossy(),
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

    fn render(&self, index: usize, width: u32) -> Result<egui::ColorImage> {
        let page = self.doc.pages().get(index as PdfPageIndex)?;
        let config = PdfRenderConfig::new()
            .set_target_width(width as Pixels)
            .render_form_data(true);
        let bitmap = page.render_with_config(&config)?;
        let size = [bitmap.width() as usize, bitmap.height() as usize];
        Ok(egui::ColorImage::from_rgba_unmultiplied(
            size,
            &bitmap.as_rgba_bytes(),
        ))
    }
}

pub fn load(path: &Path) -> Result<Document> {
    let doc = pdfium()?
        .load_pdf_from_file(path, None)
        .map_err(|e| match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                anyhow!("PDF 已加密，暂不支持输入密码")
            }
            e => anyhow!("PDF 打开失败: {e}"),
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
                location: Location::Page(page as usize),
            });
        }
        collect_bookmarks(bm.first_child(), depth + 1, out, budget);
        node = bm.next_sibling();
    }
}
