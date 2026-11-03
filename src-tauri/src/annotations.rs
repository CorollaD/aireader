//! Per-book bookmarks, highlights/notes and notes on the whole book, stored in the library
//! database.

use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::db::{Db, deleted_since};

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
    /// When it last changed; `write` keeps this up to date.
    #[serde(default)]
    pub updated: u64,
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

/// A note on the whole book rather than a passage, such as an AI answer kept for later.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookNote {
    pub id: String,
    pub title: String,
    /// Markdown.
    pub text: String,
    pub created: u64,
    pub updated: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Annotations {
    pub bookmarks: Vec<Bookmark>,
    pub highlights: Vec<Highlight>,
    pub notes: Vec<BookNote>,
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

    /// Saves what the reader holds. Its copy may be older than a sync that just ran, so nothing
    /// is deleted unless listed in `removed`, stored records newer than its copy stay, and
    /// records the sync deleted aren't brought back.
    pub fn apply(
        &self,
        book_id: &str,
        annotations: &Annotations,
        removed: &[String],
    ) -> Result<()> {
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let stored = read(&tx, book_id)?;
        let gone = |id: &String| removed.contains(id);
        let mut new = Annotations::default();
        for b in &annotations.bookmarks {
            let updated = b.updated.max(b.created);
            if !gone(&b.id) && !deleted_since(&tx, "bookmark", &b.id, updated)? {
                new.bookmarks.push(b.clone());
            }
        }
        for h in &annotations.highlights {
            if !gone(&h.id) && !deleted_since(&tx, "highlight", &h.id, h.updated)? {
                new.highlights.push(h.clone());
            }
        }
        for n in &annotations.notes {
            if !gone(&n.id) && !deleted_since(&tx, "note", &n.id, n.updated)? {
                new.notes.push(n.clone());
            }
        }
        for o in stored.bookmarks.into_iter().filter(|o| !gone(&o.id)) {
            match new.bookmarks.iter_mut().find(|n| n.id == o.id) {
                Some(n) if n.updated < o.updated => *n = o,
                Some(_) => {}
                None => new.bookmarks.push(o),
            }
        }
        for o in stored.highlights.into_iter().filter(|o| !gone(&o.id)) {
            match new.highlights.iter_mut().find(|n| n.id == o.id) {
                Some(n) if n.updated < o.updated => *n = o,
                Some(_) => {}
                None => new.highlights.push(o),
            }
        }
        for o in stored.notes.into_iter().filter(|o| !gone(&o.id)) {
            match new.notes.iter_mut().find(|n| n.id == o.id) {
                Some(n) if n.updated < o.updated => *n = o,
                Some(_) => {}
                None => new.notes.push(o),
            }
        }
        write(&tx, book_id, &new)?;
        tx.commit()?;
        Ok(())
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
        "SELECT id, location, title, excerpt, created, updated FROM bookmarks WHERE book_id = ?1 ORDER BY rowid",
    )?;
    for row in stmt.query_map(params![book_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
        ))
    })? {
        let (id, location, title, excerpt, created, updated) = row?;
        ann.bookmarks.push(Bookmark {
            id,
            location: serde_json::from_str(&location)?,
            title,
            excerpt,
            created,
            updated,
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
    let mut stmt = conn.prepare(
        "SELECT id, title, text, created, updated FROM book_notes WHERE book_id = ?1 ORDER BY rowid",
    )?;
    for row in stmt.query_map(params![book_id], |r| {
        Ok(BookNote {
            id: r.get(0)?,
            title: r.get(1)?,
            text: r.get(2)?,
            created: r.get(3)?,
            updated: r.get(4)?,
        })
    })? {
        ann.notes.push(row?);
    }
    Ok(ann)
}

/// Saves a book's annotations (call inside a transaction): what is gone is recorded as deleted,
/// for syncing.
pub fn write(conn: &Connection, book_id: &str, ann: &Annotations) -> Result<()> {
    let old = read(conn, book_id)?;
    let now = crate::library::now();
    for b in old
        .bookmarks
        .iter()
        .filter(|b| !ann.bookmarks.iter().any(|n| n.id == b.id))
    {
        crate::db::tombstone(conn, "bookmark", &b.id, now)?;
    }
    for h in old
        .highlights
        .iter()
        .filter(|h| !ann.highlights.iter().any(|n| n.id == h.id))
    {
        crate::db::tombstone(conn, "highlight", &h.id, now)?;
    }
    for n in old
        .notes
        .iter()
        .filter(|n| !ann.notes.iter().any(|x| x.id == n.id))
    {
        crate::db::tombstone(conn, "note", &n.id, now)?;
    }
    let mut new = ann.clone();
    for b in &mut new.bookmarks {
        b.updated = b.updated.max(b.created);
    }
    store(conn, book_id, &new)
}

/// Replaces a book's annotations as they are (call inside a transaction).
pub fn store(conn: &Connection, book_id: &str, ann: &Annotations) -> Result<()> {
    conn.execute("DELETE FROM bookmarks WHERE book_id = ?1", params![book_id])?;
    conn.execute(
        "DELETE FROM highlights WHERE book_id = ?1",
        params![book_id],
    )?;
    conn.execute(
        "DELETE FROM book_notes WHERE book_id = ?1",
        params![book_id],
    )?;
    for n in &ann.notes {
        conn.execute(
            "INSERT OR REPLACE INTO book_notes (id, book_id, title, text, created, updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![n.id, book_id, n.title, n.text, n.created, n.updated],
        )?;
    }
    for b in &ann.bookmarks {
        conn.execute(
            "INSERT OR REPLACE INTO bookmarks (id, book_id, location, title, excerpt, created, updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![b.id, book_id, serde_json::to_string(&b.location)?, b.title, b.excerpt, b.created, b.updated],
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

/// A chapter's title, or a generated name for one without.
pub fn chapter_name(titles: &[String], chapter: usize) -> String {
    titles
        .get(chapter)
        .filter(|t| !t.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| crate::t!("title_chapter", n = chapter + 1))
}

/// The books that have any bookmarks, highlights or notes.
pub fn books_with_notes(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT book_id FROM bookmarks UNION SELECT book_id FROM highlights
         UNION SELECT book_id FROM book_notes",
    )?;
    let ids = stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(ids)
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
    if !annotations.notes.is_empty() {
        out.push_str(&format!("## {}\n\n", crate::t!("notes_book")));
        for n in &annotations.notes {
            if !n.title.trim().is_empty() {
                out.push_str(&format!("### {}\n\n", n.title.trim()));
            }
            out.push_str(&format!("{}\n\n", n.text.trim()));
        }
    }
    let mut highlights: Vec<&Highlight> = annotations.highlights.iter().collect();
    highlights.sort_by_key(|h| (h.chapter, h.start_block, h.start));
    let mut chapter = None;
    for h in highlights {
        if chapter != Some(h.chapter) {
            chapter = Some(h.chapter);
            let name = chapter_name(chapter_titles, h.chapter);
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
    fn a_stale_save_cannot_undo_a_sync() {
        let dir = std::env::temp_dir().join(format!("aireader-ann-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("t.db")).unwrap();
        let store = AnnotationStore::new(&db);
        let mut old = hl(0, 1, "text", None);
        old.id = "edited".into();
        let mut gone = hl(0, 2, "text", None);
        gone.id = "gone".into();
        let mine = Annotations {
            highlights: vec![old.clone(), gone.clone()],
            ..Default::default()
        };
        store.save("b", &mine).unwrap();

        // A sync brings a newer edit and a new highlight while the reader holds its old copy.
        let mut newer = old.clone();
        newer.note = Some("from the other device".into());
        newer.updated = 50;
        let mut arrived = hl(0, 3, "text", None);
        arrived.id = "arrived".into();
        let conn = db.conn();
        store_for_test(
            &conn,
            "b",
            vec![newer.clone(), gone.clone(), arrived.clone()],
        );
        drop(conn);

        store.apply("b", &mine, &["gone".to_owned()]).unwrap();
        let mut ids: Vec<String> = store
            .load("b")
            .unwrap()
            .highlights
            .iter()
            .map(|h| h.id.clone())
            .collect();
        ids.sort();
        assert_eq!(ids, ["arrived", "edited"], "only what was removed is gone");
        let saved = store.load("b").unwrap();
        assert_eq!(
            saved
                .highlights
                .iter()
                .find(|h| h.id == "edited")
                .unwrap()
                .note,
            newer.note
        );
    }

    #[test]
    fn a_stale_save_cannot_bring_back_what_a_sync_deleted() {
        let dir = std::env::temp_dir().join(format!("aireader-ann-dead-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("t.db")).unwrap();
        let store = AnnotationStore::new(&db);
        let mut kept = hl(0, 1, "text", None);
        kept.id = "kept".into();
        let mut deleted = hl(0, 2, "text", None);
        deleted.id = "deleted".into();
        let mine = Annotations {
            highlights: vec![kept.clone(), deleted.clone()],
            ..Default::default()
        };
        store.save("b", &mine).unwrap();

        // Another device deleted one; the sync removed it while the reader held its old copy.
        let conn = db.conn();
        crate::db::tombstone(&conn, "highlight", "deleted", 10).unwrap();
        store_for_test(&conn, "b", vec![kept.clone()]);
        drop(conn);

        store.apply("b", &mine, &[]).unwrap();
        let ids: Vec<String> = store
            .load("b")
            .unwrap()
            .highlights
            .iter()
            .map(|h| h.id.clone())
            .collect();
        assert_eq!(ids, ["kept"]);

        // Changed after the deletion, it stays.
        let mut edited = deleted.clone();
        edited.updated = 20;
        let later = Annotations {
            highlights: vec![kept, edited],
            ..Default::default()
        };
        store.apply("b", &later, &[]).unwrap();
        assert_eq!(store.load("b").unwrap().highlights.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn book_note(id: &str, title: &str, text: &str) -> BookNote {
        BookNote {
            id: id.into(),
            title: title.into(),
            text: text.into(),
            created: 1,
            updated: 1,
        }
    }

    fn store_for_test(conn: &Connection, book: &str, highlights: Vec<Highlight>) {
        let ann = Annotations {
            highlights,
            ..Default::default()
        };
        store(conn, book, &ann).unwrap();
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
                updated: 1,
            }],
            highlights: vec![hl(0, 1, "text", Some("note"))],
            notes: vec![book_note("n", "问题", "回答")],
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
            notes: vec![],
        };
        let md = to_markdown("Book", Some("Me"), &["One".into(), "Two".into()], &ann);
        assert_eq!(
            md,
            "# Book\n\nMe\n\n## One\n\n> first\n> line\n\nmy note\n\n## Two\n\n> second\n\n"
        );
    }

    #[test]
    fn notes_on_the_book_come_first_in_markdown() {
        let ann = Annotations {
            highlights: vec![hl(0, 0, "passage", None)],
            notes: vec![book_note("n", "What is it about?", "**Power**, mostly.")],
            ..Default::default()
        };
        let md = to_markdown("Book", None, &["One".into()], &ann);
        let heading = crate::t!("notes_book");
        assert_eq!(
            md,
            format!(
                "# Book\n\n## {heading}\n\n### What is it about?\n\n**Power**, mostly.\n\n## One\n\n> passage\n\n"
            )
        );
    }

    #[test]
    fn a_deleted_note_on_the_book_stays_deleted() {
        let dir = std::env::temp_dir().join(format!("aireader-ann-note-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("t.db")).unwrap();
        let store = AnnotationStore::new(&db);
        let mine = Annotations {
            notes: vec![book_note("a", "", "one"), book_note("b", "", "two")],
            ..Default::default()
        };
        store.save("b", &mine).unwrap();
        store.apply("b", &mine, &["a".to_owned()]).unwrap();
        let ids: Vec<String> = store
            .load("b")
            .unwrap()
            .notes
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(ids, ["b"]);
        assert!(
            crate::db::deleted_at(&db.conn(), "note", "a")
                .unwrap()
                .is_some()
        );
        // A stale copy can't bring it back.
        store.apply("b", &mine, &[]).unwrap();
        assert_eq!(store.load("b").unwrap().notes.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
