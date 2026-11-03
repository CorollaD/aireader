//! Highlights, notes and bookmarks in the formats they are exported to.

use std::collections::HashSet;

use anyhow::Result;
use serde_json::json;

use crate::annotations::{self, Annotations};
use crate::library;

/// One book's notes, with what is needed to present them.
pub struct BookNotes {
    pub title: String,
    pub author: Option<String>,
    /// Chapter titles, for naming the chapters highlights are in.
    pub chapters: Vec<String>,
    pub annotations: Annotations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotesFormat {
    Markdown,
    /// Markdown with properties at the top, one file per book, for an Obsidian vault.
    Obsidian,
    Csv,
    Json,
}

impl NotesFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown | Self::Obsidian => "md",
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }

    /// Whether several books go into one file (otherwise each gets its own).
    pub fn single_file(self) -> bool {
        matches!(self, Self::Csv | Self::Json)
    }
}

/// The file contents for one book (Markdown formats).
pub fn book_markdown(book: &BookNotes, format: NotesFormat) -> String {
    let body = annotations::to_markdown(
        &book.title,
        book.author.as_deref(),
        &book.chapters,
        &book.annotations,
    );
    if format != NotesFormat::Obsidian {
        return body;
    }
    // JSON strings are valid YAML strings, with all the escaping taken care of.
    let quoted = |s: &str| serde_json::Value::from(s).to_string();
    let mut out = format!("---\ntitle: {}\n", quoted(&book.title));
    if let Some(author) = &book.author {
        out.push_str(&format!("author: {}\n", quoted(author)));
    }
    out.push_str(&format!(
        "highlights: {}\nsource: AiReader\ntags:\n  - book\n---\n\n",
        book.annotations.highlights.len()
    ));
    out + &body
}

/// A CSV field, quoted when it needs to be.
fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// One row per highlight and bookmark. Starts with a byte-order mark, without which Excel
/// misreads non-ASCII text.
pub fn to_csv(books: &[BookNotes]) -> String {
    let mut out = String::from("\u{feff}book,author,chapter,type,text,note,color,created\n");
    let mut row = |cells: [&str; 8]| {
        let line: Vec<String> = cells.iter().map(|c| field(c)).collect();
        out.push_str(&line.join(","));
        out.push('\n');
    };
    for b in books {
        let author = b.author.as_deref().unwrap_or("");
        for h in &b.annotations.highlights {
            row([
                &b.title,
                author,
                &annotations::chapter_name(&b.chapters, h.chapter),
                "highlight",
                &h.text,
                h.note.as_deref().unwrap_or(""),
                &h.color,
                &library::date(h.created),
            ]);
        }
        for n in &b.annotations.notes {
            row([
                &b.title,
                author,
                "",
                "note",
                &n.title,
                &n.text,
                "",
                &library::date(n.created),
            ]);
        }
        for m in &b.annotations.bookmarks {
            row([
                &b.title,
                author,
                &m.title,
                "bookmark",
                &m.excerpt,
                "",
                "",
                &library::date(m.created),
            ]);
        }
    }
    out
}

/// Flashcards as a spreadsheet.
pub fn cards_csv(cards: &[crate::cards::Card]) -> String {
    let mut out = String::from("\u{feff}front,back,context,book,reps,due\n");
    for c in cards {
        let line = [
            field(&c.front),
            field(&c.back),
            field(&c.context),
            field(&c.book_title),
            c.reps.to_string(),
            library::date(c.due),
        ];
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

pub fn to_json(books: &[BookNotes]) -> Result<String> {
    let list: Vec<serde_json::Value> = books
        .iter()
        .map(|b| {
            let highlights: Vec<serde_json::Value> = b
                .annotations
                .highlights
                .iter()
                .map(|h| {
                    json!({
                        "chapter": annotations::chapter_name(&b.chapters, h.chapter),
                        "text": h.text,
                        "note": h.note,
                        "color": h.color,
                        "created": library::date(h.created),
                    })
                })
                .collect();
            let bookmarks: Vec<serde_json::Value> = b
                .annotations
                .bookmarks
                .iter()
                .map(|m| {
                    json!({
                        "title": m.title,
                        "excerpt": m.excerpt,
                        "created": library::date(m.created),
                    })
                })
                .collect();
            let notes: Vec<serde_json::Value> = b
                .annotations
                .notes
                .iter()
                .map(|n| {
                    json!({
                        "title": n.title,
                        "text": n.text,
                        "created": library::date(n.created),
                    })
                })
                .collect();
            json!({
                "title": b.title,
                "author": b.author,
                "notes": notes,
                "highlights": highlights,
                "bookmarks": bookmarks,
            })
        })
        .collect();
    Ok(serde_json::to_string_pretty(&list)?)
}

/// A file name for `title` that is safe on every system and not yet in `taken`.
pub fn file_name(title: &str, extension: &str, taken: &mut HashSet<String>) -> String {
    let clean: String = title
        .chars()
        .map(|c| {
            if c.is_control() || r#"/\:*?"<>|"#.contains(c) {
                ' '
            } else {
                c
            }
        })
        .take(100)
        .collect();
    let clean = clean.trim().trim_matches('.').trim();
    let stem = if clean.is_empty() { "book" } else { clean };
    let mut name = format!("{stem}.{extension}");
    let mut n = 2;
    // Compared without case: macOS and Windows treat "A.md" and "a.md" as one file.
    while !taken.insert(name.to_lowercase()) {
        name = format!("{stem} ({n}).{extension}");
        n += 1;
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{Bookmark, Highlight};
    use crate::model::Location;

    fn book() -> BookNotes {
        BookNotes {
            title: "三国, \"演义\"".into(),
            author: Some("罗贯中".into()),
            chapters: vec!["第一回".into()],
            annotations: Annotations {
                bookmarks: vec![Bookmark {
                    id: "m".into(),
                    location: Location::Page { page: 1 },
                    title: "第 2 页".into(),
                    excerpt: "摘录".into(),
                    created: 86_400,
                    updated: 0,
                }],
                highlights: vec![Highlight {
                    id: "h".into(),
                    chapter: 0,
                    start_block: 0,
                    start: 0,
                    end_block: 0,
                    end: 2,
                    text: "滚滚\n长江".into(),
                    color: "yellow".into(),
                    note: Some("想法".into()),
                    created: 0,
                    updated: 0,
                }],
                notes: vec![crate::annotations::BookNote {
                    id: "n".into(),
                    title: "主题".into(),
                    text: "分久必合".into(),
                    created: 0,
                    updated: 0,
                }],
            },
        }
    }

    #[test]
    fn obsidian_notes_start_with_escaped_properties() {
        let md = book_markdown(&book(), NotesFormat::Obsidian);
        assert!(md.starts_with("---\ntitle: \"三国, \\\"演义\\\"\"\nauthor: \"罗贯中\"\n"));
        assert!(md.contains("highlights: 1\n"));
        assert!(md.contains("> 滚滚\n> 长江"));
        assert!(!book_markdown(&book(), NotesFormat::Markdown).starts_with("---"));
    }

    #[test]
    fn csv_quotes_fields_and_lists_both_kinds() {
        let csv = to_csv(&[book()]);
        let mut lines = csv.split('\n');
        assert_eq!(
            lines.next(),
            Some("\u{feff}book,author,chapter,type,text,note,color,created")
        );
        assert!(csv.contains(
            "\"三国, \"\"演义\"\"\",罗贯中,第一回,highlight,\"滚滚\n长江\",想法,yellow,1970-01-01"
        ));
        assert!(csv.contains(",第 2 页,bookmark,摘录,,,1970-01-02\n"));
        assert!(csv.contains(",罗贯中,,note,主题,分久必合,,1970-01-01\n"));
    }

    #[test]
    fn json_names_chapters() {
        let v: serde_json::Value = serde_json::from_str(&to_json(&[book()]).unwrap()).unwrap();
        assert_eq!(v[0]["highlights"][0]["chapter"], "第一回");
        assert_eq!(v[0]["bookmarks"][0]["excerpt"], "摘录");
        assert_eq!(v[0]["notes"][0]["text"], "分久必合");
    }

    #[test]
    fn file_names_are_safe_and_unique() {
        let mut taken = HashSet::new();
        assert_eq!(file_name("a/b: c?", "md", &mut taken), "a b  c.md");
        assert_eq!(file_name("Same", "md", &mut taken), "Same.md");
        assert_eq!(file_name("same", "md", &mut taken), "same (2).md");
        assert_eq!(file_name("...", "md", &mut taken), "book.md");
    }
}
