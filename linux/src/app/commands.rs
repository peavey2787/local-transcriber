//! Voice-command configuration, matching, and background script execution.

use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Color32, CursorIcon, RichText, Sense, ViewportCommand};
use gtk::prelude::*;
use std::thread;

use crate::config::{self, VoiceCommand};
use crate::hotkey::{friendly_name, same_shortcut, validate, CaptureOutcome, ShortcutCapture};
use crate::tray::TrayStatus;
use crate::voice_commands::{execute_command, matching_command, validate_command_list};

use super::controller::LocalSttApp;

#[derive(Debug)]
struct CommandExecutionResult {
    phrase: String,
    result: Result<(), String>,
}

#[derive(Debug, Clone)]
struct PanelMessage {
    text: String,
    ok: bool,
}

pub(super) struct VoiceCommandsState {
    pub(super) open: bool,
    pub(super) focus_pending: bool,
    enabled: bool,
    hotkey: String,
    pub(super) capturing_hotkey: bool,
    shortcut_capture: ShortcutCapture,
    commands: Vec<VoiceCommand>,
    message: Option<PanelMessage>,
    pub(super) running: bool,
    result_tx: Sender<CommandExecutionResult>,
    result_rx: Receiver<CommandExecutionResult>,
}

impl VoiceCommandsState {
    pub(super) fn from_config(config: &config::Config) -> Self {
        let (result_tx, result_rx) = unbounded();
        Self {
            open: false,
            focus_pending: false,
            enabled: config.voice_commands_enabled,
            hotkey: config.voice_commands_hotkey.clone(),
            capturing_hotkey: false,
            shortcut_capture: ShortcutCapture::default(),
            commands: config.voice_commands.clone(),
            message: None,
            running: false,
            result_tx,
            result_rx,
        }
    }

    fn reload(&mut self, config: &config::Config) {
        self.enabled = config.voice_commands_enabled;
        self.hotkey.clone_from(&config.voice_commands_hotkey);
        self.commands.clone_from(&config.voice_commands);
        self.capturing_hotkey = false;
        self.shortcut_capture.reset();
        self.message = None;
    }

    fn set_message(&mut self, text: impl Into<String>, ok: bool) {
        self.message = Some(PanelMessage {
            text: text.into(),
            ok,
        });
    }
}

fn help_text(text: impl Into<String>) -> RichText {
    RichText::new(text).size(13.0).weak()
}

fn select_script_file() -> Option<String> {
    let dialog = gtk::FileChooserDialog::with_buttons(
        Some("Select a voice-command script"),
        None::<&gtk::Window>,
        gtk::FileChooserAction::Open,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Select", gtk::ResponseType::Accept),
        ],
    );
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Linux scripts (*.sh, *.bash, *.py)"));
    filter.add_pattern("*.sh");
    filter.add_pattern("*.bash");
    filter.add_pattern("*.py");
    dialog.add_filter(&filter);
    dialog.set_modal(true);
    dialog.set_keep_above(true);

    let selected = if dialog.run() == gtk::ResponseType::Accept {
        dialog
            .filename()
            .map(|path| path.to_string_lossy().into_owned())
    } else {
        None
    };
    dialog.close();
    selected
}

impl LocalSttApp {
    pub(in crate::app) fn open_voice_commands(&mut self) {
        if self.recording_pipeline_busy() || self.voice_commands.running {
            if self.config.show_result_notifications {
                self.overlay.show_notice(
                    "Finish the current recording or command before opening Voice Commands",
                    false,
                    self.now(),
                    self.config.notification_seconds(),
                );
            }
            return;
        }
        self.settings.open = false;
        self.voice_commands.reload(&self.config);
        if let Some(problem) = &self.voice_command_problem {
            self.voice_commands
                .set_message(format!("{problem} Choose another shortcut and save."), false);
        }
        self.voice_commands.open = true;
        self.voice_commands.focus_pending = true;
        self.overlay.dismiss();
    }

    pub(in crate::app) fn close_voice_commands(&mut self) {
        self.voice_commands.open = false;
        self.voice_commands.focus_pending = false;
        self.voice_commands.capturing_hotkey = false;
        self.voice_commands.shortcut_capture.reset();
        self.overlay.dismiss();
    }

    pub(in crate::app) fn draw_voice_commands(&mut self, ctx: &egui::Context) {
        let captured = self.poll_voice_hotkey_capture(ctx);
        let mut close_requested = false;
        let mut save_requested = false;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(14, 15, 15))
                    .inner_margin(24.0),
            )
            .show(ctx, |ui| {
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
                    let drag = ui.interact(
                        header.response.rect,
                        ui.id().with("voice-commands-window-drag"),
                        Sense::drag(),
                    );
                    let drag = drag.on_hover_cursor(CursorIcon::Grab);
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close_requested = true;
                        }
                    });
                });
                ui.add_space(14.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.checkbox(
                            &mut self.voice_commands.enabled,
                            "Enable voice-command recording",
                        );
                        ui.label(help_text(
                            "The voice-command hotkey is independent from normal transcription.",
                        ));
                        self.draw_voice_hotkey(ui);
                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(12.0);
                        self.draw_command_list(ui);
                        ui.add_space(16.0);
                        if ui.button("Save changes").clicked() {
                            save_requested = true;
                        }
                        if let Some(message) = &self.voice_commands.message {
                            ui.add_space(10.0);
                            ui.label(RichText::new(&message.text).color(if message.ok {
                                Color32::from_rgb(112, 196, 135)
                            } else {
                                Color32::from_rgb(215, 93, 93)
                            }));
                        }
                    });
            });

        if save_requested {
            self.save_voice_commands();
        }
        let escape_closes = !self.voice_commands.capturing_hotkey
            && !captured
            && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if close_requested || escape_closes {
            self.close_voice_commands();
        }
    }

    fn poll_voice_hotkey_capture(&mut self, ctx: &egui::Context) -> bool {
        if !self.voice_commands.capturing_hotkey {
            return false;
        }
        for event in ctx.input(|input| input.events.clone()) {
            match self.voice_commands.shortcut_capture.feed(&event) {
                Some(CaptureOutcome::Captured(shortcut)) => {
                    self.voice_commands.hotkey = shortcut;
                    self.voice_commands.capturing_hotkey = false;
                    self.voice_commands.shortcut_capture.reset();
                    self.voice_commands.set_message(
                        format!(
                            "Captured {}. Save changes to activate it.",
                            friendly_name(&self.voice_commands.hotkey)
                        ),
                        true,
                    );
                    return true;
                }
                Some(CaptureOutcome::Unsupported(message)) => {
                    self.voice_commands.shortcut_capture.reset();
                    self.voice_commands.set_message(
                        format!("{message} Press another key or combination."),
                        false,
                    );
                }
                None => {}
            }
        }
        false
    }

    fn draw_voice_hotkey(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.label(RichText::new("Voice-command hotkey").strong());
        ui.label("Press once to start recording a command and again to stop.");
        ui.add_space(8.0);
        egui::Frame::NONE
            .fill(Color32::from_rgb(23, 25, 25))
            .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(54, 58, 58)))
            .corner_radius(egui::CornerRadius::same(9))
            .inner_margin(12.0)
            .show(ui, |ui| {
                if self.voice_commands.capturing_hotkey {
                    ui.label(
                        RichText::new("Press the voice-command shortcut now…")
                            .strong()
                            .color(Color32::from_rgb(67, 196, 214)),
                    );
                    if ui.button("Cancel capture").clicked() {
                        self.voice_commands.capturing_hotkey = false;
                        self.voice_commands.shortcut_capture.reset();
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(help_text("Current shortcut"));
                            ui.label(
                                RichText::new(friendly_name(&self.voice_commands.hotkey))
                                    .size(17.0)
                                    .strong(),
                            );
                        });
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                let label = if self.voice_commands.hotkey.trim().is_empty() {
                                    "Set shortcut"
                                } else {
                                    "Change shortcut"
                                };
                                if ui.button(label).clicked() {
                                    self.voice_commands.capturing_hotkey = true;
                                    self.voice_commands.message = None;
                                }
                            },
                        );
                    });
                }
            });
        ui.label(help_text(format!(
            "Must differ from the normal recording hotkey ({}).",
            friendly_name(&self.config.hotkey)
        )));
    }

    fn draw_command_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Commands and script chains").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Add command").clicked() {
                    self.voice_commands.commands.push(VoiceCommand {
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
        for (command_index, command) in self.voice_commands.commands.iter_mut().enumerate() {
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
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.button("Delete command").clicked() {
                                    remove_command = Some(command_index);
                                }
                            },
                        );
                    });
                    ui.label(help_text("Spoken words or phrases, separated by commas"));
                    ui.add(
                        egui::TextEdit::singleline(&mut command.phrase)
                            .hint_text("Hello, hello there, start report"),
                    );
                    ui.add_space(8.0);
                    ui.label(help_text(
                        "Scripts run from top to bottom. Linux accepts .sh, .bash, and .py files.",
                    ));

                    let mut remove_script = None;
                    for (script_index, script) in command.scripts.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let controls_width = 156.0 + ui.spacing().item_spacing.x * 2.0;
                            let path_width = (ui.available_width() - controls_width).max(140.0);
                            ui.add_sized(
                                [path_width, 28.0],
                                egui::TextEdit::singleline(script)
                                    .hint_text("/full/path/to/script.sh"),
                            );
                            if ui.button("Browse…").clicked() {
                                if let Some(selected) = select_script_file() {
                                    *script = selected;
                                }
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
            self.voice_commands.commands.remove(index);
        }
        if self.voice_commands.commands.is_empty() {
            ui.label(help_text("No commands are configured yet."));
        }
    }

    fn save_voice_commands(&mut self) {
        let enabled = self.voice_commands.enabled;
        let hotkey = self.voice_commands.hotkey.trim().to_string();
        let commands = self.voice_commands.commands.clone();

        let validation = (|| -> anyhow::Result<()> {
            if enabled {
                validate(&hotkey)?;
                if same_shortcut(&hotkey, &self.config.hotkey) {
                    anyhow::bail!(
                        "The voice-command hotkey must differ from the recording hotkey"
                    );
                }
                if commands.is_empty() {
                    anyhow::bail!("Add at least one command before enabling the feature");
                }
                validate_command_list(&commands)?;
            }
            Ok(())
        })();
        if let Err(error) = validation {
            self.voice_commands.set_message(error.to_string(), false);
            return;
        }

        if let Err(error) = self.hotkeys.configure_voice_commands(enabled, &hotkey) {
            self.voice_commands.enabled = self.config.voice_commands_enabled;
            self.voice_commands
                .hotkey
                .clone_from(&self.config.voice_commands_hotkey);
            self.voice_commands.set_message(
                format!(
                    "Could not activate the voice-command hotkey: {error:#}. The existing configuration remains active."
                ),
                false,
            );
            return;
        }

        self.voice_command_problem = None;
        self.config.voice_commands_enabled = enabled;
        self.config.voice_commands_hotkey = hotkey;
        self.config.voice_commands = commands;
        self.tray.set_voice_commands_hint(
            self.config.voice_commands_enabled,
            &self.config.voice_commands_hotkey,
        );
        match config::save(&self.config) {
            Ok(()) => self.voice_commands.set_message(
                if enabled {
                    format!(
                        "Saved. Press {} to record a voice command.",
                        friendly_name(&self.config.voice_commands_hotkey)
                    )
                } else {
                    "Saved. Voice commands are disabled.".to_string()
                },
                true,
            ),
            Err(error) => self
                .voice_commands
                .set_message(format!("Could not save voice commands: {error:#}"), false),
        }
    }

    fn restore_idle_tray_after_command(&self) {
        self.tray.set_status(TrayStatus::Idle);
        if self.hotkey_problem.is_some() {
            self.tray
                .set_tooltip("local-stt — recording shortcut required; open Settings");
        } else if self.engine.is_some() {
            self.tray.set_tooltip("local-stt — Parakeet ready");
        } else {
            self.tray
                .set_tooltip(&format!("local-stt — {}", self.startup_status));
        }
    }

    pub(super) fn dispatch_voice_command(&mut self, spoken: String) {
        log::info!("voice-command transcript: {:?}", spoken.trim());
        let Some(command) = matching_command(&self.config.voice_commands, &spoken) else {
            self.restore_idle_tray_after_command();
            if self.config.show_result_notifications {
                let message = if spoken.trim().is_empty() {
                    "No voice command was heard".to_string()
                } else {
                    format!("No voice command matched “{}”", spoken.trim())
                };
                self.overlay.show_notice(
                    message,
                    false,
                    self.now(),
                    self.config.notification_seconds(),
                );
            } else {
                self.overlay.dismiss();
            }
            return;
        };

        log::info!(
            "voice command matched {:?} from transcript {:?}",
            command.phrase,
            spoken.trim()
        );
        self.voice_commands.running = true;
        self.tray.set_status(TrayStatus::Busy);
        self.tray
            .set_tooltip(&format!("local-stt — running voice command: {}", command.phrase));
        if self.config.show_transcribing_notifications {
            self.overlay
                .show_loading(format!("Running voice command: {}", command.phrase));
        } else {
            self.overlay.dismiss();
        }

        let phrase = command.phrase.clone();
        let tx = self.voice_commands.result_tx.clone();
        let wake = self.ui_wake.clone();
        thread::spawn(move || {
            let result = execute_command(&command).map_err(|error| format!("{error:#}"));
            let _ = tx.send(CommandExecutionResult { phrase, result });
            if let Some(ctx) = wake.lock().as_ref() {
                ctx.request_repaint();
            }
        });
    }

    pub(super) fn poll_voice_command_results(&mut self) {
        while let Ok(event) = self.voice_commands.result_rx.try_recv() {
            self.voice_commands.running = false;
            self.restore_idle_tray_after_command();
            match event.result {
                Ok(()) => {
                    log::info!("voice command completed: {}", event.phrase);
                    if self.config.show_result_notifications {
                        self.overlay.show_notice(
                            format!("Voice command completed: {}", event.phrase),
                            true,
                            self.now(),
                            self.config.notification_seconds(),
                        );
                    } else {
                        self.overlay.dismiss();
                    }
                }
                Err(error) => {
                    log::error!("voice command {:?} failed: {error}", event.phrase);
                    if self.config.show_result_notifications {
                        self.overlay.show_notice(
                            format!("Voice command failed: {error}"),
                            false,
                            self.now(),
                            self.config.notification_seconds(),
                        );
                    } else {
                        self.overlay.dismiss();
                    }
                }
            }
        }
    }
}
