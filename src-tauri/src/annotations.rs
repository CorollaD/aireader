//! Per-book bookmarks and highlights/notes, stored in the library database.

use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::db::Db;

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
    db: Db,
}

impl AnnotationStore {
    pub fn new(db: &Db) -> Self {
        Self { db: db.clone() }
    }

    pub fn load(&self, book_id: &str) -> Result<Annotations> {
        read(&self.db.conn(), book_id)
    }

    /// Replaces the book's annotations.
    pub fn save(&self, book_id: &str, annotations: &Annotations) -> Result<()> {
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        write(&tx, book_id, annotations)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete(&self, book_id: &str) {
        let _ = write(&self.db.conn(), book_id, &Annotations::default());
    }
}

pub fn read(conn: &Connection, book_id: &str) -> Result<Annotations> {
    let mut ann = Annotations::default();
    let mut stmt = conn.prepare(
        "SELECT id, location, title, excerpt, created FROM bookmarks WHERE book_id = ?1 ORDER BY rowid",
    )?;
    for row in stmt.query_map(params![book_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
        ))
    })? {
        let (id, location, title, excerpt, created) = row?;
        ann.bookmarks.push(Bookmark {
            id,
            location: serde_json::from_str(&location)?,
            title,
            excerpt,
            created,
        });
    }
    let mut stmt = conn.prepare(
        "SELECT id, chapter, start_block, start_offset, end_block, end_offset, text, color, note, created, updated
         FROM highlights WHERE book_id = ?1 ORDER BY rowid",
    )?;
    for row in stmt.query_map(params![book_id], |r| {
        Ok(Highlight {
            id: r.get(0)?,
            chapter: r.get::<_, i64>(1)? as usize,
            start_block: r.get::<_, i64>(2)? as usize,
            start: r.get::<_, i64>(3)? as usize,
            end_block: r.get::<_, i64>(4)? as usize,
            end: r.get::<_, i64>(5)? as usize,
            text: r.get(6)?,
            color: r.get(7)?,
            note: r.get(8)?,
            created: r.get(9)?,
            updated: r.get(10)?,
        })
    })? {
        ann.highlights.push(row?);
    }
    Ok(ann)
}

/// Replaces a book's annotations (call inside a transaction).
pub fn write(conn: &Connection, book_id: &str, ann: &Annotations) -> Result<()> {
    conn.execute("DELETE FROM bookmarks WHERE book_id = ?1", params![book_id])?;
    conn.execute(
        "DELETE FROM highlights WHERE book_id = ?1",
        params![book_id],
    )?;
    for b in &ann.bookmarks {
        conn.execute(
            "INSERT OR REPLACE INTO bookmarks (id, book_id, location, title, excerpt, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![b.id, book_id, serde_json::to_string(&b.location)?, b.title, b.excerpt, b.created],
        )?;
    }
    for h in &ann.highlights {
        conn.execute(
            "INSERT OR REPLACE INTO highlights
             (id, book_id, chapter, start_block, start_offset, end_block, end_offset, text, color, note, created, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                h.id,
                book_id,
                h.chapter as i64,
                h.start_block as i64,
                h.start as i64,
                h.end_block as i64,
                h.end as i64,
                h.text,
                h.color,
                h.note,
                h.created,
                h.updated
            ],
        )?;
    }
    Ok(())
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
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("t.db")).unwrap();
        let store = AnnotationStore::new(&db);
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
        store.save("b2", &ann).unwrap();
        store.save("b1", &Annotations::default()).unwrap();
        assert_eq!(store.load("b1").unwrap(), Annotations::default());
        assert_eq!(store.load("b2").unwrap(), ann, "books are kept apart");
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
