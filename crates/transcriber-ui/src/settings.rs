//! Shared settings controls that do not call native operating-system APIs.

use egui::{self, Color32, RichText};

use crate::settings_state::SettingsForm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutUiAction {
    None,
    BeginCapture,
    CancelCapture,
}

impl Default for ShortcutUiAction {
    fn default() -> Self {
        Self::None
    }
}

pub fn help_text(text: impl Into<String>) -> RichText {
    RichText::new(text).size(13.0).weak()
}

pub fn draw_settings_header(ui: &mut egui::Ui, close_label: &str) -> bool {
    let close_requested = ui
        .horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("local-stt settings");
                ui.label(help_text("Changes save automatically."));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.button(close_label).clicked()
            })
            .inner
        })
        .inner;
    ui.add_space(14.0);
    close_requested
}

pub fn draw_shortcut_section(
    ui: &mut egui::Ui,
    hotkey: &str,
    capturing: bool,
    friendly_hotkey: impl Fn(&str) -> String,
) -> ShortcutUiAction {
    ui.add_space(16.0);
    ui.label(RichText::new("Recording hotkey").strong());
    ui.label("Click Set shortcut, then press the exact key or key combination you want to use.");
    ui.add_space(8.0);

    let mut action = ShortcutUiAction::None;
    egui::Frame::NONE
        .fill(Color32::from_rgb(23, 25, 25))
        .stroke(egui::Stroke::new(
            1.0_f32,
            Color32::from_rgb(54, 58, 58),
        ))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(12.0)
        .show(ui, |ui| {
            if capturing {
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
                    action = ShortcutUiAction::CancelCapture;
                }
            } else {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(help_text("Current shortcut"));
                        ui.label(
                            RichText::new(friendly_hotkey(hotkey))
                                .size(17.0)
                                .strong(),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if hotkey.trim().is_empty() {
                            "Set shortcut"
                        } else {
                            "Change shortcut"
                        };
                        if ui.button(label).clicked() {
                            action = ShortcutUiAction::BeginCapture;
                        }
                    });
                });
            }
        });
    ui.label(help_text(
        "The shortcut is captured from the keyboard; there are no preset choices or manually typed shortcut strings.",
    ));
    action
}

pub fn draw_delivery_section(
    ui: &mut egui::Ui,
    auto_paste: &mut bool,
    append_trailing_space: &mut bool,
    press_enter_after_paste: &mut bool,
) -> bool {
    ui.add_space(16.0);
    ui.label(RichText::new("Transcription delivery").strong());

    let mut changed = ui
        .checkbox(
            auto_paste,
            "Automatically paste the transcription with Ctrl+V",
        )
        .changed();
    ui.label(help_text(
        "The result is copied first. A successful auto-paste does not show the editable result textbox.",
    ));

    changed |= ui
        .checkbox(
            append_trailing_space,
            "Add a space at the end of each transcription",
        )
        .changed();
    ui.label(help_text(
        "Applies to clipboard text, automatic paste, and the editable result.",
    ));

    if !*auto_paste && *press_enter_after_paste {
        *press_enter_after_paste = false;
        changed = true;
    }
    changed |= ui
        .add_enabled(
            *auto_paste,
            egui::Checkbox::new(
                press_enter_after_paste,
                "Press Enter after automatically pasting",
            ),
        )
        .changed();
    ui.label(help_text(
        "Enter is sent only after a successful automatic paste, never when the result is only copied.",
    ));
    changed
}

pub struct NotificationSettings<'a> {
    pub duration_seconds: &'a mut u32,
    pub loading: &'a mut bool,
    pub recording: &'a mut bool,
    pub transcribing: &'a mut bool,
    pub result: &'a mut bool,
}

pub fn draw_notification_section(
    ui: &mut egui::Ui,
    settings: NotificationSettings<'_>,
    duration_range: std::ops::RangeInclusive<u32>,
) -> bool {
    ui.add_space(16.0);
    ui.label(RichText::new("Visual notifications").strong());
    ui.label("Choose any combination of status windows you want to see.");
    ui.add_space(6.0);

    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Temporary notification duration:");
        changed |= ui
            .add(
                egui::DragValue::new(settings.duration_seconds)
                    .range(duration_range)
                    .suffix(" seconds"),
            )
            .changed();
    });
    ui.label(help_text(
        "Applies to temporary notices and untouched transcription results. Recording, loading, and transcribing stay visible while active.",
    ));
    ui.add_space(4.0);
    changed |= ui
        .checkbox(settings.loading, "Show model loading and ready notifications")
        .changed();
    changed |= ui
        .checkbox(
            settings.recording,
            "Show recording notification and microphone meter",
        )
        .changed();
    changed |= ui
        .checkbox(settings.transcribing, "Show transcribing notification")
        .changed();
    changed |= ui
        .checkbox(
            settings.result,
            "Show transcription result and result/error notifications",
        )
        .changed();
    changed
}

pub fn draw_status(ui: &mut egui::Ui, message: Option<(&str, bool)>) {
    ui.add_space(16.0);
    if let Some((text, ok)) = message {
        ui.label(RichText::new(text).color(if ok {
            Color32::from_rgb(112, 196, 135)
        } else {
            Color32::from_rgb(215, 93, 93)
        }));
    }
}


pub struct SettingsPanelOptions<'a> {
    pub close_label: &'a str,
    pub recording_active: bool,
    pub scanning_devices: bool,
    pub scanning_help: Option<&'a str>,
    pub recording_help: Option<&'a str>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SettingsPanelResponse {
    pub close_requested: bool,
    pub shortcut_action: ShortcutUiAction,
    pub recording_device_changed: bool,
    pub refresh_devices: bool,
    pub preferences_changed: bool,
}

pub fn draw_settings_panel(
    ctx: &egui::Context,
    form: &mut SettingsForm,
    options: &SettingsPanelOptions<'_>,
    friendly_hotkey: impl Fn(&str) -> String + Copy,
) -> SettingsPanelResponse {
    let mut response = SettingsPanelResponse::default();
    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(Color32::from_rgb(14, 15, 15))
                .inner_margin(24.0),
        )
        .show(ctx, |ui| {
            response.close_requested = draw_settings_header(ui, options.close_label);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let device = draw_recording_device_section(ui, form, options);
                    response.recording_device_changed = device.0;
                    response.refresh_devices = device.1;

                    response.shortcut_action = draw_shortcut_section(
                        ui,
                        &form.hotkey,
                        form.capturing_hotkey,
                        friendly_hotkey,
                    );
                    response.preferences_changed |= draw_delivery_section(
                        ui,
                        &mut form.auto_paste,
                        &mut form.append_trailing_space,
                        &mut form.press_enter_after_paste,
                    );
                    response.preferences_changed |= draw_notification_section(
                        ui,
                        NotificationSettings {
                            duration_seconds: &mut form.notification_duration_seconds,
                            loading: &mut form.loading_notifications,
                            recording: &mut form.recording_notifications,
                            transcribing: &mut form.transcribing_notifications,
                            result: &mut form.result_notifications,
                        },
                        transcriber_core::config::MIN_NOTIFICATION_SECONDS
                            ..=transcriber_core::config::MAX_NOTIFICATION_SECONDS,
                    );
                    draw_status(
                        ui,
                        form.message
                            .as_ref()
                            .map(|message| (message.text.as_str(), message.ok)),
                    );
                });
        });
    response
}

fn draw_recording_device_section(
    ui: &mut egui::Ui,
    form: &mut SettingsForm,
    options: &SettingsPanelOptions<'_>,
) -> (bool, bool) {
    ui.label(RichText::new("Recording device").strong());
    ui.label("Choose which microphone local-stt opens when recording starts.");
    ui.add_space(8.0);

    let before = form.recording_device.clone();
    let device_options = form.input_devices.clone();
    let selected_text = if options.scanning_devices {
        "Scanning recording devices…".to_string()
    } else {
        form.selected_device_label()
    };
    let refresh_requested = ui
        .horizontal(|ui| {
            let button_width = 138.0;
            let combo_width =
                (ui.available_width() - button_width - ui.spacing().item_spacing.x).max(160.0);
            ui.add_enabled_ui(!options.recording_active && !options.scanning_devices, |ui| {
                egui::ComboBox::from_id_salt("recording-device")
                    .selected_text(selected_text)
                    .width(combo_width)
                    .show_ui(ui, |ui| {
                        for option in device_options {
                            ui.selectable_value(
                                &mut form.recording_device,
                                option.selection,
                                option.label,
                            );
                        }
                    });
            });
            ui.add_enabled_ui(!options.scanning_devices, |ui| {
                ui.add_sized(
                    [button_width, 30.0],
                    egui::Button::new(if options.scanning_devices {
                        "Scanning…"
                    } else {
                        "Refresh devices"
                    }),
                )
            })
            .inner
            .on_hover_text("Scan again after connecting or disconnecting a microphone")
            .clicked()
        })
        .inner;

    if options.scanning_devices {
        if let Some(message) = options.scanning_help {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(help_text(message));
            });
        }
    } else if options.recording_active {
        if let Some(message) = options.recording_help {
            ui.label(help_text(message));
        }
    }

    (before != form.recording_device, refresh_requested)
}
