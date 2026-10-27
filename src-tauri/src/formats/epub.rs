//! EPUB 2/3: spine order, NCX/nav table of contents, images resolved from the archive.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Result, anyhow};
use epub::doc::{EpubDoc, NavPoint};

use super::builder::first_heading;
use super::{html, parent_dir, percent_decode, resolve_path};
use crate::model::{Block, Chapter, Content, Document, ImageRef, Location, TocEntry};

fn path_str(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

pub fn load(path: &Path) -> Result<Document> {
    let mut doc = EpubDoc::new(path)
        .map_err(|e| anyhow!(crate::t!("parse_failed", format = "EPUB", error = e)))?;
    let title = doc
        .get_title()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| Document::file_stem(path));
    let author = doc.mdata("creator").map(|m| m.value.clone());

    let spine: Vec<(String, String)> = doc
        .spine
        .iter()
        .filter_map(|item| doc.resources.get(&item.idref))
        .map(|r| (resolve_path("", &path_str(&r.path)), r.mime.clone()))
        .collect();

    let mut images: HashMap<String, ImageRef> = HashMap::new();
    let mut chapters: Vec<Chapter> = Vec::new();
    // Archive path → (chapter index, anchors).
    let mut files: HashMap<String, (usize, HashMap<String, usize>)> = HashMap::new();
    // Archive path of each chapter.
    let mut chapter_files: Vec<String> = Vec::new();

    for (file, mime) in &spine {
        let mut load_image = |doc: &mut EpubDoc<_>, p: String| -> Option<ImageRef> {
            if let Some(img) = images.get(&p) {
                return Some(img.clone());
            }
            let img = ImageRef::new(doc.get_resource_by_path(&p)?);
            images.insert(p, img.clone());
            Some(img)
        };
        let (blocks, anchors) = if mime.starts_with("image/") {
            let blocks = load_image(&mut doc, file.clone())
                .map(Block::Image)
                .into_iter()
                .collect();
            (blocks, HashMap::new())
        } else {
            let Some(bytes) = doc.get_resource_by_path(file) else {
                continue;
            };
            let text = super::txt::decode(&bytes);
            let dir = parent_dir(file).to_owned();
            let out = html::convert(&text, &mut |e| {
                let src = e
                    .attr("src")
                    .or_else(|| e.attrs().find(|(k, _)| *k == "href").map(|(_, v)| v))?;
                load_image(&mut doc, resolve_path(&dir, src))
            });
            (out.blocks, out.anchors)
        };
        let title = first_heading(&blocks).unwrap_or_default();
        files.insert(file.clone(), (chapters.len(), anchors));
        chapter_files.push(file.clone());
        chapters.push(Chapter { title, blocks });
    }

    // Links between the book's files (notes, cross-references) lead to a chapter and block.
    for (chapter, file) in chapters.iter_mut().zip(&chapter_files) {
        for span in chapter
            .blocks
            .iter_mut()
            .filter_map(Block::spans_mut)
            .flatten()
        {
            if let Some(href) = span.href.take() {
                span.target = link_target(&href, file, &files);
            }
        }
    }

    let mut toc = Vec::new();
    collect_toc(&doc.toc, 0, &files, &mut toc);
    // TOC labels are better chapter titles than headings found in the text.
    for entry in &toc {
        if let Location::Flow { chapter, block: 0 } = entry.location
            && let Some(c) = chapters.get_mut(chapter)
            && (c.title.is_empty() || entry.depth == 0)
        {
            c.title = entry.title.clone();
        }
    }
    let (chapters, toc) = drop_empty(chapters, toc);
    let toc = if toc.is_empty() {
        Document::chapter_toc(&chapters)
    } else {
        toc
    };
    if chapters.is_empty() {
        return Err(anyhow!(crate::t!("no_content", format = "EPUB")));
    }
    let cover = doc.get_cover().map(|(bytes, _)| bytes);
    Ok(Document {
        path: path.to_owned(),
        title,
        author,
        toc,
        cover,
        content: Content::Flow(chapters),
    })
}

fn collect_toc(
    points: &[NavPoint],
    depth: usize,
    files: &HashMap<String, (usize, HashMap<String, usize>)>,
    out: &mut Vec<TocEntry>,
) {
    for p in points {
        let target = path_str(&p.content);
        let file = resolve_path("", &target);
        let fragment = target.split_once('#').map(|(_, f)| f);
        if let Some((chapter, anchors)) = files.get(&file) {
            let block = fragment.and_then(|f| anchors.get(f).copied()).unwrap_or(0);
            out.push(TocEntry {
                title: p.label.trim().to_owned(),
                depth,
                location: Location::Flow {
                    chapter: *chapter,
                    block,
                },
            });
        }
        collect_toc(&p.children, depth + 1, files, out);
    }
}

/// Removes chapters without blocks and remaps TOC targets to the next kept chapter.
/// The chapter and block an `href` in `file` points to; `None` for links outside the book.
fn link_target(
    href: &str,
    file: &str,
    files: &HashMap<String, (usize, HashMap<String, usize>)>,
) -> Option<Location> {
    if href.contains(':') {
        return None; // http:, mailto: and the like
    }
    let (path, fragment) = match href.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (href, None),
    };
    let path = if path.is_empty() {
        file.to_owned()
    } else {
        resolve_path(parent_dir(file), path)
    };
    let (chapter, anchors) = files.get(&path)?;
    let block = fragment
        .and_then(|f| anchors.get(&percent_decode(f)).copied())
        .unwrap_or(0);
    Some(Location::Flow {
        chapter: *chapter,
        block,
    })
}

fn drop_empty(chapters: Vec<Chapter>, toc: Vec<TocEntry>) -> (Vec<Chapter>, Vec<TocEntry>) {
    // Old index → (new index, whether the chapter itself was kept).
    let mut map = Vec::with_capacity(chapters.len());
    let mut kept = Vec::new();
    for c in chapters {
        map.push((kept.len(), !c.blocks.is_empty()));
        if !c.blocks.is_empty() {
            kept.push(c);
        }
    }
    let last = kept.len().saturating_sub(1);
    let remap = |loc: Location| match loc {
        Location::Flow { chapter, block } => {
            let (new, was_kept) = map[chapter];
            Location::Flow {
                chapter: new.min(last),
                block: if was_kept { block } else { 0 },
            }
        }
        page => page,
    };
    let toc = toc
        .into_iter()
        .map(|mut e| {
            e.location = remap(e.location);
            e
        })
        .collect();
    for span in kept
        .iter_mut()
        .flat_map(|c| &mut c.blocks)
        .filter_map(Block::spans_mut)
        .flatten()
    {
        span.target = span.target.map(remap);
    }
    (kept, toc)
}
