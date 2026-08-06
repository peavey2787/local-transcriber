//! Shared in-memory settings form and recording-device choice state.

use transcriber_core::audio::{InputDeviceOption, InputDeviceSelection};
use transcriber_core::config::{Config, MAX_NOTIFICATION_SECONDS, MIN_NOTIFICATION_SECONDS};

#[derive(Debug, Clone)]
pub struct SettingsMessage {
    pub text: String,
    pub ok: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SettingsChanges {
    pub hotkey: bool,
    pub refresh_devices: bool,
    pub recording_device: bool,
    pub preferences: bool,
}

impl SettingsChanges {
    pub fn any(&self) -> bool {
        self.hotkey || self.refresh_devices || self.recording_device || self.preferences
    }
}

pub struct SettingsForm {
    pub hotkey: String,
    pub capturing_hotkey: bool,
    pub recording_device: Option<InputDeviceSelection>,
    pub input_devices: Vec<InputDeviceOption>,
    devices_loading: bool,
    pub auto_paste: bool,
    pub append_trailing_space: bool,
    pub press_enter_after_paste: bool,
    pub notification_duration_seconds: u32,
    pub loading_notifications: bool,
    pub recording_notifications: bool,
    pub transcribing_notifications: bool,
    pub result_notifications: bool,
    pub message: Option<SettingsMessage>,
}

impl SettingsForm {
    pub fn from_config(config: &Config) -> Self {
        let mut form = Self {
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
        form.reload(config);
        form
    }

    pub fn reload(&mut self, config: &Config) {
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

    pub fn apply_preferences_to(&self, config: &mut Config) {
        config.auto_paste = self.auto_paste;
        config.append_trailing_space = self.append_trailing_space;
        config.press_enter_after_paste = self.press_enter_after_paste;
        config.notification_duration_seconds = self.notification_duration_seconds;
        config.show_loading_notifications = self.loading_notifications;
        config.show_recording_notifications = self.recording_notifications;
        config.show_transcribing_notifications = self.transcribing_notifications;
        config.show_result_notifications = self.result_notifications;
    }

    pub fn begin_device_scan(&mut self) -> bool {
        if self.devices_loading {
            return false;
        }
        self.devices_loading = true;
        true
    }

    pub fn finish_device_scan(&mut self) {
        self.devices_loading = false;
    }

    pub fn is_scanning_devices(&self) -> bool {
        self.devices_loading
    }

    pub fn replace_input_devices(&mut self, options: Vec<InputDeviceOption>) -> usize {
        let device_count = options
            .iter()
            .filter(|option| option.selection.is_some())
            .count();
        self.input_devices = options;
        self.ensure_saved_device_choices();
        device_count
    }

    pub fn selected_device_label(&self) -> String {
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

    pub fn set_message(&mut self, text: impl Into<String>, ok: bool) {
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
    fn saved_device_is_available_before_native_discovery_finishes() {
        let config = Config {
            recording_device: Some(InputDeviceSelection {
                name: "USB Microphone".to_string(),
                occurrence: 0,
            }),
            ..Config::default()
        };

        let state = SettingsForm::from_config(&config);

        assert_eq!(state.input_devices.len(), 2);
        assert_eq!(state.input_devices[0].selection, None);
        assert_eq!(state.selected_device_label(), "USB Microphone — saved device");
    }

    #[test]
    fn duplicate_device_scans_are_rejected() {
        let mut state = SettingsForm::from_config(&Config::default());
        assert!(state.begin_device_scan());
        assert!(!state.begin_device_scan());
        state.finish_device_scan();
        assert!(state.begin_device_scan());
    }

    #[test]
    fn preferences_are_applied_to_the_shared_config_schema() {
        let mut form = SettingsForm::from_config(&Config::default());
        form.auto_paste = true;
        form.append_trailing_space = true;
        form.press_enter_after_paste = true;
        form.notification_duration_seconds = 12;
        form.loading_notifications = false;
        form.recording_notifications = false;
        form.transcribing_notifications = false;
        form.result_notifications = false;

        let mut config = Config::default();
        form.apply_preferences_to(&mut config);

        assert!(config.auto_paste);
        assert!(config.append_trailing_space);
        assert!(config.press_enter_after_paste);
        assert_eq!(config.notification_duration_seconds, 12);
        assert!(!config.show_loading_notifications);
        assert!(!config.show_recording_notifications);
        assert!(!config.show_transcribing_notifications);
        assert!(!config.show_result_notifications);
    }
}
