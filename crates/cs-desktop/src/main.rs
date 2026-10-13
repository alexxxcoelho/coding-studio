//! Coding Studio desktop app.
//!
//! The same local server as the `coding-studio` binary, started on a free
//! port of 127.0.0.1 and shown in a native window. Nothing else changes: the
//! protocol rules, the database and the audit trail are those of cs-app and
//! cs-store. What the desktop adds is native dialogs (folders, files, the
//! database location) and a Database menu.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use cs_app::{PickKind, Picker};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use config::{DesktopConfig, DB_ENV};

const MAIN: &str = "main";

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .menu(build_menu)
        .on_menu_event(|app, event| {
            let app = app.clone();
            let id = event.id().as_ref().to_string();
            // Dialogs block, so they never run on the UI thread.
            std::thread::spawn(move || {
                if let Err(err) = on_menu(&app, &id) {
                    error_dialog(&app, &format!("{err:#}"));
                }
            });
        })
        .setup(|app| {
            let window = main_window_builder(app).build()?;
            #[cfg(target_os = "macos")]
            apply_sidebar_material(&window);
            #[cfg(not(target_os = "macos"))]
            let _ = window;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                if let Err(err) = start(&handle) {
                    error_dialog(
                        &handle,
                        &format!("Coding Studio could not start.\n\n{err:#}"),
                    );
                    handle.exit(1);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Coding Studio failed to run");
}

/// The main window. On macOS the title bar is an overlay with a hidden title,
/// so that the web layer draws its own toolbar under the traffic lights, and
/// the window is transparent so that the sidebar material (see
/// `apply_sidebar_material`) shows through. The theme follows the system.
fn main_window_builder(app: &tauri::App) -> WebviewWindowBuilder<'_, tauri::Wry, tauri::App> {
    let builder = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
        .title("Coding Studio")
        .inner_size(1440.0, 920.0)
        .min_inner_size(960.0, 600.0)
        .theme(None);
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .transparent(true)
        // Read by the web layer (web/src/lib/desktop.ts): leave the sidebar translucent.
        .initialization_script("window.__CS_VIBRANCY__ = true;");
    builder
}

/// Sidebar material behind the web layer, which keeps the sidebar background
/// translucent when it sees `__CS_VIBRANCY__`.
#[cfg(target_os = "macos")]
fn apply_sidebar_material(window: &tauri::WebviewWindow) {
    use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
    // A failure only costs the material: the web layer then draws its own
    // opaque sidebar colours.
    if let Err(err) = apply_vibrancy(window, NSVisualEffectMaterial::Sidebar, None, None) {
        eprintln!("vibrancy: {err}");
    }
}

fn config_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_config_dir()
        .context("no configuration folder on this system")?
        .join("config.json"))
}

/// Opens the database (asking where it lives on first launch), starts the
/// server and points the window at it.
fn start(app: &AppHandle) -> Result<()> {
    let cfg_path = config_path(app)?;
    let mut cfg = DesktopConfig::load(&cfg_path)?;
    let db = match cfg.resolve_db(std::env::var(DB_ENV).ok()) {
        Some(db) => db,
        None => match ask_new_db(app)? {
            Some(db) => db,
            None => {
                app.exit(0);
                return Ok(());
            }
        },
    };
    if cfg.db.as_ref() != Some(&db) && std::env::var(DB_ENV).is_err() {
        cfg.db = Some(db.clone());
        cfg.save(&cfg_path)?;
    }

    let state = cs_app::prepare(cfg.startup(db.clone()), Some(native_picker(app)))?;
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).context("no free local port")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("listener: {e}");
                return;
            }
        };
        if let Err(e) = axum::serve(listener, cs_app::router(state)).await {
            eprintln!("server stopped: {e}");
        }
    });

    let url = format!("http://127.0.0.1:{port}/");
    println!("Coding Studio {} on {url}", cs_app::VERSION);
    println!("Database: {}", db.display());
    let window = app
        .get_webview_window(MAIN)
        .ok_or_else(|| anyhow!("main window missing"))?;
    window.navigate(url.parse()?)?;
    window.set_title(&format!(
        "Coding Studio, {}",
        db.file_name().unwrap_or_default().to_string_lossy()
    ))?;
    Ok(())
}

fn native_picker(app: &AppHandle) -> Picker {
    let app = app.clone();
    Arc::new(move |kind, title| {
        let dialog = app.dialog().file().set_title(title);
        let chosen = match kind {
            PickKind::Folder => dialog.blocking_pick_folder(),
            PickKind::File => dialog.blocking_pick_file(),
        };
        chosen.and_then(|p| p.into_path().ok())
    })
}

fn ask_new_db(app: &AppHandle) -> Result<Option<PathBuf>> {
    let chosen = app
        .dialog()
        .file()
        .set_title("Where should Coding Studio keep its database?")
        .set_file_name("coding_studio.db")
        .add_filter("SQLite database", &["db"])
        .blocking_save_file();
    chosen
        .map(|p| p.into_path().map_err(|e| anyhow!("{e}")))
        .transpose()
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    // The default menu keeps Edit (copy, paste, select all), which the
    // prompt and answer text areas need.
    let menu = Menu::default(app)?;
    let database = Submenu::with_items(
        app,
        "Database",
        true,
        &[
            &MenuItem::with_id(app, "db_open", "Open Database...", true, None::<&str>)?,
            &MenuItem::with_id(app, "db_new", "New Database...", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "db_reveal", "Show Database Folder", true, None::<&str>)?,
        ],
    )?;
    menu.append(&database)?;
    Ok(menu)
}

fn on_menu(app: &AppHandle, id: &str) -> Result<()> {
    let cfg_path = config_path(app)?;
    let mut cfg = DesktopConfig::load(&cfg_path)?;
    let chosen = match id {
        "db_open" => app
            .dialog()
            .file()
            .set_title("Open a Coding Studio database")
            .add_filter("SQLite database", &["db"])
            .blocking_pick_file()
            .map(|p| p.into_path().map_err(|e| anyhow!("{e}")))
            .transpose()?,
        "db_new" => ask_new_db(app)?,
        "db_reveal" => {
            if let Some(dir) = cfg.db.as_ref().and_then(|d| d.parent()) {
                open::that(dir).with_context(|| format!("opening {}", dir.display()))?;
            }
            return Ok(());
        }
        _ => return Ok(()),
    };
    if let Some(db) = chosen {
        cfg.db = Some(db);
        cfg.save(&cfg_path)?;
        // Restarting is the simplest way to guarantee a single open database.
        app.restart();
    }
    Ok(())
}

fn error_dialog(app: &AppHandle, message: &str) {
    app.dialog()
        .message(message)
        .title("Coding Studio")
        .kind(MessageDialogKind::Error)
        .blocking_show();
}
