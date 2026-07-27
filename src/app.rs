//! Main egui app: overlay + tray + configurable hotkey + audio + ASR.

use anyhow::Result;
use arboard::Clipboard;
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Color32, RichText, ViewportCommand};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use crate::asr::AsrEngine;
use crate::audio::Recorder;
use crate::config::{self, Config};
use crate::hotkey::{friendly_name, validate, Hotkeys, UiWake};
use crate::overlay::{Overlay, OverlayAction, OverlayState, CARD_W};
use crate::paste::PasteTarget;
use crate::tray::{Tray, TrayAction};
use crate::util::SAMPLE_RATE;

const LIVE_CHUNK_SECS: u32 = 10;
const LIVE_CHUNK_SAMPLES: usize = (SAMPLE_RATE as usize) * (LIVE_CHUNK_SECS as usize);
const SETTINGS_W: f32 = 650.0;
const SETTINGS_H: f32 = 430.0;

enum WorkerMsg {
    EngineStatus(String),
    EngineReady(Result<Arc<AsrEngine>, String>),
    ChunkDone { id: usize, text: String },
}

struct LiveSession {
    next_id: usize,
    in_flight: usize,
    done: BTreeMap<usize, String>,
    expected: Option<usize>,
    finishing: bool,
}

impl LiveSession {
    fn new() -> Self {
        Self {
            next_id: 0,
            in_flight: 0,
            done: BTreeMap::new(),
            expected: None,
            finishing: false,
        }
    }

    fn all_done(&self) -> bool {
        self.expected
            .is_some_and(|n| self.done.len() >= n && self.in_flight == 0)
    }

    fn joined(&self) -> String {
        self.done
            .values()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub struct LocalSttApp {
    overlay: Overlay,
    tray: Tray,
    hotkeys: Hotkeys,
    ui_wake: UiWake,
    recorder: Recorder,
    engine: Option<Arc<AsrEngine>>,
    recording: bool,
    session: Option<LiveSession>,
    worker_tx: Sender<WorkerMsg>,
    worker_rx: Receiver<WorkerMsg>,
    started: Instant,
    last_frame: Instant,
    wake_installed: bool,
    config: Config,
    startup_status: String,
    paste_target: Option<PasteTarget>,
    settings_open: bool,
    settings_focus_pending: bool,
    settings_hotkey: String,
    settings_auto_paste: bool,
    settings_notifications: bool,
    settings_message: Option<(String, bool)>,
    hotkey_problem: Option<String>,
}

impl LocalSttApp {
    pub fn new(cc: &eframe::CreationContext<'_>, mut config: Config) -> Result<Self> {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        cc.egui_ctx.set_style(style);

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
        let recorder = Recorder::new()?;
        let (worker_tx, worker_rx) = unbounded();

        let tx = worker_tx.clone();
        let wake = ui_wake.clone();
        thread::spawn(move || {
            let status_tx = tx.clone();
            let status_wake = wake.clone();
            let result = AsrEngine::load_with_status(move |message| {
                let _ = status_tx.send(WorkerMsg::EngineStatus(message));
                if let Some(ctx) = status_wake.lock().as_ref() {
                    ctx.request_repaint();
                }
            })
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(WorkerMsg::EngineReady(result));
            if let Some(ctx) = wake.lock().as_ref() {
                ctx.request_repaint();
            }
        });

        let mut overlay = Overlay::default();
        let startup_status = "Starting local speech recognition…".to_string();
        if let Some(problem) = &hotkey_problem {
            overlay.show_persistent_notice(
                format!("{problem} Recording is disabled until a new shortcut is saved."),
                false,
            );
        } else if config.show_notifications {
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
            worker_tx,
            worker_rx,
            started: Instant::now(),
            last_frame: Instant::now(),
            wake_installed: false,
            settings_hotkey: config.hotkey.clone(),
            settings_auto_paste: config.auto_paste,
            settings_notifications: config.show_notifications,
            config,
            startup_status,
            paste_target: None,
            settings_open: false,
            settings_focus_pending: false,
            settings_message: None,
            hotkey_problem,
        })
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    fn spawn_chunk(&mut self, id: usize, audio: Vec<f32>) {
        let Some(engine) = self.engine.clone() else {
            return;
        };
        let tx = self.worker_tx.clone();
        let wake = self.ui_wake.clone();
        if let Some(s) = self.session.as_mut() {
            s.in_flight += 1;
        }
        let secs = audio.len() as f32 / SAMPLE_RATE as f32;
        println!("[local-stt] queue chunk #{id} ({secs:.1}s)");
        thread::spawn(move || {
            let label = format!("chunk#{id}");
            let text = match engine.transcribe_labeled(&audio, Some(&label)) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("[local-stt] chunk #{id} error: {e:#}");
                    String::new()
                }
            };
            let _ = tx.send(WorkerMsg::ChunkDone { id, text });
            if let Some(ctx) = wake.lock().as_ref() {
                ctx.request_repaint();
            }
        });
    }

    fn pump_live_chunks(&mut self) {
        if !self.recording || self.engine.is_none() {
            return;
        }
        while self.recorder.buffered_samples() >= LIVE_CHUNK_SAMPLES {
            let Some(chunk) = self.recorder.take_prefix(LIVE_CHUNK_SAMPLES) else {
                break;
            };
            let id = {
                let session = self.session.get_or_insert_with(LiveSession::new);
                let id = session.next_id;
                session.next_id += 1;
                id
            };
            self.spawn_chunk(id, chunk);
        }
    }

    fn toggle_record(&mut self) {
        if self.engine.is_none() {
            if self.config.show_notifications {
                self.overlay.show_loading(self.startup_status.clone());
            }
            return;
        }
        if self.session.as_ref().is_some_and(|s| s.finishing) {
            return;
        }

        if !self.recording {
            self.paste_target = self
                .config
                .auto_paste
                .then(PasteTarget::capture);
            self.recording = true;
            self.session = Some(LiveSession::new());
            self.recorder.start();
            if self.config.show_notifications {
                self.overlay.show_listening();
            }
            self.tray.set_tooltip("local-stt — recording…");
            println!("[local-stt] recording (live {LIVE_CHUNK_SECS}s chunks)");
        } else {
            self.recording = false;
            let tail = self.recorder.stop();
            let (id, expected) = {
                let session = self.session.get_or_insert_with(LiveSession::new);
                let id = session.next_id;
                session.next_id += 1;
                session.finishing = true;
                let expected = id + 1;
                session.expected = Some(expected);
                (id, expected)
            };

            if self.config.show_notifications {
                self.overlay.show_processing();
            }
            self.tray.set_tooltip("local-stt — transcribing…");
            println!("[local-stt] stopped — expecting {expected} chunks");

            if tail.len() >= (SAMPLE_RATE as usize) * 3 / 10 {
                self.spawn_chunk(id, tail);
            } else {
                let _ = self.worker_tx.send(WorkerMsg::ChunkDone {
                    id,
                    text: String::new(),
                });
            }
            self.try_finalize();
        }
    }

    fn try_finalize(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        if !session.finishing || !session.all_done() {
            return;
        }

        let text = session.joined();
        let ok = !text.is_empty();
        let footer = if ok {
            match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(&text)) {
                Ok(()) if self.config.auto_paste => {
                    let target = self.paste_target.take().unwrap_or_default();
                    match target.paste_ctrl_v() {
                        Ok(backend) => format!("Copied and pasted with Ctrl+V ({backend})"),
                        Err(error) => format!("Copied; auto-paste failed: {error}"),
                    }
                }
                Ok(()) => "Copied to clipboard".into(),
                Err(error) => format!("Clipboard error: {error}"),
            }
        } else {
            "No speech was detected".into()
        };

        if ok {
            println!("[local-stt] result: {text}");
        } else {
            println!("[local-stt] nothing heard");
        }
        if self.config.show_notifications {
            self.overlay
                .show_result(text, ok, footer, self.now());
        } else {
            self.overlay.dismiss();
        }
        self.tray.set_tooltip("local-stt — Parakeet ready");
        self.session = None;
        self.paste_target = None;
    }

    fn poll_workers(&mut self) {
        while let Ok(msg) = self.worker_rx.try_recv() {
            match msg {
                WorkerMsg::EngineStatus(message) => {
                    self.startup_status = message.clone();
                    self.tray.set_tooltip(&format!("local-stt — {message}"));
                    if self.config.show_notifications
                        && !self.settings_open
                        && self.hotkey_problem.is_none()
                    {
                        self.overlay.show_loading(message);
                    }
                }
                WorkerMsg::EngineReady(Ok(engine)) => {
                    self.startup_status = "Parakeet ready".into();
                    self.tray
                        .set_tooltip(&format!("local-stt — {}", engine.label()));
                    self.engine = Some(engine);
                    if !self.settings_open {
                        if let Some(problem) = &self.hotkey_problem {
                            self.overlay.show_persistent_notice(
                                format!(
                                    "{problem} Open the tray menu → Settings and choose another shortcut."
                                ),
                                false,
                            );
                            self.tray.set_tooltip(
                                "local-stt — recording shortcut required; open Settings",
                            );
                        } else if self.config.show_notifications {
                            self.overlay.show_notice(
                                format!(
                                    "Parakeet ready — press {}",
                                    friendly_name(&self.config.hotkey)
                                ),
                                true,
                                self.now(),
                                3.5,
                            );
                        }
                    }
                }
                WorkerMsg::EngineReady(Err(error)) => {
                    self.startup_status = format!("Model load failed: {error}");
                    self.tray.set_tooltip("local-stt — model load failed");
                    if self.config.show_notifications && !self.settings_open {
                        self.overlay.show_notice(
                            self.startup_status.clone(),
                            false,
                            self.now(),
                            12.0,
                        );
                    }
                }
                WorkerMsg::ChunkDone { id, text } => {
                    if let Some(session) = self.session.as_mut() {
                        session.in_flight = session.in_flight.saturating_sub(1);
                        session.done.insert(id, text);
                    }
                    self.try_finalize();
                }
            }
        }
    }

    fn open_settings(&mut self) {
        if self.recording || self.session.as_ref().is_some_and(|session| session.finishing) {
            if self.config.show_notifications {
                self.overlay.show_notice(
                    "Finish the current recording before opening Settings",
                    false,
                    self.now(),
                    3.5,
                );
            }
            return;
        }

        self.settings_hotkey = self.config.hotkey.clone();
        self.settings_auto_paste = self.config.auto_paste;
        self.settings_notifications = self.config.show_notifications;
        self.settings_message = self.hotkey_problem.as_ref().map(|problem| {
            (
                format!("{problem} Choose a different shortcut and click Save and apply."),
                false,
            )
        });
        self.settings_open = true;
        self.settings_focus_pending = true;
        self.overlay.dismiss();
    }

    fn apply_settings(&mut self) {
        let requested = self.settings_hotkey.trim().to_string();
        if let Err(error) = validate(&requested) {
            self.settings_message = Some((error.to_string(), false));
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
            self.settings_message = Some((
                format!(
                    "{error}. The recording shortcut has been disabled. Choose another shortcut."
                ),
                false,
            ));
            return;
        }

        self.hotkey_problem = None;
        self.config.hotkey = requested;
        self.config.auto_paste = self.settings_auto_paste;
        self.config.show_notifications = self.settings_notifications;
        self.tray.set_hotkey_hint(&self.config.hotkey);
        match config::save(&self.config) {
            Ok(()) => {
                self.settings_message = Some((
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
                if !self.config.show_notifications {
                    self.overlay.dismiss();
                }
            }
            Err(error) => {
                self.settings_message = Some((format!("Could not save settings: {error:#}"), false));
            }
        }
    }

    fn draw_settings(&mut self, ctx: &egui::Context) {
        let mut apply = false;
        let mut close = false;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(14, 15, 15))
                    .inner_margin(24.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("local-stt settings");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✕").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(14.0);

                ui.label(RichText::new("Recording hotkey").strong());
                ui.label(
                    "Use one key, or modifiers followed by one key. The default Backquote is the physical ` / ~ key.",
                );
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings_hotkey)
                        .hint_text("Backquote, F8, ctrl+shift+Space")
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label("Presets:");
                    if ui.button("Tilde / backquote").clicked() {
                        self.settings_hotkey = "Backquote".into();
                    }
                    if ui.button("F8").clicked() {
                        self.settings_hotkey = "F8".into();
                    }
                    if ui.button("Ctrl+Shift+Space").clicked() {
                        self.settings_hotkey = "ctrl+shift+Space".into();
                    }
                });
                ui.label(
                    RichText::new(
                        "Examples: KeyR, alt+KeyR, super+F9, shift+Backquote, MediaPlayPause",
                    )
                    .small()
                    .weak(),
                );

                ui.add_space(18.0);
                ui.checkbox(
                    &mut self.settings_auto_paste,
                    "Automatically paste the transcription with Ctrl+V",
                );
                ui.label(
                    RichText::new(
                        "The result is always copied first. Auto-paste uses xdotool on X11 and wtype/ydotool on Wayland.",
                    )
                    .small()
                    .weak(),
                );

                ui.add_space(12.0);
                ui.checkbox(
                    &mut self.settings_notifications,
                    "Show visual loading, recording, transcribing, and result notifications",
                );

                ui.add_space(18.0);
                if let Some((message, ok)) = &self.settings_message {
                    ui.label(
                        RichText::new(message)
                            .color(if *ok {
                                Color32::from_rgb(112, 196, 135)
                            } else {
                                Color32::from_rgb(215, 93, 93)
                            }),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Save and apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if apply {
            self.apply_settings();
        }
        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
            self.settings_focus_pending = false;
            if let Some(problem) = &self.hotkey_problem {
                self.overlay.show_persistent_notice(
                    format!("{problem} Open the tray menu → Settings to choose another shortcut."),
                    false,
                );
            } else if self.config.show_notifications && self.engine.is_none() {
                self.overlay.show_loading(self.startup_status.clone());
            }
        }
    }

    fn sync_viewport(&mut self, ctx: &egui::Context) {
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1920.0, 1080.0));

        if self.settings_open {
            let x = ((monitor.x - SETTINGS_W) * 0.5).max(0.0);
            let y = ((monitor.y - SETTINGS_H) * 0.35).max(20.0);
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, y)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(SETTINGS_W, SETTINGS_H)));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
            if self.settings_focus_pending {
                ctx.send_viewport_cmd(ViewportCommand::Focus);
                self.settings_focus_pending = false;
            }
            return;
        }

        if self.overlay.is_visible() {
            let overlay_width = CARD_W.min((monitor.x - 24.0).max(360.0));
            let x = ((monitor.x - overlay_width) * 0.5).max(0.0);
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, 70.0)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
                overlay_width,
                self.overlay.desired_height(),
            )));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        } else {
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(-32000.0, -32000.0)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(8.0, 8.0)));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        }
    }
}

impl eframe::App for LocalSttApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }

        if !self.wake_installed {
            *self.ui_wake.lock() = Some(ctx.clone());
            self.wake_installed = true;
        }

        let dt = self.last_frame.elapsed().as_secs_f32().min(0.05);
        self.last_frame = Instant::now();
        self.poll_workers();
        self.pump_live_chunks();

        let hotkey_pressed = self.hotkeys.poll_toggle();
        if self.hotkeys.is_bound() && hotkey_pressed && !self.settings_open {
            self.toggle_record();
        }
        match self.tray.poll_action() {
            Some(TrayAction::Settings) => self.open_settings(),
            Some(TrayAction::Quit) => {
                ctx.send_viewport_cmd(ViewportCommand::Close);
                return;
            }
            None => {}
        }

        if self.recording {
            self.overlay.rms = self.recorder.rms();
        }
        self.overlay.tick(self.now(), dt);
        self.sync_viewport(ctx);
        let busy = self.settings_open
            || self.recording
            || self.overlay.is_visible()
            || self.session.as_ref().is_some_and(|session| session.finishing)
            || self.engine.is_none();
        ctx.request_repaint_after(std::time::Duration::from_millis(if busy { 33 } else { 250 }));

        if self.settings_open {
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
            match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(&text)) {
                Ok(()) => {
                    println!("[local-stt] edited result copied to clipboard");
                    self.overlay.dismiss();
                }
                Err(error) => {
                    self.overlay.show_notice(
                        format!("Could not update the clipboard: {error}"),
                        false,
                        self.now(),
                        8.0,
                    );
                }
            }
        }
    }
}
