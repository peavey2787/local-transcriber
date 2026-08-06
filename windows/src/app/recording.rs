//! Recording lifecycle, live chunk scheduling, and session finalization.

use std::collections::BTreeMap;

use crate::hotkey::friendly_name;
use crate::overlay::OverlayState;
use crate::tray::TrayStatus;
use crate::platform::PasteTarget;
use crate::util::SAMPLE_RATE;

use super::controller::LocalSttApp;
use super::transcription::{QueueError, TranscriptionEvent};

const LIVE_CHUNK_SECS: u32 = 10;
const LIVE_CHUNK_SAMPLES: usize = (SAMPLE_RATE as usize) * (LIVE_CHUNK_SECS as usize);

pub(super) struct LiveSession {
    id: u64,
    next_chunk_id: usize,
    in_flight: usize,
    done: BTreeMap<usize, Result<String, String>>,
    expected: Option<usize>,
    pub(super) finishing: bool,
}

impl LiveSession {
    fn new(id: u64) -> Self {
        Self {
            id,
            next_chunk_id: 0,
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

    pub(super) fn cancel_recording_for_settings(&mut self) {
        if self.recording {
            self.recording = false;
            let _discarded_audio = self.recorder.stop();
            log::info!("recording cancelled because Settings was opened");
        }

        // Dropping the session invalidates every queued chunk from that session.
        // Worker events include the session id, so late completions cannot leak
        // into a later recording after Settings closes.
        self.session = None;
        self.paste_target = None;
        self.overlay.dismiss_immediately();
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

    fn spawn_chunk(&mut self, chunk_id: usize, audio: Vec<f32>) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let session_id = session.id;
        session.in_flight += 1;

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
                session.in_flight = session.in_flight.saturating_sub(1);
                session.done.insert(chunk_id, Err(message));
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
            let chunk_id = session.next_chunk_id;
            session.next_chunk_id += 1;
            self.spawn_chunk(chunk_id, chunk);
        }
    }

    pub(super) fn toggle_record(&mut self) {
        if self.settings_window.is_visible() {
            return;
        }
        if self.engine.is_none() {
            self.show_engine_loading_state();
            return;
        }
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.finishing)
        {
            return;
        }

        if self.recording {
            self.stop_recording();
        } else {
            self.start_recording();
        }
    }

    fn show_engine_loading_state(&mut self) {
        if self.settings_window.is_visible() {
            self.overlay.dismiss_immediately();
        } else if self.config.show_loading_notifications {
            self.overlay.show_loading(self.startup_status.clone());
        } else {
            self.overlay.dismiss();
        }
    }

    fn start_recording(&mut self) {
        if let Err(error) = self.recorder.start() {
            self.handle_microphone_start_error(error);
            return;
        }

        let session_id = self.next_session_id;
        self.next_session_id = self.next_session_id.wrapping_add(1).max(1);
        self.paste_target = self.config.auto_paste.then(PasteTarget::capture);
        self.recording = true;
        self.session = Some(LiveSession::new(session_id));
        if self.config.show_recording_notifications {
            self.overlay.show_listening();
        } else {
            self.overlay.dismiss();
        }
        self.tray.set_status(TrayStatus::Recording);
        self.tray.set_tooltip("local-stt — recording…");
        println!("[local-stt] recording (live {LIVE_CHUNK_SECS}s chunks)");
    }

    fn handle_microphone_start_error(&mut self, error: anyhow::Error) {
        self.paste_target = None;
        self.session = None;
        self.recording = false;
        let message = format!("Could not start microphone recording: {error:#}");
        eprintln!("[local-stt] {message}");
        if self.settings_window.is_visible() {
            self.overlay.dismiss_immediately();
        } else {
            self.overlay.show_notice(
                message,
                false,
                self.now(),
                self.config.notification_seconds(),
            );
        }
        self.tray.set_tooltip("local-stt — microphone unavailable");
    }

    fn stop_recording(&mut self) {
        self.recording = false;
        let tail = self.recorder.stop();
        let Some((chunk_id, expected)) = self.finish_session_schedule() else {
            self.overlay.dismiss_immediately();
            return;
        };

        if self.config.show_transcribing_notifications {
            self.overlay.show_processing();
        } else {
            self.overlay.dismiss();
        }
        self.tray.set_status(TrayStatus::Busy);
        self.tray.set_tooltip("local-stt — transcribing…");
        println!("[local-stt] stopped — expecting {expected} chunks");

        if tail.len() >= (SAMPLE_RATE as usize) * 3 / 10 {
            self.spawn_chunk(chunk_id, tail);
        } else if let Some(session) = self.session.as_mut() {
            session.done.insert(chunk_id, Ok(String::new()));
        }
        self.try_finalize();
    }

    fn finish_session_schedule(&mut self) -> Option<(usize, usize)> {
        let session = self.session.as_mut()?;
        let chunk_id = session.next_chunk_id;
        session.next_chunk_id += 1;
        session.finishing = true;
        let expected = chunk_id + 1;
        session.expected = Some(expected);
        Some((chunk_id, expected))
    }

    fn try_finalize(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        if !session.finishing || !session.all_done() {
            return;
        }

        let text = session.joined();
        let errors = session.errors();
        if errors.is_empty() {
            self.present_transcription(text);
        } else {
            self.present_transcription_failure(text, errors);
        }
        self.tray.set_status(TrayStatus::Idle);
        self.tray.set_tooltip("local-stt — Parakeet ready");
        self.session = None;
        self.paste_target = None;
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
            TranscriptionEvent::ChunkDone {
                session_id,
                chunk_id,
                result,
            } => self.handle_chunk_done(session_id, chunk_id, result),
        }
    }

    fn handle_engine_status(&mut self, message: String) {
        self.startup_status = message.clone();
        self.tray.set_tooltip(&format!("local-stt — {message}"));
        if self.settings_window.is_visible() {
            self.overlay.dismiss_immediately();
        } else if self.config.show_loading_notifications && self.hotkey_problem.is_none() {
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
        if self.settings_window.is_visible() {
            self.overlay.dismiss_immediately();
        } else if let Some(problem) = &self.hotkey_problem {
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
        if self.settings_window.is_visible() {
            self.overlay.dismiss_immediately();
        } else if self.config.show_loading_notifications {
            self.overlay.show_notice(
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
        session.in_flight = session.in_flight.saturating_sub(1);
        session.done.insert(chunk_id, result);
        self.try_finalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_preserves_chunk_order_and_reports_errors() {
        let mut session = LiveSession::new(7);
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
        let mut session = LiveSession::new(9);
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
