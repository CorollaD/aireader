//! Plain text: encoding detection (UTF-8/UTF-16/GBK/Big5/…) and chapter heading detection.

use std::path::Path;
use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;

use crate::model::{Block, Chapter, Content, Document, Span};

/// Chunk size (in characters) used when no chapter headings are found.
const CHUNK_CHARS: usize = 20_000;

static CHAPTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(第\s*[0-9０-９零〇一二三四五六七八九十百千万两]+\s*[章节回卷集部篇幕]|(?i:chapter\s+\w+|(part|book)\s+([0-9]+|[ivxlcdm]+)\b)|(序章|序言|楔子|引子|前言|后记|尾声|终章|序)(\s|$|[:：·])|番外)",
    )
    .unwrap()
});

pub fn load(path: &Path) -> Result<Document> {
    let bytes = std::fs::read(path)?;
    let text = decode(&bytes);
    let chapters = split_chapters(&text);
    Ok(Document {
        path: path.to_owned(),
        title: Document::file_stem(path),
        author: None,
        toc: Document::chapter_toc(&chapters),
        cover: None,
        content: Content::Flow(chapters),
    })
}

/// Decodes bytes using BOM, then strict UTF-8, then statistical detection.
pub fn decode(bytes: &[u8]) -> String {
    if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        return enc
            .decode_without_bom_handling(&bytes[bom_len..])
            .0
            .into_owned();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }
    let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    det.feed(bytes, true);
    let enc = det.guess(None, chardetng::Utf8Detection::Allow);
    enc.decode_without_bom_handling(bytes).0.into_owned()
}

fn is_heading(line: &str) -> bool {
    line.chars().count() <= 40 && CHAPTER_RE.is_match(line)
}

/// Chinese prose gets the customary two-character first-line indent.
fn paragraph(line: &str) -> Block {
    let cjk = line
        .chars()
        .take(8)
        .any(|c| matches!(c as u32, 0x3400..=0x9FFF | 0xF900..=0xFAFF));
    let text = if cjk {
        format!("\u{3000}\u{3000}{line}")
    } else {
        line.to_owned()
    };
    Block::Paragraph(vec![Span::plain(text)])
}

pub fn split_chapters(text: &str) -> Vec<Chapter> {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}'))
        .collect();
    let headings = lines.iter().filter(|l| is_heading(l)).count();

    let mut chapters = Vec::new();
    if headings >= 2 {
        let mut cur = Chapter::default();
        for line in lines.iter().filter(|l| !l.is_empty()) {
            if is_heading(line) {
                if !cur.blocks.is_empty() {
                    if cur.title.is_empty() {
                        cur.title = crate::t!("title_start");
                    }
                    chapters.push(std::mem::take(&mut cur));
                }
                cur.title = (*line).to_owned();
                cur.blocks.push(Block::Heading {
                    level: 2,
                    spans: vec![Span::plain(*line)],
                });
            } else {
                cur.blocks.push(paragraph(line));
            }
        }
        if !cur.blocks.is_empty() {
            chapters.push(cur);
        }
    } else {
        let mut cur = Chapter::default();
        let mut chars = 0;
        for line in lines.iter().filter(|l| !l.is_empty()) {
            cur.blocks.push(paragraph(line));
            chars += line.chars().count();
            if chars >= CHUNK_CHARS {
                chapters.push(std::mem::take(&mut cur));
                chars = 0;
            }
        }
        if !cur.blocks.is_empty() || chapters.is_empty() {
            chapters.push(cur);
        }
        if chapters.len() > 1 {
            for (i, c) in chapters.iter_mut().enumerate() {
                c.title = crate::t!("title_part", n = i + 1);
            }
        }
    }
    chapters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_gbk() {
        let (gbk, _, _) = encoding_rs::GBK
            .encode("第一章 开端\n这是一个用来检测编码的中文句子，内容足够长才能被正确识别。");
        assert!(decode(&gbk).starts_with("第一章 开端"));
    }

    #[test]
    fn decodes_utf16_bom() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in "你好".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode(&bytes), "你好");
    }

    #[test]
    fn splits_on_chinese_and_english_headings() {
        let text = "前面的话\n\n第一章 开端\n　　正文一\n第二章 发展\n正文二\nChapter 3 End\ntext";
        let ch = split_chapters(text);
        let titles: Vec<_> = ch.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Start", "第一章 开端", "第二章 发展", "Chapter 3 End"]
        );
        assert_eq!(ch[1].blocks[1].text(), "\u{3000}\u{3000}正文一");
    }

    #[test]
    fn chunks_without_headings() {
        let line = "字".repeat(1000);
        let text = vec![line.as_str(); 45].join("\n");
        let ch = split_chapters(&text);
        assert_eq!(ch.len(), 3);
        assert_eq!(ch[0].title, "Part 1");
    }
}
