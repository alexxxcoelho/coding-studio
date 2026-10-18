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

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use cs_app::{PickKind, Picker};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use config::{DesktopConfig, DB_ENV};

const MAIN: &str = "main";

fn main() {
    tauri::Builder::default()
        .manage(Projects::default())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let app = app.clone();
            let db = args
                .windows(2)
                .find(|w| w[0] == "--db")
                .map(|w| PathBuf::from(&w[1]));
            std::thread::spawn(move || {
                let result = match db {
                    Some(db) => open_project_inner(&app, db, false),
                    None => menu::open_another(&app),
                };
                if let Err(e) = result {
                    error_dialog(&app, &format!("{e:#}"));
                }
            });
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .menu(menu::build)
        .on_menu_event(|app, event| menu::on_event(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                if let Ok(mut projects) = window.app_handle().state::<Projects>().0.lock() {
                    if let Some(key) = projects
                        .iter()
                        .find(|(_, p)| p.label == window.label())
                        .map(|(k, _)| k.clone())
                    {
                        if let Some(p) = projects.remove(&key) {
                            cs_app::cancel_local_preparations(&key);
                            let _ = p.shutdown.send(());
                        }
                    }
                }
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                if let Err(e) = start(&handle) {
                    error_dialog(
                        &handle,
                        &format!("Coding Studio n’a pas pu démarrer.\n\n{e:#}"),
                    );
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Coding Studio failed to run");
}

/// Script run in the page before its own scripts: the address of the API, and
/// on macOS the flag that leaves the sidebar translucent over the window material.
fn init_script(port: u16, project_key: &str) -> String {
    let mut script = format!("window.__CS_API_BASE__ = \"http://127.0.0.1:{port}\";");
    script.push_str(&format!(
        " window.__CS_PROJECT_KEY__ = {};",
        serde_json::to_string(project_key).unwrap()
    ));
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
fn main_window_builder<'a>(
    app: &'a AppHandle,
    port: u16,
    label: &str,
    project_key: &str,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    let builder =
        WebviewWindowBuilder::new(app, label.to_string(), WebviewUrl::App("index.html".into()))
            .title("Coding Studio")
            .inner_size(1440.0, 920.0)
            .min_inner_size(960.0, 600.0)
            .theme(None)
            .initialization_script(init_script(port, project_key));
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
#[derive(Default)]
struct Projects(Mutex<BTreeMap<PathBuf, Project>>);
struct Project {
    label: String,
    shutdown: tokio::sync::oneshot::Sender<()>,
}
static NEXT_PROJECT: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn focused_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    let windows = app.webview_windows();
    windows
        .values()
        .find(|w| w.is_focused().unwrap_or(false))
        .cloned()
        .or_else(|| windows.values().next().cloned())
}
pub(crate) fn focused_database(app: &AppHandle) -> Option<PathBuf> {
    let label = focused_window(app)?.label().to_string();
    app.state::<Projects>()
        .0
        .lock()
        .ok()?
        .iter()
        .find(|(_, p)| p.label == label)
        .map(|(db, _)| db.clone())
}
fn start(app: &AppHandle) -> Result<()> {
    let path = config_path(app)?;
    let cfg = DesktopConfig::load(&path)?;
    let arg_db = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "--db")
        .map(|w| PathBuf::from(&w[1]));
    let explicit = arg_db.is_some() || std::env::var(DB_ENV).is_ok();
    let db = arg_db.or_else(|| cfg.resolve_db(std::env::var(DB_ENV).ok()));
    let db = match db {
        Some(db) => db,
        None => match ask_new_db(app)? {
            Some(db) => db,
            None => {
                app.exit(0);
                return Ok(());
            }
        },
    };
    open_project_inner(app, db, !explicit)
}
pub(crate) fn open_project(app: &AppHandle, db: PathBuf) -> Result<()> {
    open_project_inner(app, db, true)
}
fn open_project_inner(app: &AppHandle, db: PathBuf, remember: bool) -> Result<()> {
    // Canonical parent resolves aliases also for a newly created database.
    let db = if db.is_absolute() {
        db
    } else {
        std::env::current_dir()?.join(db)
    };
    let parent = db
        .parent()
        .ok_or_else(|| anyhow!("invalid database path"))?;
    std::fs::create_dir_all(parent)?;
    let key = if db.exists() {
        db.canonicalize()?
    } else {
        parent.canonicalize()?.join(db.file_name().unwrap())
    };
    let registry = app.state::<Projects>();
    let mut projects = registry
        .0
        .lock()
        .map_err(|_| anyhow!("project registry unavailable"))?;
    if let Some(p) = projects.get(&key) {
        if let Some(w) = app.get_webview_window(&p.label) {
            w.unminimize()?;
            w.show()?;
            w.set_focus()?;
        }
        return Ok(());
    }
    let cfg = DesktopConfig::load(&config_path(app)?)?;
    // Library and codebook live in the project's database; never inherit another project's files.
    let startup = DesktopConfig {
        actor: cfg.actor.clone(),
        ..Default::default()
    }
    .startup(key.clone());
    let state = cs_app::prepare(startup, Some(native_picker(app)))?;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let n = NEXT_PROJECT.fetch_add(1, Ordering::Relaxed);
    let label = if n == 0 {
        MAIN.to_string()
    } else {
        format!("project-{n}")
    };
    let project_key = format!("{}", key.display());
    let window = main_window_builder(app, port, &label, &project_key).build()?;
    window.set_title(&format!("Coding Studio · {}", key.display()))?;
    #[cfg(target_os = "macos")]
    {
        let material_window = window.clone();
        app.run_on_main_thread(move || apply_sidebar_material(&material_window))?;
    }
    let (send, receive) = tokio::sync::oneshot::channel();
    let router = cs_app::router(state).layer(axum::middleware::from_fn(cors::layer));
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("listener: {e}");
                return;
            }
        };
        if let Err(e) = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = receive.await;
            })
            .await
        {
            eprintln!("server: {e}");
        }
    });
    projects.insert(
        key.clone(),
        Project {
            label,
            shutdown: send,
        },
    );
    drop(projects);
    // Persist only the last database; old global library/codebook defaults must not bleed into it.
    let mut remembered = cfg;
    remembered.db = Some(key);
    remembered.library = None;
    remembered.coding_db = None;
    if remember {
        remembered.save(&config_path(app)?)?;
    }
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
