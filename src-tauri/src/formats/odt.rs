//! OpenDocument text (`.odt`): headings, paragraphs, bold and italic, lists, tables as rows, and
//! images.

use std::collections::HashMap;
use std::io::{Read, Seek};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use roxmltree::Node;

use super::builder::{Builder, heading_toc, split_by_headings};
use crate::model::{Block, Content, Document, ImageRef};

const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
const FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
const XLINK: &str = "http://www.w3.org/1999/xlink";

#[derive(Clone, Copy, Default)]
struct Look {
    bold: bool,
    italic: bool,
}

fn read_entry<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    let mut entry = zip.by_name(name).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// Style name → look, from the styles defined inside the document.
fn looks(doc: &roxmltree::Document) -> HashMap<String, Look> {
    doc.descendants()
        .filter(|n| n.has_tag_name((STYLE, "style")))
        .filter_map(|style| {
            let name = style.attribute((STYLE, "name"))?;
            let props = style
                .children()
                .find(|n| n.has_tag_name((STYLE, "text-properties")))?;
            let look = Look {
                bold: props.attribute((FO, "font-weight")) == Some("bold"),
                italic: props.attribute((FO, "font-style")) == Some("italic"),
            };
            Some((name.to_owned(), look))
        })
        .collect()
}

struct Converter<'z, R: Read + Seek> {
    b: Builder,
    zip: &'z mut zip::ZipArchive<R>,
    looks: HashMap<String, Look>,
}

impl<R: Read + Seek> Converter<'_, R> {
    fn inline(&mut self, node: Node) {
        for child in node.children() {
            if child.is_text() {
                self.b.text(child.text().unwrap_or(""));
                continue;
            }
            match child.tag_name().name() {
                "span" | "a" | "ruby" | "ruby-base" => {
                    let look = child
                        .attribute((TEXT, "style-name"))
                        .and_then(|name| self.looks.get(name))
                        .copied()
                        .unwrap_or_default();
                    let mut body = std::mem::replace(&mut self.b, Builder::new());
                    let saved = body.style;
                    body.style.bold |= look.bold;
                    body.style.italic |= look.italic;
                    self.b = body;
                    self.inline(child);
                    self.b.style = saved;
                }
                "s" => {
                    let count = child
                        .attribute((TEXT, "c"))
                        .and_then(|c| c.parse().ok())
                        .unwrap_or(1usize);
                    self.b.raw_text(&" ".repeat(count.min(32)));
                }
                "tab" => self.b.raw_text("\t"),
                "line-break" => self.b.line_break(),
                "frame" => self.frame(child),
                "note" | "annotation" | "bookmark" | "bookmark-start" | "bookmark-end" => {}
                _ => self.inline(child),
            }
        }
    }

    /// A picture anchored inside a paragraph.
    fn frame(&mut self, frame: Node) {
        for image in frame.children().filter(|n| n.tag_name().name() == "image") {
            let bytes = image
                .attribute((XLINK, "href"))
                .and_then(|href| read_entry(self.zip, href.trim_start_matches("./")));
            if let Some(bytes) = bytes {
                self.b.push(Block::Image(ImageRef::new(bytes)));
            }
        }
    }

    fn list(&mut self, list: Node, depth: u8) {
        for item in list
            .children()
            .filter(|n| n.tag_name().name() == "list-item")
        {
            for child in item.children().filter(Node::is_element) {
                match child.tag_name().name() {
                    "list" => self.list(child, depth + 1),
                    _ => {
                        self.inline(child);
                        self.b.flush_as(|spans| Block::ListItem {
                            marker: "•".to_owned(),
                            depth,
                            spans,
                        });
                    }
                }
            }
        }
    }

    fn body(&mut self, node: Node) {
        for child in node.children().filter(Node::is_element) {
            match child.tag_name().name() {
                "h" => {
                    let level = child
                        .attribute((TEXT, "outline-level"))
                        .and_then(|l| l.parse().ok())
                        .unwrap_or(1u8)
                        .clamp(1, 6);
                    self.inline(child);
                    self.b.flush_as(|spans| Block::Heading { level, spans });
                }
                "p" => {
                    self.inline(child);
                    self.b.flush();
                }
                "list" => self.list(child, 0),
                "table-row" => {
                    let cells: Vec<String> = child
                        .children()
                        .filter(|n| n.tag_name().name() == "table-cell")
                        .map(|cell| {
                            let parts: Vec<&str> = cell
                                .descendants()
                                .filter(|n| n.is_text())
                                .filter_map(|n| n.text())
                                .collect();
                            parts.join(" ").trim().to_owned()
                        })
                        .collect();
                    self.b.text(&cells.join(" | "));
                    self.b.flush();
                }
                "tracked-changes" | "forms" | "sequence-decls" => {}
                _ => self.body(child),
            }
        }
    }
}

fn parse<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Result<(Option<String>, Option<String>, Vec<Block>)> {
    let content = read_entry(zip, "content.xml").context("missing content.xml")?;
    let content = String::from_utf8_lossy(&content);
    let doc = roxmltree::Document::parse(&content)
        .map_err(|e| anyhow!(crate::t!("parse_failed", format = "ODT", error = e)))?;
    let body = doc
        .descendants()
        .find(|n| {
            n.tag_name().name() == "text"
                && n.parent().is_some_and(|p| p.tag_name().name() == "body")
        })
        .context("missing body")?;
    let (title, author) = read_entry(zip, "meta.xml")
        .and_then(|xml| {
            let xml = String::from_utf8_lossy(&xml).into_owned();
            let meta = roxmltree::Document::parse(&xml).ok()?;
            let get = |name: &str| {
                meta.descendants()
                    .find(|n| n.tag_name().name() == name)
                    .and_then(|n| n.text())
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned)
            };
            Some((get("title"), get("creator")))
        })
        .unwrap_or_default();
    let mut converter = Converter {
        b: Builder::new(),
        looks: looks(&doc),
        zip,
    };
    converter.body(body);
    Ok((title, author, converter.b.finish()))
}

pub fn load(path: &Path) -> Result<Document> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let (title, author, blocks) = parse(&mut zip)?;
    let chapters = split_by_headings(blocks);
    if chapters.iter().all(|c| c.blocks.is_empty()) {
        bail!(crate::t!("document_empty"));
    }
    Ok(Document {
        path: path.to_owned(),
        title: title.unwrap_or_else(|| Document::file_stem(path)),
        author,
        toc: heading_toc(&chapters, 3),
        cover: None,
        content: Content::Flow(chapters),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const CONTENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
  xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
  xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
  xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
  xmlns:xlink="http://www.w3.org/1999/xlink"
  xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
  <office:automatic-styles>
    <style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>
  </office:automatic-styles>
  <office:body><office:text>
    <text:sequence-decls><text:sequence-decl text:name="x"/></text:sequence-decls>
    <text:h text:outline-level="1">第一章</text:h>
    <text:p>普通<text:span text:style-name="T1">加粗</text:span>文字<text:s text:c="2"/>结尾</text:p>
    <text:list><text:list-item><text:p>列表一</text:p><text:list><text:list-item><text:p>子项</text:p></text:list-item></text:list></text:list-item></text:list>
    <table:table><table:table-row><table:table-cell><text:p>甲</text:p></table:table-cell><table:table-cell><text:p>乙</text:p></table:table-cell></table:table-row></table:table>
    <text:p><draw:frame><draw:image xlink:href="Pictures/a.png"/></draw:frame></text:p>
  </office:text></office:body>
</office:document-content>"#;

    const META: &str = r#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
      xmlns:dc="http://purl.org/dc/elements/1.1/"><office:meta><dc:title>文档标题</dc:title><dc:creator>作者</dc:creator></office:meta></office:document-meta>"#;

    fn odt() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("aireader-odt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.odt");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in [
            (
                "mimetype",
                b"application/vnd.oasis.opendocument.text".as_slice(),
            ),
            ("content.xml", CONTENT.as_bytes()),
            ("meta.xml", META.as_bytes()),
            ("Pictures/a.png", b"picture"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn reads_a_document() {
        let doc = load(&odt()).unwrap();
        assert_eq!(doc.title, "文档标题");
        assert_eq!(doc.author.as_deref(), Some("作者"));
        assert_eq!(doc.toc[0].title, "第一章");
        let Content::Flow(chapters) = &doc.content else {
            panic!("odt is text")
        };
        let blocks: Vec<&Block> = chapters.iter().flat_map(|c| &c.blocks).collect();
        let texts: Vec<String> = blocks.iter().map(|b| b.text()).collect();
        assert!(
            texts.iter().any(|t| t.contains("普通加粗文字")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|t| t == "甲 | 乙"), "{texts:?}");
        let bold = blocks.iter().any(|b| match b {
            Block::Paragraph(spans) => spans.iter().any(|s| s.style.bold && s.text == "加粗"),
            _ => false,
        });
        assert!(bold, "{blocks:?}");
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, Block::ListItem { depth: 1, .. }))
        );
        assert!(blocks.iter().any(|b| matches!(b, Block::Image(_))));
    }
}
