//! Markdown via pulldown-cmark. Splits chapters at the top-most heading level used.

use std::path::Path;

use anyhow::Result;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use super::builder::{Builder, first_heading, heading_toc, split_by_headings};
use crate::model::{Block, Content, Document, ImageRef};

pub fn load(path: &Path) -> Result<Document> {
    let text = super::txt::decode(&std::fs::read(path)?);
    let base = path.parent().unwrap_or(Path::new("."));
    let blocks = parse(&text, &mut |url| {
        let p = base.join(super::percent_decode(url));
        std::fs::read(p).ok().map(ImageRef::new)
    });
    let chapters = split_by_headings(blocks);
    let title = chapters
        .first()
        .and_then(|c| first_heading(&c.blocks))
        .unwrap_or_else(|| Document::file_stem(path));
    Ok(Document {
        path: path.to_owned(),
        title,
        author: None,
        toc: heading_toc(&chapters, 3),
        content: Content::Flow(chapters),
    })
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

    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_FOOTNOTES;
    for ev in Parser::new_ext(text, opts) {
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
                Tag::Paragraph | Tag::TableRow | Tag::TableHead => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker)
                }
                Tag::BlockQuote(_) => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker);
                    quote += 1;
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
                TagEnd::Paragraph | TagEnd::TableRow | TagEnd::TableHead => {
                    flush(&mut b, heading, quote, &lists, &mut item_marker)
                }
                TagEnd::TableCell => b.raw_text("    "),
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
            Event::Code(t) => b.styled(|s| s.code = true, |b| b.raw_text(&t)),
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
}
