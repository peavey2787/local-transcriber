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
    pub(super) hotkey: String,
    pub(super) capturing_hotkey: bool,
    pub(super) recording_device: Option<InputDeviceSelection>,
    pub(super) input_devices: Vec<InputDeviceOption>,
    devices_loading: bool,
    pub(super) auto_paste: bool,
    pub(super) append_trailing_space: bool,
    pub(super) press_enter_after_paste: bool,
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
            hotkey: String::new(),
            capturing_hotkey: false,
            recording_device: None,
            input_devices: Vec::new(),
            devices_loading: false,
            auto_paste: false,
            append_trailing_space: false,
            press_enter_after_paste: false,
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

    fn load_from_config(&mut self, config: &Config) {
        self.hotkey.clone_from(&config.hotkey);
        self.capturing_hotkey = false;
        self.recording_device.clone_from(&config.recording_device);
        self.ensure_saved_device_choices();
        self.auto_paste = config.auto_paste;
        self.append_trailing_space = config.append_trailing_space;
        self.press_enter_after_paste = config.press_enter_after_paste;
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
        let device_count = options
            .iter()
            .filter(|option| option.selection.is_some())
            .count();
        self.input_devices = options;
        self.ensure_saved_device_choices();
        device_count
    }

    pub(super) fn selected_device_label(&self) -> String {
        self.input_devices
            .iter()
            .find(|option| option.selection == self.recording_device)
            .map(|option| option.label.clone())
            .unwrap_or_else(|| {
                self.recording_device
                    .as_ref()
                    .map(saved_device_label)
                    .unwrap_or_else(|| "System default".to_string())
            })
    }

    pub(super) fn set_message(&mut self, text: impl Into<String>, ok: bool) {
        self.message = Some(SettingsMessage {
            text: text.into(),
            ok,
        });
    }

    fn ensure_saved_device_choices(&mut self) {
        if !self
            .input_devices
            .iter()
            .any(|option| option.selection.is_none())
        {
            self.input_devices.insert(
                0,
                InputDeviceOption {
                    selection: None,
                    label: "System default".to_string(),
                },
            );
        }

        let Some(selection) = self.recording_device.as_ref() else {
            return;
        };
        if self
            .input_devices
            .iter()
            .any(|option| option.selection.as_ref() == Some(selection))
        {
            return;
        }

        self.input_devices.push(InputDeviceOption {
            selection: Some(selection.clone()),
            label: saved_device_label(selection),
        });
    }
}

fn saved_device_label(selection: &InputDeviceSelection) -> String {
    if selection.occurrence == 0 {
        format!("{} — saved device", selection.name)
    } else {
        format!(
            "{} ({}) — saved device",
            selection.name,
            selection.occurrence + 1
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_opening_uses_saved_choices_without_scanning_windows_audio() {
        let mut config = Config::default();
        config.recording_device = Some(InputDeviceSelection {
            name: "USB Microphone".to_string(),
            occurrence: 0,
        });

        let state = SettingsState::from_config(&config);

        assert_eq!(state.input_devices.len(), 2);
        assert_eq!(state.input_devices[0].selection, None);
        assert_eq!(state.selected_device_label(), "USB Microphone — saved device");
        assert!(!state.is_scanning_devices());
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

    #[test]
    fn refreshed_choices_preserve_an_unavailable_saved_device() {
        let mut config = Config::default();
        config.recording_device = Some(InputDeviceSelection {
            name: "Disconnected microphone".to_string(),
            occurrence: 1,
        });
        let mut state = SettingsState::from_config(&config);

        let discovered = state.replace_input_devices(vec![InputDeviceOption {
            selection: None,
            label: "System default — Microphone Array".to_string(),
        }]);

        assert_eq!(discovered, 0);
        assert_eq!(state.input_devices.len(), 2);
        assert_eq!(
            state.selected_device_label(),
            "Disconnected microphone (2) — saved device"
        );
    }
}
