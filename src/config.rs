use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_model")]
    pub model: String,
    /// Global shortcut syntax accepted by global-hotkey, for example
    /// "Backquote", "F8", or "ctrl+shift+Space".
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    /// Copy every result to the clipboard and then synthesize Ctrl+V.
    #[serde(default)]
    pub auto_paste: bool,
    /// Show the top-center loading/listening/transcribing/result overlay.
    #[serde(default = "default_true")]
    pub show_notifications: bool,
}

fn default_model() -> String {
    "parakeet-int8".into()
}

fn default_hotkey() -> String {
    // The physical ` / ~ key. Shift is intentionally not required.
    "Backquote".into()
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: default_model(),
            hotkey: default_hotkey(),
            auto_paste: false,
            show_notifications: true,
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local-stt")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn models_dir() -> PathBuf {
    config_dir().join("models")
}

pub fn load() -> Config {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(s) => match serde_json::from_str(&s) {
            Ok(config) => config,
            Err(error) => {
                eprintln!(
                    "[local-stt] invalid config at {}: {error}; using defaults",
                    path.display()
                );
                Config::default()
            }
        },
        Err(_) => Config::default(),
    }
}

pub fn save(cfg: &Config) -> Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let path = config_path();
    let s = serde_json::to_string_pretty(cfg)?;
    fs::write(&path, s).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
