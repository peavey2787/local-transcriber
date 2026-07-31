//! In-memory settings form state and audio-device choices.

use crate::audio::{InputDeviceOption, InputDeviceSelection};
use crate::config::{Config, MAX_NOTIFICATION_SECONDS, MIN_NOTIFICATION_SECONDS};

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
    pub(super) recording_device: Option<InputDeviceSelection>,
    pub(super) input_devices: Vec<InputDeviceOption>,
    devices_loading: bool,
    has_device_snapshot: bool,
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
            recording_device: None,
            input_devices: Vec::new(),
            devices_loading: false,
            has_device_snapshot: false,
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

    pub(in crate::app) fn is_capturing_hotkey(&self) -> bool {
        self.capturing_hotkey
    }

    pub(super) fn should_scan_devices_on_open(&self) -> bool {
        !self.has_device_snapshot && !self.devices_loading
    }

    pub(super) fn begin_device_scan(&mut self) -> bool {
        if self.devices_loading {
            return false;
        }
        self.devices_loading = true;
        true
    }

    pub(super) fn finish_device_scan(&mut self) {
        self.devices_loading = false;
    }

    pub(super) fn is_scanning_devices(&self) -> bool {
        self.devices_loading
    }

    pub(super) fn replace_input_devices(&mut self, options: Vec<InputDeviceOption>) -> usize {
        self.input_devices = options;
        self.has_device_snapshot = true;
        self.input_devices.len().saturating_sub(1)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_device_scan_runs_only_for_the_first_settings_open() {
        let mut state = SettingsState::from_config(&Config::default());
        assert!(state.should_scan_devices_on_open());
        assert!(state.begin_device_scan());
        assert!(!state.should_scan_devices_on_open());
        state.replace_input_devices(Vec::new());
        state.finish_device_scan();
        state.load_from_config(&Config::default());
        assert!(!state.should_scan_devices_on_open());
    }

    #[test]
    fn failed_initial_scan_can_retry_on_the_next_open() {
        let mut state = SettingsState::from_config(&Config::default());
        assert!(state.begin_device_scan());
        state.finish_device_scan();
        state.load_from_config(&Config::default());
        assert!(state.should_scan_devices_on_open());
    }

    #[test]
    fn duplicate_device_scans_are_rejected_without_blocking_the_ui() {
        let mut state = SettingsState::from_config(&Config::default());
        assert!(state.begin_device_scan());
        assert!(!state.begin_device_scan());
        assert!(state.is_scanning_devices());
        state.finish_device_scan();
        assert!(state.begin_device_scan());
    }
}
