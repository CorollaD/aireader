//! Format-independent document model shared by all parsers and the UI.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Inline text run with uniform styling.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: bool,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: SpanStyle::default(),
        }
    }
}

/// Encoded image bytes plus a process-unique URI used as the egui texture cache key.
#[derive(Clone, Debug)]
pub struct ImageRef {
    pub uri: String,
    pub bytes: Arc<[u8]>,
}

impl ImageRef {
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Self {
            uri: format!("bytes://aireader/img{n}"),
            bytes: bytes.into(),
        }
    }
}

/// Reflowable content block. The list is kept flat so a block index is a stable position.
#[derive(Clone, Debug)]
pub enum Block {
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Paragraph(Vec<Span>),
    Quote(Vec<Span>),
    ListItem {
        marker: String,
        depth: u8,
        spans: Vec<Span>,
    },
    Code(String),
    Image(ImageRef),
    Rule,
}

impl Block {
    /// Plain text of the block, used for search and titles.
    pub fn text(&self) -> String {
        match self {
            Block::Heading { spans, .. }
            | Block::Paragraph(spans)
            | Block::Quote(spans)
            | Block::ListItem { spans, .. } => spans_text(spans),
            Block::Code(s) => s.clone(),
            Block::Image(_) | Block::Rule => String::new(),
        }
    }
}

pub fn spans_text(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

#[derive(Clone, Debug, Default)]
pub struct Chapter {
    pub title: String,
    pub blocks: Vec<Block>,
}

/// A position inside a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    Flow { chapter: usize, block: usize },
    Page(usize),
}

/// Table-of-contents entry, flattened with an explicit nesting depth.
#[derive(Clone, Debug)]
pub struct TocEntry {
    pub title: String,
    pub depth: usize,
    pub location: Location,
}

/// Fixed-layout page provider (PDF, comics). Rendering may happen on a worker thread.
pub trait PageSource: Send + Sync {
    fn page_count(&self) -> usize;
    /// Height divided by width of the page.
    fn page_aspect(&self, index: usize) -> f32;
    /// Renders the page scaled to `width` pixels.
    fn render(&self, index: usize, width: u32) -> anyhow::Result<egui::ColorImage>;
}

pub enum Content {
    Flow(Vec<Chapter>),
    Paged(Arc<dyn PageSource>),
}

pub struct Document {
    pub path: PathBuf,
    pub title: String,
    pub author: Option<String>,
    pub toc: Vec<TocEntry>,
    pub content: Content,
}

impl Document {
    /// Builds a TOC from chapter titles when the format has no explicit one.
    pub fn chapter_toc(chapters: &[Chapter]) -> Vec<TocEntry> {
        chapters
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.title.is_empty())
            .map(|(i, c)| TocEntry {
                title: c.title.clone(),
                depth: 0,
                location: Location::Flow {
                    chapter: i,
                    block: 0,
                },
            })
            .collect()
    }

    pub fn file_stem(path: &std::path::Path) -> String {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}
