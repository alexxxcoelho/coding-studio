//! Coding Studio desktop app.
//!
//! The same local server as the `coding-studio` binary, started on a free
//! port of 127.0.0.1, and a native window that shows the web interface.
//! The interface is not served by that server here: it is loaded by the
//! webview itself, from Tauri's asset protocol in a packaged app and from the
//! Vite dev server under `cargo tauri dev`. The window learns the address of
//! the API from `window.__CS_API_BASE__`. Nothing else changes: the protocol
//! rules, the database and the audit trail are those of cs-app and cs-store.
//! What the desktop adds is a native menu in French, native dialogs (folders,
//! files, the database location), a remembered window position and a single
//! running instance.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod cors;
mod menu;

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use cs_app::{PickKind, Picker};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use config::{DesktopConfig, DB_ENV};

const MAIN: &str = "main";

fn main() {
    tauri::Builder::default()
        // First, so that a second launch hands over to the running app before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window(MAIN) {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .menu(menu::build)
        .on_menu_event(|app, event| menu::on_event(app, event.id().as_ref()))
        .setup(|app| {
            // The port is bound now so that the page can be told the API address
            // at once; requests wait in the backlog until the server runs.
            let listener = TcpListener::bind(("127.0.0.1", 0)).context("no free local port")?;
            listener.set_nonblocking(true)?;
            let port = listener.local_addr()?.port();
            let window = main_window_builder(app, port).build()?;
            #[cfg(target_os = "macos")]
            apply_sidebar_material(&window);
            #[cfg(not(target_os = "macos"))]
            let _ = window;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                if let Err(err) = start(&handle, listener) {
                    error_dialog(
                        &handle,
                        &format!("Coding Studio n'a pas pu démarrer.\n\n{err:#}"),
                    );
                    handle.exit(1);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Coding Studio failed to run");
}

/// Script run in the page before its own scripts: the address of the API, and
/// on macOS the flag that leaves the sidebar translucent over the window material.
fn init_script(port: u16) -> String {
    let mut script = format!("window.__CS_API_BASE__ = \"http://127.0.0.1:{port}\";");
    if cfg!(target_os = "macos") {
        // Read by the web layer (web/src/lib/desktop.ts).
        script.push_str(" window.__CS_VIBRANCY__ = true;");
    }
    script
}

/// The main window. On macOS the title bar is an overlay with a hidden title,
/// so that the web layer draws its own toolbar under the traffic lights, which
/// sit in the sidebar (design/macos/Main.dc.html), and the window is
/// transparent so that the sidebar material (see `apply_sidebar_material`)
/// shows through. The theme follows the system. Size and position are restored
/// by the window-state plugin.
fn main_window_builder(
    app: &tauri::App,
    port: u16,
) -> WebviewWindowBuilder<'_, tauri::Wry, tauri::App> {
    let builder = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
        .title("Coding Studio")
        .inner_size(1440.0, 920.0)
        .min_inner_size(960.0, 600.0)
        .theme(None)
        .initialization_script(init_script(port));
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        // tao sets the title bar container to (button height + y), so the
        // buttons are centred about y points from the top (measured: y = 13
        // gave a centre at 14). The web layer draws a 52 px toolbar line that
        // starts 8 px down (app-win padding): centre 34, hence y = 33.
        .traffic_light_position(tauri::LogicalPosition::new(22.0, 33.0))
        .transparent(true);
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

/// Opens the database (asking where it lives on first launch) and serves the
/// API on the listener that the page already knows.
fn start(app: &AppHandle, listener: TcpListener) -> Result<()> {
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
    let port = listener.local_addr()?.port();
    let router = cs_app::router(state).layer(axum::middleware::from_fn(cors::layer));
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("listener: {e}");
                return;
            }
        };
        if let Err(e) = axum::serve(listener, router).await {
            eprintln!("server stopped: {e}");
        }
    });

    println!(
        "Coding Studio {} API on http://127.0.0.1:{port}/",
        cs_app::VERSION
    );
    println!("Database: {}", db.display());
    let window = app
        .get_webview_window(MAIN)
        .ok_or_else(|| anyhow!("main window missing"))?;
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
        .set_title("Où Coding Studio doit-il conserver sa base de données ?")
        .set_file_name("coding_studio.db")
        .add_filter("Base SQLite", &["db"])
        .blocking_save_file();
    chosen
        .map(|p| p.into_path().map_err(|e| anyhow!("{e}")))
        .transpose()
}

fn error_dialog(app: &AppHandle, message: &str) {
    app.dialog()
        .message(message)
        .title("Coding Studio")
        .kind(MessageDialogKind::Error)
        .blocking_show();
}
