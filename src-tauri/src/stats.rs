//! Reading statistics: time read per day, per book and per device. Each device only ever adds
//! to its own rows, so totals from several devices can be added up without counting twice.

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{Connection, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS reading_days (
    device TEXT NOT NULL,
    day TEXT NOT NULL,
    book_id TEXT NOT NULL,
    title TEXT NOT NULL,
    seconds INTEGER NOT NULL,
    PRIMARY KEY (device, day, book_id)
);
CREATE INDEX IF NOT EXISTS reading_days_day ON reading_days (day);
";

const DEVICE: &str = "device_id";
const TOP_BOOKS: usize = 10;

/// This installation's id, made on first use.
pub fn device_id(conn: &Connection) -> Result<String> {
    if let Some(id) = crate::db::kv_get(conn, DEVICE)? {
        return Ok(id);
    }
    let seed = format!("{}-{}", crate::library::now_millis(), std::process::id());
    let id: String = Sha256::digest(seed.as_bytes())
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect();
    crate::db::kv_set(conn, DEVICE, &id)?;
    Ok(id)
}

/// Adds reading time to a day (`YYYY-MM-DD`, the reader's local date). The title is kept so the
/// numbers stay readable after the book leaves the shelf.
pub fn record(
    conn: &Connection,
    day: &str,
    book_id: &str,
    title: &str,
    seconds: u64,
) -> Result<()> {
    if parse_day(day).is_none() {
        anyhow::bail!("invalid day: {day}");
    }
    conn.execute(
        "INSERT INTO reading_days (device, day, book_id, title, seconds) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(device, day, book_id) DO UPDATE SET
           seconds = seconds + excluded.seconds, title = excluded.title",
        params![device_id(conn)?, day, book_id, title, seconds as i64],
    )?;
    Ok(())
}

/// Moves reading time to another copy of the book.
pub fn move_book(conn: &Connection, from: &str, to: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO reading_days (device, day, book_id, title, seconds)
         SELECT device, day, ?2, title, seconds FROM reading_days WHERE book_id = ?1
         ON CONFLICT(device, day, book_id) DO UPDATE SET seconds = seconds + excluded.seconds",
        params![from, to],
    )?;
    conn.execute("DELETE FROM reading_days WHERE book_id = ?1", params![from])?;
    Ok(())
}

/// A day's reading of a book on one device, as it is synced.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDay {
    pub day: String,
    pub book_id: String,
    pub title: String,
    pub seconds: i64,
}

/// This device's rows: other devices' are theirs to publish.
pub fn own_days(conn: &Connection) -> Result<Vec<SyncDay>> {
    let mut stmt = conn.prepare(
        "SELECT day, book_id, title, seconds FROM reading_days WHERE device = ?1 ORDER BY 1, 2",
    )?;
    let days = stmt
        .query_map(params![device_id(conn)?], |r| {
            Ok(SyncDay {
                day: r.get(0)?,
                book_id: r.get(1)?,
                title: r.get(2)?,
                seconds: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(days)
}

/// Replaces another device's rows with what it published.
pub fn store_device(conn: &Connection, device: &str, days: &[SyncDay]) -> Result<()> {
    for d in days.iter().filter(|d| parse_day(&d.day).is_some()) {
        conn.execute(
            "INSERT OR REPLACE INTO reading_days (device, day, book_id, title, seconds)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![device, d.day, d.book_id, d.title, d.seconds],
        )?;
    }
    Ok(())
}

fn parse_day(day: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()
}

#[derive(Debug, PartialEq, Serialize)]
pub struct DayTotal {
    pub day: String,
    pub seconds: u64,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookTotal {
    pub book_id: String,
    pub title: String,
    pub seconds: u64,
}

/// Everything the statistics page shows.
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Consecutive days up to today (or yesterday: today may still come).
    pub streak: u32,
    pub longest_streak: u32,
    pub today: u64,
    pub last_7_days: u64,
    pub last_30_days: u64,
    /// Years that have any reading, newest first.
    pub years: Vec<i32>,
    /// The chosen year's reading days, in order.
    pub days: Vec<DayTotal>,
    pub year_seconds: u64,
    /// The chosen year's most-read books.
    pub books: Vec<BookTotal>,
    pub highlights: u64,
    pub cards_reviewed: u64,
}

/// (Current, longest) runs of consecutive days. `days` is sorted and has no repeats.
pub fn streaks(days: &[NaiveDate], today: NaiveDate) -> (u32, u32) {
    let (mut longest, mut run) = (0, 0);
    let mut previous: Option<NaiveDate> = None;
    for &d in days {
        run = match previous {
            Some(p) if d - p == Duration::days(1) => run + 1,
            _ => 1,
        };
        longest = longest.max(run);
        previous = Some(d);
    }
    // The run at the end counts only if it reaches today or yesterday.
    let current = match days.last() {
        Some(&last) if today - last <= Duration::days(1) => run,
        _ => 0,
    };
    (current, longest)
}

/// The report for `today` (the reader's local date) and `year`.
pub fn report(conn: &Connection, today: &str, year: i32) -> Result<Report> {
    let date = parse_day(today).ok_or_else(|| anyhow::anyhow!("invalid day: {today}"))?;
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT day, sum(seconds) FROM reading_days GROUP BY day")?;
    for row in stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
    })? {
        let (day, seconds) = row?;
        totals.insert(day, seconds);
    }
    let dates: Vec<NaiveDate> = totals.keys().filter_map(|d| parse_day(d)).collect();
    let (streak, longest_streak) = streaks(&dates, date);
    let since = |days: i64| {
        let from = (date - Duration::days(days - 1)).to_string();
        totals.range(from..).map(|(_, s)| s).sum::<u64>()
    };

    let mut years: Vec<i32> = dates.iter().map(|d| d.year()).collect();
    years.dedup();
    years.reverse();

    let days: Vec<DayTotal> = totals
        .iter()
        .filter(|(d, _)| parse_day(d).is_some_and(|d| d.year() == year))
        .map(|(day, &seconds)| DayTotal {
            day: day.clone(),
            seconds,
        })
        .collect();

    let pattern = format!("{year:04}-%");
    let mut stmt = conn.prepare(
        "SELECT book_id, title, sum(seconds) AS total FROM reading_days WHERE day LIKE ?1
         GROUP BY book_id ORDER BY total DESC, book_id LIMIT ?2",
    )?;
    let books = stmt
        .query_map(params![pattern, TOP_BOOKS as i64], |r| {
            Ok(BookTotal {
                book_id: r.get(0)?,
                title: r.get(1)?,
                seconds: r.get::<_, i64>(2)? as u64,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let in_year = |sql: &str| -> Result<u64> {
        Ok(conn.query_row(sql, params![format!("{year:04}")], |r| r.get::<_, i64>(0))? as u64)
    };
    Ok(Report {
        streak,
        longest_streak,
        today: totals.get(today).copied().unwrap_or(0),
        last_7_days: since(7),
        last_30_days: since(30),
        years,
        year_seconds: days.iter().map(|d| d.seconds).sum(),
        days,
        books,
        highlights: in_year(
            "SELECT count(*) FROM highlights
             WHERE strftime('%Y', created, 'unixepoch', 'localtime') = ?1",
        )?,
        cards_reviewed: in_year(
            "SELECT count(*) FROM card_reviews
             WHERE strftime('%Y', reviewed, 'unixepoch', 'localtime') = ?1",
        )?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(name: &str) -> crate::db::Db {
        let dir =
            std::env::temp_dir().join(format!("aireader-stats-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::db::Db::open(&dir.join("t.db")).unwrap()
    }

    fn day(s: &str) -> NaiveDate {
        parse_day(s).unwrap()
    }

    #[test]
    fn streaks_end_today_or_yesterday() {
        let days: Vec<NaiveDate> = [
            "2026-03-01",
            "2026-03-02",
            "2026-03-03",
            "2026-03-10",
            "2026-03-11",
        ]
        .map(day)
        .to_vec();
        assert_eq!(streaks(&days, day("2026-03-11")), (2, 3));
        assert_eq!(
            streaks(&days, day("2026-03-12")),
            (2, 3),
            "today may still come"
        );
        assert_eq!(streaks(&days, day("2026-03-13")), (0, 3), "broken");
        assert_eq!(streaks(&[], day("2026-03-13")), (0, 0));
        // Across a month and a leap day.
        let leap = ["2028-02-28", "2028-02-29", "2028-03-01"].map(day).to_vec();
        assert_eq!(streaks(&leap, day("2028-03-01")), (3, 3));
    }

    #[test]
    fn report_adds_up_days_books_and_devices() {
        let db = db("report");
        let conn = db.conn();
        record(&conn, "2026-03-09", "a", "甲", 600).unwrap();
        record(&conn, "2026-03-10", "a", "甲", 300).unwrap();
        record(&conn, "2026-03-10", "a", "甲", 100).unwrap();
        record(&conn, "2026-03-10", "b", "乙", 60).unwrap();
        record(&conn, "2025-12-31", "b", "乙", 7200).unwrap();
        // Another device's row, as sync will bring it.
        conn.execute(
            "INSERT INTO reading_days VALUES ('other', '2026-03-10', 'a', '甲', 40)",
            [],
        )
        .unwrap();

        let r = report(&conn, "2026-03-10", 2026).unwrap();
        assert_eq!((r.streak, r.longest_streak), (2, 2));
        assert_eq!(r.today, 500);
        assert_eq!((r.last_7_days, r.last_30_days), (1100, 1100));
        assert_eq!(r.years, [2026, 2025]);
        assert_eq!(r.year_seconds, 1100);
        assert_eq!(r.days.len(), 2);
        assert_eq!(r.books[0].title, "甲");
        assert_eq!(r.books[0].seconds, 1040);
        assert_eq!(
            report(&conn, "2026-03-10", 2025).unwrap().year_seconds,
            7200
        );
        assert!(record(&conn, "soon", "a", "甲", 1).is_err());
    }

    #[test]
    fn the_device_id_stays_and_merged_copies_keep_their_time() {
        let db = db("device");
        let conn = db.conn();
        let id = device_id(&conn).unwrap();
        assert_eq!(id.len(), 16);
        assert_eq!(device_id(&conn).unwrap(), id);
        record(&conn, "2026-03-10", "copy", "甲", 100).unwrap();
        record(&conn, "2026-03-10", "keep", "甲", 50).unwrap();
        move_book(&conn, "copy", "keep").unwrap();
        let r = report(&conn, "2026-03-10", 2026).unwrap();
        assert_eq!(r.books.len(), 1);
        assert_eq!(
            (r.books[0].book_id.as_str(), r.books[0].seconds),
            ("keep", 150)
        );
    }
}
