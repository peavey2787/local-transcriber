//! Settings state, shortcut capture, persistence, and settings-window UI.

use eframe::egui::{self, Color32, RichText};

use crate::config::{self, Config};
use crate::hotkey::{friendly_name, validate, CaptureOutcome, ShortcutCapture};
use crate::overlay::OverlayState;

use super::controller::LocalSttApp;

pub(super) struct SettingsState {
    pub(super) open: bool,
    pub(super) focus_pending: bool,
    hotkey: String,
    capturing_hotkey: bool,
    shortcut_capture: ShortcutCapture,
    auto_paste: bool,
    loading_notifications: bool,
    recording_notifications: bool,
    transcribing_notifications: bool,
    result_notifications: bool,
    message: Option<(String, bool)>,
}

impl SettingsState {
    pub(super) fn from_config(config: &Config) -> Self {
        let mut state = Self {
            open: false,
            focus_pending: false,
            hotkey: String::new(),
            capturing_hotkey: false,
            shortcut_capture: ShortcutCapture::default(),
            auto_paste: false,
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
        self.shortcut_capture.reset();
        self.auto_paste = config.auto_paste;
        self.loading_notifications = config.show_loading_notifications;
        self.recording_notifications = config.show_recording_notifications;
        self.transcribing_notifications = config.show_transcribing_notifications;
        self.result_notifications = config.show_result_notifications;
        self.message = None;
    }
}

impl LocalSttApp {
    pub(super) fn open_settings(&mut self) {
        if self.recording || self.session.as_ref().is_some_and(|session| session.finishing) {
            if self.config.show_result_notifications {
                self.overlay.show_notice(
                    "Finish the current recording before opening Settings",
                    false,
                    self.now(),
                    3.5,
                );
            }
            return;
        }

        self.settings.load_from_config(&self.config);
        self.settings.message = self.hotkey_problem.as_ref().map(|problem| {
            (
                format!("{problem} Choose a different shortcut and click Save and apply."),
                false,
            )
        });
        self.settings.open = true;
        self.settings.focus_pending = true;
        self.overlay.dismiss();
    }

    fn apply_settings(&mut self) {
        let requested = self.settings.hotkey.trim().to_string();
        if let Err(error) = validate(&requested) {
            self.settings.message = Some((error.to_string(), false));
            return;
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
            if let Err(save_error) = config::save(&self.config) {
                eprintln!(
                    "[local-stt] could not save the disabled hotkey state: {save_error:#}"
                );
            }
            self.settings.message = Some((
                format!(
                    "{error}. The recording shortcut has been disabled. Choose another shortcut."
                ),
                false,
            ));
            return;
        }

        self.hotkey_problem = None;
        self.config.hotkey = requested;
        self.config.auto_paste = self.settings.auto_paste;
        self.config.show_loading_notifications = self.settings.loading_notifications;
        self.config.show_recording_notifications = self.settings.recording_notifications;
        self.config.show_transcribing_notifications = self.settings.transcribing_notifications;
        self.config.show_result_notifications = self.settings.result_notifications;
        self.tray.set_hotkey_hint(&self.config.hotkey);
        match config::save(&self.config) {
            Ok(()) => {
                self.settings.message = Some((
                    format!(
                        "Saved. Recording hotkey: {}",
                        friendly_name(&self.config.hotkey)
                    ),
                    true,
                ));
                if self.engine.is_some() {
                    self.tray.set_tooltip("local-stt — Parakeet ready");
                } else {
                    self.tray
                        .set_tooltip(&format!("local-stt — {}", self.startup_status));
                }
                self.reconcile_overlay_preferences();
            }
            Err(error) => {
                self.settings.message = Some((format!("Could not save settings: {error:#}"), false));
            }
        }
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

    fn poll_shortcut_capture(&mut self, ctx: &egui::Context) -> bool {
        if !self.settings.capturing_hotkey {
            return false;
        }

        for event in ctx.input(|input| input.events.clone()) {
            match self.settings.shortcut_capture.feed(&event) {
                Some(CaptureOutcome::Captured(shortcut)) => {
                    self.settings.hotkey = shortcut;
                    self.settings.capturing_hotkey = false;
                    self.settings.shortcut_capture.reset();
                    self.settings.message = Some((
                        format!(
                            "Captured {}. Click Save and apply to activate it.",
                            friendly_name(&self.settings.hotkey)
                        ),
                        true,
                    ));
                    return true;
                }
                Some(CaptureOutcome::Unsupported(message)) => {
                    self.settings.shortcut_capture.reset();
                    self.settings.message = Some((
                        format!("{message} Press another key or combination."),
                        false,
                    ));
                }
                None => {}
            }
        }
        false
    }

    pub(super) fn draw_settings(&mut self, ctx: &egui::Context) {
        let captured_this_frame = self.poll_shortcut_capture(ctx);
        let mut apply = false;
        let mut close = false;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(14, 15, 15))
                    .inner_margin(24.0),
            )
            .show(ctx, |ui| {
                self.draw_settings_header(ui, &mut close);
                self.draw_shortcut_section(ui);
                self.draw_auto_paste_section(ui);
                self.draw_notification_section(ui);
                self.draw_settings_footer(ui, &mut apply, &mut close);
            });

        if apply {
            self.apply_settings();
        }
        let escape_closes = !self.settings.capturing_hotkey
            && !captured_this_frame
            && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if close || escape_closes {
            self.close_settings();
        }
    }

    fn draw_settings_header(&self, ui: &mut egui::Ui, close: &mut bool) {
        ui.horizontal(|ui| {
            ui.heading("local-stt settings");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✕").clicked() {
                    *close = true;
                }
            });
        });
        ui.add_space(14.0);
    }

    fn draw_shortcut_section(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Recording hotkey").strong());
        ui.label("Click Set shortcut, then press the exact key or key combination you want to use.");
        ui.add_space(8.0);

        egui::Frame::NONE
            .fill(Color32::from_rgb(23, 25, 25))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(54, 58, 58)))
            .corner_radius(egui::CornerRadius::same(9))
            .inner_margin(12.0)
            .show(ui, |ui| {
                if self.settings.capturing_hotkey {
                    self.draw_active_shortcut_capture(ui);
                } else {
                    self.draw_current_shortcut(ui);
                }
            });
        ui.label(
            RichText::new(
                "The shortcut is captured from the keyboard; there are no preset choices or manually typed shortcut strings.",
            )
            .small()
            .weak(),
        );
    }

    fn draw_active_shortcut_capture(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Press your shortcut now…")
                .strong()
                .color(Color32::from_rgb(67, 196, 214)),
        );
        ui.label(
            RichText::new("Optionally hold Ctrl, Alt, or Shift, then press one main key.")
                .small()
                .weak(),
        );
        ui.add_space(6.0);
        if ui.button("Cancel capture").clicked() {
            self.settings.capturing_hotkey = false;
            self.settings.shortcut_capture.reset();
            self.settings.message = None;
        }
    }

    fn draw_current_shortcut(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Current shortcut").small().weak());
                ui.label(
                    RichText::new(friendly_name(&self.settings.hotkey))
                        .size(17.0)
                        .strong(),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.settings.hotkey.trim().is_empty() {
                    "Set shortcut"
                } else {
                    "Change shortcut"
                };
                if ui.button(label).clicked() {
                    self.settings.capturing_hotkey = true;
                    self.settings.shortcut_capture.reset();
                    self.settings.message = None;
                }
            });
        });
    }

    fn draw_auto_paste_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.checkbox(
            &mut self.settings.auto_paste,
            "Automatically paste the transcription with Ctrl+V",
        );
        ui.label(
            RichText::new(
                "The result is copied first. A successful auto-paste does not show the editable result textbox.",
            )
            .small()
            .weak(),
        );
    }

    fn draw_notification_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.label(RichText::new("Visual notifications").strong());
        ui.label("Choose any combination of status windows you want to see.");
        ui.add_space(6.0);
        ui.checkbox(
            &mut self.settings.loading_notifications,
            "Show model loading and ready notifications",
        );
        ui.checkbox(
            &mut self.settings.recording_notifications,
            "Show recording notification and microphone meter",
        );
        ui.checkbox(
            &mut self.settings.transcribing_notifications,
            "Show transcribing notification",
        );
        ui.checkbox(
            &mut self.settings.result_notifications,
            "Show transcription result and result/error notifications",
        );
    }

    fn draw_settings_footer(
        &self,
        ui: &mut egui::Ui,
        apply: &mut bool,
        close: &mut bool,
    ) {
        ui.add_space(16.0);
        if let Some((message, ok)) = &self.settings.message {
            ui.label(RichText::new(message).color(if *ok {
                Color32::from_rgb(112, 196, 135)
            } else {
                Color32::from_rgb(215, 93, 93)
            }));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    !self.settings.capturing_hotkey,
                    egui::Button::new("Save and apply"),
                )
                .clicked()
            {
                *apply = true;
            }
            if ui.button("Cancel").clicked() {
                *close = true;
            }
        });
    }

    fn close_settings(&mut self) {
        self.settings.open = false;
        self.settings.focus_pending = false;
        self.settings.capturing_hotkey = false;
        self.settings.shortcut_capture.reset();
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
