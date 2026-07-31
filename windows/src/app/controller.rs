//! Thin application facade and eframe event-loop coordinator.

use anyhow::Result;
use eframe::egui::{self, Color32, ViewportCommand};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;

use crate::asr::AsrEngine;
use crate::audio::Recorder;
use crate::config::{self, Config};
use crate::hotkey::{Hotkeys, UiWake};
use crate::overlay::{Overlay, OverlayAction, OverlayState};
use crate::platform::PasteTarget;
use crate::tray::{Tray, TrayAction};

use super::lifecycle::WindowLifecycle;
use super::recording::LiveSession;
use super::settings::SettingsState;
use super::theme;
use super::transcription::TranscriptionWorker;

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
    pub(super) started: Instant,
    pub(super) last_frame: Instant,
    pub(super) wake_installed: bool,
    pub(super) config: Config,
    pub(super) startup_status: String,
    pub(super) paste_target: Option<PasteTarget>,
    pub(super) settings: SettingsState,
    pub(super) hotkey_problem: Option<String>,
    pub(super) lifecycle: WindowLifecycle,
}

impl LocalSttApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, mut config: Config) -> Result<Self> {
        theme::configure(&cc.egui_ctx);

        let ui_wake: UiWake = Arc::new(Mutex::new(None));
        let requested_hotkey = config.hotkey.clone();
        let (hotkeys, registration_warning) =
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
        let tray = Tray::new(&config.hotkey)?;
        let recorder = match Recorder::new(config.recording_device.as_ref()) {
            Ok(recorder) => recorder,
            Err(error) if config.recording_device.is_some() => {
                eprintln!(
                    "[local-stt] configured recording device is unavailable ({error:#}); using the system default"
                );
                config.recording_device = None;
                config::save(&config)?;
                Recorder::new(None)?
            }
            Err(error) => return Err(error),
        };
        let transcription = TranscriptionWorker::spawn(ui_wake.clone())?;
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
            started: Instant::now(),
            last_frame: Instant::now(),
            wake_installed: false,
            settings: SettingsState::from_config(&config),
            config,
            startup_status,
            paste_target: None,
            hotkey_problem,
            lifecycle: WindowLifecycle::default(),
        })
    }

    pub(super) fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
}

impl LocalSttApp {
    fn install_ui_wake(&mut self, ctx: &egui::Context) {
        if !self.wake_installed {
            *self.ui_wake.lock() = Some(ctx.clone());
            self.wake_installed = true;
        }
    }

    fn handle_user_commands(&mut self, ctx: &egui::Context) -> bool {
        let hotkey_pressed = self.hotkeys.poll_toggle();
        if self.hotkeys.is_bound() && hotkey_pressed && !self.settings.open {
            self.toggle_record();
        }
        match self.tray.poll_action() {
            Some(TrayAction::Settings) => self.open_settings(),
            Some(TrayAction::Quit) => {
                self.lifecycle.request_exit();
                ctx.send_viewport_cmd(ViewportCommand::Close);
                return true;
            }
            None => {}
        }
        false
    }

    fn handle_window_close(&mut self, ctx: &egui::Context) -> bool {
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if !close_requested {
            return false;
        }
        if !self.lifecycle.should_cancel_close(close_requested) {
            return true;
        }

        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        if self.settings.open {
            self.close_settings();
        } else {
            self.overlay.dismiss();
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
            || self.recording
            || self.overlay.is_visible()
            || self
                .session
                .as_ref()
                .is_some_and(|session| session.finishing)
            || self.engine.is_none();
        let interval = if busy { 33 } else { 250 };
        ctx.request_repaint_after(std::time::Duration::from_millis(interval));
    }

    fn render(&mut self, ctx: &egui::Context) {
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
        self.install_ui_wake(ctx);
        if self.handle_window_close(ctx) {
            return;
        }

        let dt = self.last_frame.elapsed().as_secs_f32().min(0.05);
        self.last_frame = Instant::now();
        self.poll_workers();
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
