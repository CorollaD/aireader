//! Per-book bookmarks and highlights/notes, stored as `annotations/<book id>.json` under the
//! library root.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::model::Location;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: String,
    pub location: Location,
    /// Chapter title or "第 n 页".
    pub title: String,
    pub excerpt: String,
    /// Unix seconds.
    pub created: u64,
}

/// A highlighted text range in a reflowable book, possibly spanning several blocks.
/// Offsets index `Block::text()` in UTF-16 code units, as the frontend measures them; the
/// backend only stores them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    pub id: String,
    pub chapter: usize,
    pub start_block: usize,
    pub start: usize,
    pub end_block: usize,
    pub end: usize,
    /// The highlighted text, used to re-find the range if the book's text changes.
    pub text: String,
    pub color: String,
    #[serde(default)]
    pub note: Option<String>,
    pub created: u64,
    pub updated: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Annotations {
    pub bookmarks: Vec<Bookmark>,
    pub highlights: Vec<Highlight>,
}

pub struct AnnotationStore {
    dir: PathBuf,
}

impl AnnotationStore {
    pub fn new(library_root: &Path) -> Self {
        Self {
            dir: library_root.join("annotations"),
        }
    }

    fn file(&self, book_id: &str) -> PathBuf {
        self.dir.join(format!("{book_id}.json"))
    }

    pub fn load(&self, book_id: &str) -> Result<Annotations> {
        match std::fs::read(self.file(book_id)) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Annotations::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, book_id: &str, annotations: &Annotations) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let file = self.file(book_id);
        if annotations.bookmarks.is_empty() && annotations.highlights.is_empty() {
            let _ = std::fs::remove_file(file);
            return Ok(());
        }
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(annotations)?)?;
        std::fs::rename(tmp, file)?;
        Ok(())
    }

    pub fn delete(&self, book_id: &str) {
        let _ = std::fs::remove_file(self.file(book_id));
    }
}

/// Renders highlights and notes as Markdown, grouped by chapter in reading order.
pub fn to_markdown(
    title: &str,
    author: Option<&str>,
    chapter_titles: &[String],
    annotations: &Annotations,
) -> String {
    let mut out = format!("# {title}\n\n");
    if let Some(author) = author {
        out.push_str(&format!("{author}\n\n"));
    }
    let mut highlights: Vec<&Highlight> = annotations.highlights.iter().collect();
    highlights.sort_by_key(|h| (h.chapter, h.start_block, h.start));
    let mut chapter = None;
    for h in highlights {
        if chapter != Some(h.chapter) {
            chapter = Some(h.chapter);
            let name = chapter_titles
                .get(h.chapter)
                .filter(|t| !t.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| crate::t!("title_chapter", n = h.chapter + 1));
            out.push_str(&format!("## {name}\n\n"));
        }
        for line in h.text.trim().lines() {
            out.push_str(&format!("> {line}\n"));
        }
        out.push('\n');
        if let Some(note) = h.note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            out.push_str(&format!("{note}\n\n"));
        }
    }
    if !annotations.bookmarks.is_empty() {
        out.push_str(&format!("## {}\n\n", crate::t!("notes_bookmarks")));
        for b in &annotations.bookmarks {
            if b.excerpt.is_empty() {
                out.push_str(&format!("- {}\n", b.title));
            } else {
                out.push_str(&format!("- {}: {}\n", b.title, b.excerpt));
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hl(chapter: usize, block: usize, text: &str, note: Option<&str>) -> Highlight {
        Highlight {
            id: format!("{chapter}-{block}"),
            chapter,
            start_block: block,
            start: 0,
            end_block: block,
            end: text.chars().count(),
            text: text.into(),
            color: "yellow".into(),
            note: note.map(Into::into),
            created: 1,
            updated: 1,
        }
    }

    #[test]
    fn save_load_delete_round_trip() {
        let dir = std::env::temp_dir().join(format!("aireader-ann-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = AnnotationStore::new(&dir);
        assert_eq!(
            store.load("b1").unwrap(),
            Annotations::default(),
            "missing file is empty"
        );
        let ann = Annotations {
            bookmarks: vec![Bookmark {
                id: "x".into(),
                location: Location::Page { page: 3 },
                title: "第 4 页".into(),
                excerpt: String::new(),
                created: 1,
            }],
            highlights: vec![hl(0, 1, "text", Some("note"))],
        };
        store.save("b1", &ann).unwrap();
        assert_eq!(store.load("b1").unwrap(), ann);
        store.save("b1", &Annotations::default()).unwrap();
        assert!(
            !dir.join("annotations/b1.json").exists(),
            "empty annotations remove the file"
        );
        store.save("b1", &ann).unwrap();
        store.delete("b1");
        assert_eq!(store.load("b1").unwrap(), Annotations::default());
    }

    #[test]
    fn markdown_groups_by_chapter_in_order() {
        let ann = Annotations {
            bookmarks: vec![],
            highlights: vec![
                hl(1, 0, "second", None),
                hl(0, 2, "first\nline", Some("my note")),
            ],
        };
        let md = to_markdown("Book", Some("Me"), &["One".into(), "Two".into()], &ann);
        assert_eq!(
            md,
            "# Book\n\nMe\n\n## One\n\n> first\n> line\n\nmy note\n\n## Two\n\n> second\n\n"
        );
    }
}
