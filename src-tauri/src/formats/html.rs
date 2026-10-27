//! HTML/XHTML → [`Block`] conversion shared by EPUB and MOBI. CSS is ignored.

use std::collections::HashMap;

use ego_tree::NodeRef;
use scraper::node::Element;
use scraper::{Html, Node};

use super::builder::Builder;
use crate::model::{Block, ImageRef};

pub struct HtmlOutput {
    pub blocks: Vec<Block>,
    /// Element `id`/`name` → index of the block where it starts.
    pub anchors: HashMap<String, usize>,
}

/// Converts an HTML document. `resolve_image` receives `<img>` and SVG `<image>` elements.
pub fn convert(
    html: &str,
    resolve_image: &mut dyn FnMut(&Element) -> Option<ImageRef>,
) -> HtmlOutput {
    let doc = Html::parse_document(html);
    let mut w = Walker {
        b: Builder::new(),
        anchors: HashMap::new(),
        resolve_image,
        quote: 0,
        heading: None,
        lists: Vec::new(),
        li_marker: None,
    };
    w.walk(doc.tree.root());
    w.flush();
    HtmlOutput {
        blocks: w.b.blocks,
        anchors: w.anchors,
    }
}

struct Walker<'r> {
    b: Builder,
    anchors: HashMap<String, usize>,
    resolve_image: &'r mut dyn FnMut(&Element) -> Option<ImageRef>,
    quote: u32,
    heading: Option<u8>,
    /// (ordered, next number) per open list.
    lists: Vec<(bool, usize)>,
    /// Marker for the first paragraph of the current `<li>`; later paragraphs get none.
    li_marker: Option<String>,
}

const BLOCK_TAGS: &[&str] = &[
    "p",
    "div",
    "section",
    "article",
    "body",
    "main",
    "header",
    "footer",
    "aside",
    "nav",
    "figure",
    "figcaption",
    "center",
    "dl",
    "dt",
    "dd",
    "address",
    "table",
    "tr",
    "caption",
    "tbody",
    "thead",
    "tfoot",
    "html",
];

impl Walker<'_> {
    fn walk(&mut self, node: NodeRef<Node>) {
        for child in node.children() {
            match child.value() {
                Node::Text(t) => self.b.text(t),
                Node::Element(e) => self.element(child, e),
                _ => {}
            }
        }
    }

    /// Emits pending text as the block kind implied by the current context.
    fn flush(&mut self) {
        if let Some(level) = self.heading {
            self.b.flush_as(|spans| Block::Heading { level, spans });
        } else if !self.lists.is_empty() && self.li_marker.is_some() {
            let depth = (self.lists.len() - 1) as u8;
            if self.b.has_text() {
                let marker = self.li_marker.replace(String::new()).unwrap_or_default();
                self.b.flush_as(|spans| Block::ListItem {
                    marker,
                    depth,
                    spans,
                });
            }
        } else if self.quote > 0 {
            self.b.flush_as(Block::Quote);
        } else {
            self.b.flush();
        }
    }

    fn push(&mut self, block: Block) {
        self.flush();
        self.b.blocks.push(block);
    }

    fn image(&mut self, e: &Element) {
        if let Some(img) = (self.resolve_image)(e) {
            self.push(Block::Image(img));
        } else if let Some(alt) = e.attr("alt").filter(|a| !a.trim().is_empty()) {
            self.b.text(&format!("[{alt}]"));
        }
    }

    fn element(&mut self, node: NodeRef<Node>, e: &Element) {
        for key in ["id", "name"] {
            if let Some(id) = e.attr(key) {
                let at = self.b.blocks.len();
                self.anchors.entry(id.to_owned()).or_insert(at);
            }
        }
        let name = e.name();
        match name {
            "head" | "script" | "style" | "title" | "noscript" | "rt" | "rp" | "template" => {}
            "br" => self.b.line_break(),
            "img" | "image" => self.image(e),
            "hr" => self.push(Block::Rule),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                let saved = self.heading.replace(name.as_bytes()[1] - b'0');
                self.walk(node);
                self.flush();
                self.heading = saved;
            }
            "blockquote" => {
                self.flush();
                self.quote += 1;
                self.walk(node);
                self.flush();
                self.quote -= 1;
            }
            "ul" | "ol" => {
                self.flush();
                let start = e.attr("start").and_then(|s| s.parse().ok()).unwrap_or(1);
                self.lists.push((name == "ol", start));
                let saved = self.li_marker.take();
                self.walk(node);
                self.flush();
                self.li_marker = saved;
                self.lists.pop();
            }
            "li" => {
                self.flush();
                let orphan = self.lists.is_empty();
                if orphan {
                    self.lists.push((false, 1));
                }
                let marker = match self.lists.last_mut() {
                    Some((true, n)) => {
                        *n += 1;
                        format!("{}.", *n - 1)
                    }
                    _ => "•".to_owned(),
                };
                let saved = self.li_marker.replace(marker);
                self.walk(node);
                self.flush();
                self.li_marker = saved;
                if orphan {
                    self.lists.pop();
                }
            }
            "pre" => {
                self.flush();
                let text: String = node
                    .descendants()
                    .filter_map(|n| n.value().as_text().map(|t| t.to_string()))
                    .collect();
                let text = text.trim_matches('\n').to_owned();
                if !text.trim().is_empty() {
                    self.push(Block::Code(text));
                }
            }
            "td" | "th" => {
                self.walk(node);
                self.b.raw_text("    ");
            }
            "b" | "strong" => self.styled_walk(node, |s| s.bold = true),
            "i" | "em" | "cite" | "dfn" | "var" => self.styled_walk(node, |s| s.italic = true),
            "code" | "tt" | "kbd" | "samp" => self.styled_walk(node, |s| s.code = true),
            "a" if e.attr("href").is_some() => {
                let saved = std::mem::replace(&mut self.b.href, e.attr("href").map(str::to_owned));
                self.styled_walk(node, |s| s.link = true);
                self.b.href = saved;
            }
            "svg" => {
                for d in node.descendants() {
                    if let Some(img) = d.value().as_element().filter(|el| el.name() == "image") {
                        self.image(img);
                    }
                }
            }
            n if BLOCK_TAGS.contains(&n) => {
                self.flush();
                self.walk(node);
                self.flush();
            }
            _ => self.walk(node),
        }
    }

    fn styled_walk(
        &mut self,
        node: NodeRef<Node>,
        change: impl FnOnce(&mut crate::model::SpanStyle),
    ) {
        let saved = self.b.style;
        change(&mut self.b.style);
        self.walk(node);
        self.b.style = saved;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(html: &str) -> HtmlOutput {
        convert(html, &mut |e| {
            e.attr("src").map(|s| ImageRef::new(s.as_bytes().to_vec()))
        })
    }

    #[test]
    fn converts_common_structure() {
        let out = conv(
            r#"<html><head><title>x</title><style>p{}</style></head><body>
            <h1 id="top">Title <em>One</em></h1>
            <p>Hello <b>bold</b> and <i>it</i>.</p>
            <div><p id="p2">Second<br/>line</p></div>
            <ul><li>a</li><li><p>b</p><p>b2</p></li></ul>
            <ol start="3"><li>x</li></ol>
            <blockquote><p>quoted</p></blockquote>
            <pre>  code
  block</pre>
            <img src="pic.png"/><hr/>
            </body></html>"#,
        );
        let b = &out.blocks;
        assert!(matches!(&b[0], Block::Heading { level: 1, .. }));
        assert_eq!(b[0].text(), "Title One");
        assert_eq!(b[1].text(), "Hello bold and it.");
        assert!(matches!(&b[1], Block::Paragraph(s) if s.iter().any(|s| s.style.bold)));
        assert_eq!(b[2].text(), "Second\nline");
        assert!(matches!(&b[3], Block::ListItem { marker, .. } if marker == "•"));
        assert!(matches!(&b[4], Block::ListItem { marker, .. } if marker == "•"));
        assert!(matches!(&b[5], Block::ListItem { marker, .. } if marker.is_empty()));
        assert!(matches!(&b[6], Block::ListItem { marker, .. } if marker == "3."));
        assert!(matches!(&b[7], Block::Quote(_)));
        assert!(matches!(&b[8], Block::Code(c) if c == "  code\n  block"));
        assert!(matches!(&b[9], Block::Image(_)));
        assert!(matches!(&b[10], Block::Rule));
        assert_eq!(out.anchors["top"], 0);
        assert_eq!(out.anchors["p2"], 2);
    }
}
