//! Applying settings changes to live subsystems and durable configuration.

use crate::audio::Recorder;
use crate::config;
use crate::hotkey::{friendly_name, validate};
use crate::overlay::OverlayState;

use super::super::controller::LocalSttApp;
use super::state::SettingsChanges;

impl LocalSttApp {
    pub(in crate::app) fn open_settings(&mut self) {
        // Settings owns the root window while it is open. Entering Settings
        // immediately cancels recording/transcription presentation and removes
        // every notification so the two modes can never overlap.
        self.cancel_recording_for_settings();
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
            self.copy_preferences_from_form();
            self.reconcile_overlay_preferences();
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
                match Recorder::new(self.settings.recording_device.as_ref()) {
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

    fn request_recording_device_scan(&mut self) {
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

    fn copy_preferences_from_form(&mut self) {
        self.config.auto_paste = self.settings.auto_paste;
        self.config.notification_duration_seconds = self.settings.notification_duration_seconds;
        self.config.show_loading_notifications = self.settings.loading_notifications;
        self.config.show_recording_notifications = self.settings.recording_notifications;
        self.config.show_transcribing_notifications = self.settings.transcribing_notifications;
        self.config.show_result_notifications = self.settings.result_notifications;
    }

    fn activate_captured_hotkey(&mut self) -> Result<(), String> {
        let requested = self.settings.hotkey.trim().to_string();
        if let Err(error) = validate(&requested) {
            self.settings.hotkey.clone_from(&self.config.hotkey);
            return Err(error.to_string());
        }

        if let Err(error) = self.hotkeys.rebind(&requested) {
            self.hotkeys.disable();
            self.config.hotkey.clear();
            self.hotkey_problem = Some(format!(
                "The recording shortcut {} is already in use by another application or unavailable on this desktop.",
                friendly_name(&requested)
            ));
            self.tray.set_hotkey_hint("");
            self.tray
                .set_tooltip("local-stt — recording shortcut required; open Settings");
            return Err(format!(
                "{error}. The recording shortcut has been disabled. Choose another shortcut."
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

    fn reconcile_overlay_preferences(&mut self) {
        let allowed = match &self.overlay.state {
            OverlayState::Hidden => true,
            OverlayState::Loading { .. } => self.config.show_loading_notifications,
            OverlayState::Listening => self.config.show_recording_notifications,
            OverlayState::Processing => self.config.show_transcribing_notifications,
            OverlayState::Result { .. } => self.config.show_result_notifications,
            OverlayState::Notice { .. } => {
                self.hotkey_problem.is_some()
                    || self.config.show_loading_notifications
                    || self.config.show_result_notifications
            }
        };
        if !allowed {
            self.overlay.dismiss();
        }
    }

    pub(in crate::app) fn close_settings(&mut self) {
        self.settings_window.hide();
        self.settings.capturing_hotkey = false;
        self.overlay.dismiss_immediately();
    }
}
