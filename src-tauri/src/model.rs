//! Format-independent document model shared by all parsers and the frontend.

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Inline text run with uniform styling.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Span {
    pub text: String,
    #[serde(flatten)]
    pub style: SpanStyle,
}

/// Serialized compactly: only `true` flags are emitted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SpanStyle {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub code: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
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

/// Encoded image bytes with a content-derived id, so re-parsing a book yields the same ids.
/// Serialized as the id only; the frontend fetches the bytes through the `book://` protocol.
#[derive(Clone, Debug)]
pub struct ImageRef {
    pub id: u64,
    pub bytes: Arc<[u8]>,
}

impl ImageRef {
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        let bytes: Arc<[u8]> = bytes.into();
        // `DefaultHasher::new()` uses fixed keys, so the id is deterministic. Keep 53 bits so the
        // id survives the round trip through a JavaScript number.
        let mut h = std::hash::DefaultHasher::new();
        bytes.hash(&mut h);
        Self {
            id: h.finish() >> 11,
            bytes,
        }
    }
}

impl Serialize for ImageRef {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.id.serialize(s)
    }
}

/// Reflowable content block. The list is kept flat so a block index is a stable position.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
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
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Location {
    Flow { chapter: usize, block: usize },
    Page { page: usize },
}

/// Table-of-contents entry, flattened with an explicit nesting depth.
#[derive(Clone, Debug, Serialize)]
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
    fn render(&self, index: usize, width: u32) -> anyhow::Result<PageImage>;
    /// The page's text layer, if it has one (PDF); `None` for images and scans.
    fn page_text(&self, _index: usize) -> Option<String> {
        None
    }
    /// Positioned text runs for making the rendered page selectable; empty without a text layer.
    fn text_runs(&self, _index: usize) -> Vec<TextRun> {
        Vec::new()
    }
}

/// A run of text on a page. Coordinates are fractions of the unrotated page size, measured
/// from the top-left corner.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextRun {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Rendered page as straight (non-premultiplied) RGBA pixels.
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The frontend (src/types.ts) relies on these exact JSON shapes.
    #[test]
    fn json_shapes_match_frontend_types() {
        let blocks = vec![
            Block::Heading {
                level: 2,
                spans: vec![Span::plain("T")],
            },
            Block::Paragraph(vec![Span {
                text: "b".into(),
                style: SpanStyle {
                    bold: true,
                    ..Default::default()
                },
            }]),
            Block::ListItem {
                marker: "•".into(),
                depth: 1,
                spans: vec![],
            },
            Block::Code("x".into()),
            Block::Rule,
        ];
        assert_eq!(
            serde_json::to_value(&blocks).unwrap(),
            json!([
                {"kind": "heading", "value": {"level": 2, "spans": [{"text": "T"}]}},
                {"kind": "paragraph", "value": [{"text": "b", "bold": true}]},
                {"kind": "listItem", "value": {"marker": "•", "depth": 1, "spans": []}},
                {"kind": "code", "value": "x"},
                {"kind": "rule"},
            ])
        );
        let img = ImageRef::new(vec![1u8, 2, 3]);
        assert_eq!(
            img.id,
            ImageRef::new(vec![1u8, 2, 3]).id,
            "ids are content-derived"
        );
        assert!(img.id < 1 << 53, "ids must be exact as JavaScript numbers");
        assert_eq!(
            serde_json::to_value(Block::Image(img.clone())).unwrap(),
            json!({"kind": "image", "value": img.id})
        );

        let loc = Location::Flow {
            chapter: 1,
            block: 2,
        };
        assert_eq!(
            serde_json::to_value(loc).unwrap(),
            json!({"kind": "flow", "chapter": 1, "block": 2})
        );
        assert_eq!(
            serde_json::to_value(Location::Page { page: 3 }).unwrap(),
            json!({"kind": "page", "page": 3})
        );
        assert_eq!(
            serde_json::from_value::<Location>(json!({"kind": "page", "page": 3})).unwrap(),
            Location::Page { page: 3 }
        );
    }
}
