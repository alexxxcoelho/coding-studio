//! Settings of the desktop app, kept as JSON in the platform config folder
//! (`~/Library/Application Support/ch.dossantos.codingstudio/config.json` on macOS).
//! The database itself never lives there unless the user chooses it: its
//! location is the first thing asked on first launch.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Overrides the database path, mainly for tests and scripted runs.
pub const DB_ENV: &str = "CODING_STUDIO_DB";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopConfig {
    /// SQLite database of the tool.
    pub db: Option<PathBuf>,
    /// Interview folder, saved as the library root at start-up.
    pub library: Option<PathBuf>,
    /// Thesis coding.db, read only, imported when the codebook is empty.
    pub coding_db: Option<PathBuf>,
    /// Name recorded as the actor of every action. Default `candidate`.
    pub actor: Option<String>,
    /// Interviewer names in platform transcripts. Empty: built-in defaults.
    #[serde(default)]
    pub interviewer_names: Vec<String>,
}

impl DesktopConfig {
    /// Reads the file; a missing file gives the default settings, a broken
    /// one is an error (never silently replaced).
    pub fn load(path: &Path) -> Result<DesktopConfig> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("{} is not valid settings JSON", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DesktopConfig::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text + "\n").with_context(|| format!("writing {}", path.display()))
    }

    /// Database to open: the environment override first, then the saved choice.
    /// `None` means the user must be asked.
    pub fn resolve_db(&self, env_value: Option<String>) -> Option<PathBuf> {
        env_value
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| self.db.clone())
    }

    pub fn startup(&self, db: PathBuf) -> cs_app::Startup {
        cs_app::Startup {
            db,
            library: self.library.clone(),
            coding_db: self.coding_db.clone(),
            actor: self
                .actor
                .clone()
                .filter(|a| !a.trim().is_empty())
                .unwrap_or_else(|| "candidate".to_string()),
            interviewer_names: self.interviewer_names.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/config.json");
        let cfg = DesktopConfig::load(&path).unwrap();
        assert_eq!(cfg, DesktopConfig::default());

        let cfg = DesktopConfig {
            db: Some("/x/studio.db".into()),
            actor: Some("Alex".into()),
            ..DesktopConfig::default()
        };
        cfg.save(&path).unwrap();
        assert_eq!(DesktopConfig::load(&path).unwrap(), cfg);
    }

    #[test]
    fn broken_file_is_an_error_not_a_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(DesktopConfig::load(&path).is_err());
    }

    #[test]
    fn env_override_wins_and_blank_actor_defaults() {
        let cfg = DesktopConfig {
            db: Some("/saved.db".into()),
            actor: Some("  ".into()),
            ..DesktopConfig::default()
        };
        assert_eq!(cfg.resolve_db(None), Some(PathBuf::from("/saved.db")));
        assert_eq!(
            cfg.resolve_db(Some("/env.db".into())),
            Some(PathBuf::from("/env.db"))
        );
        assert_eq!(
            cfg.resolve_db(Some(" ".into())),
            Some(PathBuf::from("/saved.db"))
        );
        assert_eq!(DesktopConfig::default().resolve_db(None), None);
        assert_eq!(cfg.startup("/a.db".into()).actor, "candidate");
    }
}
