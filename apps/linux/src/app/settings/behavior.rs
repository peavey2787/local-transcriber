//! Applying settings changes to live subsystems and durable configuration.

use crate::audio::create_recorder;
use crate::config;
use crate::hotkey::{friendly_name, same_shortcut, validate};

use super::state::SettingsChanges;
use super::super::controller::LocalSttApp;

impl LocalSttApp {
    pub(in crate::app) fn open_settings(&mut self) {
        if self.voice_commands.running
            || self.recording
            || self.session.as_ref().is_some_and(|session| session.finishing)
        {
            if self.config.show_result_notifications {
                let message = if self.voice_commands.running {
                    "Finish the current voice command before opening Settings"
                } else {
                    "Finish the current recording before opening Settings"
                };
                self.overlay.show_notice(
                    message,
                    false,
                    self.now(),
                    self.config.notification_seconds(),
                );
            }
            return;
        }

        self.voice_commands.open = false;
        self.voice_commands.focus_pending = false;
        self.voice_commands.form.capturing_hotkey = false;
        self.settings.load_from_config(&self.config);
        self.refresh_recording_devices(false);
        let hotkey_message = if self.settings.message.is_none() {
            self.hotkey_problem
                .as_ref()
                .map(|problem| format!("{problem} Choose a different shortcut."))
        } else {
            None
        };
        if let Some(message) = hotkey_message {
            self.settings.set_message(message, false);
        }
        self.settings.open = true;
        self.settings.focus_pending = true;
        self.overlay.dismiss();
    }

    pub(super) fn apply_settings_changes(&mut self, changes: SettingsChanges) {
        if !changes.any() {
            return;
        }

        self.settings.message = None;
        let mut should_save = false;
        let mut success_message = "Saved automatically.".to_string();

        if changes.refresh_devices {
            self.refresh_recording_devices(true);
        }

        if changes.preferences {
            self.settings.apply_preferences_to(&mut self.config);
            self.overlay
                .reconcile_preferences(&self.config, self.hotkey_problem.is_some());
            should_save = true;
        }

        if changes.recording_device {
            match create_recorder(self.settings.recording_device.as_ref()) {
                Ok(recorder) => {
                    self.recorder = recorder;
                    self.config
                        .recording_device
                        .clone_from(&self.settings.recording_device);
                    success_message = format!(
                        "Saved automatically. Recording device: {}",
                        self.settings.selected_device_label()
                    );
                    should_save = true;
                }
                Err(error) => {
                    self.settings
                        .recording_device
                        .clone_from(&self.config.recording_device);
                    self.settings.set_message(
                        format!("Could not use that recording device: {error:#}"),
                        false,
                    );
                }
            }
        }

        if changes.hotkey {
            match self.activate_captured_hotkey() {
                Ok(()) => {
                    success_message = format!(
                        "Saved automatically. Recording hotkey: {}",
                        friendly_name(&self.config.hotkey)
                    );
                    should_save = true;
                }
                Err(message) => {
                    self.settings.set_message(message, false);
                    should_save = true;
                }
            }
        }

        if !should_save {
            return;
        }

        match config::save(&self.config) {
            Ok(()) => {
                if self.settings.message.is_none() {
                    self.settings.set_message(success_message, true);
                }
            }
            Err(error) => self
                .settings
                .set_message(format!("Could not save settings: {error:#}"), false),
        }
    }

    fn refresh_recording_devices(&mut self, announce_success: bool) {
        match self.settings.refresh_input_devices() {
            Ok(device_count) if announce_success => {
                let noun = if device_count == 1 { "device" } else { "devices" };
                self.settings.set_message(
                    format!("Recording devices refreshed. Found {device_count} {noun}."),
                    true,
                );
            }
            Ok(_) => {}
            Err(error) => self
                .settings
                .set_message(format!("Could not list recording devices: {error:#}"), false),
        }
    }

    fn activate_captured_hotkey(&mut self) -> Result<(), String> {
        let requested = self.settings.hotkey.trim().to_string();
        if let Err(error) = validate(&requested) {
            self.settings.hotkey.clone_from(&self.config.hotkey);
            return Err(error.to_string());
        }

        if self.config.voice_commands_enabled
            && same_shortcut(&requested, &self.config.voice_commands_hotkey)
        {
            self.settings.hotkey.clone_from(&self.config.hotkey);
            return Err(
                "The recording hotkey must differ from the voice-command hotkey.".to_string(),
            );
        }

        if let Err(error) = self.hotkeys.rebind(&requested) {
            self.settings.hotkey.clone_from(&self.config.hotkey);
            return Err(format!(
                "{error}. The existing recording shortcut remains active; choose another shortcut."
            ));
        }

        self.hotkey_problem = None;
        self.config.hotkey = requested;
        self.tray.set_hotkey_hint(&self.config.hotkey);
        if self.engine.is_some() {
            self.tray.set_tooltip("local-stt — Parakeet ready");
        } else {
            self.tray
                .set_tooltip(&format!("local-stt — {}", self.startup_status));
        }
        Ok(())
    }

    pub(super) fn close_settings(&mut self) {
        self.settings.open = false;
        self.settings.focus_pending = false;
        self.settings.capturing_hotkey = false;
        if let Some(problem) = &self.hotkey_problem {
            self.overlay.show_persistent_notice(
                format!("{problem} Open the tray menu → Settings to choose another shortcut."),
                false,
            );
        } else if self.config.show_loading_notifications && self.engine.is_none() {
            self.overlay.show_loading(self.startup_status.clone());
        } else {
            self.overlay.dismiss();
        }
    }
}
