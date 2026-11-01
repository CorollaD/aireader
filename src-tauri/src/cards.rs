//! Flashcards, scheduled with the FSRS spaced-repetition algorithm: words and passages saved
//! while reading, shown again just before they would be forgotten.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use rs_fsrs::{FSRS, Rating};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::model::Location;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS cards (
    id TEXT PRIMARY KEY,
    book_id TEXT,
    book_title TEXT NOT NULL,
    front TEXT NOT NULL,
    back TEXT NOT NULL,
    context TEXT NOT NULL,
    location TEXT,
    created INTEGER NOT NULL,
    suspended INTEGER NOT NULL DEFAULT 0,
    due INTEGER NOT NULL,
    reps INTEGER NOT NULL DEFAULT 0,
    lapses INTEGER NOT NULL DEFAULT 0,
    memory TEXT NOT NULL,
    updated INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS cards_due ON cards (due);
CREATE INDEX IF NOT EXISTS cards_book ON cards (book_id);
CREATE TABLE IF NOT EXISTS card_reviews (
    card_id TEXT NOT NULL,
    rating INTEGER NOT NULL,
    reviewed INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS card_reviews_time ON card_reviews (reviewed);
";

const COLUMNS: &str = "id, book_id, book_title, front, back, context, location, created, suspended, due, reps, lapses";

/// A flashcard. `book_id` is cleared if the book leaves the shelf; the card stays.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: String,
    pub book_id: Option<String>,
    pub book_title: String,
    pub front: String,
    pub back: String,
    /// The sentence the card came from.
    pub context: String,
    pub location: Option<Location>,
    pub created: u64,
    pub suspended: bool,
    /// Unix seconds.
    pub due: u64,
    pub reps: i32,
    pub lapses: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCard {
    pub id: String,
    pub book_id: Option<String>,
    pub book_title: String,
    pub front: String,
    pub back: String,
    pub context: String,
    pub location: Option<Location>,
}

/// Editable fields; `None` leaves a field unchanged.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardPatch {
    pub front: Option<String>,
    pub back: Option<String>,
    pub suspended: Option<bool>,
}

#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub total: u64,
    pub due: u64,
    /// Cards reviewed since `today`.
    pub reviewed_today: u64,
}

fn time(secs: u64) -> DateTime<Utc> {
    DateTime::from_timestamp(secs as i64, 0).unwrap_or_default()
}

fn rating(n: u8) -> Result<Rating> {
    Ok(match n {
        1 => Rating::Again,
        2 => Rating::Hard,
        3 => Rating::Good,
        4 => Rating::Easy,
        _ => bail!("unknown rating: {n}"),
    })
}

fn card(r: &Row) -> rusqlite::Result<Card> {
    let location: Option<String> = r.get(6)?;
    Ok(Card {
        id: r.get(0)?,
        book_id: r.get(1)?,
        book_title: r.get(2)?,
        front: r.get(3)?,
        back: r.get(4)?,
        context: r.get(5)?,
        location: location.and_then(|l| serde_json::from_str(&l).ok()),
        created: r.get::<_, i64>(7)? as u64,
        suspended: r.get(8)?,
        due: r.get::<_, i64>(9)? as u64,
        reps: r.get(10)?,
        lapses: r.get(11)?,
    })
}

/// Saves a new card, due at once. A book's card for the same text is kept instead of being
/// duplicated: that one is returned.
pub fn add(conn: &Connection, new: NewCard, now: u64) -> Result<Card> {
    if let Some(existing) = find(conn, new.book_id.as_deref(), &new.front)? {
        return Ok(existing);
    }
    let memory = rs_fsrs::Card {
        due: time(now),
        ..rs_fsrs::Card::new()
    };
    conn.execute(
        "INSERT INTO cards (id, book_id, book_title, front, back, context, location, created, due, memory, updated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?8)",
        params![
            new.id,
            new.book_id,
            new.book_title,
            new.front.trim(),
            new.back.trim(),
            new.context,
            new.location.map(|l| serde_json::to_string(&l)).transpose()?,
            now as i64,
            now as i64,
            serde_json::to_string(&memory)?,
        ],
    )?;
    get(conn, &new.id)
}

pub fn get(conn: &Connection, id: &str) -> Result<Card> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM cards WHERE id = ?1"),
        params![id],
        card,
    )
    .optional()?
    .with_context(|| format!("no such card: {id}"))
}

/// The book's card for `front`, ignoring case.
pub fn find(conn: &Connection, book_id: Option<&str>, front: &str) -> Result<Option<Card>> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {COLUMNS} FROM cards WHERE book_id IS ?1 AND lower(front) = lower(?2)"
            ),
            params![book_id, front.trim()],
            card,
        )
        .optional()?)
}

/// Cards in the order they were added, newest first: of one book, or all.
pub fn list(conn: &Connection, book_id: Option<&str>) -> Result<Vec<Card>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM cards WHERE ?1 IS NULL OR book_id = ?1 ORDER BY created DESC, rowid DESC"
    ))?;
    let cards = stmt
        .query_map(params![book_id], card)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(cards)
}

/// Cards to review now, longest overdue first.
pub fn due(conn: &Connection, now: u64) -> Result<Vec<Card>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM cards WHERE suspended = 0 AND due <= ?1 ORDER BY due, rowid"
    ))?;
    let cards = stmt
        .query_map(params![now as i64], card)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(cards)
}

/// Counts for the review screen. `today` is the Unix time the user's day began.
pub fn stats(conn: &Connection, now: u64, today: u64) -> Result<Stats> {
    let count = |sql: &str, at: u64| -> Result<u64> {
        Ok(conn.query_row(sql, params![at as i64], |r| r.get::<_, i64>(0))? as u64)
    };
    let total: i64 = conn.query_row("SELECT count(*) FROM cards", [], |r| r.get(0))?;
    Ok(Stats {
        total: total as u64,
        due: count(
            "SELECT count(*) FROM cards WHERE suspended = 0 AND due <= ?1",
            now,
        )?,
        reviewed_today: count(
            "SELECT count(DISTINCT card_id) FROM card_reviews WHERE reviewed >= ?1",
            today,
        )?,
    })
}

pub fn update(conn: &Connection, id: &str, patch: CardPatch) -> Result<Card> {
    let old = get(conn, id)?;
    conn.execute(
        "UPDATE cards SET front = ?2, back = ?3, suspended = ?4, updated = ?5 WHERE id = ?1",
        params![
            id,
            patch.front.as_deref().map_or(old.front.as_str(), str::trim),
            patch.back.as_deref().map_or(old.back.as_str(), str::trim),
            patch.suspended.unwrap_or(old.suspended),
            crate::library::now() as i64,
        ],
    )?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM card_reviews WHERE card_id = ?1", params![id])?;
    conn.execute("DELETE FROM cards WHERE id = ?1", params![id])?;
    crate::db::tombstone(conn, "card", id, crate::library::now())
}

fn memory(conn: &Connection, id: &str) -> Result<rs_fsrs::Card> {
    let json: String = conn
        .query_row("SELECT memory FROM cards WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .optional()?
        .with_context(|| format!("no such card: {id}"))?;
    Ok(serde_json::from_str(&json)?)
}

/// Seconds until the card would come back after each answer, from Again (1) to Easy (4).
pub fn intervals(conn: &Connection, id: &str, now: u64) -> Result<[u64; 4]> {
    let at = time(now);
    let next = FSRS::default().repeat(memory(conn, id)?, at);
    let mut out = [0; 4];
    for (i, n) in (1..=4u8).enumerate() {
        let info = &next[&rating(n)?];
        out[i] = (info.card.due - at).num_seconds().max(0) as u64;
    }
    Ok(out)
}

/// Records an answer and schedules the card's next review.
pub fn review(conn: &Connection, id: &str, answer: u8, now: u64) -> Result<Card> {
    let next = FSRS::default().next(memory(conn, id)?, time(now), rating(answer)?);
    conn.execute(
        "UPDATE cards SET due = ?2, reps = ?3, lapses = ?4, memory = ?5, updated = ?6 WHERE id = ?1",
        params![
            id,
            next.card.due.timestamp(),
            next.card.reps,
            next.card.lapses,
            serde_json::to_string(&next.card)?,
            now as i64,
        ],
    )?;
    conn.execute(
        "INSERT INTO card_reviews (card_id, rating, reviewed) VALUES (?1, ?2, ?3)",
        params![id, answer, now as i64],
    )?;
    get(conn, id)
}

/// A card as it is synced: everything, including what the scheduler remembers about it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncCard {
    pub id: String,
    pub book_id: Option<String>,
    pub book_title: String,
    pub front: String,
    pub back: String,
    pub context: String,
    pub location: Option<String>,
    pub created: i64,
    pub suspended: bool,
    pub due: i64,
    pub reps: i32,
    pub lapses: i32,
    pub memory: String,
    pub updated: i64,
}

/// One answer given to a card.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReview {
    pub card_id: String,
    pub rating: i64,
    pub reviewed: i64,
}

pub fn sync_cards(conn: &Connection) -> Result<Vec<SyncCard>> {
    let mut stmt = conn.prepare(
        "SELECT id, book_id, book_title, front, back, context, location, created, suspended,
                due, reps, lapses, memory, updated FROM cards ORDER BY id",
    )?;
    let cards = stmt
        .query_map([], |r| {
            Ok(SyncCard {
                id: r.get(0)?,
                book_id: r.get(1)?,
                book_title: r.get(2)?,
                front: r.get(3)?,
                back: r.get(4)?,
                context: r.get(5)?,
                location: r.get(6)?,
                created: r.get(7)?,
                suspended: r.get(8)?,
                due: r.get(9)?,
                reps: r.get(10)?,
                lapses: r.get(11)?,
                memory: r.get(12)?,
                updated: r.get(13)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(cards)
}

pub fn sync_reviews(conn: &Connection) -> Result<Vec<SyncReview>> {
    let mut stmt =
        conn.prepare("SELECT card_id, rating, reviewed FROM card_reviews ORDER BY 1, 3, 2")?;
    let reviews = stmt
        .query_map([], |r| {
            Ok(SyncReview {
                card_id: r.get(0)?,
                rating: r.get(1)?,
                reviewed: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(reviews)
}

/// Replaces a card with a newer copy from another device.
pub fn store_synced(conn: &Connection, c: &SyncCard) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO cards (id, book_id, book_title, front, back, context, location,
           created, suspended, due, reps, lapses, memory, updated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            c.id,
            c.book_id,
            c.book_title,
            c.front,
            c.back,
            c.context,
            c.location,
            c.created,
            c.suspended,
            c.due,
            c.reps,
            c.lapses,
            c.memory,
            c.updated
        ],
    )?;
    Ok(())
}

/// Adds an answer given on another device, once.
pub fn store_review(conn: &Connection, r: &SyncReview) -> Result<()> {
    conn.execute(
        "INSERT INTO card_reviews (card_id, rating, reviewed)
         SELECT ?1, ?2, ?3 WHERE NOT EXISTS (
           SELECT 1 FROM card_reviews WHERE card_id = ?1 AND rating = ?2 AND reviewed = ?3)",
        params![r.card_id, r.rating, r.reviewed],
    )?;
    Ok(())
}

/// A book left the shelf: its cards stay, under the title they were saved with.
pub fn detach_book(conn: &Connection, book_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE cards SET book_id = NULL, location = NULL WHERE book_id = ?1",
        params![book_id],
    )?;
    Ok(())
}

/// Moves cards to another copy of the book. Positions differ between copies, so the card keeps
/// its sentence but not its place.
pub fn move_book(conn: &Connection, from: &str, to: &str) -> Result<()> {
    conn.execute(
        "UPDATE cards SET book_id = ?2, location = NULL WHERE book_id = ?1",
        params![from, to],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;
    const T0: u64 = 1_800_000_000;

    fn db(name: &str) -> crate::db::Db {
        let dir =
            std::env::temp_dir().join(format!("aireader-cards-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::db::Db::open(&dir.join("t.db")).unwrap()
    }

    fn new(id: &str, front: &str) -> NewCard {
        NewCard {
            id: id.into(),
            book_id: Some("b".into()),
            book_title: "三国".into(),
            front: front.into(),
            back: "释义".into(),
            context: "原句".into(),
            location: Some(Location::Page { page: 3 }),
        }
    }

    #[test]
    fn new_cards_are_due_and_not_duplicated() {
        let db = db("add");
        let conn = db.conn();
        let c = add(&conn, new("1", " Serendipity "), T0).unwrap();
        assert_eq!(c.front, "Serendipity");
        assert_eq!(c.location, Some(Location::Page { page: 3 }));
        let again = add(&conn, new("2", "serendipity"), T0).unwrap();
        assert_eq!(again.id, "1", "same word in the same book");
        assert_eq!(list(&conn, None).unwrap().len(), 1);
        assert_eq!(due(&conn, T0).unwrap().len(), 1);
        assert!(find(&conn, Some("b"), "SERENDIPITY").unwrap().is_some());
        assert!(find(&conn, Some("other"), "serendipity").unwrap().is_none());
    }

    #[test]
    fn answers_set_when_a_card_returns() {
        let db = db("review");
        let conn = db.conn();
        add(&conn, new("1", "word"), T0).unwrap();
        let gaps = intervals(&conn, "1", T0).unwrap();
        assert!(
            gaps.windows(2).all(|w| w[0] <= w[1]),
            "harder answers come back sooner: {gaps:?}"
        );
        assert!(gaps[0] < DAY && gaps[3] >= DAY, "{gaps:?}");

        let c = review(&conn, "1", 4, T0).unwrap();
        assert_eq!(c.reps, 1);
        assert!(c.due >= T0 + DAY);
        assert!(due(&conn, T0).unwrap().is_empty());
        assert_eq!(due(&conn, c.due).unwrap().len(), 1);

        // Reviewing on time stretches the interval; forgetting shrinks it.
        let first = c.due - T0;
        let good = intervals(&conn, "1", c.due).unwrap();
        assert!(good[2] > first, "{good:?} after {first}");
        let lapsed = review(&conn, "1", 1, c.due).unwrap();
        assert_eq!(lapsed.lapses, 1);
        assert!(lapsed.due - c.due < DAY);
        assert!(review(&conn, "1", 9, c.due).is_err());
    }

    #[test]
    fn stats_count_todays_reviews() {
        let db = db("stats");
        let conn = db.conn();
        add(&conn, new("1", "a"), T0).unwrap();
        add(&conn, new("2", "b"), T0).unwrap();
        review(&conn, "1", 1, T0).unwrap();
        review(&conn, "1", 3, T0 + 60).unwrap();
        let s = stats(&conn, T0, T0 - 3600).unwrap();
        assert_eq!(
            s,
            Stats {
                total: 2,
                due: 1,
                reviewed_today: 1
            }
        );
        assert_eq!(stats(&conn, T0, T0 + DAY).unwrap().reviewed_today, 0);
    }

    #[test]
    fn suspended_cards_wait_and_edits_keep_the_schedule() {
        let db = db("edit");
        let conn = db.conn();
        add(&conn, new("1", "a"), T0).unwrap();
        let c = update(
            &conn,
            "1",
            CardPatch {
                back: Some(" 新释义 ".into()),
                suspended: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((c.front.as_str(), c.back.as_str()), ("a", "新释义"));
        assert!(due(&conn, T0).unwrap().is_empty());
        delete(&conn, "1").unwrap();
        assert!(list(&conn, None).unwrap().is_empty());
    }

    #[test]
    fn cards_outlive_their_book() {
        let db = db("book");
        let conn = db.conn();
        add(&conn, new("1", "a"), T0).unwrap();
        add(&conn, new("2", "b"), T0).unwrap();
        move_book(&conn, "b", "keep").unwrap();
        assert_eq!(list(&conn, Some("keep")).unwrap().len(), 2);
        assert_eq!(get(&conn, "1").unwrap().location, None);
        crate::db::delete_book_data(&conn, "keep").unwrap();
        let c = get(&conn, "1").unwrap();
        assert_eq!((c.book_id, c.book_title.as_str()), (None, "三国"));
    }
}
