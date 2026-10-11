//! `coding-studio`: starts the local server and opens the web UI.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use clap::Parser;
use cs_app::{router, AppState};
use cs_store::Store;

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
    /// Thesis coding.db, read only, source of the codebook.
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
    if let Some(parent) = args.db.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let store = Store::open(&args.db).with_context(|| format!("opening {}", args.db.display()))?;
    if let Some(lib) = &args.library {
        let lib = lib.display().to_string();
        if store.setting("library_root")?.as_deref() != Some(lib.as_str()) {
            store.set_setting("library_root", &lib, &args.actor)?;
        }
    }
    if let Some(cdb) = &args.coding_db {
        if store.codebook()?.codes.is_empty() {
            let book = store
                .import_codebook(cdb, &args.actor)
                .with_context(|| format!("importing the codebook from {}", cdb.display()))?;
            println!("Codebook imported: {} codes", book.codes.len());
        }
    }
    let interviewer_names = if args.interviewer_names.is_empty() {
        cs_ingest::DEFAULT_INTERVIEWER_NAMES
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        args.interviewer_names.clone()
    };
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
        db_path: args.db.clone(),
        coding_db: args.coding_db.clone(),
        actor: args.actor.clone(),
        interviewer_names,
    };
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
