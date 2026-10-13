//! Native menu bar, in French.
//!
//! Items that act on the web layer (Réglages, sidebar, guide, import, export)
//! dispatch a `cs-menu` DOM event on `window`; web/src/lib/desktop.ts listens
//! to it. Items about the database (open, create, reveal) are handled here.

use anyhow::{anyhow, Context, Result};
use tauri::menu::{
    AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::config::DesktopConfig;
use crate::{ask_new_db, config_path, error_dialog, MAIN};

/// Menu ids that the web layer carries out; they are also the `detail` of the DOM event.
const WEB_ACTIONS: [(&str, &str); 5] = [
    ("settings", "settings"),
    ("toggle_sidebar", "toggle-sidebar"),
    ("guide", "guide"),
    ("import", "import"),
    ("export", "export"),
];

fn item(
    app: &AppHandle,
    id: &str,
    text: &str,
    accel: Option<&str>,
) -> tauri::Result<MenuItem<tauri::Wry>> {
    MenuItem::with_id(app, id, text, true, accel)
}

pub fn build(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let about = AboutMetadata {
        name: Some("Coding Studio".into()),
        version: Some(cs_app::VERSION.into()),
        copyright: app.config().bundle.copyright.clone(),
        ..Default::default()
    };
    let sep = || PredefinedMenuItem::separator(app);

    let application = Submenu::with_items(
        app,
        "Coding Studio",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("À propos de Coding Studio"), Some(about))?,
            &sep()?,
            &item(app, "settings", "Réglages…", Some("CmdOrCtrl+,"))?,
            &sep()?,
            &PredefinedMenuItem::hide(app, Some("Masquer Coding Studio"))?,
            &PredefinedMenuItem::hide_others(app, Some("Masquer les autres"))?,
            &PredefinedMenuItem::show_all(app, Some("Tout afficher"))?,
            &sep()?,
            &PredefinedMenuItem::quit(app, Some("Quitter Coding Studio"))?,
        ],
    )?;

    let file = Submenu::with_items(
        app,
        "Fichier",
        true,
        &[
            &item(app, "db_open", "Ouvrir un projet…", Some("CmdOrCtrl+O"))?,
            &item(app, "db_new", "Nouveau projet…", Some("CmdOrCtrl+N"))?,
            &item(app, "db_reveal", "Afficher le dossier du projet", None)?,
            &sep()?,
            &item(
                app,
                "import",
                "Importer des transcriptions…",
                Some("CmdOrCtrl+Shift+I"),
            )?,
            &item(app, "export", "Exporter…", Some("CmdOrCtrl+Shift+E"))?,
            &sep()?,
            &PredefinedMenuItem::close_window(app, Some("Fermer la fenêtre"))?,
        ],
    )?;

    // Predefined items: copy, paste and undo reach the text fields of the webview.
    let edit = Submenu::with_items(
        app,
        "Édition",
        true,
        &[
            &PredefinedMenuItem::undo(app, Some("Annuler"))?,
            &PredefinedMenuItem::redo(app, Some("Rétablir"))?,
            &sep()?,
            &PredefinedMenuItem::cut(app, Some("Couper"))?,
            &PredefinedMenuItem::copy(app, Some("Copier"))?,
            &PredefinedMenuItem::paste(app, Some("Coller"))?,
            &PredefinedMenuItem::select_all(app, Some("Tout sélectionner"))?,
        ],
    )?;

    let view = Submenu::with_items(
        app,
        "Présentation",
        true,
        &[
            &item(
                app,
                "toggle_sidebar",
                "Afficher la barre latérale",
                Some("CmdOrCtrl+Alt+S"),
            )?,
            &sep()?,
            &PredefinedMenuItem::fullscreen(app, Some("Plein écran"))?,
        ],
    )?;

    // The ids make macOS treat these as its own Window and Help menus.
    let window = Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        "Fenêtre",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("Réduire"))?,
            &PredefinedMenuItem::maximize(app, Some("Agrandir"))?,
        ],
    )?;

    let help = Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        "Aide",
        true,
        &[&item(app, "guide", "Guide de Coding Studio", None)?],
    )?;

    Menu::with_items(app, &[&application, &file, &edit, &view, &window, &help])
}

/// Handles a menu click. Dialogs block, so this never runs on the UI thread.
pub fn on_event(app: &AppHandle, id: &str) {
    if let Some((_, action)) = WEB_ACTIONS.iter().find(|(menu_id, _)| *menu_id == id) {
        notify_web(app, action);
        return;
    }
    let app = app.clone();
    let id = id.to_string();
    std::thread::spawn(move || {
        if let Err(err) = on_database_item(&app, &id) {
            error_dialog(&app, &format!("{err:#}"));
        }
    });
}

/// Dispatches `cs-menu` with the action name on the window of the web layer.
fn notify_web(app: &AppHandle, action: &str) {
    let Some(window) = app.get_webview_window(MAIN) else {
        return;
    };
    let detail = serde_json::to_string(action).unwrap_or_else(|_| "\"\"".into());
    let js = format!("window.dispatchEvent(new CustomEvent('cs-menu', {{ detail: {detail} }}));");
    if let Err(err) = window.eval(js) {
        eprintln!("menu: {err}");
    }
}

fn on_database_item(app: &AppHandle, id: &str) -> Result<()> {
    let cfg_path = config_path(app)?;
    let mut cfg = DesktopConfig::load(&cfg_path)?;
    let chosen = match id {
        "db_open" => app
            .dialog()
            .file()
            .set_title("Ouvrir un projet Coding Studio")
            .add_filter("Base SQLite", &["db"])
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
