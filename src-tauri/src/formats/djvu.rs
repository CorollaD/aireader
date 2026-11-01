//! DjVu scans. Pages are rendered by `djvu-rs`; many files carry a hidden text layer, and those
//! that don't can go through text recognition like a scanned PDF.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use djvu_rs::djvu_render::{RenderOptions, render_pixmap};
use djvu_rs::text::{TextZone, TextZoneKind};
use djvu_rs::{DjVuBookmark, DjVuDocument};

use crate::model::{Content, Document, Location, PageImage, PageSource, TextRun, TocEntry};

pub struct DjvuSource {
    doc: DjVuDocument,
    aspects: Vec<f32>,
}

/// A page's lines: the finest zones wanted, whatever depth a file nests them at.
fn collect_lines<'a>(zones: &'a [TextZone], out: &mut Vec<&'a TextZone>) {
    for zone in zones {
        let leaf = zone.children.is_empty();
        if matches!(zone.kind, TextZoneKind::Line) || (leaf && !zone.text.trim().is_empty()) {
            out.push(zone);
        } else {
            collect_lines(&zone.children, out);
        }
    }
}

fn runs(zones: &[TextZone], width: f32, height: f32) -> Vec<TextRun> {
    let mut lines = Vec::new();
    collect_lines(zones, &mut lines);
    lines
        .into_iter()
        .filter(|z| !z.text.trim().is_empty())
        .map(|z| TextRun {
            text: z.text.trim().to_owned(),
            x: z.rect.x as f32 / width,
            y: z.rect.y as f32 / height,
            w: z.rect.width as f32 / width,
            h: z.rect.height as f32 / height,
        })
        .collect()
}

impl PageSource for DjvuSource {
    fn page_count(&self) -> usize {
        self.aspects.len()
    }

    fn page_aspect(&self, index: usize) -> f32 {
        self.aspects.get(index).copied().unwrap_or(1.414)
    }

    fn render(&self, index: usize, width: u32) -> Result<PageImage> {
        let page = self.doc.page(index)?;
        let pixmap = render_pixmap(page, &RenderOptions::fit_to_width(page, width))?;
        Ok(PageImage {
            width: pixmap.width,
            height: pixmap.height,
            rgba: pixmap.data.to_vec(),
        })
    }

    fn page_text(&self, index: usize) -> Option<String> {
        let text = self.doc.page(index).ok()?.text().ok()??;
        (!text.trim().is_empty()).then_some(text)
    }

    fn text_runs(&self, index: usize) -> Vec<TextRun> {
        let Ok(page) = self.doc.page(index) else {
            return Vec::new();
        };
        let (width, height) = (page.width() as f32, page.height() as f32);
        match page.text_layer() {
            Ok(Some(layer)) if width > 0.0 && height > 0.0 => runs(&layer.zones, width, height),
            _ => Vec::new(),
        }
    }

    fn image_dpi(&self, index: usize) -> Option<f32> {
        Some(self.doc.page(index).ok()?.dpi() as f32)
    }

    fn photographic(&self, _index: usize) -> bool {
        true
    }
}

/// The page a bookmark's `#12` points to; other targets name page files, and are left out.
fn bookmark_page(url: &str) -> Option<usize> {
    url.strip_prefix('#')?.parse::<usize>().ok()?.checked_sub(1)
}

fn add_toc(marks: &[DjVuBookmark], depth: usize, pages: usize, toc: &mut Vec<TocEntry>) {
    for mark in marks {
        if let Some(page) = bookmark_page(&mark.url).filter(|p| *p < pages) {
            toc.push(TocEntry {
                title: mark.title.trim().to_owned(),
                depth,
                location: Location::Page { page },
            });
        }
        add_toc(&mark.children, depth + 1, pages, toc);
    }
}

pub fn load(path: &Path) -> Result<Document> {
    let bytes = std::fs::read(path)?;
    let doc = DjVuDocument::parse(&bytes)
        .map_err(|e| anyhow!(crate::t!("parse_failed", format = "DjVu", error = e)))?;
    if doc.page_count() == 0 {
        bail!(crate::t!("document_empty"));
    }
    let aspects = (0..doc.page_count())
        .map(|i| {
            let page = doc.page(i)?;
            Ok(page.height() as f32 / (page.width() as f32).max(1.0))
        })
        .collect::<Result<Vec<f32>>>()?;
    let mut toc = Vec::new();
    add_toc(doc.bookmarks(), 0, aspects.len(), &mut toc);
    Ok(Document {
        path: path.to_owned(),
        title: Document::file_stem(path),
        author: None,
        toc,
        cover: None,
        content: Content::Paged(Arc::new(DjvuSource { doc, aspects })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use djvu_rs::text::Rect;

    fn zone(kind: TextZoneKind, text: &str, children: Vec<TextZone>) -> TextZone {
        TextZone {
            kind,
            rect: Rect {
                x: 100,
                y: 200,
                width: 500,
                height: 40,
            },
            text: text.to_owned(),
            children,
        }
    }

    #[test]
    fn lines_become_runs_in_page_fractions() {
        let line = zone(
            TextZoneKind::Line,
            " 第一行 ",
            vec![zone(TextZoneKind::Word, "第一行", Vec::new())],
        );
        let page = zone(TextZoneKind::Page, "第一行", vec![line]);
        let found = runs(&[page], 1000.0, 2000.0);
        assert_eq!(found.len(), 1, "a line, not its words");
        assert_eq!(found[0].text, "第一行");
        assert_eq!((found[0].x, found[0].y), (0.1, 0.1));
        assert_eq!((found[0].w, found[0].h), (0.5, 0.02));
    }

    #[test]
    fn files_without_lines_still_give_their_words() {
        let page = zone(
            TextZoneKind::Page,
            "",
            vec![zone(TextZoneKind::Word, "词", Vec::new())],
        );
        assert_eq!(runs(&[page], 100.0, 100.0)[0].text, "词");
    }

    #[test]
    fn bookmarks_point_at_numbered_pages() {
        assert_eq!(bookmark_page("#1"), Some(0));
        assert_eq!(bookmark_page("#12"), Some(11));
        assert_eq!(bookmark_page("#0"), None);
        assert_eq!(bookmark_page("#page.djvu"), None);
        assert_eq!(bookmark_page("https://example.org"), None);
    }

    #[test]
    fn a_broken_file_is_an_error() {
        let dir = std::env::temp_dir().join(format!("aireader-djvu-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.djvu");
        std::fs::write(&file, b"AT&TFORMnot really").unwrap();
        assert!(load(&file).is_err());
    }
}
