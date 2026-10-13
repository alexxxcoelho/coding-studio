//! `coding-studio`: starts the local server and opens the web UI.

use anyhow::{Context, Result};
use clap::Parser;
use cs_app::{router, Startup};
use std::path::PathBuf;

/// Interview coding tool: library, segmentation, human and AI coding, arbitration, audit.
#[derive(Parser, Debug)]
#[command(name = "coding-studio", version)]
struct Args {
    /// SQLite database of the tool (created when absent).
    #[arg(long)]
    db: PathBuf,
    /// Folder holding one sub-folder per interview (saved as the default library).
    #[arg(long)]
    library: Option<PathBuf>,
    /// Legacy coding.db, read only, source of the codebook.
    #[arg(long)]
    coding_db: Option<PathBuf>,
    #[arg(long, default_value_t = 7878)]
    port: u16,
    /// Name recorded as the actor of every action.
    #[arg(long, default_value = "candidate")]
    actor: String,
    /// Name under which the interviewer appears in platform transcripts (repeatable).
    #[arg(long = "interviewer-name")]
    interviewer_names: Vec<String>,
    /// Do not open the browser.
    #[arg(long)]
    no_open: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let state = cs_app::prepare(
        Startup {
            db: args.db.clone(),
            library: args.library.clone(),
            coding_db: args.coding_db.clone(),
            actor: args.actor.clone(),
            interviewer_names: args.interviewer_names.clone(),
        },
        None,
    )?;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], args.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("port {} is busy; try --port", args.port))?;
    let url = format!("http://{addr}/");
    println!("Coding Studio {} on {url}", cs_app::VERSION);
    println!("Database: {}", args.db.display());
    if !args.no_open {
        let _ = open::that(&url);
    }
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
