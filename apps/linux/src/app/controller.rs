//! Thin application facade and eframe event-loop coordinator.

use anyhow::Result;
use eframe::egui::{self, Color32, ViewportCommand};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;

use transcriber_core::asr::AsrEngine;
use transcriber_core::commands::CommandWorker;
use transcriber_core::workflow::LiveSession;
use crate::audio::{create_recorder, Recorder};
use crate::config::{self, Config};
use crate::hotkey::{same_shortcut, Hotkeys, UiWake};
use transcriber_ui::overlay::{Overlay, OverlayAction, OverlayState};
use transcriber_ui::voice_commands::VoiceCommandsState;
use crate::paste::PasteTarget;
use crate::tray::Tray;
use transcriber_ui::tray::TrayAction;

use super::settings::SettingsState;
use super::transcription::{spawn as spawn_transcription, TranscriptionWorker};

pub struct LocalSttApp {
    pub(super) overlay: Overlay,
    pub(super) tray: Tray,
    pub(super) hotkeys: Hotkeys,
    pub(super) ui_wake: UiWake,
    pub(super) recorder: Recorder,
    pub(super) engine: Option<Arc<AsrEngine>>,
    pub(super) recording: bool,
    pub(super) session: Option<LiveSession>,
    pub(super) transcription: TranscriptionWorker,
    pub(super) command_worker: CommandWorker,
    pub(super) started: Instant,
    pub(super) last_frame: Instant,
    pub(super) wake_installed: bool,
    pub(super) config: Config,
    pub(super) startup_status: String,
    pub(super) paste_target: Option<PasteTarget>,
    pub(super) settings: SettingsState,
    pub(super) voice_commands: VoiceCommandsState,
    pub(super) hotkey_problem: Option<String>,
    pub(super) voice_command_problem: Option<String>,
    pub(super) next_session_id: u64,
}

impl LocalSttApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, mut config: Config) -> Result<Self> {
        transcriber_ui::theme::configure(&cc.egui_ctx);

        let ui_wake: UiWake = Arc::new(Mutex::new(None));
        let requested_hotkey = config.hotkey.clone();
        let (mut hotkeys, registration_warning) =
            Hotkeys::register(ui_wake.clone(), &requested_hotkey)?;
        let hotkey_problem = registration_warning.or_else(|| {
            requested_hotkey.trim().is_empty().then(|| {
                "No recording shortcut is set. Open the tray menu, choose Settings, and select a shortcut.".to_string()
            })
        });
        if hotkey_problem.is_some() {
            config.hotkey.clear();
            if let Err(error) = config::save(&config) {
                eprintln!("[local-stt] could not save the disabled hotkey state: {error:#}");
            }
        }
        let command_worker = crate::voice_commands::create_worker(ui_wake.clone());
        let voice_command_problem = if config.voice_commands_enabled {
            if config.voice_commands_hotkey.trim().is_empty() {
                Some("Voice commands are enabled, but no voice-command hotkey is set.".to_string())
            } else if !config.hotkey.trim().is_empty()
                && same_shortcut(&config.hotkey, &config.voice_commands_hotkey)
            {
                Some("The voice-command hotkey matches the recording hotkey.".to_string())
            } else if let Err(error) = command_worker.validate(&config.voice_commands) {
                Some(format!("The voice-command configuration is invalid: {error:#}"))
            } else {
                hotkeys
                    .configure_voice_commands(true, &config.voice_commands_hotkey)
                    .err()
                    .map(|error| format!("The voice-command hotkey is unavailable: {error:#}"))
            }
        } else {
            None
        };
        let tray = Tray::new(
            &config.hotkey,
            config.voice_commands_enabled && voice_command_problem.is_none(),
            &config.voice_commands_hotkey,
        )?;
        let recorder = match create_recorder(config.recording_device.as_ref()) {
            Ok(recorder) => recorder,
            Err(error) if config.recording_device.is_some() => {
                eprintln!(
                    "[local-stt] configured recording device is unavailable ({error:#}); using the system default"
                );
                config.recording_device = None;
                config::save(&config)?;
                create_recorder(None)?
            }
            Err(error) => return Err(error),
        };
        let transcription = spawn_transcription(ui_wake.clone())?;
        let mut overlay = Overlay::default();
        let startup_status = "Starting local speech recognition…".to_string();
        if let Some(problem) = &hotkey_problem {
            overlay.show_persistent_notice(
                format!("{problem} Recording is disabled until a new shortcut is saved."),
                false,
            );
        } else if config.show_loading_notifications {
            overlay.show_loading(startup_status.clone());
        }

        Ok(Self {
            overlay,
            tray,
            hotkeys,
            ui_wake,
            recorder,
            engine: None,
            recording: false,
            session: None,
            transcription,
            command_worker,
            started: Instant::now(),
            last_frame: Instant::now(),
            wake_installed: false,
            settings: SettingsState::from_config(&config),
            voice_commands: VoiceCommandsState::from_config(&config),
            config,
            startup_status,
            paste_target: None,
            hotkey_problem,
            voice_command_problem,
            next_session_id: 1,
        })
    }

    pub(super) fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
}

impl LocalSttApp {
    fn poll_platform_events() {
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }

    fn install_ui_wake(&mut self, ctx: &egui::Context) {
        if !self.wake_installed {
            *self.ui_wake.lock() = Some(ctx.clone());
            self.wake_installed = true;
        }
    }

    fn handle_user_commands(&mut self, ctx: &egui::Context) -> bool {
        match self.tray.poll_action() {
            Some(TrayAction::Settings) => self.open_settings(),
            Some(TrayAction::VoiceCommands) => self.open_voice_commands(),
            Some(TrayAction::Quit) => {
                ctx.send_viewport_cmd(ViewportCommand::Close);
                return true;
            }
            None => {}
        }

        if self.settings.open || self.voice_commands.open {
            let presses = self.hotkeys.poll();
            if self.voice_commands.open
                && presses.voice_command
                && !self.voice_commands.form.capturing_hotkey
            {
                self.voice_commands.set_message(
                    "Voice-command recording is paused while this editor is open. Use Test scripts here, or close the window before pressing the voice-command hotkey.",
                    false,
                );
            }
            return false;
        }

        let presses = self.hotkeys.poll();
        if self.hotkeys.is_bound() && presses.recording {
            self.toggle_record();
        } else if self.config.voice_commands_enabled
            && self.hotkeys.is_voice_commands_bound()
            && presses.voice_command
        {
            self.toggle_voice_command();
        }
        false
    }

    fn advance_frame_state(&mut self, dt: f32) {
        if self.recording {
            self.overlay.rms = self.recorder.rms();
        }
        self.overlay.tick(self.now(), dt);
    }

    fn request_next_frame(&self, ctx: &egui::Context) {
        let busy = self.settings.open
            || self.voice_commands.open
            || self.voice_commands.running
            || self.recording
            || self.overlay.is_visible()
            || self.session.as_ref().is_some_and(|session| session.finishing)
            || self.engine.is_none();
        let interval = if busy { 33 } else { 250 };
        ctx.request_repaint_after(std::time::Duration::from_millis(interval));
    }

    fn render(&mut self, ctx: &egui::Context) {
        if self.voice_commands.open {
            self.draw_voice_commands(ctx);
            return;
        }
        if self.settings.open {
            self.draw_settings(ctx);
            return;
        }
        if matches!(&self.overlay.state, OverlayState::Hidden) && self.overlay.alpha < 0.01 {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::TRANSPARENT))
                .show(ctx, |_ui| {});
            return;
        }

        let mut overlay_action = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                ui.multiply_opacity(self.overlay.alpha);
                overlay_action = self.overlay.ui(ctx, ui);
            });
        if let Some(OverlayAction::CopyDone(text)) = overlay_action {
            self.copy_edited_result(text);
        }
    }
}

impl eframe::App for LocalSttApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        Self::poll_platform_events();
        self.install_ui_wake(ctx);

        let dt = self.last_frame.elapsed().as_secs_f32().min(0.05);
        self.last_frame = Instant::now();
        self.poll_workers();
        self.poll_voice_command_results();
        self.pump_live_chunks();
        if self.handle_user_commands(ctx) {
            return;
        }

        self.advance_frame_state(dt);
        self.sync_viewport(ctx);
        self.request_next_frame(ctx);
        self.render(ctx);
    }
}
