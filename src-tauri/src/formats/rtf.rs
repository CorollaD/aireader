//! Rich Text Format: paragraphs with bold and italic. Layout and pictures are left out.

use std::path::Path;

use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow, bail};
use regex::{Captures, Regex};
use rtf_parser::{Lexer, Parser, StyleBlock};

use super::builder::{Builder, heading_toc, split_by_headings};
use super::txt;
use crate::model::{Block, Content, Document};

/// Stands for a paragraph end in the text handed to the parser.
const BREAK: char = '\u{2029}';

static CODE_PAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\ansicpg(\d+)").unwrap());
static BYTES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:\\'[0-9a-fA-F]{2})+").unwrap());
static UNICODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\u(-?\d{1,5}) ?(?:\?|\\'[0-9a-fA-F]{2})?").unwrap());
static PARAGRAPH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\(?:par|line)\b ?").unwrap());

fn encoding_of(text: &str) -> &'static encoding_rs::Encoding {
    let page = CODE_PAGE
        .captures(text)
        .and_then(|c| c[1].parse::<u16>().ok());
    match page {
        Some(936) => encoding_rs::GBK,
        Some(950) => encoding_rs::BIG5,
        Some(932) => encoding_rs::SHIFT_JIS,
        Some(949) => encoding_rs::EUC_KR,
        Some(1251) => encoding_rs::WINDOWS_1251,
        Some(1250) => encoding_rs::WINDOWS_1250,
        _ => encoding_rs::WINDOWS_1252,
    }
}

/// Rewrites what the parser reads wrongly, as plain characters: it takes only four digits of
/// `\\u`, so most Chinese comes out garbled, leaves `\\'xx` bytes undecoded for double-byte code
/// pages, and doesn't separate paragraphs.
fn normalise(text: &str) -> String {
    let encoding = encoding_of(text);
    let unicode = UNICODE.replace_all(text, |c: &Captures| {
        let code = c[1].parse::<i32>().unwrap_or(0);
        let code = if code < 0 { code + 0x10000 } else { code };
        char::from_u32(code as u32)
            .unwrap_or('\u{fffd}')
            .to_string()
    });
    let bytes = BYTES.replace_all(&unicode, |c: &Captures| {
        let raw: Vec<u8> = c[0]
            .split("\\'")
            .filter(|h| !h.is_empty())
            .filter_map(|h| u8::from_str_radix(h, 16).ok())
            .collect();
        encoding.decode(&raw).0.into_owned()
    });
    PARAGRAPH
        .replace_all(&bytes, BREAK.to_string())
        .into_owned()
}

fn parse(text: &str) -> Result<Vec<StyleBlock>> {
    let text = normalise(text);
    let tokens = Lexer::scan(&text).map_err(|e| anyhow!("{e}"))?;
    Ok(Parser::new(tokens)
        .parse()
        .map_err(|e| anyhow!("{e}"))?
        .body)
}

fn to_blocks(body: &[StyleBlock]) -> Vec<Block> {
    let mut b = Builder::new();
    for block in body {
        let (bold, italic) = (block.painter.bold, block.painter.italic);
        b.styled(
            |s| {
                s.bold = bold;
                s.italic = italic;
            },
            |b| {
                let mut lines = block.text.split(['\n', BREAK]);
                if let Some(first) = lines.next() {
                    b.text(first);
                }
                for line in lines {
                    b.flush();
                    b.text(line);
                }
            },
        );
    }
    b.finish()
}

pub fn load(path: &Path) -> Result<Document> {
    let text = txt::decode(&std::fs::read(path).context(crate::t!("read_failed"))?);
    // The parser's handling of malformed files isn't all errors.
    let body = std::panic::catch_unwind(|| parse(&text))
        .map_err(|_| anyhow!("malformed file"))
        .and_then(|parsed| parsed)
        .map_err(|e| anyhow!(crate::t!("parse_failed", format = "RTF", error = e)))?;
    let chapters = split_by_headings(to_blocks(&body));
    if chapters.iter().all(|c| c.blocks.is_empty()) {
        bail!(crate::t!("document_empty"));
    }
    Ok(Document {
        path: path.to_owned(),
        title: Document::file_stem(path),
        author: None,
        toc: heading_toc(&chapters, 3),
        cover: None,
        content: Content::Flow(chapters),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_text(name: &str, text: &str) -> Result<Document> {
        let dir = std::env::temp_dir().join(format!("aireader-rtf-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.rtf");
        std::fs::write(&file, text).unwrap();
        load(&file)
    }

    fn paragraphs(doc: &Document) -> Vec<String> {
        let Content::Flow(chapters) = &doc.content else {
            panic!("rtf is text")
        };
        chapters
            .iter()
            .flat_map(|c| &c.blocks)
            .map(Block::text)
            .collect()
    }

    #[test]
    fn keeps_paragraphs_and_styles() {
        let doc = load_text(
            "styles",
            r"{\rtf1\ansi\deff0 {\fonttbl {\f0 Times;}}\f0 First \b bold\b0  part.\par Second paragraph.\par}",
        )
        .unwrap();
        let text = paragraphs(&doc);
        assert_eq!(text.len(), 2, "{text:?}");
        assert!(text[0].contains("First bold part."), "{text:?}");
        let Content::Flow(chapters) = &doc.content else {
            unreachable!()
        };
        let Block::Paragraph(spans) = &chapters[0].blocks[0] else {
            panic!("a paragraph")
        };
        assert!(
            spans
                .iter()
                .any(|s| s.style.bold && s.text.contains("bold"))
        );
    }

    #[test]
    fn reads_unicode_escapes() {
        let escape = |code: u32| format!("{}u{code}?", char::from(92));
        let text = format!(
            "{{{b}rtf1{b}ansi{b}uc1 {a}{c} text{b}par}}",
            b = char::from(92),
            a = escape(20013),
            c = escape(25991),
        );
        let doc = load_text("unicode", &text).unwrap();
        assert!(
            paragraphs(&doc)[0].contains("中文"),
            "{:?}",
            paragraphs(&doc)
        );
    }

    #[test]
    fn reads_double_byte_code_pages() {
        // "中文" in GBK.
        let doc = load_text("gbk", r"{\rtf1\ansi\ansicpg936 \'d6\'d0\'ce\'c4 text\par}").unwrap();
        assert!(
            paragraphs(&doc)[0].contains("中文"),
            "{:?}",
            paragraphs(&doc)
        );
    }

    #[test]
    fn negative_unicode_values_wrap_around() {
        let doc = load_text("negative", r"{\rtf1\ansi\uc1 \u-1?text\par}").unwrap();
        assert!(
            paragraphs(&doc)[0].contains('\u{ffff}'),
            "{:?}",
            paragraphs(&doc)
        );
    }

    #[test]
    fn nonsense_is_an_error_not_a_crash() {
        assert!(load_text("bad", "{\\rtf1").is_err() || load_text("bad", "{").is_err());
    }
}
