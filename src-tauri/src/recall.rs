//! A few old highlights brought back each day, so what was marked isn't forgotten. Unlike
//! flashcards there is nothing to get right: each is either kept, to return later and later, or
//! dismissed for good.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::model::Location;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS highlight_reviews (
    highlight_id TEXT PRIMARY KEY,
    due INTEGER NOT NULL,
    interval_days INTEGER NOT NULL,
    retired INTEGER NOT NULL DEFAULT 0,
    last_seen INTEGER NOT NULL
);
";

/// How many highlights a day.
pub const DAILY: u64 = 5;
const DAY: u64 = 86_400;
const FIRST_INTERVAL: i64 = 7;
const MAX_INTERVAL: i64 = 180;

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyHighlight {
    pub id: String,
    pub book_id: String,
    pub book_title: String,
    pub text: String,
    pub note: Option<String>,
    pub color: String,
    pub location: Location,
}

/// What is left of today's highlights. `today` is when the reader's day began. Highlights made
/// in the last day are left out: they are still fresh.
pub fn today(conn: &Connection, now: u64, today: u64) -> Result<Vec<DailyHighlight>> {
    let seen: i64 = conn.query_row(
        "SELECT count(*) FROM highlight_reviews WHERE last_seen >= ?1",
        params![today as i64],
        |r| r.get(0),
    )?;
    let left = DAILY as i64 - seen;
    if left <= 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT h.id, h.book_id, b.title, h.text, h.note, h.color, h.chapter, h.start_block
         FROM highlights h
         JOIN books b ON b.id = h.book_id
         LEFT JOIN highlight_reviews r ON r.highlight_id = h.id
         WHERE h.created < ?1 AND coalesce(r.retired, 0) = 0 AND coalesce(r.due, 0) <= ?2
         ORDER BY random() LIMIT ?3",
    )?;
    let list = stmt
        .query_map(params![(now - DAY) as i64, now as i64, left], |r| {
            Ok(DailyHighlight {
                id: r.get(0)?,
                book_id: r.get(1)?,
                book_title: r.get(2)?,
                text: r.get(3)?,
                note: r.get(4)?,
                color: r.get(5)?,
                location: Location::Flow {
                    chapter: r.get::<_, i64>(6)? as usize,
                    block: r.get::<_, i64>(7)? as usize,
                },
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(list)
}

/// Records that a highlight was seen: kept ones return later each time.
pub fn seen(conn: &Connection, id: &str, keep: bool, now: u64) -> Result<()> {
    let interval: Option<i64> = conn
        .query_row(
            "SELECT interval_days FROM highlight_reviews WHERE highlight_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    let days = interval.map_or(FIRST_INTERVAL, |d| (d * 5 / 2).min(MAX_INTERVAL));
    conn.execute(
        "INSERT OR REPLACE INTO highlight_reviews (highlight_id, due, interval_days, retired, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            id,
            now as i64 + days * DAY as i64,
            days,
            !keep,
            now as i64
        ],
    )?;
    Ok(())
}

/// Forgets highlights that no longer exist.
pub fn clean(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM highlight_reviews WHERE highlight_id NOT IN (SELECT id FROM highlights)",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{self, Annotations, Highlight};

    const NOW: u64 = 1_800_000_000;

    fn setup(name: &str, count: usize) -> crate::db::Db {
        let dir =
            std::env::temp_dir().join(format!("aireader-recall-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::Db::open(&dir.join("t.db")).unwrap();
        {
            let conn = db.conn();
            conn.execute(
                "INSERT INTO books (id, position, sha256, title, data) VALUES ('b', 0, 's', '三国', '{}')",
                [],
            )
            .unwrap();
            let highlight = |i: usize, created: u64| Highlight {
                id: format!("h{i}"),
                chapter: 1,
                start_block: i,
                start: 0,
                end_block: i,
                end: 2,
                text: format!("划线{i}"),
                color: "yellow".into(),
                note: None,
                created,
                updated: created,
            };
            let mut highlights: Vec<Highlight> =
                (0..count).map(|i| highlight(i, NOW - 10 * DAY)).collect();
            highlights.push(highlight(99, NOW - 60));
            let ann = Annotations {
                highlights,
                ..Default::default()
            };
            annotations::write(&conn, "b", &ann).unwrap();
        }
        db
    }

    #[test]
    fn a_few_old_highlights_each_day() {
        let db = setup("daily", 8);
        let conn = db.conn();
        let start = NOW - 3600;
        let list = today(&conn, NOW, start).unwrap();
        assert_eq!(list.len() as u64, DAILY);
        assert!(list.iter().all(|h| h.id != "h99"), "not the fresh one");
        assert_eq!(list[0].book_title, "三国");
        for h in &list[..2] {
            seen(&conn, &h.id, true, NOW).unwrap();
        }
        assert_eq!(
            today(&conn, NOW, start).unwrap().len(),
            3,
            "the rest of today"
        );
        for h in &list[2..] {
            seen(&conn, &h.id, true, NOW).unwrap();
        }
        assert!(today(&conn, NOW, start).unwrap().is_empty());
        // The next day brings the ones not shown yet.
        let next = NOW + DAY;
        let more = today(&conn, next, next - 3600).unwrap();
        assert_eq!(more.len(), 4, "three left, and the one no longer fresh");
    }

    #[test]
    fn kept_highlights_return_later_and_dismissed_ones_never() {
        let db = setup("schedule", 2);
        let conn = db.conn();
        let later = NOW + 8 * DAY;
        seen(&conn, "h0", true, NOW).unwrap();
        seen(&conn, "h1", false, NOW).unwrap();
        seen(&conn, "h99", false, NOW).unwrap();
        let ids = |at: u64| -> Vec<String> {
            today(&conn, at, at)
                .unwrap()
                .into_iter()
                .map(|h| h.id)
                .collect()
        };
        assert!(ids(NOW + DAY).is_empty(), "not due yet");
        assert_eq!(ids(later), ["h0"]);
        seen(&conn, "h0", true, later).unwrap();
        let days: i64 = conn
            .query_row(
                "SELECT interval_days FROM highlight_reviews WHERE highlight_id = 'h0'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(days, 17);
        assert!(ids(later + 10 * DAY).is_empty());
        assert_eq!(ids(later + 18 * DAY), ["h0"]);
    }

    #[test]
    fn removed_highlights_are_forgotten() {
        let db = setup("clean", 1);
        let conn = db.conn();
        seen(&conn, "h0", true, NOW).unwrap();
        annotations::write(&conn, "b", &Annotations::default()).unwrap();
        clean(&conn).unwrap();
        let left: i64 = conn
            .query_row("SELECT count(*) FROM highlight_reviews", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }
}
