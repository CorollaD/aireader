//! Markdown via pulldown-cmark. Splits chapters at the top-most heading level used.
//! Understands the GitHub and Obsidian extras notes often use: front matter, task lists,
//! tables, alerts, wiki links and math (shown as written).

use std::path::{Path, PathBuf};

use anyhow::Result;
use pulldown_cmark::{BlockQuoteKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};

use super::builder::{Builder, first_heading, heading_toc, split_by_headings};
use crate::model::{Block, Content, Document, ImageRef};

/// Where a book in the library keeps copies of the local images its Markdown shows.
const IMAGES: &str = "images";
/// At most this much image data is copied in with one file.
const MAX_IMAGE_BYTES: u64 = 200 << 20;

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_GFM
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_MATH
}

pub fn load(path: &Path) -> Result<Document> {
    let text = super::txt::decode(&std::fs::read(path)?);
    let base = path.parent().unwrap_or(Path::new("."));
    let blocks = parse(&text, &mut |url| {
        // The image beside the file, or the copy made when it was imported.
        std::fs::read(base.join(super::percent_decode(url)))
            .or_else(|_| std::fs::read(base.join(image_copy(url))))
            .ok()
            .map(ImageRef::new)
    });
    let chapters = split_by_headings(blocks);
    let meta = front_matter(&text);
    let title = meta
        .title
        .or_else(|| chapters.first().and_then(|c| first_heading(&c.blocks)))
        .unwrap_or_else(|| Document::file_stem(path));
    Ok(Document {
        path: path.to_owned(),
        title,
        author: meta.author,
        toc: heading_toc(&chapters, 3),
        cover: None,
        content: Content::Flow(chapters),
    })
}

/// The copy of the image at `url`, relative to the book's folder in the library.
fn image_copy(url: &str) -> PathBuf {
    let digest = Sha256::digest(url.as_bytes());
    let name: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    Path::new(IMAGES).join(name)
}

/// Copies the local images the Markdown file `src` shows into `dir`, where its copy is: the
/// library keeps only the file, so they would be lost otherwise.
pub fn copy_images(src: &Path, dir: &Path) {
    let Ok(bytes) = std::fs::read(src) else {
        return;
    };
    let text = super::txt::decode(&bytes);
    let base = src.parent().unwrap_or(Path::new("."));
    let mut total = 0;
    for event in Parser::new_ext(&text, options()) {
        let Event::Start(Tag::Image { dest_url, .. }) = event else {
            continue;
        };
        if dest_url.contains("://") || dest_url.starts_with("data:") {
            continue;
        }
        let from = base.join(super::percent_decode(&dest_url));
        let to = dir.join(image_copy(&dest_url));
        let Ok(meta) = std::fs::metadata(&from) else {
            continue;
        };
        if !meta.is_file() || total + meta.len() > MAX_IMAGE_BYTES || to.exists() {
            continue;
        }
        if std::fs::create_dir_all(dir.join(IMAGES)).is_ok() && std::fs::copy(&from, &to).is_ok() {
            total += meta.len();
        }
    }
}

#[derive(Default, Debug, PartialEq)]
struct FrontMatter {
    title: Option<String>,
    author: Option<String>,
}

/// The title and author from YAML front matter, if the file starts with some.
fn front_matter(text: &str) -> FrontMatter {
    let mut meta = FrontMatter::default();
    let mut inside = false;
    for event in Parser::new_ext(text, options()) {
        match event {
            Event::Start(Tag::MetadataBlock(_)) => inside = true,
            Event::End(TagEnd::MetadataBlock(_)) => break,
            Event::Text(t) if inside => {
                for line in t.lines() {
                    let Some((key, value)) = line.split_once(':') else {
                        continue;
                    };
                    let value = value
                        .trim()
                        .trim_matches(['[', ']'])
                        .trim()
                        .trim_matches(['"', '\'']);
                    let value = (!value.is_empty()).then(|| value.to_owned());
                    match key.trim().to_lowercase().as_str() {
                        "title" if meta.title.is_none() => meta.title = value,
                        "author" | "authors" if meta.author.is_none() => meta.author = value,
                        _ => {}
                    }
                }
            }
            // Front matter comes first or not at all.
            _ if !inside => break,
            _ => {}
        }
    }
    meta
}

pub fn parse(text: &str, resolve_image: &mut dyn FnMut(&str) -> Option<ImageRef>) -> Vec<Block> {
    let mut b = Builder::new();
    let mut heading: Option<u8> = None;
    let mut quote = 0u32;
    // Next number per open list (`None` = bullet list); `item_marker` is taken by the
    // first paragraph of a list item.
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut item_marker: Option<String> = None;
    let mut code: Option<String> = None;
    let mut image_alt = false;
    let mut metadata = false;
    let mut header_row = false;
    let mut cell = 0usize;

    fn flush(
        b: &mut Builder,
        heading: Option<u8>,
        quote: u32,
        lists: &[Option<u64>],
        marker: &mut Option<String>,
    ) {
        if let Some(level) = heading {
            b.flush_as(|spans| Block::Heading { level, spans });
        } else if let Some(m) = marker.as_mut().filter(|_| b.has_text()) {
            let marker = std::mem::take(m);
            let depth = lists.len().saturating_sub(1) as u8;
            b.flush_as(|spans| Block::ListItem {
                marker,
                depth,
                spans,
            });
        } else if quote > 0 {
            b.flush_as(Block::Quote);
        } else {
            b.flush();
        }
    }

    for ev in Parser::new_ext(text, options()) {
        if metadata {
            metadata = !matches!(ev, Event::End(TagEnd::MetadataBlock(_)));
            continue;
        }
        if let Some(buf) = code.as_mut() {
            match ev {
                Event::Text(t) => buf.push_str(&t),
                Event::End(TagEnd::CodeBlock) => {
                    let text = code.take().unwrap().trim_end_matches('\n').to_owned();
                    b.push(Block::Code(text));
                }
                _ => {}
            }
            continue;
        }
        match ev {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    heading = Some(heading_level(level));
                }
                Tag::MetadataBlock(_) => metadata = true,
                Tag::Paragraph => flush(&mut b, heading, quote, &lists, &mut item_marker),
                // A table reads row by row, its cells set apart, the header row in bold.
                Tag::TableHead | Tag::TableRow => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    header_row = matches!(tag, Tag::TableHead);
                    b.style.bold = header_row;
                    cell = 0;
                }
                Tag::TableCell => {
                    if cell > 0 {
                        b.styled(|s| s.bold = false, |b| b.raw_text(" | "));
                    }
                    cell += 1;
                }
                Tag::BlockQuote(kind) => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    quote += 1;
                    if let Some(kind) = kind {
                        // The label is a line of its own: the alert's paragraph starts anew.
                        b.styled(|s| s.bold = true, |b| b.raw_text(alert_label(kind)));
                    }
                }
                Tag::CodeBlock(_) => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    code = Some(String::new());
                }
                Tag::List(start) => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    lists.push(start);
                }
                Tag::Item => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    item_marker = Some(match lists.last_mut() {
                        Some(Some(n)) => {
                            *n += 1;
                            format!("{}.", *n - 1)
                        }
                        _ => "•".to_owned(),
                    });
                }
                Tag::Emphasis => b.style.italic = true,
                Tag::Strong => b.style.bold = true,
                Tag::Link { .. } => b.style.link = true,
                Tag::Image { dest_url, .. } => {
                    if let Some(img) = resolve_image(&dest_url) {
                        flush(&mut b, heading, quote, &lists, &mut item_marker);
                        b.push(Block::Image(img));
                        image_alt = true;
                    }
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading(_) => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    heading = None;
                }
                TagEnd::Paragraph => flush(&mut b, heading, quote, &lists, &mut item_marker),
                TagEnd::TableHead | TagEnd::TableRow => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    if header_row {
                        b.style.bold = false;
                        header_row = false;
                    }
                }
                TagEnd::BlockQuote(_) => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    quote -= 1;
                }
                TagEnd::Item => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    item_marker = None;
                }
                TagEnd::List(_) => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    lists.pop();
                }
                TagEnd::Emphasis => b.style.italic = false,
                TagEnd::Strong => b.style.bold = false,
                TagEnd::Link => b.style.link = false,
                TagEnd::Image => image_alt = false,
                _ => {}
            },
            // Alt text of an image that was rendered is dropped.
            Event::Text(_) if image_alt => {}
            Event::Text(t) => b.text(&t),
            Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) => {
                b.styled(|s| s.code = true, |b| b.raw_text(&t))
            }
            Event::SoftBreak => b.text("\n"),
            Event::HardBreak => b.line_break(),
            Event::Rule => {
                flush(&mut b, heading, quote, &lists, &mut item_marker);
                b.push(Block::Rule);
            }
            Event::TaskListMarker(done) => b.raw_text(if done { "☑ " } else { "☐ " }),
            _ => {}
        }
    }
    flush(&mut b, heading, quote, &lists, &mut item_marker);
    b.finish()
}

fn heading_level(level: HeadingLevel) -> u8 {
    level as u8
}

fn alert_label(kind: BlockQuoteKind) -> &'static str {
    match kind {
        BlockQuoteKind::Note => "Note",
        BlockQuoteKind::Tip => "Tip",
        BlockQuoteKind::Important => "Important",
        BlockQuoteKind::Warning => "Warning",
        BlockQuoteKind::Caution => "Caution",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_splits() {
        let md = "# Book\n\nintro *it* **b** `c`\n\n## One\n\n- a\n- b\n\n1. x\n2. y\n\n> q\n\n```\nfn x() {}\n```\n\n## Two\n\n---\n\n![alt](img.png)\n";
        let blocks = parse(md, &mut |_| Some(ImageRef::new(vec![1u8])));
        let ch = split_by_headings(blocks);
        let titles: Vec<_> = ch.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["Book", "One", "Two"]);
        assert_eq!(ch[0].blocks[1].text(), "intro it b c");
        assert!(matches!(&ch[1].blocks[1], Block::ListItem { marker, .. } if marker == "•"));
        assert!(matches!(&ch[1].blocks[4], Block::ListItem { marker, .. } if marker == "2."));
        assert!(matches!(&ch[1].blocks[5], Block::Quote(_)));
        assert!(matches!(&ch[1].blocks[6], Block::Code(c) if c == "fn x() {}"));
        assert!(matches!(&ch[2].blocks[1], Block::Rule));
        assert!(matches!(&ch[2].blocks[2], Block::Image(_)));
    }

    #[test]
    fn note_extras_read_cleanly() {
        let md = "---\ntitle: \"读书笔记\"\nauthor: [张三]\n---\n\n# 第一章\n\n- [ ] 待办\n- [x] 完成\n\n| 名称 | 值 |\n|---|---|\n| a | 1 |\n\n> [!WARNING]\n> 小心\n\n见 [[另一篇|别名]]，公式 $x^2$。\n";
        let blocks = parse(md, &mut |_| None);
        let texts: Vec<String> = blocks.iter().map(Block::text).collect();
        assert_eq!(
            texts,
            [
                "第一章",
                "☐ 待办",
                "☑ 完成",
                "名称 | 值",
                "a | 1",
                "Warning",
                "小心",
                "见 别名，公式 x^2。"
            ]
        );
        let Block::Paragraph(header) = &blocks[3] else {
            panic!("{:?}", blocks[3])
        };
        assert!(header.iter().any(|s| s.style.bold && s.text == "名称"));
        let Block::Paragraph(row) = &blocks[4] else {
            panic!()
        };
        assert!(row.iter().all(|s| !s.style.bold));
        assert!(matches!(&blocks[5], Block::Quote(_)) && matches!(&blocks[6], Block::Quote(_)));
        assert_eq!(
            front_matter(md),
            FrontMatter {
                title: Some("读书笔记".into()),
                author: Some("张三".into())
            }
        );
        assert_eq!(
            front_matter("# 标题\n\ntitle: no\n"),
            FrontMatter::default()
        );
    }

    #[test]
    fn images_beside_the_file_come_along_into_the_library() {
        let dir = std::env::temp_dir().join(format!("aireader-md-img-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("notes/attachments")).unwrap();
        std::fs::write(dir.join("notes/attachments/图 1.png"), [1u8, 2, 3]).unwrap();
        let src = dir.join("notes/a.md");
        std::fs::write(
            &src,
            "# A\n\n![one](attachments/%E5%9B%BE%201.png)\n\n![web](https://example.com/x.png)\n",
        )
        .unwrap();
        // The library keeps the file in a folder of its own.
        let book = dir.join("library/book");
        std::fs::create_dir_all(&book).unwrap();
        std::fs::copy(&src, book.join("a.md")).unwrap();
        copy_images(&src, &book);

        let doc = load(&book.join("a.md")).unwrap();
        let Content::Flow(chapters) = &doc.content else {
            panic!()
        };
        let images: Vec<_> = chapters
            .iter()
            .flat_map(|c| &c.blocks)
            .filter_map(|b| match b {
                Block::Image(img) => Some(img.bytes.to_vec()),
                _ => None,
            })
            .collect();
        assert_eq!(images, [vec![1u8, 2, 3]]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
