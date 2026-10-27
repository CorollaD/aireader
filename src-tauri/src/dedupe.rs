//! Finding the same book stored in several files (another format, another scan, another
//! source), judging which copy is best, and moving reading data from the others onto it.
//!
//! Candidates share a normalised title and compatible authors; text fingerprints then decide:
//! different translations or heavily revised editions share little text and stay apart.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::annotations::{Annotations, Bookmark, Highlight};
use crate::library::Book;
use crate::model::{Content, Document, Location};

/// Bump when `Profile` or how it is computed changes, so cached profiles are rebuilt.
pub const PROFILE_VERSION: u32 = 1;

/// Fingerprint size: the smallest hashes of the book's 5-character shingles.
const SKETCH: usize = 256;
const SHINGLE: usize = 5;
/// Estimated share of shingles two copies of the same text have in common, at least.
const SAME_TEXT: f32 = 0.4;
/// Paged books without text count as the same when their page counts are this close.
const PAGE_TOLERANCE: f32 = 0.15;
/// Pages sampled for scan resolution.
const DPI_SAMPLES: usize = 5;

// --- titles ------------------------------------------------------------------------------------

/// Source tags that download sites append to file names and titles.
const SOURCE_TAGS: &[&str] = &[
    "z-library",
    "zlibrary",
    "z-lib",
    "zlib",
    "annas-archive",
    "anna's archive",
    "libgen",
    "epubee",
];

/// A title reduced to what identifies the book: no bracketed parts ("（第2版）", "[PDF]"),
/// no source tags, no punctuation or spaces, lower case, full-width forms folded.
pub fn title_key(title: &str) -> String {
    let folded: String = title
        .chars()
        .map(|c| match c as u32 {
            0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            0x3000 => ' ',
            _ => c,
        })
        .collect::<String>()
        .to_lowercase();
    let mut out = String::new();
    let mut depth = 0usize;
    for c in folded.chars() {
        match c {
            '(' | '[' | '{' | '（' | '【' | '《' | '〈' | '「' => depth += 1,
            ')' | ']' | '}' | '）' | '】' | '》' | '〉' | '」' => {
                depth = depth.saturating_sub(1)
            }
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    // "Title - Z-Library", "Title_libgen"
    for tag in SOURCE_TAGS {
        if let Some(pos) = out.find(tag)
            && pos > 0
        {
            out.truncate(pos);
        }
    }
    let key: String = out.chars().filter(|c| c.is_alphanumeric()).collect();
    // A title that was only a bracketed phrase keeps its letters.
    if key.is_empty() {
        folded.chars().filter(|c| c.is_alphanumeric()).collect()
    } else {
        key
    }
}

fn author_key(author: Option<&str>) -> Option<String> {
    let key: String = author?
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    (!key.is_empty()).then_some(key)
}

/// Authors match when equal, when one contains the other ("Lewis Carroll" / "Carroll"), or
/// when either is unknown.
fn authors_compatible(a: Option<&str>, b: Option<&str>) -> bool {
    match (author_key(a), author_key(b)) {
        (Some(x), Some(y)) => x.contains(&y) || y.contains(&x),
        _ => true,
    }
}

/// Groups of books (indices into `books`) whose titles and authors say they may be the same
/// book. Only these need their contents compared.
pub fn candidate_groups(books: &[Book]) -> Vec<Vec<usize>> {
    let mut by_title: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, b) in books.iter().enumerate() {
        let key = title_key(&b.title);
        if key.chars().count() >= 2 {
            by_title.entry(key).or_default().push(i);
        }
    }
    let mut groups = Vec::new();
    for members in by_title.into_values().filter(|m| m.len() > 1) {
        // Split by author: each book joins the first group whose authors all fit.
        let mut split: Vec<Vec<usize>> = Vec::new();
        for i in members {
            let a = books[i].author.as_deref();
            match split.iter_mut().find(|g| {
                g.iter()
                    .all(|&j| authors_compatible(a, books[j].author.as_deref()))
            }) {
                Some(g) => g.push(i),
                None => split.push(vec![i]),
            }
        }
        groups.extend(split.into_iter().filter(|g| g.len() > 1));
    }
    groups
}

// --- profiles ----------------------------------------------------------------------------------

/// What is known about one file's contents, for comparing and ranking copies.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub version: u32,
    /// Letters and digits of text (reflowable chapters or a PDF's text layer).
    pub chars: usize,
    pub pages: Option<usize>,
    pub toc_entries: usize,
    pub images: usize,
    /// Median scan resolution of sampled pages, for paged books with pictures.
    pub dpi: Option<f32>,
    /// Sorted smallest shingle hashes; empty without text.
    pub sketch: Vec<u64>,
}

impl Profile {
    pub fn has_text(&self) -> bool {
        self.chars >= 200 && !self.sketch.is_empty()
    }
}

fn shingle_hash(chars: &[char]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    chars.hash(&mut h);
    h.finish()
}

/// The text's fingerprint: hashes of overlapping 5-character pieces, keeping the smallest.
fn sketch(text: &[char]) -> Vec<u64> {
    if text.len() < SHINGLE {
        return Vec::new();
    }
    let mut hashes: Vec<u64> = text.windows(SHINGLE).map(shingle_hash).collect();
    hashes.sort_unstable();
    hashes.dedup();
    hashes.truncate(SKETCH);
    hashes
}

/// Estimated share of shingles two texts have in common (0..1).
pub fn similarity(a: &[u64], b: &[u64]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    // Among the smallest hashes of the union, how many appear in both sketches.
    let (mut i, mut j, mut seen, mut both) = (0, 0, 0, 0);
    let k = a.len().min(b.len());
    while seen < k && (i < a.len() || j < b.len()) {
        match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) if x == y => {
                both += 1;
                i += 1;
                j += 1;
            }
            (Some(x), Some(y)) if x < y => i += 1,
            (Some(_), Some(_)) => j += 1,
            (Some(_), None) => i += 1,
            (None, _) => j += 1,
        }
        seen += 1;
    }
    both as f32 / seen.max(1) as f32
}

fn text_chars(text: &str) -> impl Iterator<Item = char> + '_ {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
}

pub fn profile(doc: &Document) -> Profile {
    let mut text: Vec<char> = Vec::new();
    let mut p = Profile {
        version: PROFILE_VERSION,
        toc_entries: doc.toc.len(),
        ..Default::default()
    };
    match &doc.content {
        Content::Flow(chapters) => {
            for ch in chapters {
                for block in &ch.blocks {
                    if matches!(block, crate::model::Block::Image(_)) {
                        p.images += 1;
                    }
                    text.extend(text_chars(&block.text()));
                }
            }
        }
        Content::Paged(src) => {
            let n = src.page_count();
            p.pages = Some(n);
            for page in 0..n {
                if let Some(t) = src.page_text(page) {
                    text.extend(text_chars(&t));
                }
            }
            let mut dpis: Vec<f32> = (0..DPI_SAMPLES)
                .map(|k| (n * (2 * k + 1)) / (2 * DPI_SAMPLES))
                .filter(|&page| page < n)
                .filter_map(|page| src.image_dpi(page))
                .collect();
            dpis.sort_by(f32::total_cmp);
            p.dpi = dpis.get(dpis.len() / 2).copied();
            p.images = dpis.len();
        }
    }
    p.chars = text.len();
    p.sketch = sketch(&text);
    p
}

/// Profiles of already analysed files (the `profiles` table), by content hash.
pub struct ProfileCache {
    db: crate::db::Db,
    profiles: HashMap<String, Profile>,
    changed: Vec<String>,
}

impl ProfileCache {
    pub fn open(db: &crate::db::Db) -> Self {
        let mut profiles = HashMap::new();
        if let Ok(mut stmt) = db.conn().prepare("SELECT sha256, data FROM profiles") {
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
            for (sha, data) in rows.into_iter().flatten().flatten() {
                if let Ok(p) = serde_json::from_str(&data) {
                    profiles.insert(sha, p);
                }
            }
        }
        Self {
            db: db.clone(),
            profiles,
            changed: Vec::new(),
        }
    }

    pub fn get(&self, sha256: &str) -> Option<&Profile> {
        self.profiles
            .get(sha256)
            .filter(|p| p.version == PROFILE_VERSION)
    }

    pub fn put(&mut self, sha256: &str, profile: Profile) {
        self.profiles.insert(sha256.to_owned(), profile);
        self.changed.push(sha256.to_owned());
    }

    /// Writes new profiles and drops those of files no longer in the library.
    pub fn save(&mut self, keep: &[String]) -> anyhow::Result<()> {
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        for sha in self.changed.drain(..) {
            if let Some(p) = self.profiles.get(&sha) {
                tx.execute(
                    "INSERT OR REPLACE INTO profiles (sha256, data) VALUES (?1, ?2)",
                    rusqlite::params![sha, serde_json::to_string(p)?],
                )?;
            }
        }
        let stale: Vec<String> = self
            .profiles
            .keys()
            .filter(|sha| !keep.contains(sha))
            .cloned()
            .collect();
        for sha in stale {
            tx.execute("DELETE FROM profiles WHERE sha256 = ?1", [&sha])?;
            self.profiles.remove(&sha);
        }
        tx.commit()?;
        Ok(())
    }
}

// --- grouping and ranking ----------------------------------------------------------------------

/// One copy in a duplicate group, as shown to the user.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Copy {
    pub id: String,
    pub score: f32,
    pub chars: usize,
    pub pages: Option<usize>,
    pub dpi: Option<f32>,
    pub has_text: bool,
    pub toc_entries: usize,
    /// Share of the group's longest text this copy has (0..1); low means parts are missing.
    pub completeness: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    /// Best first.
    pub copies: Vec<Copy>,
    /// False when some copy could only be matched by title and page count (no text to compare).
    pub confident: bool,
}

fn pages_close(a: &Profile, b: &Profile) -> bool {
    match (a.pages, b.pages) {
        (Some(x), Some(y)) => {
            let (x, y) = (x as f32, y as f32);
            (x - y).abs() <= PAGE_TOLERANCE * x.max(y)
        }
        _ => false,
    }
}

fn format_points(format: &str, has_text: bool) -> f32 {
    match format {
        "EPUB" => 20.0,
        "MOBI" => 15.0,
        "PDF" if has_text => 12.0,
        "DOCX" | "FB2" => 10.0,
        "MD" => 8.0,
        "TXT" => 5.0,
        _ => 0.0,
    }
}

/// A higher score is a better copy to keep: real text first, then scan sharpness, format,
/// structure, and completeness relative to the group.
fn score(book: &Book, p: &Profile, completeness: f32) -> f32 {
    let mut s = format_points(&book.format, p.has_text());
    if p.has_text() {
        s += 40.0;
    } else if let Some(dpi) = p.dpi {
        s += dpi.min(400.0) / 400.0 * 30.0;
    }
    if p.toc_entries > 1 {
        s += 8.0;
    }
    if book.has_cover {
        s += 4.0;
    }
    if book.author.is_some() {
        s += 2.0;
    }
    if completeness < 0.9 {
        s -= 30.0 * (1.0 - completeness);
    }
    s
}

/// Splits candidates into groups of the same book and ranks each group's copies.
pub fn group_duplicates(
    books: &[Book],
    profiles: &HashMap<String, Profile>,
) -> Vec<DuplicateGroup> {
    let mut out = Vec::new();
    for candidates in candidate_groups(books) {
        // Union-find: copies with text join when their texts match. A copy without text (a
        // scan) can only be guessed at: it joins the one group of text copies when there is
        // exactly one, or other scans with a similar page count.
        let mut parent: Vec<usize> = (0..candidates.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let profile_of = |x: usize| profiles.get(&books[candidates[x]].id);
        let known: Vec<usize> = (0..candidates.len())
            .filter(|&x| profile_of(x).is_some())
            .collect();
        let (texts, scans): (Vec<usize>, Vec<usize>) = known
            .iter()
            .partition(|&&x| profile_of(x).unwrap().has_text());
        for (k, &x) in texts.iter().enumerate() {
            for &y in &texts[k + 1..] {
                if similarity(
                    &profile_of(x).unwrap().sketch,
                    &profile_of(y).unwrap().sketch,
                ) >= SAME_TEXT
                {
                    let (rx, ry) = (root(&mut parent, x), root(&mut parent, y));
                    parent[rx] = ry;
                }
            }
        }
        let mut text_roots: Vec<usize> = texts.iter().map(|&x| root(&mut parent, x)).collect();
        text_roots.sort_unstable();
        text_roots.dedup();
        let mut guessed = vec![false; candidates.len()];
        for (k, &x) in scans.iter().enumerate() {
            if let [only] = text_roots[..] {
                let rx = root(&mut parent, x);
                parent[rx] = root(&mut parent, only);
                guessed[x] = true;
            } else if text_roots.is_empty() {
                for &y in &scans[k + 1..] {
                    if pages_close(profile_of(x).unwrap(), profile_of(y).unwrap()) {
                        let (rx, ry) = (root(&mut parent, x), root(&mut parent, y));
                        parent[rx] = ry;
                        guessed[x] = true;
                        guessed[y] = true;
                    }
                }
            }
        }
        let mut sets: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &x in &known {
            let r = root(&mut parent, x);
            sets.entry(r).or_default().push(x);
        }
        for members in sets.into_values().filter(|m| m.len() > 1) {
            let size = |x: usize| {
                let p = &profiles[&books[candidates[x]].id];
                if p.has_text() {
                    p.chars
                } else {
                    p.pages.unwrap_or(0)
                }
            };
            let longest = members.iter().map(|&x| size(x)).max().unwrap_or(0).max(1);
            let mut copies: Vec<Copy> = members
                .iter()
                .map(|&x| {
                    let book = &books[candidates[x]];
                    let p = &profiles[&book.id];
                    let completeness = (size(x) as f32 / longest as f32).min(1.0);
                    Copy {
                        id: book.id.clone(),
                        score: score(book, p, completeness),
                        chars: p.chars,
                        pages: p.pages,
                        dpi: p.dpi,
                        has_text: p.has_text(),
                        toc_entries: p.toc_entries,
                        completeness,
                    }
                })
                .collect();
            copies.sort_by(|a, b| b.score.total_cmp(&a.score).then(b.chars.cmp(&a.chars)));
            out.push(DuplicateGroup {
                copies,
                confident: !members.iter().any(|&x| guessed[x]),
            });
        }
    }
    out
}

// --- merging -----------------------------------------------------------------------------------

/// Position as a fraction of the book (blocks for reflowable books, pages otherwise).
pub fn fraction(doc: &Document, loc: Location) -> f32 {
    match (&doc.content, loc) {
        (Content::Flow(chapters), Location::Flow { chapter, block }) => {
            let total: usize = chapters.iter().map(|c| c.blocks.len().max(1)).sum();
            let before: usize = chapters
                .iter()
                .take(chapter)
                .map(|c| c.blocks.len().max(1))
                .sum();
            ((before + block) as f32 / total.max(1) as f32).min(1.0)
        }
        (Content::Paged(src), Location::Page { page }) => {
            page as f32 / (src.page_count().max(2) - 1) as f32
        }
        _ => 0.0,
    }
}

pub fn location_at(doc: &Document, fraction: f32) -> Location {
    let f = fraction.clamp(0.0, 1.0);
    match &doc.content {
        Content::Paged(src) => Location::Page {
            page: (f * src.page_count().saturating_sub(1) as f32).round() as usize,
        },
        Content::Flow(chapters) => {
            let total: usize = chapters.iter().map(|c| c.blocks.len().max(1)).sum();
            let mut target = (f * total as f32) as usize;
            for (chapter, c) in chapters.iter().enumerate() {
                let n = c.blocks.len().max(1);
                if target < n {
                    return Location::Flow {
                        chapter,
                        block: target.min(c.blocks.len().saturating_sub(1)),
                    };
                }
                target -= n;
            }
            Location::Flow {
                chapter: chapters.len().saturating_sub(1),
                block: 0,
            }
        }
    }
}

fn title_at(doc: &Document, loc: Location) -> String {
    match (&doc.content, loc) {
        (Content::Flow(chapters), Location::Flow { chapter, .. }) => chapters
            .get(chapter)
            .map(|c| c.title.clone())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| crate::t!("title_chapter", n = chapter + 1)),
        (_, Location::Page { page }) => crate::t!("title_page", n = page + 1),
        (_, Location::Flow { chapter, .. }) => crate::t!("title_chapter", n = chapter + 1),
    }
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Where a highlighted passage is in a reflowable book: the block holding its first line and,
/// for passages over several paragraphs, the block holding its last line.
fn find_highlight(doc: &Document, h: &Highlight) -> Option<Highlight> {
    let Content::Flow(chapters) = &doc.content else {
        return None;
    };
    let lines: Vec<&str> = h
        .text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let (first, last) = (*lines.first()?, *lines.last()?);
    if first.chars().count() < 2 {
        return None;
    }
    for (ci, ch) in chapters.iter().enumerate() {
        for (bi, block) in ch.blocks.iter().enumerate() {
            let text = block.text();
            let Some(pos) = text.find(first) else {
                continue;
            };
            let start = utf16_len(&text[..pos]);
            if lines.len() == 1 {
                return Some(Highlight {
                    chapter: ci,
                    start_block: bi,
                    start,
                    end_block: bi,
                    end: start + utf16_len(first),
                    ..h.clone()
                });
            }
            // The last line starts one of the following paragraphs.
            for (bj, later) in ch
                .blocks
                .iter()
                .enumerate()
                .skip(bi + 1)
                .take(lines.len() + 5)
            {
                let t = later.text();
                if let Some(p) = t.find(last) {
                    return Some(Highlight {
                        chapter: ci,
                        start_block: bi,
                        start,
                        end_block: bj,
                        end: utf16_len(&t[..p]) + utf16_len(last),
                        ..h.clone()
                    });
                }
            }
        }
    }
    None
}

/// The page whose text contains the passage's first line (spacing ignored).
fn find_page(doc: &Document, text: &str) -> Option<usize> {
    let Content::Paged(src) = &doc.content else {
        return None;
    };
    let needle: String = text
        .lines()
        .next()?
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if needle.chars().count() < 4 {
        return None;
    }
    (0..src.page_count()).find(|&p| {
        src.page_text(p).is_some_and(|t| {
            t.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .contains(&needle)
        })
    })
}

#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    /// Highlights placed exactly in the kept book.
    pub highlights: usize,
    /// Bookmarks (including highlights that became bookmarks) placed by approximate position.
    pub bookmarks: usize,
}

/// Adds the annotations of a removed copy to the kept book's, mapping positions across the
/// two files: highlights by their text, everything else by position in the book.
pub fn transfer_annotations(
    from: &Document,
    from_ann: &Annotations,
    to: &Document,
    to_ann: &mut Annotations,
) -> Transfer {
    let mut report = Transfer::default();
    let flow_target = matches!(to.content, Content::Flow(_));
    for h in &from_ann.highlights {
        // Already moved (as a highlight, or as a bookmark when it had no place).
        if to_ann.highlights.iter().any(|x| x.id == h.id)
            || to_ann.bookmarks.iter().any(|x| x.id == h.id)
        {
            continue;
        }
        if flow_target && let Some(found) = find_highlight(to, h) {
            to_ann.highlights.push(found);
            report.highlights += 1;
            continue;
        }
        // No place for a highlight: keep it as a bookmark holding the passage and its note.
        let from_loc = Location::Flow {
            chapter: h.chapter,
            block: h.start_block,
        };
        let location = find_page(to, &h.text)
            .map(|page| Location::Page { page })
            .unwrap_or_else(|| location_at(to, fraction(from, from_loc)));
        let mut excerpt: String = h.text.chars().take(200).collect();
        if let Some(note) = h.note.as_deref().filter(|n| !n.trim().is_empty()) {
            excerpt.push_str("\n— ");
            excerpt.push_str(note);
        }
        to_ann.bookmarks.push(Bookmark {
            id: h.id.clone(),
            location,
            title: title_at(to, location),
            excerpt,
            created: h.created,
        });
        report.bookmarks += 1;
    }
    for b in &from_ann.bookmarks {
        if to_ann.bookmarks.iter().any(|x| x.id == b.id) {
            continue;
        }
        let location = location_at(to, fraction(from, b.location));
        to_ann.bookmarks.push(Bookmark {
            location,
            title: title_at(to, location),
            ..b.clone()
        });
        report.bookmarks += 1;
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Chapter, Span};
    use std::path::PathBuf;

    fn flow(chapters: Vec<Vec<&str>>) -> Document {
        Document {
            path: PathBuf::new(),
            title: "t".into(),
            author: None,
            toc: Vec::new(),
            content: Content::Flow(
                chapters
                    .into_iter()
                    .enumerate()
                    .map(|(i, paras)| Chapter {
                        title: format!("第{}回", i + 1),
                        blocks: paras
                            .into_iter()
                            .map(|t| Block::Paragraph(vec![Span::plain(t)]))
                            .collect(),
                    })
                    .collect(),
            ),
        }
    }

    fn book(id: &str, title: &str, author: Option<&str>, format: &str) -> Book {
        let mut b: Book = serde_json::from_value(serde_json::json!({
            "id": id, "title": title, "author": author, "format": format, "fileName": "f",
            "size": 1, "sha256": id, "hasCover": false, "added": 0, "lastRead": null,
            "progress": 0.0, "position": null
        }))
        .unwrap();
        b.group = None;
        b
    }

    #[test]
    fn titles_ignore_editions_sources_and_punctuation() {
        assert_eq!(
            title_key("Rust 程序设计语言（第2版）"),
            title_key("rust程序设计语言")
        );
        assert_eq!(
            title_key("Alice's Adventures in Wonderland - Z-Library"),
            title_key("Alice’s Adventures in Wonderland")
        );
        assert_eq!(title_key("三国演义 [PDF]"), title_key("三国演义"));
        assert_ne!(title_key("三国演义"), title_key("三国志"));
        assert_eq!(title_key("（序）"), "序");
    }

    #[test]
    fn candidates_need_matching_titles_and_compatible_authors() {
        let books = vec![
            book("a", "三国演义", Some("罗贯中"), "EPUB"),
            book("b", "三国演义（珍藏版）", None, "PDF"),
            book("c", "三国演义", Some("某人"), "TXT"),
            book("d", "水浒传", Some("施耐庵"), "EPUB"),
        ];
        assert_eq!(candidate_groups(&books), vec![vec![0, 1]]);
    }

    #[test]
    fn same_text_is_similar_and_other_texts_are_not() {
        let base = "话说天下大势，分久必合，合久必分。周末七国分争，并入于秦。及秦灭之后，楚、汉分争，又并入于汉。".repeat(20);
        let a: Vec<char> = text_chars(&base).collect();
        let b: Vec<char> = text_chars(&format!("前言：本书为整理版。{base}")).collect();
        let other: Vec<char> = text_chars(
            &"Once upon a time there was a little girl called Alice who fell down a hole."
                .repeat(20),
        )
        .collect();
        assert!(similarity(&sketch(&a), &sketch(&b)) > 0.8);
        assert!(similarity(&sketch(&a), &sketch(&other)) < 0.05);
    }

    #[test]
    fn groups_rank_text_over_scans_and_split_translations() {
        let books = vec![
            book("txt", "Alice", None, "TXT"),
            book("epub", "Alice", None, "EPUB"),
            book("scan", "Alice", None, "PDF"),
            book("translation", "Alice", None, "EPUB"),
        ];
        let text = "alice was beginning to get very tired of sitting by her sister on the bank "
            .repeat(30);
        let chars: Vec<char> = text_chars(&text).collect();
        let other: Vec<char> = text_chars(
            &"爱丽丝靠着姐姐坐在河岸边很久了，由于没有什么事情可做，她开始感到厌倦。".repeat(30),
        )
        .collect();
        let with_text = |toc: usize| Profile {
            chars: chars.len(),
            sketch: sketch(&chars),
            toc_entries: toc,
            ..Default::default()
        };
        let profiles: HashMap<String, Profile> = [
            ("txt".to_owned(), with_text(0)),
            ("epub".to_owned(), with_text(12)),
            (
                "scan".to_owned(),
                Profile {
                    pages: Some(100),
                    dpi: Some(300.0),
                    ..Default::default()
                },
            ),
            (
                "translation".to_owned(),
                Profile {
                    chars: other.len(),
                    sketch: sketch(&other),
                    ..Default::default()
                },
            ),
        ]
        .into();
        let groups = group_duplicates(&books, &profiles);
        // The translation stays apart, and the scan can't be told apart from either: left out.
        assert_eq!(groups.len(), 1);
        let ids: Vec<&str> = groups[0].copies.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["epub", "txt"]);
        assert!(groups[0].confident);
        // Without the translation the scan joins the only text group, as a guess.
        let groups = group_duplicates(&books[..3], &profiles);
        let ids: Vec<&str> = groups[0].copies.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["epub", "txt", "scan"]);
        assert!(!groups[0].confident);
    }

    #[test]
    fn annotations_move_by_text_or_position() {
        let from = flow(vec![vec!["甲乙丙丁", "天下大势，分久必合"], vec!["结尾"]]);
        let to = flow(vec![
            vec!["序言"],
            vec!["甲乙丙丁", "话说天下大势，分久必合"],
            vec!["结尾"],
        ]);
        let h = Highlight {
            id: "h1".into(),
            chapter: 0,
            start_block: 1,
            start: 0,
            end_block: 1,
            end: 4,
            text: "天下大势".into(),
            color: "yellow".into(),
            note: Some("注".into()),
            created: 1,
            updated: 1,
        };
        let lost = Highlight {
            id: "h2".into(),
            text: "找不到的句子".into(),
            ..h.clone()
        };
        let from_ann = Annotations {
            bookmarks: vec![Bookmark {
                id: "b1".into(),
                location: Location::Flow {
                    chapter: 2,
                    block: 0,
                },
                title: "x".into(),
                excerpt: String::new(),
                created: 1,
            }],
            highlights: vec![h, lost],
        };
        let mut to_ann = Annotations::default();
        let report = transfer_annotations(&from, &from_ann, &to, &mut to_ann);
        assert_eq!(
            report,
            Transfer {
                highlights: 1,
                bookmarks: 2
            }
        );
        let moved = &to_ann.highlights[0];
        assert_eq!(
            (moved.chapter, moved.start_block, moved.start, moved.end),
            (1, 1, 2, 6)
        );
        assert!(
            to_ann
                .bookmarks
                .iter()
                .any(|b| b.id == "h2" && b.excerpt.contains("找不到的句子\n— 注"))
        );
        assert_eq!(
            to_ann
                .bookmarks
                .iter()
                .find(|b| b.id == "b1")
                .unwrap()
                .location,
            Location::Flow {
                chapter: 2,
                block: 0
            }
        );
        // Running it again adds nothing twice.
        assert_eq!(
            transfer_annotations(&from, &from_ann, &to, &mut to_ann),
            Transfer::default()
        );
    }
}
