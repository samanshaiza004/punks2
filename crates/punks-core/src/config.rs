use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybinds {
    #[serde(default = "default_navigate_up")]
    pub navigate_up: String,
    #[serde(default = "default_navigate_down")]
    pub navigate_down: String,
    #[serde(default = "default_navigate_back")]
    pub navigate_back: String,
    #[serde(default = "default_confirm")]
    pub confirm: String,
}

fn default_navigate_up() -> String {
    "W".into()
}
fn default_navigate_down() -> String {
    "S".into()
}
fn default_navigate_back() -> String {
    "A".into()
}
fn default_confirm() -> String {
    "D".into()
}

impl Default for Keybinds {
    fn default() -> Self {
        Keybinds {
            navigate_up: default_navigate_up(),
            navigate_down: default_navigate_down(),
            navigate_back: default_navigate_back(),
            confirm: default_confirm(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PunksConfig {
    #[serde(default)]
    pub last_directory: Option<PathBuf>,
    #[serde(default)]
    pub keybinds: Keybinds,
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("punks").join("config.json"))
}

pub fn load() -> PunksConfig {
    let Some(path) = config_path() else {
        return PunksConfig::default();
    };

    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
            log::warn!("failed to parse {}: {e}", path.display());
            PunksConfig::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PunksConfig::default(),
        Err(e) => {
            log::warn!("failed to read {}: {e}", path.display());
            PunksConfig::default()
        }
    }
}

pub fn save(config: &PunksConfig) {
    let Some(path) = config_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("failed to create {}: {e}", parent.display());
            return;
        }
    }

    let json = match serde_json::to_string_pretty(config) {
        Ok(j) => j,
        Err(e) => {
            log::warn!("failed to serialize config: {e}");
            return;
        }
    };

    if let Err(e) = std::fs::write(&path, json) {
        log::warn!("failed to write {}: {e}", path.display());
    }
}
