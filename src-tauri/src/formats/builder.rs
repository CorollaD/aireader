//! Accumulates styled inline text and emits flat [`Block`]s. Shared by the markup parsers.

use crate::model::{Block, Chapter, Location, Span, SpanStyle, TocEntry};

#[derive(Default)]
pub struct Builder {
    pub blocks: Vec<Block>,
    pub style: SpanStyle,
    /// `href` of the link being read, if any.
    pub href: Option<String>,
    spans: Vec<Span>,
    pending_space: bool,
    pending_newline: bool,
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    fn last_char(&self) -> Option<char> {
        self.spans.last().and_then(|s| s.text.chars().last())
    }

    fn push_str(&mut self, s: &str) {
        match self.spans.last_mut() {
            Some(last) if last.style == self.style && last.href == self.href => {
                last.text.push_str(s)
            }
            _ => self.spans.push(Span {
                text: s.to_owned(),
                style: self.style,
                href: self.href.clone(),
                target: None,
            }),
        }
    }

    /// Appends text, collapsing whitespace runs like HTML does. A line break between two CJK
    /// characters is dropped instead of becoming a space.
    pub fn text(&mut self, s: &str) {
        let mut buf = String::with_capacity(s.len());
        for c in s.chars() {
            if c.is_whitespace() && c != '\u{a0}' {
                if !buf.is_empty() || self.last_char().is_some_and(|l| l != '\n') {
                    self.pending_space = true;
                    self.pending_newline |= c == '\n' || c == '\r';
                }
                continue;
            }
            if self.pending_space {
                let prev = buf.chars().last().or_else(|| self.last_char());
                let cjk_join = self.pending_newline && prev.is_some_and(is_cjk) && is_cjk(c);
                if !cjk_join {
                    buf.push(' ');
                }
                self.pending_space = false;
                self.pending_newline = false;
            }
            buf.push(c);
        }
        if !buf.is_empty() {
            self.push_str(&buf);
        }
    }

    /// Appends text verbatim (preformatted content, plain-text formats).
    pub fn raw_text(&mut self, s: &str) {
        if !s.is_empty() {
            if self.pending_space && !s.starts_with('\n') {
                self.push_str(" ");
            }
            self.push_str(s);
            self.pending_space = false;
            self.pending_newline = false;
        }
    }

    pub fn line_break(&mut self) {
        self.raw_text("\n");
    }

    pub fn has_text(&self) -> bool {
        self.spans.iter().any(|s| !s.text.trim().is_empty())
    }

    /// Takes the accumulated spans (trimmed) without emitting a block.
    pub fn take_spans(&mut self) -> Vec<Span> {
        self.pending_space = false;
        self.pending_newline = false;
        let mut spans = std::mem::take(&mut self.spans);
        if !spans.iter().any(|s| !s.text.trim().is_empty()) {
            return Vec::new();
        }
        if let Some(first) = spans.first_mut() {
            first.text = first.text.trim_start().to_owned();
        }
        if let Some(last) = spans.last_mut() {
            last.text = last.text.trim_end().to_owned();
        }
        spans.retain(|s| !s.text.is_empty());
        spans
    }

    /// Emits accumulated text as a block built by `make`; does nothing if there is no text.
    pub fn flush_as(&mut self, make: impl FnOnce(Vec<Span>) -> Block) {
        let spans = self.take_spans();
        if !spans.is_empty() {
            self.blocks.push(make(spans));
        }
    }

    pub fn flush(&mut self) {
        self.flush_as(Block::Paragraph);
    }

    /// Flushes pending text as a paragraph, then appends `block`.
    pub fn push(&mut self, block: Block) {
        self.flush();
        self.blocks.push(block);
    }

    /// Runs `f` with a modified style, restoring the previous style afterwards.
    pub fn styled<R>(
        &mut self,
        change: impl FnOnce(&mut SpanStyle),
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved = self.style;
        change(&mut self.style);
        let r = f(self);
        self.style = saved;
        r
    }

    pub fn finish(mut self) -> Vec<Block> {
        self.flush();
        self.blocks
    }
}

/// Text of the first heading, used as a fallback chapter title.
pub fn first_heading(blocks: &[Block]) -> Option<String> {
    blocks.iter().find_map(|b| match b {
        Block::Heading { .. } => {
            let t = b.text().split_whitespace().collect::<Vec<_>>().join(" ");
            (!t.is_empty()).then_some(t)
        }
        _ => None,
    })
}

/// Splits blocks into chapters at the smallest heading level that occurs at least twice.
pub fn split_by_headings(blocks: Vec<Block>) -> Vec<Chapter> {
    let level = (1..=3).find(|l| {
        blocks
            .iter()
            .filter(|b| matches!(b, Block::Heading { level, .. } if level == l))
            .count()
            >= 2
    });
    let Some(level) = level else {
        return vec![Chapter {
            title: first_heading(&blocks).unwrap_or_default(),
            blocks,
        }];
    };
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut cur = Chapter::default();
    for block in blocks {
        if let Block::Heading { level: l, .. } = &block
            && *l <= level
        {
            if !cur.blocks.is_empty() {
                chapters.push(std::mem::take(&mut cur));
            }
            cur.title = block.text();
        }
        cur.blocks.push(block);
    }
    chapters.push(cur);
    chapters
}

/// TOC with one entry per heading up to `max_level`, nested by heading level.
pub fn heading_toc(chapters: &[Chapter], max_level: u8) -> Vec<TocEntry> {
    let min_level = chapters
        .iter()
        .flat_map(|c| &c.blocks)
        .filter_map(|b| match b {
            Block::Heading { level, .. } => Some(*level),
            _ => None,
        })
        .min()
        .unwrap_or(1);
    let mut toc = Vec::new();
    for (ci, c) in chapters.iter().enumerate() {
        for (bi, b) in c.blocks.iter().enumerate() {
            if let Block::Heading { level, .. } = b
                && *level <= max_level.max(min_level)
            {
                let title = b.text().split_whitespace().collect::<Vec<_>>().join(" ");
                if !title.is_empty() {
                    toc.push(TocEntry {
                        title,
                        depth: (*level - min_level) as usize,
                        location: Location::Flow {
                            chapter: ci,
                            block: bi,
                        },
                    });
                }
            }
        }
    }
    if toc.is_empty() {
        crate::model::Document::chapter_toc(chapters)
    } else {
        toc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace_and_joins_cjk_lines() {
        let mut b = Builder::new();
        b.text("  hello \n  world ");
        b.flush();
        b.text("中文\n  段落 中 文");
        b.flush();
        assert_eq!(b.blocks[0].text(), "hello world");
        assert_eq!(b.blocks[1].text(), "中文段落 中 文");
    }

    #[test]
    fn merges_same_style_and_splits_styles() {
        let mut b = Builder::new();
        b.text("a ");
        b.styled(|s| s.bold = true, |b| b.text("bold"));
        b.text(" c");
        let spans = b.take_spans();
        assert_eq!(spans.len(), 3);
        assert!(spans[1].style.bold);
        assert_eq!(spans[2].text, " c");
    }
}
