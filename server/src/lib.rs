//! AiReader's sync server: accounts, and a private folder of files for each account that the
//! app syncs through (the same file operations it uses with WebDAV).

pub mod accounts;
pub mod api;
pub mod store;

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    /// Holds `accounts.db` and `files/<account id>/`.
    pub data: PathBuf,
    /// Whether anyone may create an account; otherwise accounts are made on the command line.
    pub open_registration: bool,
    /// Most bytes an account may store; 0 for no limit.
    pub quota_bytes: u64,
    pub max_upload_bytes: u64,
}

/// Opens the data folder and builds the HTTP service.
pub fn app(config: Config) -> anyhow::Result<axum::Router> {
    std::fs::create_dir_all(config.data.join("files"))?;
    let accounts = accounts::Accounts::open(&config.data.join("accounts.db"))?;
    Ok(api::router(api::AppState::new(config, accounts)))
}
