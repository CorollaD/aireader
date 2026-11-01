//! The sync commands, and the timer that runs a sync now and then.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::remote::{Remote, WebDav};
use super::{ROOT, SyncConfig};
use crate::ai::commands::{secret, set_secret};
use crate::commands::{CmdResult, err, shared_library, state};
use crate::t;

const PASSWORD: &str = "sync:webdav";
const FIRST_SYNC: Duration = Duration::from_secs(15);
const INTERVAL: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
pub struct SyncState {
    running: AtomicBool,
    last_error: Mutex<Option<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    config: SyncConfig,
    has_password: bool,
    last_sync: Option<u64>,
    running: bool,
    error: Option<String>,
}

fn saved_config(app: &AppHandle) -> Result<SyncConfig> {
    super::config(&state(app).library().db().conn())
}

pub fn enabled(app: &AppHandle) -> bool {
    saved_config(app).is_ok_and(|c| !c.backend.is_empty())
}

/// Opens the place a configuration points to.
fn remote_for(config: &SyncConfig, password: Option<String>) -> Result<Remote> {
    match config.backend.as_str() {
        "webdav" => Ok(Remote::WebDav(WebDav::new(
            &config.url,
            ROOT,
            &config.username,
            &password.unwrap_or_default(),
        )?)),
        "folder" => {
            // A cloud folder that isn't there (a drive not mounted) must not be silently recreated.
            if !Path::new(&config.folder).is_dir() {
                return Err(anyhow!(t!("sync_no_folder")));
            }
            Ok(Remote::Folder(Path::new(&config.folder).join(ROOT)))
        }
        _ => Err(anyhow!(t!("sync_off"))),
    }
}

fn saved_remote(app: &AppHandle) -> Result<(SyncConfig, Remote)> {
    let config = saved_config(app)?;
    let remote = remote_for(&config, secret(app, PASSWORD))?;
    Ok((config, remote))
}

fn changed_status(app: &AppHandle) {
    let _ = app.emit("sync-status", ());
}

/// Runs a sync unless one is running or sync is off. The outcome stays in the status.
async fn sync(app: &AppHandle, quick: bool) -> Result<()> {
    let st = app.state::<SyncState>();
    if !enabled(app) || st.running.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    changed_status(app);
    let result = async {
        let (config, remote) = saved_remote(app)?;
        super::run(
            shared_library(app),
            &remote,
            config.files,
            quick,
            |merged| {
                let _ = app.emit("sync-changed", merged);
            },
        )
        .await
    }
    .await;
    st.running.store(false, Ordering::SeqCst);
    *st.last_error.lock().unwrap() = result.as_ref().err().map(|e| format!("{e:#}"));
    changed_status(app);
    if let Err(e) = &result {
        log::warn!("sync failed: {e:#}");
    }
    result.map(|_| ())
}

/// Syncs soon after starting, then every few minutes.
pub fn start_timer(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_SYNC).await;
        loop {
            let _ = sync(&app, false).await;
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

#[tauri::command]
pub fn sync_status(app: AppHandle) -> CmdResult<SyncStatus> {
    let st = app.state::<SyncState>();
    let last_sync = super::last_sync(&state(&app).library().db().conn()).map_err(err)?;
    Ok(SyncStatus {
        config: saved_config(&app).map_err(err)?,
        has_password: secret(&app, PASSWORD).is_some(),
        last_sync,
        running: st.running.load(Ordering::SeqCst),
        error: st.last_error.lock().unwrap().clone(),
    })
}

/// The password for a form's contents: what was typed, or the saved one if it is for this server.
fn form_password(
    app: &AppHandle,
    config: &SyncConfig,
    typed: Option<String>,
) -> Result<Option<String>> {
    Ok(typed.or_else(|| {
        saved_config(app)
            .is_ok_and(|saved| super::same_server(&saved, config))
            .then(|| secret(app, PASSWORD))
            .flatten()
    }))
}

#[tauri::command]
pub fn sync_save(
    app: AppHandle,
    config: SyncConfig,
    password: Option<String>,
) -> CmdResult<SyncStatus> {
    let password = form_password(&app, &config, password).map_err(err)?;
    if config.backend == "webdav" {
        remote_for(&config, None).map_err(err)?;
    }
    set_secret(&app, PASSWORD, password.as_deref().unwrap_or("")).map_err(err)?;
    let conn_state = state(&app);
    super::set_config(&conn_state.library().db().conn(), &config).map_err(err)?;
    sync_status(app)
}

/// Checks that a form's contents work, before they are saved.
#[tauri::command]
pub async fn sync_test(
    app: AppHandle,
    config: SyncConfig,
    password: Option<String>,
) -> CmdResult<()> {
    let password = form_password(&app, &config, password).map_err(err)?;
    remote_for(&config, password)
        .map_err(err)?
        .check()
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn sync_now(app: AppHandle, quick: bool) -> CmdResult<()> {
    sync(&app, quick).await.map_err(err)
}

/// Fetches a book's file from the sync folder.
#[tauri::command]
pub async fn sync_fetch_book(app: AppHandle, id: String) -> CmdResult<()> {
    let (_, remote) = saved_remote(&app).map_err(err)?;
    super::fetch_book(shared_library(&app), &remote, &id)
        .await
        .map_err(err)?;
    crate::commands::start_indexing(&app);
    Ok(())
}
