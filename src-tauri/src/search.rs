//! Full-text search over reflowable chapters.

use serde::Serialize;

use crate::model::Chapter;

#[derive(Debug, Serialize)]
pub struct Hit {
    pub chapter: usize,
    pub block: usize,
    pub snippet: String,
}

/// Case-insensitive substring search; one hit per block, at most `max` hits.
pub fn search(chapters: &[Chapter], query: &str, max: usize) -> Vec<Hit> {
    let query = query.trim().to_lowercase();
    let mut hits = Vec::new();
    if query.is_empty() {
        return hits;
    }
    for (ci, ch) in chapters.iter().enumerate() {
        for (bi, block) in ch.blocks.iter().enumerate() {
            let text = block.text();
            let lower = text.to_lowercase();
            let Some(pos) = lower.find(&query) else {
                continue;
            };
            // Byte offsets only line up when lowercasing kept the length.
            let src = if lower.len() == text.len() {
                &text
            } else {
                &lower
            };
            let start = src[..pos]
                .char_indices()
                .rev()
                .nth(24)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let end = src[pos..]
                .char_indices()
                .nth(query.chars().count() + 40)
                .map(|(i, _)| pos + i)
                .unwrap_or(src.len());
            let snippet = format!(
                "{}{}{}",
                if start > 0 { "…" } else { "" },
                src[start..end].replace('\n', " "),
                if end < src.len() { "…" } else { "" }
            );
            hits.push(Hit {
                chapter: ci,
                block: bi,
                snippet,
            });
            if hits.len() >= max {
                return hits;
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Span};

    #[test]
    fn finds_case_insensitive_with_snippet() {
        let ch = Chapter {
            title: "c".into(),
            blocks: vec![
                Block::Paragraph(vec![Span::plain("nothing here")]),
                Block::Paragraph(vec![Span::plain("The Rabbit-Hole was deep")]),
            ],
        };
        let hits = search(&[ch], "rabbit", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].chapter, hits[0].block), (0, 1));
        assert_eq!(hits[0].snippet, "The Rabbit-Hole was deep");
    }
}
