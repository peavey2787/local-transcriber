//! Shared Voice Commands editor for both desktop applications.

use egui::{self, Color32, CursorIcon, RichText, Sense, ViewportCommand};
use transcriber_core::config::{Config, VoiceCommand};

#[derive(Debug, Clone)]
pub struct PanelMessage {
    pub text: String,
    pub ok: bool,
}

#[derive(Debug, Clone)]
pub struct VoiceCommandsForm {
    pub enabled: bool,
    pub hotkey: String,
    pub capturing_hotkey: bool,
    pub commands: Vec<VoiceCommand>,
    pub message: Option<PanelMessage>,
}

impl VoiceCommandsForm {
    pub fn from_config(config: &Config) -> Self {
        Self {
            enabled: config.voice_commands_enabled,
            hotkey: config.voice_commands_hotkey.clone(),
            capturing_hotkey: false,
            commands: config.voice_commands.clone(),
            message: None,
        }
    }

    pub fn reload(&mut self, config: &Config) {
        self.enabled = config.voice_commands_enabled;
        self.hotkey.clone_from(&config.voice_commands_hotkey);
        self.commands.clone_from(&config.voice_commands);
        self.capturing_hotkey = false;
        self.message = None;
    }

    pub fn set_message(&mut self, text: impl Into<String>, ok: bool) {
        self.message = Some(PanelMessage {
            text: text.into(),
            ok,
        });
    }
}


#[derive(Debug)]
pub struct VoiceCommandsState {
    pub open: bool,
    pub focus_pending: bool,
    pub form: VoiceCommandsForm,
    pub running: bool,
}

impl VoiceCommandsState {
    pub fn from_config(config: &Config) -> Self {
        Self {
            open: false,
            focus_pending: false,
            form: VoiceCommandsForm::from_config(config),
            running: false,
        }
    }

    pub fn reload(&mut self, config: &Config) {
        self.form.reload(config);
    }

    pub fn set_message(&mut self, text: impl Into<String>, ok: bool) {
        self.form.set_message(text, ok);
    }
}

pub struct VoiceCommandsPanelOptions<'a> {
    pub platform_name: &'a str,
    pub accepted_extensions: &'a str,
    pub path_hint: &'a str,
    pub browse_enabled: bool,
    pub recording_hotkey: &'a str,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VoiceCommandsPanelResponse {
    pub save_requested: bool,
    pub close_requested: bool,
    pub browse_request: Option<(usize, usize)>,
}

fn help_text(text: impl Into<String>) -> RichText {
    RichText::new(text).size(13.0).weak()
}

pub fn draw_voice_commands_panel(
    ctx: &egui::Context,
    form: &mut VoiceCommandsForm,
    options: &VoiceCommandsPanelOptions<'_>,
    friendly_hotkey: impl Fn(&str) -> String,
) -> VoiceCommandsPanelResponse {
    let mut response = VoiceCommandsPanelResponse::default();

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(Color32::from_rgb(14, 15, 15))
                .inner_margin(24.0),
        )
        .show(ctx, |ui| {
            draw_header(ctx, ui, &mut response);
            ui.add_space(14.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.checkbox(&mut form.enabled, "Enable voice-command recording");
                    ui.label(help_text(
                        "The voice-command hotkey is independent from normal transcription.",
                    ));
                    draw_hotkey(ui, form, options.recording_hotkey, &friendly_hotkey);
                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(12.0);
                    draw_command_list(ui, form, options, &mut response);
                    ui.add_space(16.0);
                    if ui.button("Save changes").clicked() {
                        response.save_requested = true;
                    }
                    if let Some(message) = &form.message {
                        ui.add_space(10.0);
                        ui.label(RichText::new(&message.text).color(if message.ok {
                            Color32::from_rgb(112, 196, 135)
                        } else {
                            Color32::from_rgb(215, 93, 93)
                        }));
                    }
                });
        });

    response
}

fn draw_header(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    response: &mut VoiceCommandsPanelResponse,
) {
    ui.horizontal(|ui| {
        let header_width = (ui.available_width() - 88.0).max(240.0);
        let header = ui.allocate_ui_with_layout(
            egui::vec2(header_width, 50.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.heading("Voice Commands");
                ui.label(help_text(
                    "Record a phrase with a separate hotkey and run its scripts in order.",
                ));
            },
        );
        let drag = ui
            .interact(
                header.response.rect,
                ui.id().with("voice-commands-window-drag"),
                Sense::drag(),
            )
            .on_hover_cursor(CursorIcon::Grab);
        if drag.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                response.close_requested = true;
            }
        });
    });
}

fn draw_hotkey(
    ui: &mut egui::Ui,
    form: &mut VoiceCommandsForm,
    recording_hotkey: &str,
    friendly_hotkey: &impl Fn(&str) -> String,
) {
    ui.add_space(14.0);
    ui.label(RichText::new("Voice-command hotkey").strong());
    ui.label("Press once to start recording a command and again to stop.");
    ui.add_space(8.0);
    egui::Frame::NONE
        .fill(Color32::from_rgb(23, 25, 25))
        .stroke(egui::Stroke::new(
            1.0_f32,
            Color32::from_rgb(54, 58, 58),
        ))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(12.0)
        .show(ui, |ui| {
            if form.capturing_hotkey {
                ui.label(
                    RichText::new("Press the voice-command key or key combination now…")
                        .strong(),
                );
                if ui.button("Cancel capture").clicked() {
                    form.capturing_hotkey = false;
                }
            } else {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Current shortcut");
                        ui.label(RichText::new(friendly_hotkey(&form.hotkey)).strong());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if form.hotkey.trim().is_empty() {
                            "Set shortcut"
                        } else {
                            "Change shortcut"
                        };
                        if ui.button(label).clicked() {
                            form.capturing_hotkey = true;
                            form.message = None;
                        }
                    });
                });
            }
        });
    ui.label(help_text(format!(
        "Must differ from the normal recording hotkey ({}).",
        friendly_hotkey(recording_hotkey)
    )));
}

fn draw_command_list(
    ui: &mut egui::Ui,
    form: &mut VoiceCommandsForm,
    options: &VoiceCommandsPanelOptions<'_>,
    response: &mut VoiceCommandsPanelResponse,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Commands and script chains").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Add command").clicked() {
                form.commands.push(VoiceCommand {
                    phrase: String::new(),
                    scripts: vec![String::new()],
                });
            }
        });
    });
    ui.add(
        egui::Label::new(help_text(
            "Separate alternative spoken words or phrases with commas. Matching ignores capitalization, repeated spaces, and punctuation around words, but otherwise remains exact.",
        ))
        .wrap(),
    );
    ui.add_space(8.0);

    let mut remove_command = None;
    for (command_index, command) in form.commands.iter_mut().enumerate() {
        egui::Frame::NONE
            .fill(Color32::from_rgb(20, 22, 22))
            .stroke(egui::Stroke::new(
                1.0_f32,
                Color32::from_rgb(48, 52, 52),
            ))
            .corner_radius(egui::CornerRadius::same(9))
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Command {}", command_index + 1)).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Delete command").clicked() {
                            remove_command = Some(command_index);
                        }
                    });
                });
                ui.label(help_text("Spoken words or phrases, separated by commas"));
                ui.add(
                    egui::TextEdit::singleline(&mut command.phrase)
                        .hint_text("Hello, hello there, start report"),
                );
                ui.add_space(8.0);
                ui.label(help_text(format!(
                    "Scripts run from top to bottom. {} accepts {} files.",
                    options.platform_name, options.accepted_extensions
                )));

                let mut remove_script = None;
                for (script_index, script) in command.scripts.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let browse_width = if options.browse_enabled { 78.0 } else { 0.0 };
                        let controls_width = 78.0
                            + browse_width
                            + ui.spacing().item_spacing.x
                                * if options.browse_enabled { 2.0 } else { 1.0 };
                        let path_width = (ui.available_width() - controls_width).max(140.0);
                        ui.add_sized(
                            [path_width, 28.0],
                            egui::TextEdit::singleline(script).hint_text(options.path_hint),
                        );
                        if options.browse_enabled && ui.button("Browse…").clicked() {
                            response.browse_request = Some((command_index, script_index));
                        }
                        if ui.button("Remove").clicked() {
                            remove_script = Some(script_index);
                        }
                    });
                }
                if let Some(index) = remove_script {
                    command.scripts.remove(index);
                }
                if ui.button("Add script").clicked() {
                    command.scripts.push(String::new());
                }
            });
        ui.add_space(10.0);
    }
    if let Some(index) = remove_command {
        form.commands.remove(index);
    }
    if form.commands.is_empty() {
        ui.label(help_text("No commands are configured yet."));
    }
}
