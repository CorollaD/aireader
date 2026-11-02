//! `aireader-server` serves the API; `aireader-server user …` manages accounts.

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use aireader_server::Config;
use aireader_server::accounts::Accounts;

#[derive(Parser)]
#[command(version, about = "Sync server for AiReader")]
struct Cli {
    /// Where accounts and files are kept.
    #[arg(long, env = "AIREADER_DATA", default_value = "data", global = true)]
    data: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    serve: Serve,
}

#[derive(clap::Args)]
struct Serve {
    /// Address to listen on. Put HTTPS in front of it (Caddy, Nginx…) when it faces the internet.
    #[arg(long, env = "AIREADER_LISTEN", default_value = "0.0.0.0:8080")]
    listen: String,
    /// Let anyone create an account from the app.
    #[arg(long, env = "AIREADER_OPEN_REGISTRATION")]
    open_registration: bool,
    /// Storage per account in megabytes; 0 for no limit.
    #[arg(long, env = "AIREADER_QUOTA_MB", default_value_t = 0)]
    quota_mb: u64,
    /// Largest single file in megabytes.
    #[arg(long, env = "AIREADER_MAX_UPLOAD_MB", default_value_t = 1024)]
    max_upload_mb: u64,
}

#[derive(Subcommand)]
enum Command {
    /// Manage accounts.
    User {
        #[command(subcommand)]
        action: UserAction,
    },
}

#[derive(Subcommand)]
enum UserAction {
    /// Add an account. The password is asked for (or read from the AIREADER_PASSWORD variable).
    Add { name: String },
    /// Set a new password; the account is signed out on every device.
    Passwd { name: String },
    /// Remove an account and its files.
    Delete { name: String },
    /// List the accounts.
    List,
}

/// A new password, typed twice at a terminal, or taken from `AIREADER_PASSWORD`.
fn ask_password() -> Result<String> {
    if let Ok(password) = std::env::var("AIREADER_PASSWORD") {
        return Ok(password);
    }
    if !std::io::stdin().is_terminal() {
        bail!(
            "no terminal to ask for the password: set AIREADER_PASSWORD, or run with `-it` under Docker"
        );
    }
    let first = rpassword::prompt_password("Password: ")?;
    let again = rpassword::prompt_password("Again: ")?;
    if first != again {
        bail!("the two passwords differ");
    }
    Ok(first)
}

fn manage(data: &std::path::Path, action: UserAction) -> Result<()> {
    std::fs::create_dir_all(data).with_context(|| format!("can't create {}", data.display()))?;
    let accounts = Accounts::open(&data.join("accounts.db"))?;
    match action {
        UserAction::Add { name } => {
            aireader_server::accounts::check_name(&name)?;
            let user = accounts.add(&name, &ask_password()?)?;
            println!("added {}", user.name);
        }
        UserAction::Passwd { name } => {
            accounts.set_password(&name, &ask_password()?)?;
            println!("new password set for {name}; it is signed out everywhere");
        }
        UserAction::Delete { name } => {
            let id = accounts.remove(&name)?;
            let files = data.join("files").join(id.to_string());
            if files.exists() {
                std::fs::remove_dir_all(&files)?;
            }
            println!("deleted {name} and its files");
        }
        UserAction::List => {
            for (name, created) in accounts.list()? {
                println!("{name}\t(created {created})");
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let cli = Cli::parse();
    if let Some(Command::User { action }) = cli.command {
        return manage(&cli.data, action);
    }
    let config = Config {
        data: cli.data,
        open_registration: cli.serve.open_registration,
        quota_bytes: cli.serve.quota_mb * 1024 * 1024,
        max_upload_bytes: cli.serve.max_upload_mb * 1024 * 1024,
    };
    let app = aireader_server::app(config.clone())?;
    let accounts = Accounts::open(&config.data.join("accounts.db"))?;
    if accounts.is_empty()? && !config.open_registration {
        tracing::warn!(
            "no accounts yet: add one with `aireader-server user add <name>`, or start with --open-registration"
        );
    }
    drop(accounts);
    let listener = tokio::net::TcpListener::bind(&cli.serve.listen)
        .await
        .with_context(|| format!("can't listen on {}", cli.serve.listen))?;
    tracing::info!(
        "listening on {} (data in {})",
        cli.serve.listen,
        config.data.display()
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let ctrl_c = tokio::signal::ctrl_c();
            #[cfg(unix)]
            {
                let mut term =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("signal handler");
                tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
            }
            #[cfg(not(unix))]
            let _ = ctrl_c.await;
        })
        .await?;
    Ok(())
}
