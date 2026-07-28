use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Global shortcut syntax accepted by global-hotkey. This value is produced
    /// by the Settings shortcut-capture control rather than typed manually.
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    /// Copy every result to the clipboard and then synthesize Ctrl+V.
    #[serde(default)]
    pub auto_paste: bool,
    /// Show model checking, downloading, extraction, loading, warmup, and ready
    /// notifications.
    #[serde(default = "default_true")]
    pub show_loading_notifications: bool,
    /// Show the recording overlay and live microphone meter.
    #[serde(default = "default_true")]
    pub show_recording_notifications: bool,
    /// Show the transcribing/processing overlay.
    #[serde(default = "default_true")]
    pub show_transcribing_notifications: bool,
    /// Show editable transcription results and compact result/error notices.
    #[serde(default = "default_true")]
    pub show_result_notifications: bool,
    /// Migration input for configurations written before notification controls
    /// were separated. It is intentionally never written back to disk.
    #[serde(default, rename = "show_notifications", skip_serializing)]
    legacy_show_notifications: Option<bool>,
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
            hotkey: default_hotkey(),
            auto_paste: false,
            show_loading_notifications: true,
            show_recording_notifications: true,
            show_transcribing_notifications: true,
            show_result_notifications: true,
            legacy_show_notifications: None,
        }
    }
}

impl Config {
    fn migrate_legacy_notifications(mut self) -> Self {
        if let Some(enabled) = self.legacy_show_notifications.take() {
            self.show_loading_notifications = enabled;
            self.show_recording_notifications = enabled;
            self.show_transcribing_notifications = enabled;
            self.show_result_notifications = enabled;
        }
        self
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
        Ok(s) => match serde_json::from_str::<Config>(&s) {
            Ok(config) => config.migrate_legacy_notifications(),
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
    prepare_private_dir(&dir)?;
    let path = config_path();
    let temporary = path.with_extension("json.part");
    let serialized = serde_json::to_vec_pretty(cfg)?;

    let write_result = (|| -> Result<()> {
        let mut file = File::create(&temporary)
            .with_context(|| format!("create {}", temporary.display()))?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("protect {}", temporary.display()))?;
        file.write_all(&serialized)
            .with_context(|| format!("write {}", temporary.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("finish {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("flush {}", temporary.display()))?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("activate {}", path.display()))?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

pub(crate) fn prepare_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("protect {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_notification_toggle_migrates_to_all_four_controls() {
        let cfg: Config = serde_json::from_str(
            r#"{"model":"parakeet-int8","hotkey":"Backquote","show_notifications":false}"#,
        )
        .unwrap();
        let cfg = cfg.migrate_legacy_notifications();
        assert!(!cfg.show_loading_notifications);
        assert!(!cfg.show_recording_notifications);
        assert!(!cfg.show_transcribing_notifications);
        assert!(!cfg.show_result_notifications);
    }

    #[test]
    fn new_config_does_not_serialize_the_legacy_toggle() {
        let json = serde_json::to_string(&Config::default()).unwrap();
        assert!(!json.contains("show_notifications"));
        assert!(!json.contains("\"model\""));
        assert!(json.contains("show_loading_notifications"));
        assert!(json.contains("show_recording_notifications"));
        assert!(json.contains("show_transcribing_notifications"));
        assert!(json.contains("show_result_notifications"));
    }
}
