//! Settings-window rendering and control change detection.

use eframe::egui::{self, Color32, RichText};

use crate::config::{MAX_NOTIFICATION_SECONDS, MIN_NOTIFICATION_SECONDS};
use crate::hotkey::{friendly_name, CaptureOutcome};

use super::state::SettingsChanges;
use super::super::controller::LocalSttApp;

fn help_text(text: impl Into<String>) -> RichText {
    RichText::new(text).size(13.0).weak()
}

impl LocalSttApp {
    pub(in crate::app) fn draw_settings(&mut self, ctx: &egui::Context) {
        let captured_this_frame = self.poll_shortcut_capture(ctx);
        let mut changes = SettingsChanges {
            hotkey: captured_this_frame,
            ..SettingsChanges::default()
        };
        let mut close = false;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(14, 15, 15))
                    .inner_margin(24.0),
            )
            .show(ctx, |ui| {
                self.draw_settings_header(ui, &mut close);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let (device_changed, refresh_requested) =
                            self.draw_recording_device_section(ui);
                        changes.recording_device |= device_changed;
                        changes.refresh_devices |= refresh_requested;
                        self.draw_shortcut_section(ui);
                        changes.preferences |= self.draw_auto_paste_section(ui);
                        changes.preferences |= self.draw_notification_section(ui);
                        self.draw_settings_status(ui);
                    });
            });

        self.apply_settings_changes(changes);
        let escape_closes = !self.settings.capturing_hotkey
            && !captured_this_frame
            && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if close || escape_closes {
            self.close_settings();
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
                    self.settings.set_message(
                        format!(
                            "Captured {}. Activating and saving automatically…",
                            friendly_name(&self.settings.hotkey)
                        ),
                        true,
                    );
                    return true;
                }
                Some(CaptureOutcome::Unsupported(message)) => {
                    self.settings.shortcut_capture.reset();
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

    fn draw_settings_header(&self, ui: &mut egui::Ui, close: &mut bool) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("local-stt settings");
                ui.label(help_text("Changes save automatically."));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✕").clicked() {
                    *close = true;
                }
            });
        });
        ui.add_space(14.0);
    }

    fn draw_recording_device_section(&mut self, ui: &mut egui::Ui) -> (bool, bool) {
        ui.label(RichText::new("Recording device").strong());
        ui.label("Choose which microphone local-stt opens when recording starts.");
        ui.add_space(8.0);

        let before = self.settings.recording_device.clone();
        let options = self.settings.input_devices.clone();
        let refresh_requested = ui
            .horizontal(|ui| {
                let button_width = 138.0;
                let combo_width =
                    (ui.available_width() - button_width - ui.spacing().item_spacing.x).max(160.0);
                egui::ComboBox::from_id_salt("recording-device")
                    .selected_text(self.settings.selected_device_label())
                    .width(combo_width)
                    .show_ui(ui, |ui| {
                        for option in options {
                            ui.selectable_value(
                                &mut self.settings.recording_device,
                                option.selection,
                                option.label,
                            );
                        }
                    });
                ui.add_sized([button_width, 30.0], egui::Button::new("Refresh devices"))
                    .on_hover_text("Scan again after connecting or disconnecting a microphone")
                    .clicked()
            })
            .inner;
        (before != self.settings.recording_device, refresh_requested)
    }

    fn draw_shortcut_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
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
        ui.label(help_text(
            "The shortcut is captured from the keyboard; there are no preset choices or manually typed shortcut strings.",
        ));
    }

    fn draw_active_shortcut_capture(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Press your shortcut now…")
                .strong()
                .color(Color32::from_rgb(67, 196, 214)),
        );
        ui.label(help_text(
            "Optionally hold Ctrl, Alt, or Shift, then press one main key.",
        ));
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
                ui.label(help_text("Current shortcut"));
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

    fn draw_auto_paste_section(&mut self, ui: &mut egui::Ui) -> bool {
        ui.add_space(16.0);
        let changed = ui
            .checkbox(
                &mut self.settings.auto_paste,
                "Automatically paste the transcription with Ctrl+V",
            )
            .changed();
        ui.label(help_text(
            "The result is copied first. A successful auto-paste does not show the editable result textbox.",
        ));
        changed
    }

    fn draw_notification_section(&mut self, ui: &mut egui::Ui) -> bool {
        ui.add_space(16.0);
        ui.label(RichText::new("Visual notifications").strong());
        ui.label("Choose any combination of status windows you want to see.");
        ui.add_space(6.0);

        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Temporary notification duration:");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.settings.notification_duration_seconds)
                        .range(MIN_NOTIFICATION_SECONDS..=MAX_NOTIFICATION_SECONDS)
                        .suffix(" seconds"),
                )
                .changed();
        });
        ui.label(help_text(
            "Applies to temporary notices and untouched transcription results. Recording, loading, and transcribing stay visible while active.",
        ));
        ui.add_space(4.0);
        changed |= ui
            .checkbox(
                &mut self.settings.loading_notifications,
                "Show model loading and ready notifications",
            )
            .changed();
        changed |= ui
            .checkbox(
                &mut self.settings.recording_notifications,
                "Show recording notification and microphone meter",
            )
            .changed();
        changed |= ui
            .checkbox(
                &mut self.settings.transcribing_notifications,
                "Show transcribing notification",
            )
            .changed();
        changed |= ui
            .checkbox(
                &mut self.settings.result_notifications,
                "Show transcription result and result/error notifications",
            )
            .changed();
        changed
    }

    fn draw_settings_status(&self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        if let Some(message) = &self.settings.message {
            ui.label(RichText::new(&message.text).color(if message.ok {
                Color32::from_rgb(112, 196, 135)
            } else {
                Color32::from_rgb(215, 93, 93)
            }));
        }
    }
}
