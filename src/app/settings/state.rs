//! In-memory settings form state and audio-device choices.

use anyhow::Result;

use crate::audio::{input_device_options, InputDeviceOption, InputDeviceSelection};
use crate::config::{Config, MAX_NOTIFICATION_SECONDS, MIN_NOTIFICATION_SECONDS};
use crate::hotkey::ShortcutCapture;

pub(super) struct SettingsMessage {
    pub(super) text: String,
    pub(super) ok: bool,
}

#[derive(Default)]
pub(super) struct SettingsChanges {
    pub(super) hotkey: bool,
    pub(super) refresh_devices: bool,
    pub(super) recording_device: bool,
    pub(super) preferences: bool,
}

impl SettingsChanges {
    pub(super) fn any(&self) -> bool {
        self.hotkey || self.refresh_devices || self.recording_device || self.preferences
    }
}

pub(in crate::app) struct SettingsState {
    pub(in crate::app) open: bool,
    pub(in crate::app) focus_pending: bool,
    pub(super) hotkey: String,
    pub(super) capturing_hotkey: bool,
    pub(super) shortcut_capture: ShortcutCapture,
    pub(super) recording_device: Option<InputDeviceSelection>,
    pub(super) input_devices: Vec<InputDeviceOption>,
    pub(super) auto_paste: bool,
    pub(super) notification_duration_seconds: u32,
    pub(super) loading_notifications: bool,
    pub(super) recording_notifications: bool,
    pub(super) transcribing_notifications: bool,
    pub(super) result_notifications: bool,
    pub(super) message: Option<SettingsMessage>,
}

impl SettingsState {
    pub(in crate::app) fn from_config(config: &Config) -> Self {
        let mut state = Self {
            open: false,
            focus_pending: false,
            hotkey: String::new(),
            capturing_hotkey: false,
            shortcut_capture: ShortcutCapture::default(),
            recording_device: None,
            input_devices: Vec::new(),
            auto_paste: false,
            notification_duration_seconds: 0,
            loading_notifications: false,
            recording_notifications: false,
            transcribing_notifications: false,
            result_notifications: false,
            message: None,
        };
        state.load_from_config(config);
        state
    }

    pub(super) fn load_from_config(&mut self, config: &Config) {
        self.hotkey.clone_from(&config.hotkey);
        self.capturing_hotkey = false;
        self.shortcut_capture.reset();
        self.recording_device.clone_from(&config.recording_device);
        self.auto_paste = config.auto_paste;
        self.notification_duration_seconds = config
            .notification_duration_seconds
            .clamp(MIN_NOTIFICATION_SECONDS, MAX_NOTIFICATION_SECONDS);
        self.loading_notifications = config.show_loading_notifications;
        self.recording_notifications = config.show_recording_notifications;
        self.transcribing_notifications = config.show_transcribing_notifications;
        self.result_notifications = config.show_result_notifications;
        self.message = None;
    }

    pub(super) fn refresh_input_devices(&mut self) -> Result<usize> {
        self.input_devices = input_device_options()?;
        Ok(self.input_devices.len().saturating_sub(1))
    }

    pub(super) fn selected_device_label(&self) -> String {
        self.input_devices
            .iter()
            .find(|option| option.selection == self.recording_device)
            .map(|option| option.label.clone())
            .unwrap_or_else(|| {
                self.recording_device
                    .as_ref()
                    .map(|_| "Unavailable recording device".to_string())
                    .unwrap_or_else(|| "System default".to_string())
            })
    }

    pub(super) fn set_message(&mut self, text: impl Into<String>, ok: bool) {
        self.message = Some(SettingsMessage {
            text: text.into(),
            ok,
        });
    }
}
