//! The AI features: chapter/page/book summaries, chat about a text selection, and bookshelf
//! organisation. Everything here works on a [`ChatModel`] and plain data, so it can be tested
//! with rig's mock models.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use rig_core::completion::{CompletionRequest, Message};
use serde::{Deserialize, Serialize};

use super::config::AiConfig;
use super::provider::{ChatModel, complete_text, estimate_tokens, friendly_error, stream_text};
use crate::model::{Content, Document};

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
}

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
            "你是一位细致的读书助手。只根据提供的书籍内容回答，不要编造书中没有的情节、人物或观点。请使用{}回答。",
            self.cfg.language
        )
    }

    fn request(&self, prompt: String, max_tokens: u64) -> CompletionRequest {
        CompletionRequest::new(Message::user(prompt))
            .preamble(self.system())
            .max_tokens(max_tokens)
    }

    async fn complete(&self, prompt: String, max_tokens: u64) -> Result<String> {
        complete_text(&self.model, self.request(prompt, max_tokens))
            .await
            .map_err(|e| anyhow!(friendly_error(&self.cfg, &e)))
    }

    async fn stream(&self, prompt: String, max_tokens: u64) -> Result<String> {
        self.stream_request(self.request(prompt, max_tokens)).await
    }

    async fn stream_request(&self, request: CompletionRequest) -> Result<String> {
        stream_text(&self.model, request, |t| {
            (self.emit)(AiEvent::Delta { text: t.to_owned() })
        })
        .await
        .map_err(|e| anyhow!(friendly_error(&self.cfg, &e)))
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
        bail!("这本书不是文字类书籍")
    };
    let ch = chapters.get(chapter).ok_or_else(|| anyhow!("章节不存在"))?;
    let text = ch
        .blocks
        .iter()
        .map(|b| b.text())
        .filter(|t| !t.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        bail!("这一章没有文字内容");
    }
    let title = if ch.title.is_empty() {
        format!("第 {} 章", chapter + 1)
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
        bail!("这本书不是按页显示的文档")
    };
    let to = to.min(src.page_count().saturating_sub(1));
    let text = (from..=to)
        .filter_map(|p| src.page_text(p))
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        bail!("这些页面没有可提取的文字（可能是扫描版或图片）");
    }
    let title = if from == to {
        format!("第 {} 页", from + 1)
    } else {
        format!("第 {}–{} 页", from + 1, to + 1)
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
                        title: format!("第 {}–{} 页", start + 1, p),
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
                    title: format!("第 {}–{} 页", start + 1, src.page_count()),
                    text,
                });
            }
            units.retain(|u| !u.text.trim().is_empty());
            units
        }
    };
    if units.is_empty() {
        bail!("这本书没有可以总结的文字（可能是扫描版或图片）");
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
    file: PathBuf,
    data: BTreeMap<String, BTreeMap<String, String>>,
}

impl SummaryCache {
    pub fn open(dir: PathBuf) -> Self {
        let file = dir.join("summaries.json");
        let data = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { file, data }
    }

    pub fn get(&self, model: &str, unit: &str) -> Option<&String> {
        self.data.get(model)?.get(unit)
    }

    pub fn put(&mut self, model: &str, unit: &str, summary: String) -> Result<()> {
        self.data
            .entry(model.into())
            .or_default()
            .insert(unit.into(), summary);
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.data)?)?;
        std::fs::rename(tmp, &self.file)?;
        Ok(())
    }
}

// --- summaries -------------------------------------------------------------------------------

/// Summarises one unit. Text over the budget is summarised in parts, then merged. With
/// `stream`, the final answer is streamed as `Delta` events.
pub async fn summarize_unit(
    r: &Runner<'_>,
    book: &str,
    unit: &Unit,
    stream: bool,
) -> Result<String> {
    if estimate_tokens(&unit.text) <= r.budget {
        let prompt = format!(
            "下面是《{book}》的「{}」。请总结这部分内容：先用一两段话概括主要内容，再用列表列出关键要点（人物、事件、观点或论证）。\n\n<content>\n{}\n</content>",
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
            label: format!("「{}」第 {}/{} 部分", unit.title, i + 1, parts.len()),
        });
        let prompt = format!(
            "下面是《{book}》「{}」的第 {}/{} 部分。请简要总结这一部分的要点，之后会与其他部分合并。\n\n<content>\n{part}\n</content>",
            unit.title,
            i + 1,
            parts.len()
        );
        partials.push(r.complete(prompt, SUMMARY_TOKENS).await?);
    }
    let joined = partials
        .iter()
        .enumerate()
        .map(|(i, s)| format!("## 第 {} 部分\n{s}", i + 1))
        .collect::<Vec<_>>()
        .join("\n\n");
    let prompt = format!(
        "下面是《{book}》「{}」各部分的摘要。请合并成一份完整的总结：先概述，再列出关键要点，去掉重复内容。\n\n{joined}",
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
    let s = summarize_unit(r, book, unit, true).await?;
    cache.put(&model, &unit.key, s.clone())?;
    Ok((s, false))
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
    let mut summaries = Vec::with_capacity(units.len());
    for (i, unit) in units.iter().enumerate() {
        (r.emit)(AiEvent::Progress {
            done: i,
            total: units.len(),
            label: format!("正在总结「{}」", unit.title),
        });
        let s = match cache.get(&model, &unit.key) {
            Some(s) => s.clone(),
            None => {
                let s = summarize_unit(r, book, unit, false).await?;
                cache.put(&model, &unit.key, s.clone())?;
                s
            }
        };
        summaries.push(format!("## {}\n{s}", unit.title));
    }
    (r.emit)(AiEvent::Progress {
        done: units.len(),
        total: units.len(),
        label: "正在汇总全书".into(),
    });
    // Merge in groups until everything fits in one request.
    while estimate_tokens(&summaries.join("\n\n")) > r.budget && summaries.len() > 1 {
        let mut merged = Vec::new();
        for group in split_by_budget(&summaries.join("\n\n"), r.budget) {
            let prompt = format!(
                "下面是《{book}》若干章节的摘要。请把它们合并成一份更简洁的摘要，保留各章的标题和关键内容。\n\n{group}"
            );
            merged.push(r.complete(prompt, MERGE_TOKENS).await?);
        }
        if merged.len() >= summaries.len() {
            break;
        }
        summaries = merged;
    }
    let prompt = format!(
        "下面是《{book}》各部分的摘要。请写一份全书总结，包括：1. 全书主旨（一段话）；2. 结构与各部分脉络；3. 核心观点或主要情节；4. 值得注意的细节。\n\n{}",
        summaries.join("\n\n")
    );
    let s = r.stream(prompt, MERGE_TOKENS).await?;
    cache.put(&model, "book", s.clone())?;
    Ok((s, false))
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

fn mostly_cjk(text: &str) -> bool {
    let total = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
    let cjk = text
        .chars()
        .filter(|c| matches!(*c as u32, 0x3400..=0x9FFF | 0xF900..=0xFAFF))
        .count();
    cjk * 2 > total
}

pub async fn chat(
    r: &Runner<'_>,
    book_title: &str,
    selection: &str,
    context: &str,
    action: Option<ChatAction>,
    history: &[ChatTurn],
) -> Result<String> {
    let mut system = format!(
        "{}\n\n书名：《{book_title}》\n用户在书中选中了这段文字：\n<selection>\n{selection}\n</selection>",
        r.system()
    );
    if !context.trim().is_empty() && context.trim() != selection.trim() {
        system.push_str(&format!(
            "\n它所在的上下文：\n<context>\n{context}\n</context>"
        ));
    }
    let first = action.map(|a| match a {
        ChatAction::Explain => {
            "请解释选中的文字：它的含义，以及涉及的背景、典故或术语。如有上下文，请结合上下文说明。"
                .to_owned()
        }
        ChatAction::Translate => {
            let target = if mostly_cjk(selection) && r.cfg.language.contains('中') {
                "英文".to_owned()
            } else {
                r.cfg.language.clone()
            };
            format!(
                "请把选中的文字翻译成{target}，保持原文的语气；难以直译的地方，在译文后简要注释。"
            )
        }
        ChatAction::Summarize => "请概括选中文字的要点。".to_owned(),
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
        bail!("没有要发送的问题")
    };
    let request = CompletionRequest::new(last)
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
    pub groups: Vec<String>,
    /// The beginning of the text, to help classify unfamiliar titles.
    pub excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub book_id: String,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Deserialize)]
struct SuggestionList {
    suggestions: Vec<Suggestion>,
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
                        "groups": { "type": "array", "items": { "type": "string" } },
                        "author": { "type": ["string", "null"] }
                    },
                    "required": ["bookId", "groups", "author"],
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
    let start = reply.find('{').ok_or_else(|| anyhow!("回复中没有 JSON"))?;
    let end = reply.rfind('}').ok_or_else(|| anyhow!("回复中没有 JSON"))?;
    let list: SuggestionList =
        serde_json::from_str(&reply[start..=end]).map_err(|e| anyhow!("JSON 格式不对：{e}"))?;
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
        let mut groups: Vec<String> = Vec::new();
        for g in s
            .groups
            .iter()
            .map(|g| g.trim().trim_matches(['「', '」', '"']).to_owned())
        {
            if !g.is_empty() && g.chars().count() <= 20 && !groups.contains(&g) && groups.len() < 3
            {
                groups.push(g);
            }
        }
        // Only fill in authors that are missing; never overwrite the book's own metadata.
        let author = s
            .author
            .map(|a| a.trim().to_owned())
            .filter(|a| !a.is_empty() && a.chars().count() <= 60 && book.author.is_none());
        if groups.is_empty() && author.is_none() {
            continue;
        }
        out.push(Suggestion {
            book_id: s.book_id,
            groups,
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
        "你是图书管理助手，负责给书架上的书分组。每本书建议 1–2 个分组，优先使用已有分组；需要新分组时用简短的{}名称（例如：小说、历史、计算机、经济、哲学）。作者为空的书，如果你能确定作者，可以给出作者，否则作者填 null。只为列出的书给建议，bookId 必须原样使用。",
        r.cfg.language
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
    let prompt = format!("请为下面这些书给出分组建议：\n{listing}");
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
                    "上面的回答无法解析（{first_error}）。请只返回符合要求的 JSON。"
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
        let mut cache = SummaryCache::open(dir.clone());
        let unit = Unit {
            key: "ch:0".into(),
            title: "第一章".into(),
            text: "正文".repeat(50),
        };
        let (s, cached) = summarize_cached(&r, &mut cache, "书", &unit).await.unwrap();
        assert_eq!((s.as_str(), cached), ("概要", false));
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                AiEvent::Delta { text: "概".into() },
                AiEvent::Delta { text: "要".into() }
            ]
        );
        // Second time comes from disk without a request.
        let mut cache = SummaryCache::open(dir);
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
        let s = summarize_unit(&r, "书", &unit, false).await.unwrap();
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

    fn briefs() -> Vec<BookBrief> {
        vec![
            BookBrief {
                id: "a".into(),
                title: "三国演义".into(),
                author: None,
                format: "TXT".into(),
                groups: vec![],
                excerpt: String::new(),
            },
            BookBrief {
                id: "b".into(),
                title: "Rust 编程".into(),
                author: Some("某人".into()),
                format: "EPUB".into(),
                groups: vec![],
                excerpt: String::new(),
            },
        ]
    }

    #[test]
    fn suggestions_are_validated() {
        let reply = r#"好的：{"suggestions":[
            {"bookId":"a","groups":[" 小说 ","历史","小说","古典","多余"],"author":"罗贯中"},
            {"bookId":"b","groups":["计算机"],"author":"冒名"},
            {"bookId":"zzz","groups":["x"],"author":null},
            {"bookId":"a","groups":["重复"],"author":null}
        ]} 完毕"#;
        let s = parse_suggestions(reply, &briefs()).unwrap();
        assert_eq!(
            s,
            vec![
                Suggestion {
                    book_id: "a".into(),
                    groups: vec!["小说".into(), "历史".into(), "古典".into()],
                    author: Some("罗贯中".into())
                },
                Suggestion {
                    book_id: "b".into(),
                    groups: vec!["计算机".into()],
                    author: None
                },
            ]
        );
        assert!(parse_suggestions("没有 JSON", &briefs()).is_err());
    }

    #[tokio::test]
    async fn organize_retries_once_on_bad_json() {
        let good = r#"{"suggestions":[{"bookId":"a","groups":["小说"],"author":null}]}"#;
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
