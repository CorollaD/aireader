//! Word `.docx`: paragraphs, heading styles, bold/italic runs, lists, tables (as rows) and images.

use std::collections::HashMap;
use std::io::{Read, Seek};
use std::path::Path;

use anyhow::{Context, Result, bail};
use roxmltree::Node;

use super::builder::{Builder, heading_toc, split_by_headings};
use super::resolve_path;
use crate::model::{Block, Content, Document, ImageRef};

pub fn load(path: &Path) -> Result<Document> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let doc = parse(&mut zip)?;
    let title = doc.title.unwrap_or_else(|| Document::file_stem(path));
    let chapters = split_by_headings(doc.blocks);
    if chapters.iter().all(|c| c.blocks.is_empty()) {
        bail!(crate::t!("document_empty"));
    }
    Ok(Document {
        path: path.to_owned(),
        title,
        author: doc.author,
        toc: heading_toc(&chapters, 3),
        content: Content::Flow(chapters),
    })
}

pub struct Parsed {
    pub title: Option<String>,
    pub author: Option<String>,
    pub blocks: Vec<Block>,
}

fn read_entry<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn read_xml<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    read_entry(zip, name).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Attribute by local name, ignoring the namespace prefix.
fn attr<'a>(n: Node<'a, '_>, local: &str) -> Option<&'a str> {
    n.attributes()
        .find(|a| a.name() == local)
        .map(|a| a.value())
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.tag_name().name() == name)
}

/// `<w:b/>` or `<w:b w:val="1"/>` is on; `w:val="0"`/`"false"` is off.
fn toggle(props: Option<Node>, name: &str) -> bool {
    props
        .and_then(|p| child(p, name))
        .is_some_and(|n| !matches!(attr(n, "val"), Some("0" | "false" | "none")))
}

pub fn parse<R: Read + Seek>(zip: &mut zip::ZipArchive<R>) -> Result<Parsed> {
    let document = read_xml(zip, "word/document.xml").context("missing word/document.xml")?;
    let doc = roxmltree::Document::parse(&document)
        .map_err(|e| anyhow::anyhow!(crate::t!("parse_failed", format = "DOCX", error = e)))?;

    // Relationship id → archive path of the target.
    let mut rels = HashMap::new();
    if let Some(xml) = read_xml(zip, "word/_rels/document.xml.rels")
        && let Ok(r) = roxmltree::Document::parse(&xml)
    {
        for n in r
            .descendants()
            .filter(|n| n.tag_name().name() == "Relationship")
        {
            if let (Some(id), Some(target)) = (attr(n, "Id"), attr(n, "Target")) {
                rels.insert(id.to_owned(), resolve_path("word", target));
            }
        }
    }

    // Style id → heading level (or 0 for quote styles).
    let mut styles: HashMap<String, StyleKind> = HashMap::new();
    if let Some(xml) = read_xml(zip, "word/styles.xml")
        && let Ok(s) = roxmltree::Document::parse(&xml)
    {
        for st in s.descendants().filter(|n| n.tag_name().name() == "style") {
            let Some(id) = attr(st, "styleId") else {
                continue;
            };
            let name = child(st, "name")
                .and_then(|n| attr(n, "val"))
                .unwrap_or(id)
                .to_lowercase();
            let outline = child(st, "pPr")
                .and_then(|p| child(p, "outlineLvl"))
                .and_then(|o| attr(o, "val"))
                .and_then(|v| v.parse::<u8>().ok());
            if let Some(kind) = style_kind(&name, outline) {
                styles.insert(id.to_owned(), kind);
            }
        }
    }

    let (title, author) = read_xml(zip, "docProps/core.xml")
        .and_then(|xml| {
            let d = roxmltree::Document::parse(&xml).ok()?;
            let get = |name: &str| {
                d.descendants()
                    .find(|n| n.tag_name().name() == name)
                    .and_then(|n| n.text())
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned)
            };
            Some((get("title"), get("creator")))
        })
        .unwrap_or_default();

    let body = doc.descendants().find(|n| n.tag_name().name() == "body");
    let body_size = body.map(dominant_size).unwrap_or(DEFAULT_SIZE);
    let mut cv = Converter {
        b: Builder::new(),
        zip,
        rels: &rels,
        styles: &styles,
        images: HashMap::new(),
        body_size,
    };
    if let Some(body) = body {
        cv.container(body);
    }
    Ok(Parsed {
        title,
        author,
        blocks: cv.b.finish(),
    })
}

/// Word's default run size in half-points (11pt).
const DEFAULT_SIZE: f32 = 22.0;

/// Run font size in half-points, if set directly on the run.
fn run_size(r: Node) -> Option<f32> {
    child(child(r, "rPr")?, "sz")
        .and_then(|s| attr(s, "val"))
        .and_then(|v| v.parse().ok())
}

fn run_text_len(r: Node) -> usize {
    r.children()
        .filter(|c| c.tag_name().name() == "t")
        .filter_map(|t| t.text())
        .map(|t| t.trim().chars().count())
        .sum()
}

/// The font size carrying the most text, taken as the body text size.
fn dominant_size(body: Node) -> f32 {
    let mut weights: HashMap<u32, usize> = HashMap::new();
    for r in body.descendants().filter(|n| n.tag_name().name() == "r") {
        let size = run_size(r).unwrap_or(DEFAULT_SIZE);
        *weights.entry(size as u32).or_default() += run_text_len(r);
    }
    weights
        .into_iter()
        .max_by_key(|(_, w)| *w)
        .map(|(s, _)| s as f32)
        .unwrap_or(DEFAULT_SIZE)
}

/// Heading level guessed from direct formatting: short paragraphs in a clearly larger
/// (or bold and somewhat larger) font than the body text.
fn formatted_heading(p: Node, body_size: f32) -> Option<u8> {
    let runs: Vec<Node> = p
        .descendants()
        .filter(|n| n.tag_name().name() == "r" && run_text_len(*n) > 0)
        .collect();
    let len: usize = runs.iter().map(|r| run_text_len(*r)).sum();
    if runs.is_empty() || len > 80 {
        return None;
    }
    let ratio = runs
        .iter()
        .map(|r| run_size(*r).unwrap_or(DEFAULT_SIZE))
        .fold(f32::MAX, f32::min)
        / body_size;
    let bold = runs.iter().all(|r| toggle(child(*r, "rPr"), "b"));
    match ratio {
        r if r >= 1.8 => Some(1),
        r if r >= 1.4 => Some(2),
        r if r >= 1.15 && bold => Some(3),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum StyleKind {
    Heading(u8),
    Quote,
}

fn style_kind(name: &str, outline: Option<u8>) -> Option<StyleKind> {
    if name == "title" {
        return Some(StyleKind::Heading(1));
    }
    if name == "subtitle" {
        return Some(StyleKind::Heading(2));
    }
    if let Some(n) = name
        .strip_prefix("heading ")
        .and_then(|n| n.trim().parse::<u8>().ok())
    {
        return Some(StyleKind::Heading(n.clamp(1, 6)));
    }
    if name.contains("quote") {
        return Some(StyleKind::Quote);
    }
    outline
        .filter(|o| *o < 9)
        .map(|o| StyleKind::Heading((o + 1).min(6)))
}

struct Converter<'z, R: Read + Seek> {
    b: Builder,
    zip: &'z mut zip::ZipArchive<R>,
    rels: &'z HashMap<String, String>,
    styles: &'z HashMap<String, StyleKind>,
    images: HashMap<String, ImageRef>,
    body_size: f32,
}

impl<R: Read + Seek> Converter<'_, R> {
    fn container(&mut self, n: Node) {
        for c in n.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "p" => self.paragraph(c),
                "tbl" => self.table(c),
                "sdt" | "sdtContent" | "customXml" | "ins" | "smartTag" => self.container(c),
                _ => {}
            }
        }
    }

    fn paragraph(&mut self, p: Node) {
        let ppr = child(p, "pPr");
        let style = ppr
            .and_then(|pp| child(pp, "pStyle"))
            .and_then(|s| attr(s, "val"))
            .and_then(|id| {
                self.styles
                    .get(id)
                    .copied()
                    .or_else(|| style_kind(&id.to_lowercase(), None))
            });
        let outline = ppr
            .and_then(|pp| child(pp, "outlineLvl"))
            .and_then(|o| attr(o, "val"))
            .and_then(|v| v.parse::<u8>().ok())
            .and_then(|o| style_kind("", Some(o)));
        let list_depth = ppr.and_then(|pp| child(pp, "numPr")).map(|np| {
            child(np, "ilvl")
                .and_then(|l| attr(l, "val"))
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(0)
        });

        let guessed = || formatted_heading(p, self.body_size).map(StyleKind::Heading);
        let kind = style.or(outline).or_else(|| {
            if list_depth.is_none() {
                guessed()
            } else {
                None
            }
        });
        self.inline(p);
        match (kind, list_depth) {
            (Some(StyleKind::Heading(level)), _) => {
                self.b.flush_as(|spans| Block::Heading { level, spans })
            }
            (_, Some(depth)) => self.b.flush_as(|spans| Block::ListItem {
                marker: "•".into(),
                depth,
                spans,
            }),
            (Some(StyleKind::Quote), _) => self.b.flush_as(Block::Quote),
            _ => self.b.flush(),
        }
    }

    fn inline(&mut self, n: Node) {
        for c in n.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "r" => self.run(c),
                "hyperlink" => {
                    let saved = self.b.style;
                    self.b.style.link = true;
                    self.inline(c);
                    self.b.style = saved;
                }
                "pPr" | "del" | "moveFrom" | "commentRangeStart" | "commentRangeEnd" => {}
                _ => self.inline(c),
            }
        }
    }

    fn run(&mut self, r: Node) {
        let rpr = child(r, "rPr");
        let saved = self.b.style;
        self.b.style.bold |= toggle(rpr, "b");
        self.b.style.italic |= toggle(rpr, "i");
        for c in r.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "t" => self.b.raw_text(c.text().unwrap_or_default()),
                "tab" => self.b.raw_text("    "),
                "br" if attr(c, "type") != Some("page") => self.b.line_break(),
                "cr" => self.b.line_break(),
                "drawing" | "pict" | "object" => self.drawing(c),
                _ => {}
            }
        }
        self.b.style = saved;
    }

    /// Finds `a:blip r:embed` (DrawingML) or `v:imagedata r:id` (VML) inside a run.
    fn drawing(&mut self, n: Node) {
        let ids: Vec<&str> = n
            .descendants()
            .filter_map(|d| match d.tag_name().name() {
                "blip" => attr(d, "embed"),
                "imagedata" => attr(d, "id"),
                _ => None,
            })
            .collect();
        for id in ids {
            let Some(target) = self.rels.get(id) else {
                continue;
            };
            let img = match self.images.get(target) {
                Some(img) => img.clone(),
                None => {
                    let Some(bytes) = read_entry(self.zip, target) else {
                        continue;
                    };
                    if image::guess_format(&bytes).is_err() {
                        continue;
                    }
                    let img = ImageRef::new(bytes);
                    self.images.insert(target.clone(), img.clone());
                    img
                }
            };
            self.b.push(Block::Image(img));
        }
    }

    fn table(&mut self, t: Node) {
        for row in t.children().filter(|c| c.tag_name().name() == "tr") {
            for cell in row.children().filter(|c| c.tag_name().name() == "tc") {
                for p in cell.descendants().filter(|d| d.tag_name().name() == "p") {
                    self.inline(p);
                    self.b.raw_text(" ");
                }
                self.b.raw_text("   ");
            }
            self.b.flush();
        }
    }
}
