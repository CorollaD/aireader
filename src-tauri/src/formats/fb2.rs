//! FictionBook 2 (`.fb2`, `.fb2.zip`).

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::Engine;
use roxmltree::{Node, ParsingOptions};

use super::builder::Builder;
use crate::model::{Block, Chapter, Content, Document, ImageRef, Location, TocEntry};

pub fn load(path: &Path) -> Result<Document> {
    parse(path, &std::fs::read(path)?)
}

pub fn load_zip(path: &Path) -> Result<Document> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let name = zip
        .file_names()
        .filter_map(Result::ok)
        .find(|n| n.to_lowercase().ends_with(".fb2"))
        .map(|n| n.into_owned())
        .with_context(|| crate::t!("archive_no_fb2"))?;
    let mut bytes = Vec::new();
    zip.by_name(&name)?.read_to_end(&mut bytes)?;
    parse(path, &bytes)
}

/// Decodes using the encoding named in the XML declaration (FB2 is often windows-1251).
fn decode_xml(bytes: &[u8]) -> String {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]);
    let label = head
        .split_once("encoding=")
        .and_then(|(_, rest)| rest.get(1..))
        .and_then(|rest| rest.split(['"', '\'']).next());
    match label.and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes())) {
        Some(enc) if enc != encoding_rs::UTF_8 => enc.decode(bytes).0.into_owned(),
        _ => super::txt::decode(bytes),
    }
}

pub fn parse(path: &Path, bytes: &[u8]) -> Result<Document> {
    let text = decode_xml(bytes);
    // roxmltree only accepts UTF-8 input; drop the declaration that names another encoding.
    let xml = match text.find("?>") {
        Some(i) if text.trim_start().starts_with("<?xml") => &text[i + 2..],
        _ => &text[..],
    };
    let opts = ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(xml, opts)
        .map_err(|e| anyhow::anyhow!(crate::t!("parse_failed", format = "FB2", error = e)))?;
    let root = doc.root_element();

    let binaries: HashMap<&str, ImageRef> = children(root, "binary")
        .filter_map(|b| {
            let data: String = b.text()?.split_whitespace().collect();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .ok()?;
            Some((b.attribute("id")?, ImageRef::new(bytes)))
        })
        .collect();

    let title_info = child(root, "description").and_then(|d| child(d, "title-info"));
    let title = title_info
        .and_then(|t| child(t, "book-title"))
        .and_then(|t| t.text())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| Document::file_stem(path));
    let author = title_info.and_then(|t| child(t, "author")).map(|a| {
        ["first-name", "middle-name", "last-name"]
            .iter()
            .filter_map(|k| child(a, k).and_then(|n| n.text()))
            .collect::<Vec<_>>()
            .join(" ")
    });

    let mut chapters = Vec::new();
    let mut toc = Vec::new();
    // The first body is the main text; others (usually "notes") become extra chapters.
    for body in children(root, "body") {
        let sections: Vec<_> = children(body, "section").collect();
        if sections.is_empty() {
            let mut cv = Converter::new(&binaries);
            cv.container(body, 0, &mut toc, chapters.len());
            push_chapter(&mut chapters, cv, body.attribute("name").unwrap_or(&title));
            continue;
        }
        // Content before the first section (title, epigraph) goes into its own chapter.
        let mut intro = Converter::new(&binaries);
        for n in body
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() != "section")
        {
            intro.element(n, 0, &mut Vec::new(), 0);
        }
        if intro.b.has_text() || !intro.b.blocks.is_empty() {
            push_chapter(
                &mut chapters,
                intro,
                body.attribute("name").unwrap_or(&title),
            );
        }
        for section in sections {
            let mut cv = Converter::new(&binaries);
            let idx = chapters.len();
            let first_toc = toc.len();
            cv.section(section, 0, &mut toc, idx);
            let name = toc
                .get(first_toc)
                .map(|e: &TocEntry| e.title.clone())
                .unwrap_or_default();
            push_chapter(&mut chapters, cv, &name);
        }
    }
    if chapters.is_empty() {
        bail!(crate::t!("no_content", format = "FB2"));
    }
    if toc.is_empty() {
        toc = Document::chapter_toc(&chapters);
    }
    Ok(Document {
        path: path.to_owned(),
        title,
        author,
        toc,
        content: Content::Flow(chapters),
    })
}

fn push_chapter(chapters: &mut Vec<Chapter>, cv: Converter, title: &str) {
    let blocks = cv.b.finish();
    if !blocks.is_empty() {
        chapters.push(Chapter {
            title: title.to_owned(),
            blocks,
        });
    }
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.tag_name().name() == name)
}

fn children<'a, 'i: 'a>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    n.children().filter(move |c| c.tag_name().name() == name)
}

struct Converter<'b> {
    b: Builder,
    binaries: &'b HashMap<&'b str, ImageRef>,
    quote: u32,
}

impl<'b> Converter<'b> {
    fn new(binaries: &'b HashMap<&'b str, ImageRef>) -> Self {
        Self {
            b: Builder::new(),
            binaries,
            quote: 0,
        }
    }

    fn flush(&mut self) {
        if self.quote > 0 {
            self.b.flush_as(Block::Quote)
        } else {
            self.b.flush()
        }
    }

    /// A `<section>`: its title becomes a heading and a TOC entry.
    fn section(&mut self, n: Node, depth: usize, toc: &mut Vec<TocEntry>, chapter: usize) {
        if let Some(t) = child(n, "title") {
            let text = t
                .children()
                .filter(|c| c.is_element())
                .map(|p| {
                    p.descendants()
                        .filter(|d| d.is_text())
                        .filter_map(|d| d.text())
                        .collect::<String>()
                })
                .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if !text.is_empty() {
                toc.push(TocEntry {
                    title: text,
                    depth,
                    location: Location::Flow {
                        chapter,
                        block: self.b.blocks.len(),
                    },
                });
            }
        }
        self.container(n, depth, toc, chapter);
    }

    fn container(&mut self, n: Node, depth: usize, toc: &mut Vec<TocEntry>, chapter: usize) {
        for c in n.children().filter(|c| c.is_element()) {
            self.element(c, depth, toc, chapter);
        }
    }

    fn element(&mut self, n: Node, depth: usize, toc: &mut Vec<TocEntry>, chapter: usize) {
        match n.tag_name().name() {
            "section" => {
                self.flush();
                self.section(n, depth + 1, toc, chapter);
            }
            "title" => {
                let level = (depth + 1).min(6) as u8;
                for p in n.children().filter(|c| c.is_element()) {
                    self.inline(p);
                    self.b.flush_as(|spans| Block::Heading { level, spans });
                }
            }
            "subtitle" => {
                self.inline(n);
                let level = (depth + 2).min(6) as u8;
                self.b.flush_as(|spans| Block::Heading { level, spans });
            }
            "p" | "v" | "text-author" => {
                self.inline(n);
                self.flush();
            }
            "empty-line" => {}
            "epigraph" | "cite" | "annotation" => {
                self.flush();
                self.quote += 1;
                self.container(n, depth, toc, chapter);
                self.quote -= 1;
            }
            "poem" | "stanza" => {
                self.container(n, depth, toc, chapter);
            }
            "image" => self.image(n),
            "table" => {
                for row in n.children().filter(|c| c.tag_name().name() == "tr") {
                    for cell in row.children().filter(|c| c.is_element()) {
                        self.inline(cell);
                        self.b.raw_text("    ");
                    }
                    self.flush();
                }
            }
            _ => self.container(n, depth, toc, chapter),
        }
    }

    fn image(&mut self, n: Node) {
        let href = n
            .attributes()
            .find(|a| a.name() == "href")
            .map(|a| a.value());
        if let Some(img) = href.and_then(|h| self.binaries.get(h.trim_start_matches('#'))) {
            self.flush();
            self.b.push(Block::Image(img.clone()));
        }
    }

    fn inline(&mut self, n: Node) {
        for c in n.children() {
            if c.is_text() {
                self.b.text(c.text().unwrap_or_default());
                continue;
            }
            let saved = self.b.style;
            match c.tag_name().name() {
                "strong" => self.b.style.bold = true,
                "emphasis" => self.b.style.italic = true,
                "code" => self.b.style.code = true,
                "a" => self.b.style.link = true,
                "image" => {
                    self.image(c);
                    continue;
                }
                _ => {}
            }
            self.inline(c);
            self.b.style = saved;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description><title-info><author><first-name>Lev</first-name><last-name>Tolstoy</last-name></author>
<book-title>War</book-title></title-info></description>
<body>
<title><p>War</p></title>
<section><title><p>Part One</p></title>
  <p>Hello <emphasis>it</emphasis> <strong>b</strong></p>
  <section><title><p>Chapter 1</p></title><p>text</p><image l:href="#pic"/></section>
</section>
<section><title><p>Part Two</p></title><epigraph><p>quote</p></epigraph><poem><stanza><v>line</v></stanza></poem></section>
</body>
<body name="notes"><section><title><p>1</p></title><p>note</p></section></body>
<binary id="pic" content-type="image/png">iVBORw0KGgo=</binary>
</FictionBook>"##;

    #[test]
    fn parses_sections_and_images() {
        let doc = parse(Path::new("x.fb2"), SAMPLE.as_bytes()).unwrap();
        assert_eq!(doc.title, "War");
        assert_eq!(doc.author.as_deref(), Some("Lev Tolstoy"));
        let Content::Flow(ch) = &doc.content else {
            panic!()
        };
        let titles: Vec<_> = ch.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["War", "Part One", "Part Two", "1"]);
        assert_eq!(ch[1].blocks[1].text(), "Hello it b");
        assert!(ch[1].blocks.iter().any(|b| matches!(b, Block::Image(_))));
        assert!(matches!(ch[2].blocks[1], Block::Quote(_)));
        let toc: Vec<_> = doc
            .toc
            .iter()
            .map(|e| (e.title.as_str(), e.depth))
            .collect();
        assert_eq!(
            toc,
            [("Part One", 0), ("Chapter 1", 1), ("Part Two", 0), ("1", 0)]
        );
        assert_eq!(
            doc.toc[1].location,
            Location::Flow {
                chapter: 1,
                block: 2
            }
        );
    }

    #[test]
    fn decodes_cp1251() {
        let (body, _, _) = encoding_rs::WINDOWS_1251.encode("<?xml version=\"1.0\" encoding=\"windows-1251\"?><FictionBook><body><section><p>Привет</p></section></body></FictionBook>");
        let doc = parse(Path::new("x.fb2"), &body).unwrap();
        let Content::Flow(ch) = &doc.content else {
            panic!()
        };
        assert_eq!(ch[0].blocks[0].text(), "Привет");
    }
}
