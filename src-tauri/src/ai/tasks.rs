//! The AI features: chapter/page/book summaries, questions about the book, chat about a text
//! selection, and bookshelf organisation. Everything here works on a [`ChatModel`] and plain data, so it can be tested
//! with rig's mock models.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, anyhow, bail};
use rig_core::completion::{CompletionRequest, Message};
use serde::{Deserialize, Serialize};

use super::config::AiConfig;
use super::provider::{
    ChatModel, TokenUsage, complete_reply, complete_text, estimate_tokens, friendly_error,
    is_transient, stream_text,
};
use super::retrieval::BookIndex;
use crate::model::{Content, Document, Location};

const SUMMARY_TOKENS: u64 = 2_048;
const MERGE_TOKENS: u64 = 4_096;
const CHAT_TOKENS: u64 = 2_048;
/// Chapters shorter than this (in characters) are skipped in book summaries.
const MIN_UNIT_CHARS: usize = 200;

// --- requests and events ---------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatAction {
    Explain,
    Translate,
    Summarize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChatTurn {
    /// "user" or "assistant".
    pub role: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AiTask {
    #[serde(rename_all = "camelCase")]
    SummarizeChapter { book_id: String, chapter: usize },
    /// Pages `from..=to` of a paged book.
    #[serde(rename_all = "camelCase")]
    SummarizePages {
        book_id: String,
        from: usize,
        to: usize,
    },
    #[serde(rename_all = "camelCase")]
    SummarizeBook { book_id: String },
    /// A conversation about a selected passage. `action` produces the first question; later
    /// turns are in `history` (ending with the user's newest message).
    #[serde(rename_all = "camelCase")]
    Chat {
        book_title: String,
        selection: String,
        context: String,
        action: Option<ChatAction>,
        history: Vec<ChatTurn>,
    },
    /// A question about the whole book, answered from retrieved passages and the text around
    /// the reader's position. `history` ends with the user's newest message.
    /// A mind map (a nested Markdown list) of part of the book or of given text.
    #[serde(rename_all = "camelCase")]
    MindMap {
        book_id: String,
        scope: MindMapScope,
        /// Ignore a cached map and draw a new one.
        #[serde(default)]
        refresh: bool,
    },
    /// Builds the book's semantic index with the configured embedding model.
    #[serde(rename_all = "camelCase")]
    BuildIndex { book_id: String },
    #[serde(rename_all = "camelCase")]
    Ask {
        book_id: String,
        location: Option<Location>,
        history: Vec<ChatTurn>,
    },
}

/// A numbered passage an answer may cite as `[n]`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Source {
    pub n: usize,
    pub title: String,
    pub location: Location,
    pub excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AiEvent {
    Delta {
        text: String,
    },
    Progress {
        done: usize,
        total: usize,
        label: String,
    },
    Done {
        text: String,
        cached: bool,
    },
    Error {
        message: String,
    },
    /// The passages given to the model for a question, sent before the answer.
    Sources {
        sources: Vec<Source>,
    },
    /// Tokens used by one request of the task (a task may make several).
    Usage {
        #[serde(flatten)]
        usage: TokenUsage,
    },
}

/// Pauses before sending a failed request again.
const RETRY_DELAYS: [std::time::Duration; 2] = [
    std::time::Duration::from_secs(2),
    std::time::Duration::from_secs(8),
];

/// What a task needs: the model, settings, a per-request input budget, and an event sink.
pub struct Runner<'a> {
    pub model: ChatModel,
    pub cfg: AiConfig,
    /// Input tokens of book text per request.
    pub budget: usize,
    pub emit: &'a (dyn Fn(AiEvent) + Send + Sync),
}

impl Runner<'_> {
    fn system(&self) -> String {
        format!(
            "You are a careful reading assistant. Answer only from the book content provided; \
             do not invent plot points, characters or arguments that are not in it. \
             Write your answer in {}.",
            self.cfg.answer_language()
        )
    }

    fn request(&self, prompt: String, max_tokens: u64) -> CompletionRequest {
        CompletionRequest::new(Message::user(prompt))
            .preamble(self.system())
            .max_tokens(max_tokens)
    }

    /// One request, sent again after a pause when it fails for a passing reason (see
    /// [`is_transient`]), so one hiccup doesn't end a long whole-book run.
    async fn complete(&self, prompt: String, max_tokens: u64) -> Result<String> {
        let mut attempt = 0;
        loop {
            match complete_reply(&self.model, self.request(prompt.clone(), max_tokens)).await {
                Ok((text, usage)) => {
                    (self.emit)(AiEvent::Usage { usage });
                    return Ok(text);
                }
                Err(e) if attempt < RETRY_DELAYS.len() && is_transient(&e) => {
                    log::info!("AI request failed ({e}); retrying");
                    tokio::time::sleep(RETRY_DELAYS[attempt]).await;
                    attempt += 1;
                }
                Err(e) => return Err(anyhow!(friendly_error(&self.cfg, &e))),
            }
        }
    }

    async fn stream(&self, prompt: String, max_tokens: u64) -> Result<String> {
        self.stream_request(self.request(prompt, max_tokens)).await
    }

    async fn stream_request(&self, request: CompletionRequest) -> Result<String> {
        let (text, usage) = stream_text(&self.model, request, |t| {
            (self.emit)(AiEvent::Delta { text: t.to_owned() })
        })
        .await
        .map_err(|e| anyhow!(friendly_error(&self.cfg, &e)))?;
        if let Some(usage) = usage {
            (self.emit)(AiEvent::Usage { usage });
        }
        Ok(text)
    }
}

// --- text units ------------------------------------------------------------------------------

/// A piece of a book summarised on its own: a chapter, or a run of PDF pages.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub key: String,
    pub title: String,
    pub text: String,
}

pub fn chapter_unit(doc: &Document, chapter: usize) -> Result<Unit> {
    let Content::Flow(chapters) = &doc.content else {
        bail!(crate::t!("not_text_book"))
    };
    let ch = chapters
        .get(chapter)
        .ok_or_else(|| anyhow!(crate::t!("chapter_not_found")))?;
    let text = ch
        .blocks
        .iter()
        .map(|b| b.text())
        .filter(|t| !t.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        bail!(crate::t!("ai_chapter_empty"));
    }
    let title = if ch.title.is_empty() {
        crate::t!("title_chapter", n = chapter + 1)
    } else {
        ch.title.clone()
    };
    Ok(Unit {
        key: format!("ch:{chapter}"),
        title,
        text,
    })
}

pub fn pages_unit(doc: &Document, from: usize, to: usize) -> Result<Unit> {
    let Content::Paged(src) = &doc.content else {
        bail!(crate::t!("not_paged_book"))
    };
    let to = to.min(src.page_count().saturating_sub(1));
    let text = (from..=to)
        .filter_map(|p| src.page_text(p))
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        bail!(crate::t!("ai_pages_no_text"));
    }
    let title = if from == to {
        crate::t!("title_page", n = from + 1)
    } else {
        crate::t!("title_pages", from = from + 1, to = to + 1)
    };
    Ok(Unit {
        key: format!("pages:{from}-{to}"),
        title,
        text,
    })
}

/// The units a whole-book summary is built from.
pub fn book_units(doc: &Document, budget: usize) -> Result<Vec<Unit>> {
    let units = match &doc.content {
        Content::Flow(chapters) => (0..chapters.len())
            .filter_map(|i| chapter_unit(doc, i).ok())
            .filter(|u| u.text.chars().count() >= MIN_UNIT_CHARS)
            .collect(),
        Content::Paged(src) => {
            // Group consecutive pages until a group reaches the budget.
            let mut units = Vec::new();
            let mut start = 0;
            let mut text = String::new();
            for p in 0..src.page_count() {
                let page = src.page_text(p).unwrap_or_default();
                if !text.is_empty() && estimate_tokens(&text) + estimate_tokens(&page) > budget {
                    units.push(Unit {
                        key: format!("pages:{start}-{}", p - 1),
                        title: crate::t!("title_pages", from = start + 1, to = p),
                        text: std::mem::take(&mut text),
                    });
                    start = p;
                }
                text.push_str(&page);
                text.push('\n');
            }
            if !text.trim().is_empty() {
                units.push(Unit {
                    key: format!("pages:{start}-{}", src.page_count().saturating_sub(1)),
                    title: crate::t!("title_pages", from = start + 1, to = src.page_count()),
                    text,
                });
            }
            units.retain(|u| !u.text.trim().is_empty());
            units
        }
    };
    if units.is_empty() {
        bail!(crate::t!("ai_book_no_text"));
    }
    Ok(units)
}

/// Splits text at line breaks into pieces of at most `budget` tokens (long lines are cut).
pub fn split_by_budget(text: &str, budget: usize) -> Vec<String> {
    let budget = budget.max(200);
    let mut pieces = Vec::new();
    let mut cur = String::new();
    let mut cur_tokens = 0;
    let push_line =
        |line: &str, pieces: &mut Vec<String>, cur: &mut String, cur_tokens: &mut usize| {
            let t = estimate_tokens(line) + 1;
            if *cur_tokens + t > budget && !cur.is_empty() {
                pieces.push(std::mem::take(cur));
                *cur_tokens = 0;
            }
            cur.push_str(line);
            cur.push('\n');
            *cur_tokens += t;
        };
    for line in text.lines() {
        if estimate_tokens(line) > budget {
            // Cut an overlong line by characters (never inside a character).
            let chars: Vec<char> = line.chars().collect();
            for chunk in chars.chunks(budget) {
                push_line(
                    &chunk.iter().collect::<String>(),
                    &mut pieces,
                    &mut cur,
                    &mut cur_tokens,
                );
            }
        } else {
            push_line(line, &mut pieces, &mut cur, &mut cur_tokens);
        }
    }
    if !cur.trim().is_empty() {
        pieces.push(cur);
    }
    pieces
}

// --- summary cache ---------------------------------------------------------------------------

/// Summaries per model and unit, in `ai/<book>/summaries.json`.
pub struct SummaryCache {
    db: crate::db::Db,
    book_id: String,
    data: BTreeMap<String, BTreeMap<String, String>>,
}

impl SummaryCache {
    /// The cached summaries (and mind maps) of one book, by model and unit.
    pub fn open(db: &crate::db::Db, book_id: &str) -> Self {
        let mut data: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        if let Ok(mut stmt) = db
            .conn()
            .prepare("SELECT model, unit, text FROM ai_cache WHERE book_id = ?1")
        {
            let rows = stmt.query_map([book_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            });
            for (model, unit, text) in rows.into_iter().flatten().flatten() {
                data.entry(model).or_default().insert(unit, text);
            }
        }
        Self {
            db: db.clone(),
            book_id: book_id.to_owned(),
            data,
        }
    }

    pub fn get(&self, model: &str, unit: &str) -> Option<&String> {
        self.data.get(model)?.get(unit)
    }

    pub fn put(&mut self, model: &str, unit: &str, summary: String) -> Result<()> {
        self.db.conn().execute(
            "INSERT OR REPLACE INTO ai_cache (book_id, model, unit, text) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![self.book_id, model, unit, summary],
        )?;
        self.data
            .entry(model.into())
            .or_default()
            .insert(unit.into(), summary);
        Ok(())
    }
}

// --- summaries -------------------------------------------------------------------------------

/// Summarises one unit. Text over the budget is summarised in parts, then merged. With
/// `stream`, the final answer is streamed as `Delta` events.
/// Parts of long units are cached under `<unit key>#<part>/<parts>`, so a run that stopped
/// part-way (cancelled, or an error) picks up where it left off.
pub async fn summarize_unit(
    r: &Runner<'_>,
    book: &str,
    unit: &Unit,
    stream: bool,
    mut cache: Option<&mut SummaryCache>,
) -> Result<String> {
    if estimate_tokens(&unit.text) <= r.budget {
        let prompt = format!(
            "Below is \"{}\" from the book \"{book}\". Summarize it: first give the main content in one or two paragraphs, then list the key points (people, events, ideas or arguments).\n\n<content>\n{}\n</content>",
            unit.title, unit.text
        );
        return if stream {
            r.stream(prompt, SUMMARY_TOKENS).await
        } else {
            r.complete(prompt, SUMMARY_TOKENS).await
        };
    }
    let parts = split_by_budget(&unit.text, r.budget);
    let mut partials = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        (r.emit)(AiEvent::Progress {
            done: i,
            total: parts.len(),
            label: crate::t!(
                "ai_summarizing_part",
                title = unit.title,
                i = i + 1,
                n = parts.len()
            ),
        });
        let prompt = format!(
            "Below is part {1} of {2} of \"{0}\" from the book \"{book}\". Briefly summarize the key points of this part; it will be merged with the other parts later.\n\n<content>\n{part}\n</content>",
            unit.title,
            i + 1,
            parts.len()
        );
        let key = format!("{}#{}/{}", unit.key, i + 1, parts.len());
        let model = r.cfg.model_key();
        let partial = match cache.as_deref().and_then(|c| c.get(&model, &key)) {
            Some(s) => s.clone(),
            None => {
                let s = r.complete(prompt, SUMMARY_TOKENS).await?;
                if let Some(c) = cache.as_deref_mut() {
                    c.put(&model, &key, s.clone())?;
                }
                s
            }
        };
        partials.push(partial);
    }
    let joined = partials
        .iter()
        .enumerate()
        .map(|(i, s)| format!("## Part {}\n{s}", i + 1))
        .collect::<Vec<_>>()
        .join("\n\n");
    let prompt = format!(
        "Below are summaries of the parts of \"{}\" from the book \"{book}\". Merge them into one complete summary: an overview first, then the key points, without repetition.\n\n{joined}",
        unit.title
    );
    if stream {
        r.stream(prompt, MERGE_TOKENS).await
    } else {
        r.complete(prompt, MERGE_TOKENS).await
    }
}

/// Summarises one unit with caching; cached results are returned without a request.
pub async fn summarize_cached(
    r: &Runner<'_>,
    cache: &mut SummaryCache,
    book: &str,
    unit: &Unit,
) -> Result<(String, bool)> {
    let model = r.cfg.model_key();
    if let Some(s) = cache.get(&model, &unit.key) {
        return Ok((s.clone(), true));
    }
    let s = summarize_unit(r, book, unit, true, Some(cache)).await?;
    cache.put(&model, &unit.key, s.clone())?;
    Ok((s, false))
}

/// Summaries of every unit (from the cache when possible), headed by their titles. Reports
/// progress as it goes.
async fn unit_summaries(
    r: &Runner<'_>,
    cache: &mut SummaryCache,
    book: &str,
    units: &[Unit],
) -> Result<Vec<String>> {
    let model = r.cfg.model_key();
    let mut summaries = Vec::with_capacity(units.len());
    for (i, unit) in units.iter().enumerate() {
        (r.emit)(AiEvent::Progress {
            done: i,
            total: units.len(),
            label: crate::t!("ai_summarizing", title = unit.title),
        });
        let s = match cache.get(&model, &unit.key) {
            Some(s) => s.clone(),
            None => {
                let s = summarize_unit(r, book, unit, false, Some(cache)).await?;
                cache.put(&model, &unit.key, s.clone())?;
                s
            }
        };
        summaries.push(format!("## {}\n{s}", unit.title));
    }
    (r.emit)(AiEvent::Progress {
        done: units.len(),
        total: units.len(),
        label: crate::t!("ai_combining"),
    });
    Ok(summaries)
}

/// Merges summaries in groups until all of them fit in one request.
async fn condense(r: &Runner<'_>, book: &str, mut summaries: Vec<String>) -> Result<Vec<String>> {
    while estimate_tokens(&summaries.join("\n\n")) > r.budget && summaries.len() > 1 {
        let mut merged = Vec::new();
        for group in split_by_budget(&summaries.join("\n\n"), r.budget) {
            let prompt = format!(
                "Below are summaries of several chapters of the book \"{book}\". Merge them into one shorter summary that keeps each chapter's title and key content.\n\n{group}"
            );
            merged.push(r.complete(prompt, MERGE_TOKENS).await?);
        }
        if merged.len() >= summaries.len() {
            break;
        }
        summaries = merged;
    }
    Ok(summaries)
}

/// Whole-book summary: each unit (cached), then merged — in groups first if the unit
/// summaries together exceed the budget.
pub async fn summarize_book(
    r: &Runner<'_>,
    cache: &mut SummaryCache,
    book: &str,
    units: &[Unit],
) -> Result<(String, bool)> {
    let model = r.cfg.model_key();
    if let Some(s) = cache.get(&model, "book") {
        return Ok((s.clone(), true));
    }
    let summaries = unit_summaries(r, cache, book, units).await?;
    let summaries = condense(r, book, summaries).await?;
    let prompt = format!(
        "Below are summaries of the parts of the book \"{book}\". Write a summary of the whole book with: 1. its central theme (one paragraph); 2. its structure and how the parts connect; 3. the core ideas or main plot; 4. details worth noticing.\n\n{}",
        summaries.join("\n\n")
    );
    let s = r.stream(prompt, MERGE_TOKENS).await?;
    cache.put(&model, "book", s.clone())?;
    Ok((s, false))
}

// --- mind maps -------------------------------------------------------------------------------

/// What a mind map covers.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MindMapScope {
    Chapter {
        chapter: usize,
    },
    Pages {
        from: usize,
        to: usize,
    },
    Book,
    /// Free text, e.g. the current conversation.
    Text {
        title: String,
        text: String,
    },
}

/// How the mind-map task asks for the outline.
pub const MIND_MAP_FORMAT: &str = "Write the mind map as a nested Markdown bullet list: the \
     first line is a single root item naming the topic, and each level below is indented by \
     two spaces. Use at most 4 levels and about 60 items, each a short phrase (no sentences). \
     Group related ideas under clear branch names. Output only the list.";

/// Keeps only the outline from a model's mind-map reply: the contents of a fenced block if
/// there is one, list items, and headings (turned into list levels).
pub fn clean_outline(reply: &str) -> String {
    let body = match reply.find("```") {
        Some(start) => {
            let after = &reply[start + 3..];
            let after = after.split_once('\n').map_or("", |(_, rest)| rest);
            after.split("```").next().unwrap_or(after)
        }
        None => reply,
    };
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let level = 1 + rest.chars().take_while(|&c| c == '#').count();
            let text = rest.trim_start_matches('#').trim();
            if !text.is_empty() {
                out.push(format!("{}- {text}", "  ".repeat(level - 1)));
            }
        } else if let Some(item) = list_item(trimmed) {
            out.push(format!("{}- {item}", " ".repeat(indent)));
        }
    }
    out.join("\n")
}

/// The text of a bullet or numbered list item.
fn list_item(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix(['-', '*', '+']) {
        return rest.strip_prefix(' ').map(str::trim);
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &line[digits..];
        return rest
            .strip_prefix(['.', ')'])
            .and_then(|r| r.strip_prefix(' '))
            .map(str::trim);
    }
    None
}

async fn draw_mind_map(r: &Runner<'_>, book: &str, topic: &str, material: &str) -> Result<String> {
    let prompt = format!(
        "Make a mind map of \"{topic}\" from the book \"{book}\", based on the material below. \
         {MIND_MAP_FORMAT}\n\n<material>\n{material}\n</material>"
    );
    Ok(clean_outline(&r.stream(prompt, MERGE_TOKENS).await?))
}

/// A mind map of a chapter, pages, the whole book or given text. Maps of book content are
/// cached per model; long material is summarised first (reusing cached summaries).
pub async fn mind_map(
    r: &Runner<'_>,
    cache: &mut SummaryCache,
    doc: &Document,
    book: &str,
    scope: MindMapScope,
    refresh: bool,
) -> Result<(String, bool)> {
    let model = r.cfg.model_key();
    let cached = |cache: &SummaryCache, key: &str| {
        if refresh {
            None
        } else {
            cache.get(&model, key).cloned()
        }
    };
    let (key, topic, material) = match scope {
        MindMapScope::Text { title, text } => {
            let text: String = split_by_budget(&text, r.budget)
                .into_iter()
                .next()
                .unwrap_or_default();
            return Ok((draw_mind_map(r, book, &title, &text).await?, false));
        }
        MindMapScope::Book => {
            if let Some(s) = cached(cache, "mindmap:book") {
                return Ok((s, true));
            }
            let units = book_units(doc, r.budget)?;
            let summaries = unit_summaries(r, cache, book, &units).await?;
            let material = condense(r, book, summaries).await?.join("\n\n");
            ("mindmap:book".to_owned(), book.to_owned(), material)
        }
        MindMapScope::Chapter { chapter } => {
            let unit = chapter_unit(doc, chapter)?;
            let key = format!("mindmap:{}", unit.key);
            if let Some(s) = cached(cache, &key) {
                return Ok((s, true));
            }
            let material = unit_material(r, cache, book, &unit).await?;
            (key, unit.title, material)
        }
        MindMapScope::Pages { from, to } => {
            let unit = pages_unit(doc, from, to)?;
            let key = format!("mindmap:{}", unit.key);
            if let Some(s) = cached(cache, &key) {
                return Ok((s, true));
            }
            let material = unit_material(r, cache, book, &unit).await?;
            (key, unit.title, material)
        }
    };
    let outline = draw_mind_map(r, book, &topic, &material).await?;
    if !outline.trim().is_empty() {
        cache.put(&model, &key, outline.clone())?;
    }
    Ok((outline, false))
}

/// The unit's text when it fits, otherwise its (cached) summary.
async fn unit_material(
    r: &Runner<'_>,
    cache: &mut SummaryCache,
    book: &str,
    unit: &Unit,
) -> Result<String> {
    if estimate_tokens(&unit.text) <= r.budget {
        return Ok(unit.text.clone());
    }
    let model = r.cfg.model_key();
    if let Some(s) = cache.get(&model, &unit.key) {
        return Ok(s.clone());
    }
    (r.emit)(AiEvent::Progress {
        done: 0,
        total: 1,
        label: crate::t!("ai_summarizing", title = unit.title),
    });
    let s = summarize_unit(r, book, unit, false, Some(&mut *cache)).await?;
    cache.put(&model, &unit.key, s.clone())?;
    Ok(s)
}

/// Rough cost of a whole-book summary.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookEstimate {
    pub units: usize,
    pub cached_units: usize,
    pub book_cached: bool,
    pub input_tokens: usize,
    pub output_tokens: usize,
    /// USD, when the model's prices are known.
    pub cost: Option<f64>,
}

pub fn estimate_book(
    units: &[Unit],
    cache: &SummaryCache,
    cfg: &AiConfig,
    pricing: Option<(f64, f64)>,
) -> BookEstimate {
    let model = cfg.model_key();
    let todo: Vec<&Unit> = units
        .iter()
        .filter(|u| cache.get(&model, &u.key).is_none())
        .collect();
    let input_tokens =
        todo.iter().map(|u| estimate_tokens(&u.text)).sum::<usize>() + units.len() * 500;
    let output_tokens = todo.len() * 600 + 1_500;
    BookEstimate {
        units: units.len(),
        cached_units: units.len() - todo.len(),
        book_cached: cache.get(&model, "book").is_some(),
        input_tokens,
        output_tokens,
        cost: pricing
            .map(|(i, o)| (input_tokens as f64 * i + output_tokens as f64 * o) / 1_000_000.0),
    }
}

// --- chat about a selection ------------------------------------------------------------------

pub async fn chat(
    r: &Runner<'_>,
    book_title: &str,
    selection: &str,
    context: &str,
    action: Option<ChatAction>,
    history: &[ChatTurn],
) -> Result<String> {
    let mut system = format!(
        "{}\n\nBook: \"{book_title}\"\nThe reader selected this passage in the book:\n<selection>\n{selection}\n</selection>",
        r.system()
    );
    if !context.trim().is_empty() && context.trim() != selection.trim() {
        system.push_str(&format!(
            "\nThe surrounding text:\n<context>\n{context}\n</context>"
        ));
    }
    let first = action.map(|a| match a {
        ChatAction::Explain => "Explain the selected passage: what it means, and any background, \
             allusions or terms it involves. Use the surrounding text where it helps."
            .to_owned(),
        ChatAction::Translate => {
            let target = r.cfg.answer_language();
            format!(
                "Translate the selected passage into {target}, keeping the tone of the original. \
                 If it is already written in {target}, translate it into English instead \
                 (or into Simplified Chinese if {target} is English). Where a literal \
                 translation doesn't work, add a short note after the translation."
            )
        }
        ChatAction::Summarize => "Summarize the key points of the selected passage.".to_owned(),
    });
    let mut turns: Vec<Message> = Vec::new();
    if let Some(q) = first {
        turns.push(Message::user(q));
    }
    for t in history {
        turns.push(if t.role == "assistant" {
            Message::assistant(t.text.clone())
        } else {
            Message::user(t.text.clone())
        });
    }
    let Some(last) = turns.pop() else {
        bail!(crate::t!("ai_no_question"))
    };
    let request = CompletionRequest::new(last)
        .messages(turns)
        .preamble(system)
        .max_tokens(CHAT_TOKENS);
    r.stream_request(request).await
}

// --- questions about the book -----------------------------------------------------------------

/// Share of the input budget for the text around the reader's position.
const NEARBY_SHARE: usize = 3;
const MAX_SOURCES: usize = 12;
const EXCERPT_CHARS: usize = 90;

/// Picks passages for a question: the ones around the reader's position, then the best keyword
/// matches, within `budget` tokens. Returned in reading order.
/// Merges ranked lists of passage indices (reciprocal rank fusion): passages ranked high in
/// either list, or fairly high in both, come first.
pub fn fuse(lists: &[&[usize]]) -> Vec<usize> {
    const K: f64 = 60.0;
    let mut scores: HashMap<usize, f64> = HashMap::new();
    for list in lists {
        for (rank, &i) in list.iter().enumerate() {
            *scores.entry(i).or_default() += 1.0 / (K + rank as f64 + 1.0);
        }
    }
    let mut ranked: Vec<(usize, f64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.into_iter().map(|(i, _)| i).collect()
}

/// Picks passages for a question: the ones around the reader's position, then the best
/// matches — keyword search fused with `semantic` (passages ranked by meaning, if the book has
/// a semantic index) — within `budget` tokens. Returned in reading order.
pub fn select_passages(
    index: &BookIndex,
    location: Option<Location>,
    query: &str,
    semantic: &[usize],
    budget: usize,
) -> Vec<usize> {
    let mut chosen: Vec<usize> = Vec::new();
    let mut used = 0;
    let mut take = |i: usize, limit: usize, chosen: &mut Vec<usize>| -> bool {
        if chosen.contains(&i) || chosen.len() >= MAX_SOURCES {
            return chosen.len() < MAX_SOURCES;
        }
        let cost = estimate_tokens(&index.passages[i].text);
        if used + cost > limit {
            return false;
        }
        used += cost;
        chosen.push(i);
        true
    };
    if let Some(here) = location.and_then(|l| index.passage_at(l)) {
        // The current passage, then alternately the ones after and before it.
        let near_limit = budget / NEARBY_SHARE;
        let mut order = vec![here];
        for d in 1..=3 {
            order.push(here + d);
            if let Some(before) = here.checked_sub(d) {
                order.push(before);
            }
        }
        for i in order.into_iter().filter(|&i| i < index.passages.len()) {
            if !take(i, near_limit, &mut chosen) {
                break;
            }
        }
    }
    let keyword = index.search(query, MAX_SOURCES * 2);
    let semantic: Vec<usize> = semantic
        .iter()
        .copied()
        .filter(|&i| i < index.passages.len())
        .collect();
    for i in fuse(&[&keyword, &semantic]) {
        if !take(i, budget, &mut chosen) {
            break;
        }
    }
    chosen.sort_unstable();
    chosen
}

pub async fn ask(
    r: &Runner<'_>,
    book_title: &str,
    index: &BookIndex,
    location: Option<Location>,
    history: &[ChatTurn],
    semantic: &[usize],
) -> Result<String> {
    let Some(last) = history.iter().rev().find(|t| t.role != "assistant") else {
        bail!(crate::t!("ai_no_question"))
    };
    // The previous question helps with follow-ups such as "what happened to him next?".
    let query: String = history
        .iter()
        .rev()
        .filter(|t| t.role != "assistant")
        .take(2)
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let picked = select_passages(index, location, &query, semantic, r.budget);
    let sources: Vec<Source> = picked
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let p = &index.passages[i];
            Source {
                n: k + 1,
                title: p.title.clone(),
                location: p.location,
                excerpt: p.text.chars().take(EXCERPT_CHARS).collect(),
            }
        })
        .collect();
    (r.emit)(AiEvent::Sources {
        sources: sources.clone(),
    });
    let here = location
        .and_then(|l| index.passage_at(l))
        .map(|i| index.passages[i].title.clone());
    let mut system = format!(
        "{}\n\nThe reader is asking about the book \"{book_title}\". Answer from the numbered \
         excerpts below and cite the ones you use as [n], e.g. [2] or [1][3]. If the excerpts \
         don't contain the answer, say so briefly and answer only what they support.",
        r.system()
    );
    if let Some(here) = here {
        system.push_str(&format!(
            " The reader is currently in \"{here}\"; \"this chapter\" refers to it."
        ));
    }
    system.push_str("\n\n<excerpts>\n");
    for (s, &i) in sources.iter().zip(&picked) {
        system.push_str(&format!(
            "[{}] ({})\n{}\n\n",
            s.n, s.title, index.passages[i].text
        ));
    }
    system.push_str("</excerpts>");
    let mut turns: Vec<Message> = history
        .iter()
        .map(|t| {
            if t.role == "assistant" {
                Message::assistant(t.text.clone())
            } else {
                Message::user(t.text.clone())
            }
        })
        .collect();
    // `last` is the newest user message, so the list is not empty.
    let prompt = turns
        .pop()
        .unwrap_or_else(|| Message::user(last.text.clone()));
    let request = CompletionRequest::new(prompt)
        .messages(turns)
        .preamble(system)
        .max_tokens(CHAT_TOKENS);
    r.stream_request(request).await
}

// --- bookshelf organisation --------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct BookBrief {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub format: String,
    /// The book's current group.
    pub group: Option<String>,
    /// The beginning of the text, to help classify unfamiliar titles.
    pub excerpt: String,
}

/// A suggested group (only when it differs from the book's current one) and/or author.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub book_id: String,
    pub group: Option<String>,
    pub author: Option<String>,
}

/// A suggestion as the model writes it; some models still answer with a `groups` list.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSuggestion {
    book_id: String,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default)]
    author: Option<String>,
}

#[derive(Deserialize)]
struct SuggestionList {
    suggestions: Vec<RawSuggestion>,
}

fn suggestion_schema() -> serde_json::Value {
    serde_json::json!({
        "title": "shelf_suggestions",
        "type": "object",
        "properties": {
            "suggestions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "bookId": { "type": "string" },
                        "group": { "type": "string" },
                        "author": { "type": ["string", "null"] }
                    },
                    "required": ["bookId", "group", "author"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["suggestions"],
        "additionalProperties": false
    })
}

/// Parses the model's reply (tolerating text around the JSON) and keeps only valid suggestions.
pub fn parse_suggestions(reply: &str, books: &[BookBrief]) -> Result<Vec<Suggestion>> {
    let start = reply
        .find('{')
        .ok_or_else(|| anyhow!("no JSON in the reply"))?;
    let end = reply
        .rfind('}')
        .ok_or_else(|| anyhow!("no JSON in the reply"))?;
    let list: SuggestionList =
        serde_json::from_str(&reply[start..=end]).map_err(|e| anyhow!("invalid JSON: {e}"))?;
    let by_id: HashMap<&str, &BookBrief> = books.iter().map(|b| (b.id.as_str(), b)).collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for s in list.suggestions {
        let Some(book) = by_id.get(s.book_id.as_str()) else {
            continue;
        };
        if !seen.insert(s.book_id.clone()) {
            continue;
        }
        let group = s
            .group
            .into_iter()
            .chain(s.groups)
            .map(|g| g.trim().trim_matches(['「', '」', '"']).to_owned())
            .find(|g| !g.is_empty() && g.chars().count() <= 20)
            // Suggesting the group the book is already in is no suggestion.
            .filter(|g| book.group.as_deref() != Some(g.as_str()));
        // Only fill in authors that are missing; never overwrite the book's own metadata.
        let author = s
            .author
            .map(|a| a.trim().to_owned())
            .filter(|a| !a.is_empty() && a.chars().count() <= 60 && book.author.is_none());
        if group.is_none() && author.is_none() {
            continue;
        }
        out.push(Suggestion {
            book_id: s.book_id,
            group,
            author,
        });
    }
    Ok(out)
}

pub async fn organize(
    r: &Runner<'_>,
    books: &[BookBrief],
    existing_groups: &[String],
    native_schema: bool,
) -> Result<Vec<Suggestion>> {
    let schema = suggestion_schema();
    let mut system = format!(
        "You are a library assistant who sorts the books on a shelf into groups. Each book \
         belongs to exactly one group: suggest one group per book, preferring existing groups \
         and keeping the book's current group when it fits; when a new group is needed, give \
         it a short name in {} (such as Fiction, History, Computing, Economics, Philosophy). For a \
         book with no author, give the author if you are sure of it, otherwise null. Only make \
         suggestions for the listed books, and copy each bookId exactly.",
        r.cfg.answer_language()
    );
    if !native_schema {
        system.push_str("\n\n");
        system.push_str(&rig_core::structured_output::prompted_augmentation(
            &schema.to_string(),
        ));
    }
    let listing = serde_json::to_string_pretty(
        &serde_json::json!({ "existingGroups": existing_groups, "books": books }),
    )?;
    let prompt = format!("Suggest a group for each of these books:\n{listing}");
    let make = |messages: Vec<Message>| -> Result<CompletionRequest> {
        let mut req = CompletionRequest::new(Message::user(prompt.clone()))
            .messages(messages)
            .preamble(system.clone())
            .max_tokens(MERGE_TOKENS);
        if native_schema {
            req = req.output_schema(
                schemars::Schema::try_from(schema.clone()).map_err(|e| anyhow!("{e}"))?,
            );
        }
        Ok(req)
    };
    let reply = complete_text(&r.model, make(Vec::new())?)
        .await
        .map_err(|e| anyhow!(friendly_error(&r.cfg, &e)))?;
    match parse_suggestions(&reply, books) {
        Ok(s) => Ok(s),
        Err(first_error) => {
            // One retry, telling the model what was wrong with its answer.
            let mut req = make(Vec::new())?;
            req = req
                .message(Message::assistant(reply))
                .message(Message::user(format!(
                    "Your answer could not be parsed ({first_error}). Reply with only the required JSON."
                )));
            let reply = complete_text(&r.model, req)
                .await
                .map_err(|e| anyhow!(friendly_error(&r.cfg, &e)))?;
            parse_suggestions(&reply, books)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig_core::test_utils::{MockCompletionModel, MockStreamEvent, MockTurn, mock_final};
    use std::sync::Mutex;

    fn cfg() -> AiConfig {
        AiConfig {
            provider: "mock".into(),
            model: "m".into(),
            ..Default::default()
        }
    }

    /// A streamed reply made of `parts`.
    fn streamed(parts: &[&str]) -> Vec<MockStreamEvent> {
        let mut events: Vec<MockStreamEvent> =
            parts.iter().map(|p| MockStreamEvent::text(*p)).collect();
        events.push(MockStreamEvent::FinalResponse(mock_final(
            Default::default(),
        )));
        events
    }

    #[test]
    fn splits_without_breaking_characters() {
        let text = format!("{}\n{}\nshort", "字".repeat(500), "x".repeat(10));
        let parts = split_by_budget(&text, 200);
        assert!(parts.iter().all(|p| estimate_tokens(p) <= 200 + 2));
        assert_eq!(parts.concat().replace('\n', ""), text.replace('\n', ""));
    }

    #[tokio::test]
    async fn short_unit_streams_one_summary_and_caches_it() {
        let model = MockCompletionModel::from_stream_turns([streamed(&["概", "要"])]).erase();
        let events = Mutex::new(Vec::new());
        let emit = |e: AiEvent| events.lock().unwrap().push(e);
        let r = Runner {
            model,
            cfg: cfg(),
            budget: 1_000,
            emit: &emit,
        };
        let dir = std::env::temp_dir().join(format!("aireader-ai-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::Db::open(&dir.join("t.db")).unwrap();
        let mut cache = SummaryCache::open(&db, "b");
        let unit = Unit {
            key: "ch:0".into(),
            title: "第一章".into(),
            text: "正文".repeat(50),
        };
        let (s, cached) = summarize_cached(&r, &mut cache, "书", &unit).await.unwrap();
        assert_eq!((s.as_str(), cached), ("概要", false));
        let events = std::mem::take(&mut *events.lock().unwrap());
        assert_eq!(
            events[..2],
            [
                AiEvent::Delta { text: "概".into() },
                AiEvent::Delta { text: "要".into() }
            ]
        );
        // The usage of the request, when the service reports it, comes after the text.
        assert!(
            events[2..]
                .iter()
                .all(|e| matches!(e, AiEvent::Usage { .. }))
        );
        // Second time comes from the database without a request.
        let mut cache = SummaryCache::open(&db, "b");
        let (s, cached) = summarize_cached(&r, &mut cache, "书", &unit).await.unwrap();
        assert_eq!((s.as_str(), cached), ("概要", true));
    }

    #[tokio::test]
    async fn long_unit_is_summarised_in_parts_then_merged() {
        let model = MockCompletionModel::from_turns([
            MockTurn::text("甲"),
            MockTurn::text("乙"),
            MockTurn::text("合并"),
        ]);
        let r = Runner {
            model: model.clone().erase(),
            cfg: cfg(),
            budget: 300,
            emit: &|_| {},
        };
        let unit = Unit {
            key: "ch:1".into(),
            title: "长章".into(),
            text: format!("{}\n{}", "字".repeat(280), "文".repeat(280)),
        };
        let s = summarize_unit(&r, "书", &unit, false, None).await.unwrap();
        assert_eq!(s, "合并");
        let requests = model.requests();
        assert_eq!(requests.len(), 3);
        let last = format!("{:?}", requests[2].chat_history.last());
        assert!(
            last.contains("甲") && last.contains("乙"),
            "the merge request includes both partial summaries"
        );
    }

    #[tokio::test]
    async fn summarised_parts_are_kept_for_the_next_run() {
        let dir = std::env::temp_dir().join(format!("aireader-parts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::Db::open(&dir.join("t.db")).unwrap();
        let mut cache = SummaryCache::open(&db, "b");
        let unit = Unit {
            key: "ch:1".into(),
            title: "长章".into(),
            text: format!("{}\n{}", "字".repeat(280), "文".repeat(280)),
        };
        let first = MockCompletionModel::from_turns([
            MockTurn::text("甲"),
            MockTurn::text("乙"),
            MockTurn::text("合并"),
        ]);
        let r = Runner {
            model: first.erase(),
            cfg: cfg(),
            budget: 300,
            emit: &|_| {},
        };
        summarize_unit(&r, "书", &unit, false, Some(&mut cache))
            .await
            .unwrap();
        let model = cfg().model_key();
        assert_eq!(
            cache.get(&model, "ch:1#2/2").map(String::as_str),
            Some("乙")
        );
        // A later run (e.g. after the merge failed) only sends the merge request.
        let second = MockCompletionModel::from_turns([MockTurn::text("再合并")]);
        let r = Runner {
            model: second.clone().erase(),
            ..r
        };
        let s = summarize_unit(&r, "书", &unit, false, Some(&mut cache))
            .await
            .unwrap();
        assert_eq!(s, "再合并");
        assert_eq!(second.requests().len(), 1);
    }

    #[tokio::test]
    async fn chat_builds_conversation_with_selection_context() {
        let model = MockCompletionModel::from_stream_turns([streamed(&["答"])]);
        let r = Runner {
            model: model.clone().erase(),
            cfg: cfg(),
            budget: 1_000,
            emit: &|_| {},
        };
        let history = vec![
            ChatTurn {
                role: "assistant".into(),
                text: "前面的解释".into(),
            },
            ChatTurn {
                role: "user".into(),
                text: "再说说".into(),
            },
        ];
        let s = chat(
            &r,
            "书",
            "选中的话",
            "整段上下文",
            Some(ChatAction::Explain),
            &history,
        )
        .await
        .unwrap();
        assert_eq!(s, "答");
        let req = &model.requests()[0];
        assert!(req.system_instructions().unwrap().contains("选中的话"));
        assert!(req.system_instructions().unwrap().contains("整段上下文"));
        // explain question + assistant answer + newest user message
        assert_eq!(
            req.chat_history
                .iter()
                .filter(|m| !matches!(m.as_ref(), Message::System { .. }))
                .count(),
            3
        );
    }

    fn index() -> BookIndex {
        use super::super::retrieval::Passage;
        let p = |c: usize, title: &str, text: &str| Passage {
            title: title.into(),
            location: Location::Flow {
                chapter: c,
                block: 0,
            },
            text: text.into(),
        };
        BookIndex::from_passages(vec![
            p(0, "第一回", "刘备在桃园与关羽、张飞结义。"),
            p(1, "第二回", "曹操煮酒论英雄。"),
            p(2, "第三回", "诸葛亮草船借箭。"),
            p(3, "第四回", "赤壁之战，周瑜火攻。"),
        ])
    }

    #[test]
    fn passages_combine_position_and_matches_in_reading_order() {
        let index = index();
        // Near chapter 3 (passages 2..), plus the keyword match in chapter 0.
        let picked = select_passages(
            &index,
            Some(Location::Flow {
                chapter: 3,
                block: 0,
            }),
            "桃园",
            &[],
            10_000,
        );
        assert_eq!(picked[0], 0);
        assert!(picked.contains(&3) && picked.contains(&2));
        assert!(picked.windows(2).all(|w| w[0] < w[1]));
        // A tiny budget still returns the best match and nothing over budget.
        assert_eq!(select_passages(&index, None, "曹操", &[], 20), [1]);
        // Semantic matches join keyword ones: "the battle by the river" shares no keyword with
        // passage 3 (赤壁之战), but the semantic ranking found it.
        assert_eq!(
            select_passages(&index, None, "江上的那场大战", &[3], 10_000),
            [3]
        );
        assert_eq!(select_passages(&index, None, "曹操", &[3], 10_000), [1, 3]);
    }

    #[test]
    fn fusion_favours_passages_both_lists_agree_on() {
        assert_eq!(fuse(&[&[1, 2, 3], &[3, 2, 9]]), [3, 2, 1, 9]);
        assert_eq!(fuse(&[&[5], &[]]), [5]);
    }

    #[tokio::test]
    async fn ask_cites_numbered_sources() {
        let model = MockCompletionModel::from_stream_turns([streamed(&["见 [1]"])]);
        let events = Mutex::new(Vec::new());
        let emit = |e: AiEvent| events.lock().unwrap().push(e);
        let r = Runner {
            model: model.clone().erase(),
            cfg: cfg(),
            budget: 10_000,
            emit: &emit,
        };
        let history = vec![ChatTurn {
            role: "user".into(),
            text: "谁在桃园结义？".into(),
        }];
        let s = ask(&r, "三国演义", &index(), None, &history, &[])
            .await
            .unwrap();
        assert_eq!(s, "见 [1]");
        let events = events.lock().unwrap().clone();
        let AiEvent::Sources { sources } = &events[0] else {
            panic!("sources first: {events:?}")
        };
        assert_eq!(sources[0].n, 1);
        assert_eq!(sources[0].title, "第一回");
        let system = model.requests()[0]
            .system_instructions()
            .unwrap()
            .to_owned();
        assert!(system.contains("[1] (第一回)\n刘备在桃园"));
        assert!(ask(&r, "书", &index(), None, &[], &[]).await.is_err());
    }

    #[test]
    fn outlines_are_cleaned() {
        let reply = "好的，下面是导图：\n```mindmap\n- 第一回\n  - 人物\n    * 刘备\n  1. 事件\n```\n希望有帮助";
        assert_eq!(
            clean_outline(reply),
            "- 第一回\n  - 人物\n    - 刘备\n  - 事件"
        );
        assert_eq!(
            clean_outline("# 书\n## 主题\n说明文字\n- 要点"),
            "- 书\n  - 主题\n- 要点"
        );
        assert_eq!(clean_outline("没有列表"), "");
    }

    #[tokio::test]
    async fn chapter_mind_maps_are_cached() {
        use crate::model::{Block, Chapter, Span};
        let model = MockCompletionModel::from_stream_turns([streamed(&["- 第一回\n", "  - 结义"])]);
        let r = Runner {
            model: model.clone().erase(),
            cfg: cfg(),
            budget: 1_000,
            emit: &|_| {},
        };
        let doc = Document {
            path: std::path::PathBuf::new(),
            title: "三国".into(),
            author: None,
            toc: Vec::new(),
            cover: None,
            content: Content::Flow(vec![Chapter {
                title: "第一回".into(),
                blocks: vec![Block::Paragraph(vec![Span::plain("桃园结义")])],
            }]),
        };
        let dir = std::env::temp_dir().join(format!("aireader-mm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::Db::open(&dir.join("t.db")).unwrap();
        let mut cache = SummaryCache::open(&db, "b");
        let scope = || MindMapScope::Chapter { chapter: 0 };
        let first = mind_map(&r, &mut cache, &doc, "三国", scope(), false)
            .await
            .unwrap();
        assert_eq!(first, ("- 第一回\n  - 结义".to_owned(), false));
        let prompt = format!("{:?}", model.requests()[0].chat_history);
        assert!(prompt.contains("桃园结义") && prompt.contains("nested Markdown bullet list"));
        let again = mind_map(&r, &mut cache, &doc, "三国", scope(), false)
            .await
            .unwrap();
        assert_eq!(again, (first.0, true));
        assert_eq!(model.requests().len(), 1, "the cached map needs no request");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn briefs() -> Vec<BookBrief> {
        vec![
            BookBrief {
                id: "a".into(),
                title: "三国演义".into(),
                author: None,
                format: "TXT".into(),
                group: None,
                excerpt: String::new(),
            },
            BookBrief {
                id: "b".into(),
                title: "Rust 编程".into(),
                author: Some("某人".into()),
                format: "EPUB".into(),
                group: Some("计算机".into()),
                excerpt: String::new(),
            },
        ]
    }

    #[test]
    fn suggestions_are_validated() {
        let reply = r#"好的：{"suggestions":[
            {"bookId":"a","group":" 小说 ","author":"罗贯中"},
            {"bookId":"b","group":"计算机","author":"冒名"},
            {"bookId":"zzz","group":"x","author":null},
            {"bookId":"a","group":"重复","author":null}
        ]} 完毕"#;
        let s = parse_suggestions(reply, &briefs()).unwrap();
        // b is already in 计算机 and has an author, so it gets no suggestion.
        assert_eq!(
            s,
            vec![Suggestion {
                book_id: "a".into(),
                group: Some("小说".into()),
                author: Some("罗贯中".into())
            }]
        );
        // An old-style list still yields one group.
        let legacy =
            r#"{"suggestions":[{"bookId":"b","groups":["", "编程", "计算机"],"author":null}]}"#;
        assert_eq!(
            parse_suggestions(legacy, &briefs()).unwrap()[0]
                .group
                .as_deref(),
            Some("编程")
        );
        assert!(parse_suggestions("没有 JSON", &briefs()).is_err());
    }

    #[tokio::test]
    async fn organize_retries_once_on_bad_json() {
        let good = r#"{"suggestions":[{"bookId":"a","group":"小说","author":null}]}"#;
        let model = MockCompletionModel::from_turns([
            MockTurn::text("抱歉，我不太确定"),
            MockTurn::text(good),
        ]);
        let r = Runner {
            model: model.clone().erase(),
            cfg: cfg(),
            budget: 1_000,
            emit: &|_| {},
        };
        let s = organize(&r, &briefs(), &[], false).await.unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(model.requests().len(), 2);
        assert!(
            model.requests()[0]
                .system_instructions()
                .unwrap()
                .contains("bookId"),
            "schema is in the prompt"
        );
    }
}
