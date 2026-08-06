//! Recording lifecycle, live chunk scheduling, and session finalization.

use crate::hotkey::friendly_name;
use transcriber_ui::overlay::OverlayState;
use transcriber_ui::tray::TrayStatus;
use crate::paste::PasteTarget;
use transcriber_core::audio::SAMPLE_RATE;

use super::controller::LocalSttApp;
use super::transcription::{QueueError, TranscriptionEvent};

use transcriber_core::workflow::{
    LiveSession, RecordingPurpose, LIVE_CHUNK_SAMPLES, LIVE_CHUNK_SECONDS,
    MIN_FINAL_CHUNK_SAMPLES,
};

impl LocalSttApp {
    pub(super) fn recording_pipeline_busy(&self) -> bool {
        self.recording
            || self
                .session
                .as_ref()
                .is_some_and(|session| session.finishing)
    }

    fn spawn_chunk(&mut self, session_id: u64, chunk_id: usize, audio: Vec<f32>) {
        let secs = audio.len() as f32 / SAMPLE_RATE as f32;
        log::info!("queued transcription session {session_id} chunk #{chunk_id} ({secs:.1}s)");

        if let Err(error) = self.transcription.queue(session_id, chunk_id, audio) {
            let message = match error {
                QueueError::Full => {
                    "recognizer queue is full; recording is arriving faster than it can be transcribed"
                        .to_string()
                }
                QueueError::WorkerStopped => {
                    "recognizer worker is no longer running".to_string()
                }
            };
            if let Some(session) = self
                .session
                .as_mut()
                .filter(|session| session.id == session_id)
            {
                session.mark_queue_failure(chunk_id, message);
            }
        }
    }

    pub(super) fn pump_live_chunks(&mut self) {
        if !self.recording || self.engine.is_none() {
            return;
        }
        while self.recorder.buffered_samples() >= LIVE_CHUNK_SAMPLES {
            let Some(chunk) = self.recorder.take_prefix(LIVE_CHUNK_SAMPLES) else {
                break;
            };
            let Some(session) = self.session.as_mut() else {
                break;
            };
            let session_id = session.id;
            let chunk_id = session.schedule_chunk();
            self.spawn_chunk(session_id, chunk_id, chunk);
        }
    }

    pub(super) fn toggle_record(&mut self) {
        self.toggle_recording_purpose(RecordingPurpose::Transcription);
    }

    pub(super) fn toggle_voice_command(&mut self) {
        if !self.config.voice_commands_enabled
            || !self.hotkeys.is_voice_commands_bound()
            || self.voice_commands.running
        {
            return;
        }
        self.toggle_recording_purpose(RecordingPurpose::VoiceCommand);
    }

    fn toggle_recording_purpose(&mut self, purpose: RecordingPurpose) {
        if self.voice_commands.running {
            return;
        }
        if self.engine.is_none() {
            self.show_engine_loading_state();
            return;
        }
        if self.session.as_ref().is_some_and(|session| session.finishing) {
            return;
        }

        if self.recording {
            if self.session.as_ref().is_some_and(|session| session.purpose == purpose) {
                self.stop_recording();
            }
        } else {
            self.start_recording(purpose);
        }
    }

    fn show_engine_loading_state(&mut self) {
        if self.config.show_loading_notifications {
            self.overlay.show_loading(self.startup_status.clone());
        } else {
            self.overlay.dismiss();
        }
    }

    fn start_recording(&mut self, purpose: RecordingPurpose) {
        if let Err(error) = self.recorder.start() {
            self.handle_microphone_start_error(error);
            return;
        }

        self.paste_target = (purpose == RecordingPurpose::Transcription
            && self.config.auto_paste)
            .then(PasteTarget::capture);
        self.recording = true;
        let session_id = self.next_session_id;
        self.next_session_id = self.next_session_id.wrapping_add(1).max(1);
        self.session = Some(LiveSession::new(session_id, purpose));
        if self.config.show_recording_notifications {
            match purpose {
                RecordingPurpose::Transcription => self.overlay.show_listening(),
                RecordingPurpose::VoiceCommand => self.overlay.show_command_listening(),
            }
        } else {
            self.overlay.dismiss();
        }
        self.tray.set_status(TrayStatus::Recording);
        self.tray.set_tooltip(match purpose {
            RecordingPurpose::Transcription => "local-stt — recording…",
            RecordingPurpose::VoiceCommand => "local-stt — recording voice command…",
        });
        println!("[local-stt] recording {purpose:?} (live {LIVE_CHUNK_SECONDS}s chunks)");
    }

    fn handle_microphone_start_error(&mut self, error: anyhow::Error) {
        self.paste_target = None;
        self.session = None;
        self.recording = false;
        let message = format!("Could not start microphone recording: {error:#}");
        eprintln!("[local-stt] {message}");
        self.overlay.show_notice(
            message,
            false,
            self.now(),
            self.config.notification_seconds(),
        );
        self.tray.set_tooltip("local-stt — microphone unavailable");
    }

    fn stop_recording(&mut self) {
        self.recording = false;
        let tail = self.recorder.stop();
        let Some((session_id, chunk_id, expected)) = self.finish_session_schedule() else {
            self.overlay.dismiss();
            return;
        };

        let purpose = self
            .session
            .as_ref()
            .map(|session| session.purpose)
            .unwrap_or(RecordingPurpose::Transcription);
        if self.config.show_transcribing_notifications {
            match purpose {
                RecordingPurpose::Transcription => self.overlay.show_processing(),
                RecordingPurpose::VoiceCommand => self.overlay.show_command_processing(),
            }
        } else {
            self.overlay.dismiss();
        }
        self.tray.set_status(TrayStatus::Busy);
        self.tray.set_tooltip(match purpose {
            RecordingPurpose::Transcription => "local-stt — transcribing…",
            RecordingPurpose::VoiceCommand => "local-stt — recognizing voice command…",
        });
        println!("[local-stt] stopped — expecting {expected} chunks");

        if tail.len() >= MIN_FINAL_CHUNK_SAMPLES {
            self.spawn_chunk(session_id, chunk_id, tail);
        } else if let Some(session) = self.session.as_mut() {
            session.complete_chunk(chunk_id, Ok(String::new()));
        }
        self.try_finalize();
    }

    fn finish_session_schedule(&mut self) -> Option<(u64, usize, usize)> {
        let session = self.session.as_mut()?;
        let session_id = session.id;
        let (chunk_id, expected) = session.finish_schedule();
        Some((session_id, chunk_id, expected))
    }

    fn try_finalize(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        if !session.finishing || !session.all_done() {
            return;
        }

        let purpose = session.purpose;
        let text = session.joined();
        let errors = session.errors();
        self.session = None;

        match purpose {
            RecordingPurpose::Transcription if errors.is_empty() => {
                self.present_transcription(text);
                self.tray.set_status(TrayStatus::Idle);
                self.tray.set_tooltip("local-stt — Parakeet ready");
            }
            RecordingPurpose::Transcription => {
                self.paste_target = None;
                self.present_transcription_failure(text, errors);
                self.tray.set_status(TrayStatus::Idle);
                self.tray.set_tooltip("local-stt — Parakeet ready");
            }
            RecordingPurpose::VoiceCommand if errors.is_empty() => {
                self.paste_target = None;
                self.dispatch_voice_command(text);
            }
            RecordingPurpose::VoiceCommand => {
                self.paste_target = None;
                self.tray.set_status(TrayStatus::Idle);
                self.tray.set_tooltip("local-stt — Parakeet ready");
                let summary = format!(
                    "Voice-command recognition failed for {} audio chunk(s)",
                    errors.len()
                );
                for error in errors {
                    log::error!("{error}");
                }
                if self.config.show_result_notifications {
                    self.overlay.show_notice(
                        summary,
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

    pub(super) fn poll_workers(&mut self) {
        while let Some(event) = self.transcription.try_recv() {
            self.handle_transcription_event(event);
        }
    }

    fn handle_transcription_event(&mut self, event: TranscriptionEvent) {
        match event {
            TranscriptionEvent::EngineStatus(message) => self.handle_engine_status(message),
            TranscriptionEvent::EngineReady(Ok(engine)) => self.handle_engine_ready(engine),
            TranscriptionEvent::EngineReady(Err(error)) => self.handle_engine_error(error),
            TranscriptionEvent::ChunkDone { job, result } => {
                self.handle_chunk_done(job.session_id, job.chunk_id, result);
            }
        }
    }

    fn handle_engine_status(&mut self, message: String) {
        self.startup_status = message.clone();
        self.tray.set_tooltip(&format!("local-stt — {message}"));
        if self.config.show_loading_notifications
            && !self.settings.open
            && !self.voice_commands.open
            && self.hotkey_problem.is_none()
        {
            self.overlay.show_loading(message);
        } else if matches!(&self.overlay.state, OverlayState::Loading { .. }) {
            self.overlay.dismiss();
        }
    }

    fn handle_engine_ready(&mut self, engine: std::sync::Arc<transcriber_core::asr::AsrEngine>) {
        self.startup_status = "Parakeet ready".into();
        self.tray.set_status(TrayStatus::Idle);
        self.tray
            .set_tooltip(&format!("local-stt — {}", engine.label()));
        self.engine = Some(engine);
        if self.settings.open || self.voice_commands.open {
            return;
        }

        if let Some(problem) = &self.hotkey_problem {
            self.overlay.show_persistent_notice(
                format!("{problem} Open the tray menu → Settings and choose another shortcut."),
                false,
            );
            self.tray
                .set_tooltip("local-stt — recording shortcut required; open Settings");
        } else if self.config.show_loading_notifications {
            self.overlay.show_notice(
                format!(
                    "Parakeet ready — press {}",
                    friendly_name(&self.config.hotkey)
                ),
                true,
                self.now(),
                self.config.notification_seconds(),
            );
        } else if matches!(&self.overlay.state, OverlayState::Loading { .. }) {
            self.overlay.dismiss();
        }
    }

    fn handle_engine_error(&mut self, error: String) {
        self.startup_status = format!("Model load failed: {error}");
        self.tray.set_status(TrayStatus::Idle);
        self.tray.set_tooltip("local-stt — model load failed");
        if self.config.show_loading_notifications
            && !self.settings.open
            && !self.voice_commands.open
        {
            self.overlay
                .show_notice(
                    self.startup_status.clone(),
                    false,
                    self.now(),
                    self.config.notification_seconds(),
                );
        } else if matches!(&self.overlay.state, OverlayState::Loading { .. }) {
            self.overlay.dismiss();
        }
    }

    fn handle_chunk_done(
        &mut self,
        session_id: u64,
        chunk_id: usize,
        result: Result<String, String>,
    ) {
        let Some(session) = self
            .session
            .as_mut()
            .filter(|session| session.id == session_id)
        else {
            log::debug!("discarding stale transcription session {session_id} chunk #{chunk_id}");
            return;
        };
        session.complete_chunk(chunk_id, result);
        self.try_finalize();
    }
}
