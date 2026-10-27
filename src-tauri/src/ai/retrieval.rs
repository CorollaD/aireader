//! Keyword retrieval for questions about a whole book: the book is cut into passages of about
//! [`PASSAGE_CHARS`] characters, each remembering where it starts, and ranked with BM25.
//! Chinese and Japanese text is indexed as overlapping character pairs, other text as words.

use std::collections::HashMap;

use crate::model::{Content, Document, Location};

/// Target passage length in characters.
pub const PASSAGE_CHARS: usize = 800;

const K1: f64 = 1.2;
const B: f64 = 0.75;

#[derive(Clone, Debug, PartialEq)]
pub struct Passage {
    /// Chapter title or page label, shown with citations.
    pub title: String,
    /// Where the passage starts.
    pub location: Location,
    pub text: String,
}

/// Cuts a book into passages in reading order. Paragraphs are kept whole unless a single
/// paragraph is longer than a passage.
pub fn passages(doc: &Document) -> Vec<Passage> {
    let mut out = Vec::new();
    match &doc.content {
        Content::Flow(chapters) => {
            for (ci, ch) in chapters.iter().enumerate() {
                let title = if ch.title.trim().is_empty() {
                    crate::t!("title_chapter", n = ci + 1)
                } else {
                    ch.title.clone()
                };
                let mut text = String::new();
                let mut start = 0;
                for (bi, block) in ch.blocks.iter().enumerate() {
                    let t = block.text();
                    let t = t.trim();
                    if t.is_empty() {
                        continue;
                    }
                    if !text.is_empty() && text.chars().count() + t.chars().count() > PASSAGE_CHARS
                    {
                        out.push(Passage {
                            title: title.clone(),
                            location: Location::Flow {
                                chapter: ci,
                                block: start,
                            },
                            text: std::mem::take(&mut text),
                        });
                    }
                    if text.is_empty() {
                        start = bi;
                    }
                    for piece in split_chars(t, PASSAGE_CHARS) {
                        if !text.is_empty()
                            && text.chars().count() + piece.chars().count() > PASSAGE_CHARS
                        {
                            out.push(Passage {
                                title: title.clone(),
                                location: Location::Flow {
                                    chapter: ci,
                                    block: start,
                                },
                                text: std::mem::take(&mut text),
                            });
                            start = bi;
                        }
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(piece);
                    }
                }
                if !text.trim().is_empty() {
                    out.push(Passage {
                        title,
                        location: Location::Flow {
                            chapter: ci,
                            block: start,
                        },
                        text,
                    });
                }
            }
        }
        Content::Paged(src) => {
            for page in 0..src.page_count() {
                let Some(text) = src.page_text(page) else {
                    continue;
                };
                for piece in split_chars(text.trim(), PASSAGE_CHARS) {
                    out.push(Passage {
                        title: crate::t!("title_page", n = page + 1),
                        location: Location::Page { page },
                        text: piece.to_owned(),
                    });
                }
            }
        }
    }
    out
}

/// Splits `text` into pieces of at most `max` characters, on character boundaries.
fn split_chars(text: &str, max: usize) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let cut = rest
            .char_indices()
            .nth(max)
            .map(|(i, _)| i)
            .unwrap_or(rest.len());
        pieces.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    pieces
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF      // kana
        | 0x3400..=0x4DBF    // CJK extension A
        | 0x4E00..=0x9FFF    // CJK unified ideographs
        | 0xAC00..=0xD7AF    // Hangul syllables
        | 0xF900..=0xFAFF)
}

/// Index terms: lowercase words for alphabetic text, character pairs for CJK runs (a lone
/// CJK character is its own term).
pub fn terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut run: Vec<char> = Vec::new();
    let flush_run = |run: &mut Vec<char>, out: &mut Vec<String>| {
        match run.len() {
            0 => {}
            1 => out.push(run[0].to_string()),
            _ => out.extend(run.windows(2).map(|w| w.iter().collect())),
        }
        run.clear();
    };
    for c in text.chars() {
        if is_cjk(c) {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            run.push(c);
        } else {
            flush_run(&mut run, &mut out);
            if c.is_alphanumeric() {
                word.extend(c.to_lowercase());
            } else if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
        }
    }
    flush_run(&mut run, &mut out);
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// A searchable book.
pub struct BookIndex {
    pub passages: Vec<Passage>,
    freqs: Vec<HashMap<String, u32>>,
    lens: Vec<usize>,
    doc_freq: HashMap<String, u32>,
    avg_len: f64,
}

impl BookIndex {
    pub fn build(doc: &Document) -> Self {
        Self::from_passages(passages(doc))
    }

    pub fn from_passages(passages: Vec<Passage>) -> Self {
        let mut freqs = Vec::with_capacity(passages.len());
        let mut lens = Vec::with_capacity(passages.len());
        let mut doc_freq: HashMap<String, u32> = HashMap::new();
        for p in &passages {
            let ts = terms(&p.text);
            lens.push(ts.len());
            let mut f: HashMap<String, u32> = HashMap::new();
            for t in ts {
                *f.entry(t).or_default() += 1;
            }
            for t in f.keys() {
                *doc_freq.entry(t.clone()).or_default() += 1;
            }
            freqs.push(f);
        }
        let avg_len = lens.iter().sum::<usize>() as f64 / lens.len().max(1) as f64;
        Self {
            passages,
            freqs,
            lens,
            doc_freq,
            avg_len,
        }
    }

    /// Indices of the best passages for `query`, best first (only passages that match).
    pub fn search(&self, query: &str, limit: usize) -> Vec<usize> {
        let mut query_terms = terms(query);
        query_terms.sort();
        query_terms.dedup();
        let n = self.passages.len() as f64;
        let mut scored: Vec<(usize, f64)> = (0..self.passages.len())
            .filter_map(|i| {
                let score: f64 = query_terms
                    .iter()
                    .filter_map(|t| {
                        let tf = *self.freqs[i].get(t)? as f64;
                        let df = *self.doc_freq.get(t)? as f64;
                        let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                        let norm = K1 * (1.0 - B + B * self.lens[i] as f64 / self.avg_len.max(1.0));
                        Some(idf * tf * (K1 + 1.0) / (tf + norm))
                    })
                    .sum();
                (score > 0.0).then_some((i, score))
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.into_iter().take(limit).map(|(i, _)| i).collect()
    }

    /// The passage containing `location` (the last one starting at or before it).
    pub fn passage_at(&self, location: Location) -> Option<usize> {
        let key = |l: Location| match l {
            Location::Flow { chapter, block } => (chapter, block),
            Location::Page { page } => (page, 0),
        };
        let target = key(location);
        let i = self.passages.partition_point(|p| key(p.location) <= target);
        (!self.passages.is_empty()).then(|| i.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Chapter, Span};
    use std::path::PathBuf;

    fn para(text: &str) -> Block {
        Block::Paragraph(vec![Span::plain(text)])
    }

    fn book(chapters: Vec<(&str, Vec<&str>)>) -> Document {
        Document {
            path: PathBuf::new(),
            title: "t".into(),
            author: None,
            toc: Vec::new(),
            cover: None,
            content: Content::Flow(
                chapters
                    .into_iter()
                    .map(|(title, paras)| Chapter {
                        title: title.into(),
                        blocks: paras.into_iter().map(para).collect(),
                    })
                    .collect(),
            ),
        }
    }

    #[test]
    fn terms_mix_words_and_cjk_pairs() {
        assert_eq!(
            terms("Hello, World! 刘备与关羽"),
            ["hello", "world", "刘备", "备与", "与关", "关羽"]
        );
        assert_eq!(terms("曹 操"), ["曹", "操"]);
    }

    #[test]
    fn passages_keep_paragraphs_and_locations() {
        let long = "字".repeat(PASSAGE_CHARS * 2 + 10);
        let doc = book(vec![
            ("一", vec!["甲", "乙"]),
            ("二", vec![long.as_str(), "尾"]),
        ]);
        let ps = passages(&doc);
        assert_eq!(ps[0].text, "甲\n乙");
        assert_eq!(
            ps[0].location,
            Location::Flow {
                chapter: 0,
                block: 0
            }
        );
        assert!(ps.iter().all(|p| p.text.chars().count() <= PASSAGE_CHARS));
        // The long paragraph is cut without breaking characters; the tail joins the last piece.
        assert_eq!(ps.len(), 4);
        assert_eq!(ps[3].text.chars().last(), Some('尾'));
        assert!(ps[1..].iter().all(|p| p.title == "二"));
    }

    #[test]
    fn search_ranks_matching_passages() {
        let doc = book(vec![
            ("一", vec!["刘备在桃园与关羽、张飞结义。"]),
            ("二", vec!["曹操煮酒论英雄。"]),
            ("三", vec!["The White Rabbit ran past Alice."]),
        ]);
        let index = BookIndex::build(&doc);
        assert_eq!(index.search("桃园结义是怎么回事", 5), [0]);
        assert_eq!(index.search("曹操", 5), [1]);
        assert_eq!(index.search("rabbit", 5), [2]);
        assert!(index.search("无关的词", 5).is_empty());
        assert_eq!(
            index.passage_at(Location::Flow {
                chapter: 1,
                block: 3
            }),
            Some(1)
        );
        assert_eq!(
            index.passage_at(Location::Flow {
                chapter: 0,
                block: 0
            }),
            Some(0)
        );
    }
}
