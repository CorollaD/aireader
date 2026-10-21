//! Format detection and dispatch.

pub mod builder;
pub mod cbz;
pub mod docx;
pub mod epub;
pub mod fb2;
pub mod html;
pub mod markdown;
pub mod mobi;
pub mod pdf;
pub mod txt;

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};

use crate::model::{Block, Content, Document};

/// File extensions offered in the open dialog and accepted when importing folders.
pub const EXTENSIONS: &[&str] = &[
    "epub", "pdf", "txt", "md", "markdown", "mobi", "azw", "azw3", "prc", "fb2", "docx", "cbz",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Epub,
    Pdf,
    Txt,
    Markdown,
    Mobi,
    Fb2,
    Fb2Zip,
    Docx,
    Cbz,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Epub => "EPUB",
            Format::Pdf => "PDF",
            Format::Txt => "TXT",
            Format::Markdown => "MD",
            Format::Mobi => "MOBI",
            Format::Fb2 | Format::Fb2Zip => "FB2",
            Format::Docx => "DOCX",
            Format::Cbz => "CBZ",
        }
    }
}

/// Whether a file name has one of the supported extensions (used when scanning folders).
pub fn is_supported(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if name.starts_with('.') {
        return false;
    }
    name.ends_with(".fb2.zip")
        || name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| EXTENSIONS.contains(&ext))
}

/// Cover image as encoded bytes (EPUB cover, first page, or first image in the text).
pub fn cover_image(path: &Path, doc: &Document) -> Option<Vec<u8>> {
    match &doc.content {
        Content::Paged(src) => {
            let page = src.render(0, 480).ok()?;
            let img = image::RgbaImage::from_raw(page.width, page.height, page.rgba)?;
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png).ok()?;
            Some(out.into_inner())
        }
        Content::Flow(chapters) => {
            if detect(path).ok() == Some(Format::Epub)
                && let Some((bytes, _)) = ::epub::doc::EpubDoc::new(path)
                    .ok()
                    .and_then(|mut d| d.get_cover())
            {
                return Some(bytes);
            }
            chapters
                .iter()
                .take(3)
                .flat_map(|c| &c.blocks)
                .find_map(|b| match b {
                    Block::Image(img) => Some(img.bytes.to_vec()),
                    _ => None,
                })
        }
    }
}

pub fn open(path: &Path) -> Result<Document> {
    let format = detect(path)?;
    let doc = match format {
        Format::Epub => epub::load(path),
        Format::Pdf => pdf::load(path),
        Format::Txt => txt::load(path),
        Format::Markdown => markdown::load(path),
        Format::Mobi => mobi::load(path),
        Format::Fb2 => fb2::load(path),
        Format::Fb2Zip => fb2::load_zip(path),
        Format::Docx => docx::load(path),
        Format::Cbz => cbz::load(path),
    };
    doc.with_context(|| format!("无法打开 {}", path.display()))
}

pub fn detect(path: &Path) -> Result<Format> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if name.ends_with(".fb2.zip") {
        return Ok(Format::Fb2Zip);
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    Ok(match ext {
        "epub" => Format::Epub,
        "pdf" => Format::Pdf,
        "txt" | "text" | "log" => Format::Txt,
        "md" | "markdown" | "mdown" => Format::Markdown,
        "mobi" | "azw" | "azw3" | "prc" | "pdb" => Format::Mobi,
        "fb2" => Format::Fb2,
        "docx" => Format::Docx,
        "cbz" => Format::Cbz,
        _ => sniff(path)?,
    })
}

/// Detects the format from file contents when the extension is unknown.
fn sniff(path: &Path) -> Result<Format> {
    let mut head = [0u8; 1024];
    let n = std::fs::File::open(path)
        .context("无法读取文件")?
        .read(&mut head)?;
    let head = &head[..n];
    if head.starts_with(b"%PDF") {
        return Ok(Format::Pdf);
    }
    if head.len() >= 68 && (&head[60..68] == b"BOOKMOBI" || &head[60..68] == b"TEXtREAd") {
        return Ok(Format::Mobi);
    }
    if head.starts_with(b"PK") {
        let file = std::fs::File::open(path)?;
        let zip = zip::ZipArchive::new(file)?;
        let names: Vec<String> = zip
            .file_names()
            .filter_map(Result::ok)
            .map(|n| n.to_lowercase())
            .collect();
        if names.iter().any(|n| n == "mimetype" || n.ends_with(".opf")) {
            return Ok(Format::Epub);
        }
        if names.iter().any(|n| n == "word/document.xml") {
            return Ok(Format::Docx);
        }
        if names.iter().any(|n| n.ends_with(".fb2")) {
            return Ok(Format::Fb2Zip);
        }
        return Ok(Format::Cbz);
    }
    if String::from_utf8_lossy(head).contains("<FictionBook") {
        return Ok(Format::Fb2);
    }
    Ok(Format::Txt)
}

/// Decodes `%XX` escapes in a URL path. Invalid sequences are kept verbatim.
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(hi << 4 | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolves `rel` against directory `base` (both `/`-separated archive paths),
/// dropping any `#fragment` and normalising `.`/`..` segments.
pub fn resolve_path(base: &str, rel: &str) -> String {
    let rel = rel.split('#').next().unwrap_or("");
    let rel = percent_decode(rel);
    let mut parts: Vec<&str> = if rel.starts_with('/') {
        Vec::new()
    } else {
        base.split('/').collect()
    };
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.retain(|p| !p.is_empty());
    parts.join("/")
}

/// Directory part of an archive path (`a/b/c.html` → `a/b`).
pub fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_helpers() {
        assert_eq!(
            resolve_path("OEBPS/text", "../images/a%20b.png#x"),
            "OEBPS/images/a b.png"
        );
        assert_eq!(resolve_path("", "./ch1.xhtml"), "ch1.xhtml");
        assert_eq!(parent_dir("OEBPS/text/c.xhtml"), "OEBPS/text");
        assert_eq!(percent_decode("%E4%B8%AD%zz"), "中%zz");
    }
}
