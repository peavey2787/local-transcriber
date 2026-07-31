//! Thin application facade and eframe event-loop coordinator.

use anyhow::Result;
use eframe::egui::{self, Color32, ViewportCommand};
use std::sync::Arc;
use std::time::Instant;

use crate::asr::AsrEngine;
use crate::audio::Recorder;
use crate::config::{self, Config};
use crate::hotkey::Hotkeys;
use crate::overlay::{Overlay, OverlayAction, OverlayState};
use crate::platform::PasteTarget;
use crate::tray::{Tray, TrayAction};
use crate::ui_wake::UiWake;

use super::lifecycle::{CloseDecision, WindowLifecycle};
use super::recording::LiveSession;
use super::settings::{SettingsDeviceDiscovery, SettingsState, SettingsWindowState};
use super::theme;
use super::transcription::TranscriptionWorker;
use super::viewport::RootViewportState;

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
    pub(super) config: Config,
    pub(super) startup_status: String,
    pub(super) paste_target: Option<PasteTarget>,
    pub(super) settings: SettingsState,
    pub(super) settings_window: SettingsWindowState,
    pub(super) settings_device_discovery: SettingsDeviceDiscovery,
    pub(super) hotkey_problem: Option<String>,
    pub(super) lifecycle: WindowLifecycle,
    pub(super) viewport: RootViewportState,
    pub(super) next_session_id: u64,
}

impl LocalSttApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, mut config: Config) -> Result<Self> {
        theme::configure(&cc.egui_ctx);

        let ui_wake = UiWake::default();
        // Install the repaint target before tray and hotkey callbacks are registered.
        // External Windows events can then wake the persistent root immediately,
        // including during startup and after Settings has been closed.
        ui_wake.install(&cc.egui_ctx);
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
        let tray = Tray::new(&config.hotkey, ui_wake.clone())?;
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
        let settings_device_discovery = SettingsDeviceDiscovery::spawn(ui_wake.clone())?;
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
            settings: SettingsState::from_config(&config),
            settings_window: SettingsWindowState::default(),
            settings_device_discovery,
            config,
            startup_status,
            paste_target: None,
            hotkey_problem,
            lifecycle: WindowLifecycle::default(),
            viewport: RootViewportState::default(),
            next_session_id: 1,
        })
    }

    pub(super) fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
}

impl LocalSttApp {
    fn handle_user_commands(&mut self, ctx: &egui::Context) -> bool {
        let tray_action = self.tray.poll_action();
        if matches!(tray_action, Some(TrayAction::Quit)) {
            log::debug!("handling tray Quit command");
            self.lifecycle.request_exit();
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return true;
        }

        let hotkey_pressed = self.hotkeys.poll_toggle();
        if matches!(tray_action, Some(TrayAction::Settings)) {
            log::debug!("handling tray Settings command");
            self.open_settings();
            return false;
        }

        if should_toggle_recording(
            self.hotkeys.is_bound(),
            hotkey_pressed,
            self.settings_window.is_visible(),
            self.settings.is_capturing_hotkey(),
        ) {
            self.toggle_record();
        }
        false
    }

    fn handle_window_close(&mut self, ctx: &egui::Context) -> bool {
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        match self.lifecycle.decide(close_requested) {
            CloseDecision::None => false,
            CloseDecision::Exit => true,
            CloseDecision::CancelAndHide => {
                // Cancel the native close first and defer viewport movement until
                // the next frame. Settings is a presentation of the persistent
                // root window, not the process lifetime owner.
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                if self.settings_window.is_visible() {
                    self.close_settings();
                } else {
                    self.overlay.dismiss_immediately();
                }
                ctx.request_repaint();
                true
            }
            CloseDecision::ContinueAfterCancel => {
                // Some Windows/winit combinations report the same close request
                // for multiple frames. Keep cancelling it, but do not starve
                // tray commands, hotkeys, worker events, or viewport updates.
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                false
            }
        }
    }

    fn advance_frame_state(&mut self, dt: f32) {
        if self.recording {
            self.overlay.rms = self.recorder.rms();
        }
        self.overlay.tick(self.now(), dt);
    }

    fn request_next_frame(&self, ctx: &egui::Context) {
        let busy = self.settings_window.is_visible()
            || self.recording_pipeline_busy()
            || self.overlay.is_visible()
            || self.engine.is_none();
        let interval = if busy { 33 } else { 250 };
        ctx.request_repaint_after(std::time::Duration::from_millis(interval));
    }

    fn render(&mut self, ctx: &egui::Context) {
        if self.settings_window.is_visible() {
            self.render_settings(ctx);
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

fn should_toggle_recording(
    hotkey_is_bound: bool,
    hotkey_pressed: bool,
    settings_visible: bool,
    capturing_replacement_hotkey: bool,
) -> bool {
    hotkey_is_bound
        && hotkey_pressed
        && !settings_visible
        && !capturing_replacement_hotkey
}

impl eframe::App for LocalSttApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui_wake.install(ctx);
        if self.handle_window_close(ctx) {
            return;
        }

        let dt = self.last_frame.elapsed().as_secs_f32().min(0.05);
        self.last_frame = Instant::now();

        // Tray Settings/Quit commands have priority over worker completions and
        // recording hotkeys. Opening Settings therefore cancels the active
        // pipeline before a queued transcription can publish another result.
        if self.handle_user_commands(ctx) {
            return;
        }
        self.poll_workers();
        self.poll_settings_workers();
        self.pump_live_chunks();

        self.advance_frame_state(dt);
        self.sync_root_viewport(ctx);
        self.request_next_frame(ctx);
        self.render(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::should_toggle_recording;

    #[test]
    fn settings_window_blocks_the_recording_hotkey() {
        assert!(!should_toggle_recording(true, true, true, false));
    }

    #[test]
    fn recording_hotkey_works_when_settings_are_closed() {
        assert!(should_toggle_recording(true, true, false, false));
    }

    #[test]
    fn shortcut_capture_consumes_hotkey_input() {
        assert!(!should_toggle_recording(true, true, true, true));
    }
}
