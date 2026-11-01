//! Search across the whole library, with SQLite's FTS5.
//!
//! Each book is stored as passages ([`crate::ai::retrieval::passages`]). FTS5's tokenizer treats
//! a run of Chinese or Japanese as one word, so the indexed text has spaces around those
//! characters and queries become phrases of single characters; any substring can then be found,
//! including two-character words. The index keeps no copy of that text (it is contentless):
//! results are shown from the stored passages.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{Connection, params};
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::ai::retrieval::{self, is_cjk};
use crate::model::{Document, Location};

/// Rows looked at for one query, and what is shown of them.
const MAX_ROWS: i64 = 500;
const HITS_PER_BOOK: usize = 5;

/// What the database holds for full-text search.
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS search_books (
    book_id TEXT PRIMARY KEY,
    sha256 TEXT NOT NULL,
    passages INTEGER NOT NULL,
    updated INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS search_passages (
    id INTEGER PRIMARY KEY,
    book_id TEXT NOT NULL,
    title TEXT NOT NULL,
    location TEXT NOT NULL,
    text TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS search_passages_book ON search_passages (book_id);
CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
    body,
    content = '',
    contentless_delete = 1,
    tokenize = 'unicode61 remove_diacritics 2'
);
";

/// Text as it goes into the index: CJK characters become words of their own. Characters with
/// several encodings become one: PDFs often use Kangxi radicals for common ideographs (`⼀` for
/// `一`), and full-width letters and digits are the same to a reader.
pub fn index_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 2);
    for c in text.nfkc() {
        if is_cjk(c) {
            out.push(' ');
            out.push(c);
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// An FTS5 query for what the user typed: every word must appear, the end of a word may be
/// missing. `None` when there is nothing to search for.
pub fn match_query(query: &str) -> Option<String> {
    let phrases: Vec<String> = query
        .split_whitespace()
        .map(|word| {
            index_text(word)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .map(|word| format!("\"{}\"*", word.replace('"', "\"\"")))
        .collect();
    (!phrases.is_empty()).then(|| phrases.join(" "))
}

/// The books among `books` (id, content hash) that have not been indexed.
pub fn pending(conn: &Connection, books: &[(String, String)]) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT book_id, sha256 FROM search_books")?;
    let done: HashMap<String, String> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(books
        .iter()
        .filter(|(id, sha)| done.get(id) != Some(sha))
        .map(|(id, _)| id.clone())
        .collect())
}

/// Removes a book from the index (call inside a transaction).
pub fn delete_book(conn: &Connection, book_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM search_fts WHERE rowid IN (SELECT id FROM search_passages WHERE book_id = ?1)",
        params![book_id],
    )?;
    conn.execute(
        "DELETE FROM search_passages WHERE book_id = ?1",
        params![book_id],
    )?;
    conn.execute(
        "DELETE FROM search_books WHERE book_id = ?1",
        params![book_id],
    )?;
    Ok(())
}

/// Indexes a book; without a document (it can't be read) it is only marked as done, so it isn't
/// tried on every start. Returns the number of passages.
pub fn index_book(
    conn: &mut Connection,
    book_id: &str,
    sha256: &str,
    doc: Option<&Document>,
) -> Result<usize> {
    let passages = doc.map(retrieval::passages).unwrap_or_default();
    let tx = conn.transaction()?;
    delete_book(&tx, book_id)?;
    {
        let mut row = tx.prepare(
            "INSERT INTO search_passages (book_id, title, location, text) VALUES (?1, ?2, ?3, ?4)",
        )?;
        let mut fts = tx.prepare("INSERT INTO search_fts (rowid, body) VALUES (?1, ?2)")?;
        for p in &passages {
            let id = row.insert(params![
                book_id,
                p.title,
                serde_json::to_string(&p.location)?,
                p.text
            ])?;
            fts.execute(params![id, index_text(&p.text)])?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO search_books (book_id, sha256, passages, updated) VALUES (?1, ?2, ?3, ?4)",
        params![book_id, sha256, passages.len() as i64, crate::library::now() as i64],
    )?;
    tx.commit()?;
    Ok(passages.len())
}

#[derive(Debug, PartialEq, Serialize)]
pub struct TextHit {
    pub location: Location,
    /// Chapter or page.
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookHits {
    pub book_id: String,
    pub hits: Vec<TextHit>,
}

/// The passages matching `query`, best first, grouped by book.
pub fn search(conn: &Connection, query: &str) -> Result<Vec<BookHits>> {
    let Some(expr) = match_query(query) else {
        return Ok(Vec::new());
    };
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut stmt = conn.prepare(
        "SELECT p.book_id, p.title, p.location, p.text
         FROM search_fts f JOIN search_passages p ON p.id = f.rowid
         WHERE search_fts MATCH ?1 ORDER BY f.rank LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![expr, MAX_ROWS], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut books: Vec<BookHits> = Vec::new();
    for row in rows {
        let (book_id, title, location, text) = row?;
        let i = match books.iter().position(|b| b.book_id == book_id) {
            Some(i) => i,
            None => {
                books.push(BookHits {
                    book_id,
                    hits: Vec::new(),
                });
                books.len() - 1
            }
        };
        if books[i].hits.len() < HITS_PER_BOOK {
            books[i].hits.push(TextHit {
                location: serde_json::from_str(&location)?,
                title,
                snippet: crate::search::snippet_around(&text, &words),
            });
        }
    }
    Ok(books)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Chapter, Content, Span};

    fn doc(paragraphs: &[&str]) -> Document {
        Document {
            path: "x".into(),
            title: "t".into(),
            author: None,
            toc: Vec::new(),
            cover: None,
            content: Content::Flow(vec![Chapter {
                title: "第一回".into(),
                blocks: paragraphs
                    .iter()
                    .map(|p| Block::Paragraph(vec![Span::plain(*p)]))
                    .collect(),
            }]),
        }
    }

    fn db(name: &str) -> crate::db::Db {
        let dir = std::env::temp_dir().join(format!("aireader-fts-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::db::Db::open(&dir.join("t.db")).unwrap()
    }

    #[test]
    fn queries_become_safe_phrases() {
        assert_eq!(match_query("孔明").as_deref(), Some("\"孔 明\"*"));
        assert_eq!(
            match_query("rabbit 洞").as_deref(),
            Some("\"rabbit\"* \"洞\"*")
        );
        assert_eq!(match_query("a\"b").as_deref(), Some("\"a\"\"b\"*"));
        assert_eq!(match_query(" ?! "), None);
    }

    #[test]
    fn characters_with_several_encodings_are_one() {
        let db = db("nfkc");
        let mut conn = db.conn();
        // The text layer of a PDF: `⼀` and `⼈` are Kangxi radicals, not ideographs.
        index_book(&mut conn, "a", "1", Some(&doc(&["这是⼀个⼈，ＡＩ"]))).unwrap();
        assert_eq!(search(&conn, "一个人").unwrap().len(), 1);
        assert_eq!(search(&conn, "ai").unwrap().len(), 1);
    }

    #[test]
    fn finds_chinese_words_and_english_prefixes_across_books() {
        let db = db("find");
        let mut conn = db.conn();
        let a = doc(&["诸葛孔明草船借箭。", "The Rabbit-Hole was deep."]);
        let b = doc(&["孔明挥泪斩马谡。"]);
        assert_eq!(index_book(&mut conn, "a", "sha-a", Some(&a)).unwrap(), 1);
        index_book(&mut conn, "b", "sha-b", Some(&b)).unwrap();

        let hits = search(&conn, "孔明").unwrap();
        assert_eq!(hits.len(), 2);
        let ids: Vec<&str> = hits.iter().map(|h| h.book_id.as_str()).collect();
        assert!(ids.contains(&"a") && ids.contains(&"b"));
        assert!(search(&conn, "明草").unwrap().len() == 1, "any substring");
        assert!(search(&conn, "孔草").unwrap().is_empty(), "in order");
        let hits = search(&conn, "rabb").unwrap();
        assert_eq!(hits[0].book_id, "a");
        assert_eq!(hits[0].hits[0].title, "第一回");
        assert!(hits[0].hits[0].snippet.contains("Rabbit-Hole"));
        assert!(search(&conn, "AND OR NOT \"").unwrap().is_empty());
    }

    #[test]
    fn books_are_reindexed_and_removed_cleanly() {
        let db = db("remove");
        let mut conn = db.conn();
        let books = vec![
            ("a".to_owned(), "1".to_owned()),
            ("b".to_owned(), "2".to_owned()),
        ];
        assert_eq!(pending(&conn, &books).unwrap(), ["a", "b"]);
        index_book(&mut conn, "a", "1", Some(&doc(&["旧的内容"]))).unwrap();
        index_book(&mut conn, "b", "2", None).unwrap();
        assert!(pending(&conn, &books).unwrap().is_empty());
        index_book(&mut conn, "a", "1", Some(&doc(&["新的文字"]))).unwrap();
        assert!(search(&conn, "旧的").unwrap().is_empty());
        assert_eq!(search(&conn, "新的").unwrap().len(), 1);
        delete_book(&conn, "a").unwrap();
        assert!(search(&conn, "新的").unwrap().is_empty());
        assert_eq!(pending(&conn, &books).unwrap(), ["a"]);
        let left: i64 = conn
            .query_row("SELECT count(*) FROM search_passages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }
}
