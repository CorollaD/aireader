//! AiReader: a desktop reader for EPUB, PDF, TXT, Markdown, MOBI/AZW3, FB2, DOCX and CBZ.

pub mod ai;
pub mod annotations;
pub mod commands;
pub mod formats;
pub mod i18n;
pub mod library;
pub mod model;
pub mod protocol;
pub mod search;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
            // Ignore flags (e.g. `-psn_…` added by macOS) and keep the first existing file.
            let startup = std::env::args_os()
                .skip(1)
                .map(std::path::PathBuf::from)
                .find(|p| p.is_file());
            app.manage(commands::AppState::new(library, startup));
            app.manage(ai::commands::AiState::default());
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("book", protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::list_books,
            commands::import_books,
            commands::take_startup_book,
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
            commands::add_reading_time,
            commands::set_settings,
            commands::set_locale,
            ai::commands::ai_get_config,
            ai::commands::ai_set_config,
            ai::commands::ai_test,
            ai::commands::ai_list_models,
            ai::commands::ai_estimate_book,
            ai::commands::ai_run,
            ai::commands::ai_cancel,
            ai::commands::ai_organize,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AiReader");
}
