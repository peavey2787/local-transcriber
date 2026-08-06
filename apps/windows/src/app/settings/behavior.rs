//! Applying settings changes to live subsystems and durable configuration.

use crate::audio::create_recorder;
use crate::config;
use crate::hotkey::{friendly_name, same_shortcut, validate};

use super::super::controller::LocalSttApp;
use super::state::SettingsChanges;

impl LocalSttApp {
    pub(in crate::app) fn open_settings(&mut self) {
        if self.voice_commands.running {
            if self.config.show_result_notifications {
                self.overlay.show_notice(
                    "Finish the current voice command before opening Settings",
                    false,
                    self.now(),
                    self.config.notification_seconds(),
                );
            }
            return;
        }

        // Settings owns the root window while it is open. Entering Settings
        // immediately cancels recording/transcription presentation and removes
        // every notification so the two modes can never overlap.
        self.voice_commands.open = false;
        self.voice_commands.focus_pending = false;
        self.voice_commands.form.capturing_hotkey = false;
        self.cancel_recording_for_editor();
        let became_visible = self.settings_window.show();
        if became_visible {
            self.settings.capturing_hotkey = false;
        }
        if let Some(problem) = &self.hotkey_problem {
            self.settings
                .set_message(format!("{problem} Choose a different shortcut."), false);
        }
    }

    pub(super) fn apply_settings_changes(&mut self, changes: SettingsChanges) {
        if !changes.any() {
            return;
        }

        self.settings.message = None;
        let mut should_save = false;
        let mut success_message = "Saved automatically.".to_string();

        if changes.refresh_devices {
            self.request_recording_device_scan();
        }

        if changes.preferences {
            self.settings.apply_preferences_to(&mut self.config);
            self.overlay
                .reconcile_preferences(&self.config, self.hotkey_problem.is_some());
            should_save = true;
        }

        if changes.recording_device {
            if self.recording_pipeline_busy() {
                self.settings
                    .recording_device
                    .clone_from(&self.config.recording_device);
                self.settings.set_message(
                    "Stop the current recording before changing the recording device.",
                    false,
                );
            } else {
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

    pub(in crate::app) fn request_recording_device_scan(&mut self) {
        if !self.settings.begin_device_scan() {
            self.settings
                .set_message("A recording-device scan is already in progress.", false);
            return;
        }

        if let Err(message) = self.settings_device_discovery.request() {
            self.settings.finish_device_scan();
            self.settings.set_message(message, false);
        }
    }

    pub(in crate::app) fn poll_settings_workers(&mut self) {
        while let Some(result) = self.settings_device_discovery.try_recv() {
            self.settings.finish_device_scan();
            match result {
                Ok(options) => {
                    let device_count = self.settings.replace_input_devices(options);
                    let noun = if device_count == 1 { "device" } else { "devices" };
                    self.settings.set_message(
                        format!("Recording devices refreshed. Found {device_count} {noun}."),
                        true,
                    );
                }
                Err(error) => self.settings.set_message(
                    format!("Could not list recording devices: {error}"),
                    false,
                ),
            }
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

    pub(in crate::app) fn close_settings(&mut self) {
        self.settings_window.hide();
        self.settings.capturing_hotkey = false;
        self.overlay.dismiss_immediately();
    }
}
