use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::audio::InputDeviceSelection;

pub(crate) const MIN_NOTIFICATION_SECONDS: u32 = 1;
pub(crate) const MAX_NOTIFICATION_SECONDS: u32 = 60;
const DEFAULT_NOTIFICATION_SECONDS: u32 = 6;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct VoiceCommand {
    pub(crate) phrase: String,
    #[serde(default)]
    pub(crate) scripts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Config {
    /// Global shortcut syntax accepted by global-hotkey. This value is produced
    /// by the Settings shortcut-capture control rather than typed manually.
    #[serde(default = "default_hotkey")]
    pub(crate) hotkey: String,
    /// The selected microphone. `None` follows the system default device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recording_device: Option<InputDeviceSelection>,
    /// Copy every result to the clipboard and then synthesize Ctrl+V.
    #[serde(default)]
    pub(crate) auto_paste: bool,
    /// Add one trailing space to non-empty transcription text before delivery.
    #[serde(default)]
    pub(crate) append_trailing_space: bool,
    /// Press Enter after a successful automatic paste.
    #[serde(default)]
    pub(crate) press_enter_after_paste: bool,
    /// Enable a second recording mode that maps spoken phrases to scripts.
    #[serde(default)]
    pub(crate) voice_commands_enabled: bool,
    /// Global shortcut for voice-command recording. Must differ from `hotkey`.
    #[serde(default)]
    pub(crate) voice_commands_hotkey: String,
    /// Exact phrase-to-script-chain mappings.
    #[serde(default)]
    pub(crate) voice_commands: Vec<VoiceCommand>,
    /// Seconds before temporary notices and untouched results close.
    #[serde(default = "default_notification_seconds")]
    pub(crate) notification_duration_seconds: u32,
    /// Show model checking, downloading, extraction, loading, warmup, and ready
    /// notifications.
    #[serde(default = "default_true")]
    pub(crate) show_loading_notifications: bool,
    /// Show the recording overlay and live microphone meter.
    #[serde(default = "default_true")]
    pub(crate) show_recording_notifications: bool,
    /// Show the transcribing/processing overlay.
    #[serde(default = "default_true")]
    pub(crate) show_transcribing_notifications: bool,
    /// Show editable transcription results and compact result/error notices.
    #[serde(default = "default_true")]
    pub(crate) show_result_notifications: bool,
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
    fn normalized(mut self) -> Self {
        self.notification_duration_seconds = self
            .notification_duration_seconds
            .clamp(MIN_NOTIFICATION_SECONDS, MAX_NOTIFICATION_SECONDS);
        self
    }

    pub(crate) fn notification_seconds(&self) -> f64 {
        self.notification_duration_seconds
            .clamp(MIN_NOTIFICATION_SECONDS, MAX_NOTIFICATION_SECONDS) as f64
    }
}

pub(crate) fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local-stt")
}

pub(crate) fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub(crate) fn models_dir() -> PathBuf {
    config_dir().join("models")
}

pub(crate) fn load() -> Config {
    let path = config_path();
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

pub(crate) fn save(config: &Config) -> Result<()> {
    let dir = config_dir();
    prepare_private_dir(&dir)?;
    let path = config_path();
    let temporary = path.with_extension("json.part");
    let serialized = serde_json::to_vec_pretty(config)?;

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
    fn defaults_include_system_microphone_and_six_second_notices() {
        let config = Config::default();
        assert!(config.recording_device.is_none());
        assert_eq!(config.notification_duration_seconds, 6);
    }

    #[test]
    fn notification_duration_is_clamped_when_loaded() {
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
    fn recording_device_round_trips_without_display_state() {
        let config = Config {
            recording_device: Some(InputDeviceSelection {
                name: "USB Microphone".to_string(),
                occurrence: 1,
            }),
            ..Config::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.recording_device, config.recording_device);
        assert!(!json.contains("label"));
    }

    #[test]
    fn voice_commands_round_trip() {
        let config = Config {
            voice_commands_enabled: true,
            voice_commands_hotkey: "control+shift+KeyV".to_string(),
            voice_commands: vec![VoiceCommand {
                phrase: "Open reports".to_string(),
                scripts: vec![r"/home/user/scripts/report.sh".to_string()],
            }],
            ..Config::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: Config = serde_json::from_str(&json).unwrap();
        assert!(decoded.voice_commands_enabled);
        assert_eq!(decoded.voice_commands_hotkey, config.voice_commands_hotkey);
        assert_eq!(decoded.voice_commands, config.voice_commands);
    }

    #[test]
    fn current_config_has_no_retired_fields() {
        let json = serde_json::to_string(&Config::default()).unwrap();
        assert!(!json.contains("show_notifications"));
        assert!(!json.contains("\"model\""));
        assert!(json.contains("notification_duration_seconds"));
        assert!(json.contains("append_trailing_space"));
        assert!(json.contains("press_enter_after_paste"));
        assert!(json.contains("voice_commands_enabled"));
        assert!(json.contains("voice_commands_hotkey"));
        assert!(json.contains("voice_commands"));
    }
}
