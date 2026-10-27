//! AiReader: a desktop reader for EPUB, PDF, TXT, Markdown, MOBI/AZW3, FB2, DOCX and CBZ.

pub mod ai;
pub mod annotations;
pub mod commands;
pub mod db;
pub mod dedupe;
pub mod formats;
pub mod i18n;
pub mod library;
pub mod model;
pub mod protocol;
pub mod search;

use std::path::{Path, PathBuf};

use tauri::{Manager, RunEvent, WindowEvent};

/// The first existing file among command-line arguments (flags such as macOS's `-psn_…` are
/// skipped), relative to `cwd`.
fn file_arg(args: impl IntoIterator<Item = impl Into<PathBuf>>, cwd: &Path) -> Option<PathBuf> {
    args.into_iter()
        .map(|a| cwd.join(a.into()))
        .find(|p| p.is_file())
}

pub fn run() {
    let app = tauri::Builder::default()
        // Must come first: a second launch (e.g. opening a book from the file manager while
        // the app runs) hands its file to this instance and quits, so only one process ever
        // writes the library.
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            if let Some(file) = file_arg(args.into_iter().skip(1), Path::new(&cwd)) {
                commands::request_open(app, file);
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        // Logs go to the terminal and to the app's log folder (e.g. ~/Library/Logs/<identifier>).
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(2 << 20)
                .build(),
        )
        .setup(|app| {
            if let Ok(dir) = app.path().resource_dir() {
                formats::pdf::set_resource_dir(dir);
            }
            // `AIREADER_LIBRARY_DIR` points the app at another library (tests, portable use).
            let root = match std::env::var_os("AIREADER_LIBRARY_DIR") {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_data_dir()?.join("library"),
            };
            let library = library::Library::open(&root)?;
            let cwd = std::env::current_dir().unwrap_or_default();
            let startup = file_arg(std::env::args_os().skip(1), &cwd);
            app.manage(commands::AppState::new(library, startup));
            app.manage(ai::commands::AiState::default());
            Ok(())
        })
        // Closing goes through the frontend first, so it can save what is still pending.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                commands::begin_close(window.app_handle());
            }
        })
        .register_asynchronous_uri_scheme_protocol("book", protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::list_books,
            commands::import_books,
            commands::take_startup_book,
            commands::exit_app,
            commands::log_error,
            commands::update_book,
            commands::remove_books,
            commands::create_group,
            commands::rename_group,
            commands::delete_group,
            commands::open_book,
            commands::get_chapter,
            commands::get_page_text,
            commands::search_book,
            commands::save_progress,
            commands::get_settings,
            commands::get_annotations,
            commands::save_annotations,
            commands::export_notes,
            commands::list_chats,
            commands::save_chat,
            commands::delete_chat,
            commands::add_reading_time,
            commands::set_settings,
            commands::set_locale,
            commands::save_export,
            commands::backup_library,
            commands::list_fonts,
            commands::import_font,
            commands::remove_font,
            commands::find_duplicates,
            commands::merge_duplicates,
            ai::commands::ai_get_config,
            ai::commands::ai_set_config,
            ai::commands::ai_test,
            ai::commands::ai_list_models,
            ai::commands::ai_estimate_book,
            ai::commands::ai_run,
            ai::commands::ai_cancel,
            ai::commands::ai_semantic_status,
            ai::commands::ai_organize,
        ])
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(e) => {
            log::error!("AiReader failed to start: {e:#}");
            eprintln!("AiReader failed to start: {e:#}");
            std::process::exit(1);
        }
    };
    app.run(|app, event| match event {
        // Quitting from the menu or the keyboard (⌘Q) also saves first; an exit the app asked
        // for itself carries a code and goes ahead.
        RunEvent::ExitRequested {
            code: None, api, ..
        } => {
            api.prevent_exit();
            commands::begin_close(app);
        }
        // Files opened from the Finder (macOS hands them over as events, not arguments).
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        RunEvent::Opened { urls } => {
            if let Some(file) = urls
                .iter()
                .filter_map(|u| u.to_file_path().ok())
                .find(|p| p.is_file())
            {
                commands::request_open(app, file);
            }
        }
        _ => {}
    });
}
