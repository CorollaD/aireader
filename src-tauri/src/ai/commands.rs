//! Tauri commands for AI settings and tasks. Long tasks run in the background and report
//! through a `Channel`; they can be cancelled by id.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{Result, bail};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use super::config::{AiConfig, KeyringStore, SecretStore};
use super::provider::{self, ChatModel};
use super::tasks::{
    self, AiEvent, AiTask, BookBrief, BookEstimate, Runner, Suggestion, SummaryCache,
};
use crate::commands::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

/// AI state kept alongside [`AppState`].
pub struct AiState {
    secrets: Box<dyn SecretStore>,
    /// Running tasks by id, for cancellation.
    running: Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            secrets: Box::new(KeyringStore),
            running: Mutex::new(HashMap::new()),
        }
    }
}

fn app_state(app: &AppHandle) -> &AppState {
    app.state::<AppState>().inner()
}

fn ai_state(app: &AppHandle) -> &AiState {
    app.state::<AiState>().inner()
}

fn config(app: &AppHandle) -> AiConfig {
    app_state(app).library().ai_config().clone()
}

/// The configured model, refusing cloud services the user has not agreed to send book text to.
fn model(app: &AppHandle) -> Result<(ChatModel, AiConfig)> {
    let cfg = config(app);
    if cfg.is_configured() && !cfg.is_local() && !cfg.consented {
        bail!("使用云端 AI 服务前，需要先同意发送书籍内容");
    }
    connect(app, cfg)
}

fn connect(app: &AppHandle, cfg: AiConfig) -> Result<(ChatModel, AiConfig)> {
    let key = ai_state(app).secrets.get(&cfg.key_name());
    Ok((provider::connect(&cfg, key)?, cfg))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    config: AiConfig,
    has_key: bool,
    local: bool,
    configured: bool,
}

fn status(app: &AppHandle) -> AiStatus {
    let config = config(app);
    AiStatus {
        has_key: ai_state(app).secrets.get(&config.key_name()).is_some(),
        local: config.is_local(),
        configured: config.is_configured(),
        config,
    }
}

#[tauri::command]
pub async fn ai_get_config(app: AppHandle) -> CmdResult<AiStatus> {
    tauri::async_runtime::spawn_blocking(move || status(&app))
        .await
        .map_err(err)
}

/// Saves settings. `api_key`: `None` keeps the stored key, `Some("")` removes it.
#[tauri::command]
pub async fn ai_set_config(
    app: AppHandle,
    config: AiConfig,
    api_key: Option<String>,
) -> CmdResult<AiStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = config.cleaned();
        if let Some(key) = api_key {
            let secrets = &ai_state(&app).secrets;
            if key.trim().is_empty() {
                secrets.delete(&config.key_name()).map_err(err)?;
            } else {
                secrets
                    .set(&config.key_name(), key.trim())
                    .map_err(|e| format!("无法保存到系统钥匙串：{e:#}"))?;
            }
        }
        let mut lib = app_state(&app).library();
        lib.set_ai_config(config);
        lib.save().map_err(err)?;
        drop(lib);
        Ok(status(&app))
    })
    .await
    .map_err(err)?
}

/// Sends a tiny request and returns the model's reply.
#[tauri::command]
pub async fn ai_test(app: AppHandle) -> CmdResult<String> {
    use rig_core::completion::{CompletionRequest, Message};
    // The test sends no book text, so it does not need the user's consent.
    let (model, cfg) = connect(&app, config(&app)).map_err(err)?;
    let request =
        CompletionRequest::new(Message::user("请只回复四个字：连接成功")).max_tokens(64u64);
    let reply = provider::complete_text(&model, request)
        .await
        .map_err(|e| provider::friendly_error(&cfg, &e))?;
    Ok(reply.trim().chars().take(80).collect())
}

/// Models installed locally (Ollama only; other services use preset lists).
#[tauri::command]
pub async fn ai_list_models(config: AiConfig) -> CmdResult<Vec<String>> {
    let config = config.cleaned();
    if config.provider != "ollama" {
        return Ok(Vec::new());
    }
    provider::list_ollama_models(&config).await.map_err(err)
}

async fn open_book(
    app: &AppHandle,
    id: &str,
) -> Result<(std::sync::Arc<crate::commands::OpenBook>, String)> {
    let (app2, id2) = (app.clone(), id.to_owned());
    let book =
        tauri::async_runtime::spawn_blocking(move || app_state(&app2).open_book(&id2)).await??;
    let title = app_state(app)
        .library()
        .get(id)
        .map(|b| b.title.clone())
        .unwrap_or_default();
    Ok((book, title))
}

fn cache(app: &AppHandle, id: &str) -> SummaryCache {
    SummaryCache::open(app_state(app).library().ai_dir(id))
}

#[tauri::command]
pub async fn ai_estimate_book(app: AppHandle, book_id: String) -> CmdResult<BookEstimate> {
    let (model, cfg) = model(&app).map_err(err)?;
    let (book, _) = open_book(&app, &book_id).await.map_err(err)?;
    let budget = provider::input_budget(&cfg, &model);
    let units = tasks::book_units(&book.doc, budget).map_err(err)?;
    Ok(tasks::estimate_book(
        &units,
        &cache(&app, &book_id),
        &cfg,
        provider::pricing(&model),
    ))
}

async fn run_task(
    app: &AppHandle,
    task: AiTask,
    emit: &(dyn Fn(AiEvent) + Send + Sync),
) -> Result<(String, bool)> {
    let (model, cfg) = model(app)?;
    let budget = provider::input_budget(&cfg, &model);
    let runner = Runner {
        model,
        cfg,
        budget,
        emit,
    };
    match task {
        AiTask::SummarizeChapter { book_id, chapter } => {
            let (book, title) = open_book(app, &book_id).await?;
            let unit = tasks::chapter_unit(&book.doc, chapter)?;
            tasks::summarize_cached(&runner, &mut cache(app, &book_id), &title, &unit).await
        }
        AiTask::SummarizePages { book_id, from, to } => {
            let (book, title) = open_book(app, &book_id).await?;
            let unit = tasks::pages_unit(&book.doc, from, to)?;
            tasks::summarize_cached(&runner, &mut cache(app, &book_id), &title, &unit).await
        }
        AiTask::SummarizeBook { book_id } => {
            let (book, title) = open_book(app, &book_id).await?;
            let units = tasks::book_units(&book.doc, budget)?;
            tasks::summarize_book(&runner, &mut cache(app, &book_id), &title, &units).await
        }
        AiTask::Chat {
            book_title,
            selection,
            context,
            action,
            history,
        } => {
            let text =
                tasks::chat(&runner, &book_title, &selection, &context, action, &history).await?;
            Ok((text, false))
        }
    }
}

/// Starts a task in the background; progress, text and the result arrive on `on_event`.
#[tauri::command]
pub fn ai_run(app: AppHandle, task_id: String, task: AiTask, on_event: Channel<AiEvent>) {
    let id = task_id.clone();
    let app2 = app.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let channel = on_event.clone();
        let emit = move |e: AiEvent| {
            let _ = channel.send(e);
        };
        let event = match run_task(&app2, task, &emit).await {
            Ok((text, cached)) => AiEvent::Done { text, cached },
            Err(e) => AiEvent::Error {
                message: format!("{e:#}"),
            },
        };
        let _ = on_event.send(event);
        ai_state(&app2).running.lock().unwrap().remove(&id);
    });
    ai_state(&app)
        .running
        .lock()
        .unwrap()
        .insert(task_id, handle);
}

/// Stops a running task; dropping its stream cancels the provider request.
#[tauri::command]
pub fn ai_cancel(app: AppHandle, task_id: String) {
    if let Some(handle) = ai_state(&app).running.lock().unwrap().remove(&task_id) {
        handle.abort();
    }
}

const ORGANIZE_BATCH: usize = 40;
const EXCERPT_CHARS: usize = 160;

/// Group/author suggestions for the given books. Nothing is changed until the user applies them.
#[tauri::command]
pub async fn ai_organize(app: AppHandle, book_ids: Vec<String>) -> CmdResult<Vec<Suggestion>> {
    let (model, cfg) = model(&app).map_err(err)?;
    let (entries, groups) = {
        let lib = app_state(&app).library();
        let entries: Vec<_> = book_ids
            .iter()
            .filter_map(|id| lib.get(id).cloned())
            .collect();
        (entries, lib.groups().to_vec())
    };
    if entries.is_empty() {
        return Err("没有要整理的书".into());
    }
    // Excerpts help with unfamiliar titles, but parsing many books is slow.
    let with_excerpts = entries.len() <= 30;
    let mut briefs = Vec::with_capacity(entries.len());
    for b in entries {
        let excerpt = if with_excerpts {
            excerpt(&app, &b.id).await
        } else {
            String::new()
        };
        briefs.push(BookBrief {
            id: b.id,
            title: b.title,
            author: b.author,
            format: b.format,
            groups: b.groups,
            excerpt,
        });
    }
    let native = model.spec().is_some_and(|s| s.structured_output);
    let budget = provider::input_budget(&cfg, &model);
    let runner = Runner {
        model,
        cfg,
        budget,
        emit: &|_| {},
    };
    let mut out = Vec::new();
    for batch in briefs.chunks(ORGANIZE_BATCH) {
        out.extend(
            tasks::organize(&runner, batch, &groups, native)
                .await
                .map_err(err)?,
        );
    }
    Ok(out)
}

async fn excerpt(app: &AppHandle, id: &str) -> String {
    let Ok((book, _)) = open_book(app, id).await else {
        return String::new();
    };
    let text = match &book.doc.content {
        crate::model::Content::Flow(chapters) => chapters
            .iter()
            .flat_map(|c| &c.blocks)
            .map(|b| b.text())
            .filter(|t| t.trim().chars().count() > 20)
            .take(3)
            .collect::<Vec<_>>()
            .join(" "),
        crate::model::Content::Paged(src) => (0..src.page_count().min(3))
            .filter_map(|p| src.page_text(p))
            .collect::<Vec<_>>()
            .join(" "),
    };
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(EXCERPT_CHARS)
        .collect()
}
