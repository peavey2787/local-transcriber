//! Recording lifecycle, live chunk scheduling, and session finalization.

use std::collections::BTreeMap;

use crate::hotkey::friendly_name;
use crate::overlay::OverlayState;
use crate::tray::TrayStatus;
use crate::paste::PasteTarget;
use crate::util::SAMPLE_RATE;

use super::controller::LocalSttApp;
use super::transcription::{QueueError, TranscriptionEvent};

const LIVE_CHUNK_SECS: u32 = 10;
const LIVE_CHUNK_SAMPLES: usize = (SAMPLE_RATE as usize) * (LIVE_CHUNK_SECS as usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecordingPurpose {
    Transcription,
    VoiceCommand,
}

pub(super) struct LiveSession {
    purpose: RecordingPurpose,
    next_id: usize,
    in_flight: usize,
    done: BTreeMap<usize, Result<String, String>>,
    expected: Option<usize>,
    pub(super) finishing: bool,
}

impl LiveSession {
    fn new(purpose: RecordingPurpose) -> Self {
        Self {
            purpose,
            next_id: 0,
            in_flight: 0,
            done: BTreeMap::new(),
            expected: None,
            finishing: false,
        }
    }

    fn all_done(&self) -> bool {
        self.expected
            .is_some_and(|expected| self.done.len() >= expected && self.in_flight == 0)
    }

    fn joined(&self) -> String {
        self.done
            .values()
            .filter_map(|result| result.as_ref().ok())
            .filter(|text| !text.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn errors(&self) -> Vec<String> {
        self.done
            .iter()
            .filter_map(|(id, result)| {
                result
                    .as_ref()
                    .err()
                    .map(|error| format!("chunk #{id}: {error}"))
            })
            .collect()
    }
}

impl LocalSttApp {
    pub(super) fn recording_pipeline_busy(&self) -> bool {
        self.recording
            || self
                .session
                .as_ref()
                .is_some_and(|session| session.finishing)
    }

    fn spawn_chunk(&mut self, id: usize, audio: Vec<f32>) {
        if let Some(session) = self.session.as_mut() {
            session.in_flight += 1;
        }
        let secs = audio.len() as f32 / SAMPLE_RATE as f32;
        log::info!("queued transcription chunk #{id} ({secs:.1}s)");

        if let Err(error) = self.transcription.queue(id, audio) {
            let message = match error {
                QueueError::Full => {
                    "recognizer queue is full; recording is arriving faster than it can be transcribed"
                        .to_string()
                }
                QueueError::WorkerStopped => {
                    "recognizer worker is no longer running".to_string()
                }
            };
            if let Some(session) = self.session.as_mut() {
                session.in_flight = session.in_flight.saturating_sub(1);
                session.done.insert(id, Err(message));
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
            let id = session.next_id;
            session.next_id += 1;
            self.spawn_chunk(id, chunk);
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
        self.session = Some(LiveSession::new(purpose));
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
        println!("[local-stt] recording {purpose:?} (live {LIVE_CHUNK_SECS}s chunks)");
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
        let (id, expected) = self.finish_session_schedule();

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

        if tail.len() >= (SAMPLE_RATE as usize) * 3 / 10 {
            self.spawn_chunk(id, tail);
        } else if let Some(session) = self.session.as_mut() {
            session.done.insert(id, Ok(String::new()));
        }
        self.try_finalize();
    }

    fn finish_session_schedule(&mut self) -> (usize, usize) {
        let session = self
            .session
            .get_or_insert_with(|| LiveSession::new(RecordingPurpose::Transcription));
        let id = session.next_id;
        session.next_id += 1;
        session.finishing = true;
        let expected = id + 1;
        session.expected = Some(expected);
        (id, expected)
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
        self.paste_target = None;

        match purpose {
            RecordingPurpose::Transcription if errors.is_empty() => {
                self.present_transcription(text);
                self.tray.set_status(TrayStatus::Idle);
                self.tray.set_tooltip("local-stt — Parakeet ready");
            }
            RecordingPurpose::Transcription => {
                self.present_transcription_failure(text, errors);
                self.tray.set_status(TrayStatus::Idle);
                self.tray.set_tooltip("local-stt — Parakeet ready");
            }
            RecordingPurpose::VoiceCommand if errors.is_empty() => {
                self.dispatch_voice_command(text);
            }
            RecordingPurpose::VoiceCommand => {
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
            TranscriptionEvent::ChunkDone { id, result } => {
                self.handle_chunk_done(id, result);
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

    fn handle_engine_ready(&mut self, engine: std::sync::Arc<crate::asr::AsrEngine>) {
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

    fn handle_chunk_done(&mut self, id: usize, result: Result<String, String>) {
        if let Some(session) = self.session.as_mut() {
            session.in_flight = session.in_flight.saturating_sub(1);
            session.done.insert(id, result);
        }
        self.try_finalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_preserves_chunk_order_and_reports_errors() {
        let mut session = LiveSession::new(RecordingPurpose::Transcription);
        session.done.insert(2, Ok("third".into()));
        session.done.insert(0, Ok("first".into()));
        session.done.insert(1, Err("decoder failed".into()));

        assert_eq!(session.joined(), "first third");
        assert_eq!(
            session.errors(),
            vec!["chunk #1: decoder failed".to_string()]
        );
    }

    #[test]
    fn session_is_complete_only_after_expected_work_finishes() {
        let mut session = LiveSession::new(RecordingPurpose::Transcription);
        session.finishing = true;
        session.expected = Some(2);
        session.in_flight = 1;
        session.done.insert(0, Ok(String::new()));
        assert!(!session.all_done());

        session.in_flight = 0;
        session.done.insert(1, Ok(String::new()));
        assert!(session.all_done());
    }
}
