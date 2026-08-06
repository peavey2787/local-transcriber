//! Shared configuration schema, normalization, and atomic persistence workflow.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::audio::InputDeviceSelection;

pub const MIN_NOTIFICATION_SECONDS: u32 = 1;
pub const MAX_NOTIFICATION_SECONDS: u32 = 60;
const DEFAULT_NOTIFICATION_SECONDS: u32 = 6;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceCommand {
    pub phrase: String,
    #[serde(default)]
    pub scripts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_device: Option<InputDeviceSelection>,
    #[serde(default)]
    pub auto_paste: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    #[serde(default)]
    pub press_enter_after_paste: bool,
    #[serde(default)]
    pub voice_commands_enabled: bool,
    #[serde(default)]
    pub voice_commands_hotkey: String,
    #[serde(default)]
    pub voice_commands: Vec<VoiceCommand>,
    #[serde(default = "default_notification_seconds")]
    pub notification_duration_seconds: u32,
    #[serde(default = "default_true")]
    pub show_loading_notifications: bool,
    #[serde(default = "default_true")]
    pub show_recording_notifications: bool,
    #[serde(default = "default_true")]
    pub show_transcribing_notifications: bool,
    #[serde(default = "default_true")]
    pub show_result_notifications: bool,
}

fn default_hotkey() -> String {
    "Backquote".into()
}

fn default_notification_seconds() -> u32 {
    DEFAULT_NOTIFICATION_SECONDS
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            recording_device: None,
            auto_paste: false,
            append_trailing_space: false,
            press_enter_after_paste: false,
            voice_commands_enabled: false,
            voice_commands_hotkey: String::new(),
            voice_commands: Vec::new(),
            notification_duration_seconds: default_notification_seconds(),
            show_loading_notifications: true,
            show_recording_notifications: true,
            show_transcribing_notifications: true,
            show_result_notifications: true,
        }
    }
}

impl Config {
    pub fn normalized(mut self) -> Self {
        self.notification_duration_seconds = self
            .notification_duration_seconds
            .clamp(MIN_NOTIFICATION_SECONDS, MAX_NOTIFICATION_SECONDS);
        self
    }

    pub fn notification_seconds(&self) -> f64 {
        self.notification_duration_seconds
            .clamp(MIN_NOTIFICATION_SECONDS, MAX_NOTIFICATION_SECONDS) as f64
    }
}

/// Native storage operations kept outside the shared configuration logic.
pub trait ConfigStorage {
    fn config_dir(&self) -> PathBuf;
    fn prepare_directory(&self, path: &Path) -> Result<()>;

    fn protect_temporary_file(&self, _path: &Path) -> Result<()> {
        Ok(())
    }

    fn replace_file(&self, temporary: &Path, destination: &Path) -> Result<()>;
}

pub fn config_path(storage: &dyn ConfigStorage) -> PathBuf {
    storage.config_dir().join("config.json")
}

pub fn models_dir(storage: &dyn ConfigStorage) -> PathBuf {
    storage.config_dir().join("models")
}

pub fn load(storage: &dyn ConfigStorage) -> Config {
    let path = config_path(storage);
    match fs::read_to_string(&path) {
        Ok(serialized) => match serde_json::from_str::<Config>(&serialized) {
            Ok(config) => config.normalized(),
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

pub fn save(storage: &dyn ConfigStorage, config: &Config) -> Result<()> {
    let dir = storage.config_dir();
    storage.prepare_directory(&dir)?;
    let path = config_path(storage);
    let temporary = path.with_extension("json.part");
    let serialized = serde_json::to_vec_pretty(config)?;

    let write_result = (|| -> Result<()> {
        let mut file =
            File::create(&temporary).with_context(|| format!("create {}", temporary.display()))?;
        storage.protect_temporary_file(&temporary)?;
        file.write_all(&serialized)
            .with_context(|| format!("write {}", temporary.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("finish {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("flush {}", temporary.display()))?;
        drop(file);
        storage.replace_file(&temporary, &path)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_include_system_microphone_and_six_second_notices() {
        let config = Config::default();
        assert!(config.recording_device.is_none());
        assert_eq!(config.notification_duration_seconds, 6);
    }

    #[test]
    fn notification_duration_is_clamped() {
        let too_long: Config = serde_json::from_str(
            r#"{"hotkey":"Backquote","notification_duration_seconds":999}"#,
        )
        .unwrap();
        let too_short: Config = serde_json::from_str(
            r#"{"hotkey":"Backquote","notification_duration_seconds":0}"#,
        )
        .unwrap();

        assert_eq!(too_long.normalized().notification_duration_seconds, 60);
        assert_eq!(too_short.normalized().notification_duration_seconds, 1);
    }

    #[test]
    fn config_round_trips_shared_fields() {
        let config = Config {
            recording_device: Some(InputDeviceSelection {
                name: "USB Microphone".to_string(),
                occurrence: 1,
            }),
            voice_commands_enabled: true,
            voice_commands_hotkey: "control+shift+KeyV".to_string(),
            voice_commands: vec![VoiceCommand {
                phrase: "Open reports".to_string(),
                scripts: vec!["report.py".to_string()],
            }],
            ..Config::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.recording_device, config.recording_device);
        assert_eq!(decoded.voice_commands, config.voice_commands);
        assert!(!json.contains("label"));
    }
}
