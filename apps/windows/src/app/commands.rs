//! Voice-command editor integration and background script execution.

use eframe::egui;
use transcriber_core::commands::matching_command;
use transcriber_ui::tray::TrayStatus;
use transcriber_ui::voice_commands::{draw_voice_commands_panel, VoiceCommandsPanelOptions};

use crate::config;
use crate::hotkey::{capture_shortcut, friendly_name, same_shortcut, validate, CaptureOutcome};
use crate::platform::select_voice_command_script;

use super::controller::LocalSttApp;

fn begin_native_window_drag() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, SendMessageW, HTCAPTION, WM_NCLBUTTONDOWN,
    };

    // SAFETY: The foreground HWND is borrowed only for this synchronous
    // non-client drag request. ReleaseCapture hands mouse capture back to the
    // window manager before WM_NCLBUTTONDOWN begins the native move loop.
    unsafe {
        let window = GetForegroundWindow();
        if !window.is_null() {
            ReleaseCapture();
            SendMessageW(window, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
        }
    }
}

impl LocalSttApp {
    pub(in crate::app) fn open_voice_commands(&mut self) {
        if self.voice_commands.running {
            if self.config.show_result_notifications {
                self.overlay.show_notice(
                    "Finish the current command before opening Voice Commands",
                    false,
                    self.now(),
                    self.config.notification_seconds(),
                );
            }
            return;
        }
        self.cancel_recording_for_editor();
        self.settings_window.hide();
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
        self.voice_commands.form.capturing_hotkey = false;
        self.overlay.dismiss_immediately();
    }

    pub(in crate::app) fn render_voice_commands(&mut self, ctx: &egui::Context) {
        let captured = self.poll_voice_hotkey_capture(ctx);
        let panel = draw_voice_commands_panel(
            ctx,
            &mut self.voice_commands.form,
            &VoiceCommandsPanelOptions {
                platform_name: "Windows",
                accepted_extensions: ".ps1, .bat, .cmd, and .py",
                path_hint: r"C:\full\path\to\script.ps1",
                browse_enabled: true,
                recording_hotkey: &self.config.hotkey,
            },
            friendly_name,
        );

        if panel.drag_requested {
            begin_native_window_drag();
        }
        if let Some((command_index, script_index)) = panel.browse_request {
            match select_voice_command_script() {
                Ok(Some(selected)) => {
                    if let Some(script) = self
                        .voice_commands
                        .form
                        .commands
                        .get_mut(command_index)
                        .and_then(|command| command.scripts.get_mut(script_index))
                    {
                        *script = selected;
                    }
                }
                Ok(None) => {}
                Err(error) => self.voice_commands.set_message(error, false),
            }
        }
        if let Some(command_index) = panel.test_request {
            self.test_voice_command(command_index);
        }
        if panel.save_requested {
            self.save_voice_commands();
        }
        let escape_closes = !self.voice_commands.form.capturing_hotkey
            && !captured
            && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if panel.close_requested || escape_closes {
            self.close_voice_commands();
        }
    }

    fn poll_voice_hotkey_capture(&mut self, ctx: &egui::Context) -> bool {
        if !self.voice_commands.form.capturing_hotkey {
            return false;
        }
        for event in ctx.input(|input| input.events.clone()) {
            match capture_shortcut(&event) {
                Some(CaptureOutcome::Captured(shortcut)) => {
                    self.voice_commands.form.hotkey = shortcut;
                    self.voice_commands.form.capturing_hotkey = false;
                    self.voice_commands.set_message(
                        format!(
                            "Captured {}. Save changes to activate it.",
                            friendly_name(&self.voice_commands.form.hotkey)
                        ),
                        true,
                    );
                    return true;
                }
                Some(CaptureOutcome::Unsupported(message)) => {
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

    fn save_voice_commands(&mut self) {
        let enabled = self.voice_commands.form.enabled;
        let hotkey = self.voice_commands.form.hotkey.trim().to_string();
        let commands = self.voice_commands.form.commands.clone();

        let validation = (|| -> anyhow::Result<()> {
            if enabled {
                validate(&hotkey)?;
                if same_shortcut(&hotkey, &self.config.hotkey) {
                    anyhow::bail!(
                        "The voice-command hotkey must differ from the recording hotkey"
                    );
                }
                self.command_worker.validate(&commands)?;
            }
            Ok(())
        })();
        if let Err(error) = validation {
            self.voice_commands.set_message(error.to_string(), false);
            return;
        }

        if let Err(error) = self.hotkeys.configure_voice_commands(enabled, &hotkey) {
            self.voice_commands.form.enabled = self.config.voice_commands_enabled;
            self.voice_commands
                .form
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
                        "Saved. Close this window, then press {} to record a voice command. Use Test scripts to verify execution while this window is open.",
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

    fn test_voice_command(&mut self, command_index: usize) {
        if self.voice_commands.running {
            self.voice_commands
                .set_message("A voice command is already running.", false);
            return;
        }
        let Some(command) = self
            .voice_commands
            .form
            .commands
            .get(command_index)
            .cloned()
        else {
            self.voice_commands
                .set_message("The selected command no longer exists.", false);
            return;
        };
        if let Err(error) = self.command_worker.validate(std::slice::from_ref(&command)) {
            self.voice_commands.set_message(error.to_string(), false);
            return;
        }

        self.voice_commands.running = true;
        self.voice_commands
            .set_message(format!("Testing scripts for: {}", command.phrase), true);
        self.tray.set_status(TrayStatus::Busy);
        self.tray
            .set_tooltip(&format!("local-stt — testing voice command: {}", command.phrase));
        self.command_worker.execute(command);
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
        let Some(command) = matching_command(&self.config.voice_commands, &spoken) else {
            log::warn!("recognized voice command did not match a configured alias");
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

        self.command_worker.execute(command);
    }

    pub(super) fn poll_voice_command_results(&mut self) {
        while let Some(event) = self.command_worker.try_recv() {
            self.voice_commands.running = false;
            self.restore_idle_tray_after_command();
            match event.result {
                Ok(_output) => {
                    let message = format!("Voice command completed: {}", event.phrase);
                    log::info!("voice command completed: {}", event.phrase);
                    if self.voice_commands.open {
                        self.voice_commands.set_message(message, true);
                        self.overlay.dismiss();
                    } else if self.config.show_result_notifications {
                        self.overlay.show_notice(
                            message,
                            true,
                            self.now(),
                            self.config.notification_seconds(),
                        );
                    } else {
                        self.overlay.dismiss();
                    }
                }
                Err(error) => {
                    let message = format!("Voice command failed: {error}");
                    log::error!("voice command {:?} failed: {error}", event.phrase);
                    if self.voice_commands.open {
                        self.voice_commands.set_message(message, false);
                        self.overlay.dismiss();
                    } else if self.config.show_result_notifications {
                        self.overlay.show_notice(
                            message,
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
