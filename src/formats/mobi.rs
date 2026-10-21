//! MOBI / AZW / AZW3 (KF8, best effort). The text is decompressed by the `mobi` crate and
//! fed through the shared HTML converter. DRM-protected books are rejected.

use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Result, bail};
use regex::Regex;

use super::builder::first_heading;
use super::html;
use crate::model::{Chapter, Content, Document, ImageRef};

/// Section boundaries: MOBI page breaks and the per-file skeleton start in KF8.
static SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<mbp:pagebreak[^>]*>|(<\?xml[^>]*>\s*)?(<!DOCTYPE[^>]*>\s*)?<html[\s>]")
        .unwrap()
});

pub fn load(path: &Path) -> Result<Document> {
    let book = mobi::Mobi::from_path(path)?;
    if book.encryption() != mobi::headers::Encryption::No {
        bail!("此书受 DRM 保护，无法打开");
    }
    let raw = book.content_as_string_lossy();
    let text = html_part(&raw);
    if text.trim().is_empty() {
        bail!("无法解码 MOBI 文本内容");
    }

    let records = book.raw_records();
    let first_image = book.metadata.mobi.first_image_index as usize;
    let image = |n: usize| -> Option<ImageRef> {
        let rec = records.range(first_image + n.checked_sub(1)?..).first()?;
        image::guess_format(rec.content).ok()?;
        Some(ImageRef::new(rec.content.to_vec()))
    };

    let mut chapters = Vec::new();
    for part in split_sections(text) {
        let out = html::convert(part, &mut |e| {
            if let Some(idx) = e.attr("recindex") {
                return image(idx.trim_start_matches('0').parse().ok()?);
            }
            let src = e.attr("src")?;
            image(kindle_embed_index(src)?)
        });
        if out.blocks.iter().all(|b| b.text().trim().is_empty()) && !out.blocks.iter().any(is_image)
        {
            continue;
        }
        let title = first_heading(&out.blocks)
            .or_else(|| short_first_line(&out.blocks))
            .unwrap_or_default();
        chapters.push(Chapter {
            title,
            blocks: out.blocks,
        });
    }
    for (i, c) in chapters.iter_mut().enumerate() {
        if c.title.is_empty() {
            c.title = format!("第 {} 节", i + 1);
        }
    }
    if chapters.is_empty() {
        bail!("MOBI 中没有可显示的内容");
    }
    let title = Some(book.title())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| Document::file_stem(path));
    Ok(Document {
        path: path.to_owned(),
        title,
        author: book.author(),
        toc: Document::chapter_toc(&chapters),
        content: Content::Flow(chapters),
    })
}

fn is_image(b: &crate::model::Block) -> bool {
    matches!(b, crate::model::Block::Image(_))
}

/// The HTML text flow only: KF8 appends CSS/SVG flows after the last `</html>`, and
/// record padding can leave junk before the first document.
fn html_part(text: &str) -> &str {
    let lower = text.to_ascii_lowercase();
    let start = [lower.find("<?xml"), lower.find("<html")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(0);
    let end = lower
        .rfind("</html>")
        .map(|i| i + "</html>".len())
        .filter(|e| *e > start)
        .unwrap_or(text.len());
    &text[start..end]
}

/// MOBI files often style chapter titles with `<font>` instead of headings.
fn short_first_line(blocks: &[crate::model::Block]) -> Option<String> {
    let text = blocks
        .iter()
        .map(|b| b.text())
        .find(|t| !t.trim().is_empty())?;
    let line = text.lines().next()?.trim();
    (line.chars().count() <= 50).then(|| line.to_owned())
}

/// Splits raw book HTML into sections at page breaks / KF8 file starts.
pub fn split_sections(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut last = 0;
    for m in SPLIT_RE.find_iter(text) {
        // Keep the `<html>`/`<?xml` tag with the following section; drop page-break tags.
        let start = m.start();
        if start > last {
            parts.push(&text[last..start]);
        }
        last = if m.as_str().to_lowercase().starts_with("<mbp") {
            m.end()
        } else {
            start
        };
    }
    parts.push(&text[last..]);
    parts.retain(|p| !p.trim().is_empty());
    parts
}

/// Parses `kindle:embed:XXXX?mime=…` (base-32 resource number, 1-based).
fn kindle_embed_index(src: &str) -> Option<usize> {
    let code = src.strip_prefix("kindle:embed:")?;
    let code = code.split(['?', '#']).next()?;
    usize::from_str_radix(code, 32).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_pagebreaks() {
        let parts = split_sections("<p>a</p><mbp:pagebreak/><p>b</p><MBP:PAGEBREAK /> <p>c</p>");
        assert_eq!(parts, ["<p>a</p>", "<p>b</p>", " <p>c</p>"]);
        let kf8 = split_sections(
            "<?xml version='1.0'?><html><body>1</body></html><?xml version='1.0'?><html>2</html>",
        );
        assert_eq!(kf8.len(), 2);
        assert!(kf8[1].contains('2'));
    }

    #[test]
    fn keeps_only_html_flow() {
        assert_eq!(
            html_part("\0junk<html><p>a</p></html>\0 z.css { margin: 0 }"),
            "<html><p>a</p></html>"
        );
        assert_eq!(html_part("<p>no wrapper</p>"), "<p>no wrapper</p>");
    }

    #[test]
    fn parses_kindle_embed() {
        assert_eq!(
            kindle_embed_index("kindle:embed:000A?mime=image/jpg"),
            Some(10)
        );
        assert_eq!(kindle_embed_index("kindle:embed:0001"), Some(1));
        assert_eq!(kindle_embed_index("images/a.jpg"), None);
    }
}
