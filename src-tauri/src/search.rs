//! Full-text search: reflowable books by paragraph, PDFs by page (through their text layer).

use serde::Serialize;

use crate::model::{Content, Location};

#[derive(Debug, Serialize)]
pub struct Hit {
    pub location: Location,
    pub snippet: String,
}

/// A book's text prepared for searching: each paragraph (or page) with a lowercased copy.
/// Built once per opened book; PDFs read every page's text layer, so building takes a moment.
pub struct SearchText {
    units: Vec<(Location, String, String)>,
}

impl SearchText {
    pub fn build(content: &Content) -> Self {
        let mut units = Vec::new();
        match content {
            Content::Flow(chapters) => {
                for (ci, ch) in chapters.iter().enumerate() {
                    for (bi, block) in ch.blocks.iter().enumerate() {
                        let text = block.text();
                        if !text.trim().is_empty() {
                            let location = Location::Flow {
                                chapter: ci,
                                block: bi,
                            };
                            units.push((location, text.to_lowercase(), text));
                        }
                    }
                }
            }
            Content::Paged(src) => {
                for page in 0..src.page_count() {
                    if let Some(text) = src.page_text(page) {
                        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                        units.push((Location::Page { page }, text.to_lowercase(), text));
                    }
                }
            }
        }
        Self { units }
    }

    /// Case-insensitive substring search; one hit per paragraph or page, at most `max` hits.
    pub fn search(&self, query: &str, max: usize) -> Vec<Hit> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        self.units
            .iter()
            .filter_map(|(location, lower, text)| {
                let pos = lower.find(&query)?;
                Some(Hit {
                    location: *location,
                    snippet: snippet(text, lower, pos, query.chars().count()),
                })
            })
            .take(max)
            .collect()
    }
}

/// The match at byte `pos` of `lower` with some context on both sides.
fn snippet(text: &str, lower: &str, pos: usize, query_chars: usize) -> String {
    // Byte offsets only line up when lowercasing kept the length.
    let src = if lower.len() == text.len() {
        text
    } else {
        lower
    };
    let start = src[..pos]
        .char_indices()
        .rev()
        .nth(24)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let end = src[pos..]
        .char_indices()
        .nth(query_chars + 40)
        .map(|(i, _)| pos + i)
        .unwrap_or(src.len());
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        src[start..end].replace('\n', " "),
        if end < src.len() { "…" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Chapter, Span};

    #[test]
    fn finds_case_insensitive_with_snippet() {
        let ch = Chapter {
            title: "c".into(),
            blocks: vec![
                Block::Paragraph(vec![Span::plain("nothing here")]),
                Block::Paragraph(vec![Span::plain("The Rabbit-Hole was deep")]),
            ],
        };
        let text = SearchText::build(&Content::Flow(vec![ch]));
        let hits = text.search("rabbit", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].location,
            Location::Flow {
                chapter: 0,
                block: 1
            }
        );
        assert_eq!(hits[0].snippet, "The Rabbit-Hole was deep");
        assert!(text.search("  ", 10).is_empty());
    }

    #[test]
    fn long_text_is_cut_around_the_match() {
        let long = format!("{}needle{}", "a".repeat(100), "b".repeat(100));
        let ch = Chapter {
            title: "c".into(),
            blocks: vec![Block::Paragraph(vec![Span::plain(&long)])],
        };
        let hits = SearchText::build(&Content::Flow(vec![ch])).search("NEEDLE", 10);
        assert!(hits[0].snippet.starts_with('…') && hits[0].snippet.ends_with('…'));
        assert!(hits[0].snippet.contains("needle"));
    }
}
