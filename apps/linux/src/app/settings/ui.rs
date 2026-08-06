//! Linux settings-window integration and shortcut capture.

use eframe::egui;
use transcriber_ui::settings::{
    draw_settings_panel, SettingsPanelOptions, ShortcutUiAction,
};

use crate::hotkey::{capture_shortcut, friendly_name, CaptureOutcome};

use super::super::controller::LocalSttApp;
use super::state::SettingsChanges;

impl LocalSttApp {
    pub(in crate::app) fn draw_settings(&mut self, ctx: &egui::Context) {
        let captured_this_frame = self.poll_shortcut_capture(ctx);
        let recording_active = self.recording_pipeline_busy();
        let response = draw_settings_panel(
            ctx,
            &mut self.settings,
            &SettingsPanelOptions {
                close_label: "✕",
                recording_active,
                scanning_devices: false,
                scanning_help: None,
                recording_help: Some(
                    "The microphone choice is temporarily unavailable while recording stops.",
                ),
            },
            friendly_name,
        );
        self.apply_shortcut_ui_action(response.shortcut_action);
        self.apply_settings_changes(SettingsChanges {
            hotkey: captured_this_frame,
            refresh_devices: response.refresh_devices,
            recording_device: response.recording_device_changed,
            preferences: response.preferences_changed,
        });

        let escape_closes = !self.settings.capturing_hotkey
            && !captured_this_frame
            && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if response.close_requested || escape_closes {
            self.close_settings();
        }
    }

    fn poll_shortcut_capture(&mut self, ctx: &egui::Context) -> bool {
        if !self.settings.capturing_hotkey {
            return false;
        }

        for event in ctx.input(|input| input.events.clone()) {
            match capture_shortcut(&event) {
                Some(CaptureOutcome::Captured(shortcut)) => {
                    self.settings.hotkey = shortcut;
                    self.settings.capturing_hotkey = false;
                    let hotkey_name = friendly_name(&self.settings.hotkey);
                    self.settings.set_message(
                        format!(
                            "Captured {hotkey_name}. Activating and saving automatically…"
                        ),
                        true,
                    );
                    return true;
                }
                Some(CaptureOutcome::Unsupported(message)) => {
                    self.settings.set_message(
                        format!("{message} Press another key or combination."),
                        false,
                    );
                }
                None => {}
            }
        }
        false
    }

    fn apply_shortcut_ui_action(&mut self, action: ShortcutUiAction) {
        match action {
            ShortcutUiAction::None => {}
            ShortcutUiAction::BeginCapture => {
                self.settings.capturing_hotkey = true;
                self.settings.message = None;
            }
            ShortcutUiAction::CancelCapture => {
                self.settings.capturing_hotkey = false;
                self.settings.message = None;
            }
        }
    }
}
