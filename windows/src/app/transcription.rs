//! Bounded transcription-worker facade.
//!
//! This module owns the recognizer thread, its queue, status events, and UI
//! wakeups. The GUI controller never manipulates raw worker channels directly.

use anyhow::Context;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TrySendError};
use std::sync::Arc;
use std::thread;

use crate::asr::AsrEngine;
use crate::ui_wake::UiWake;

const MAX_QUEUED_CHUNKS: usize = 8;

pub(super) enum TranscriptionEvent {
    EngineStatus(String),
    EngineReady(std::result::Result<Arc<AsrEngine>, String>),
    ChunkDone {
        session_id: u64,
        chunk_id: usize,
        result: std::result::Result<String, String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QueueError {
    Full,
    WorkerStopped,
}

pub(super) struct TranscriptionWorker {
    jobs: Sender<ChunkJob>,
    events: Receiver<TranscriptionEvent>,
    _thread: thread::JoinHandle<()>,
}

struct ChunkJob {
    session_id: u64,
    chunk_id: usize,
    audio: Vec<f32>,
}

impl TranscriptionWorker {
    pub(super) fn spawn(ui_wake: UiWake) -> anyhow::Result<Self> {
        let (event_tx, events) = unbounded();
        // At most eight 10-second chunks (about 5 MiB of mono f32 audio) can
        // wait behind the dedicated recognizer worker.
        let (jobs, job_rx) = bounded::<ChunkJob>(MAX_QUEUED_CHUNKS);

        let worker = thread::Builder::new()
            .name("local-stt-recognizer".into())
            .spawn(move || run_worker(job_rx, event_tx, ui_wake))
            .context("start recognizer worker")?;

        Ok(Self {
            jobs,
            events,
            _thread: worker,
        })
    }

    pub(super) fn queue(
        &self,
        session_id: u64,
        chunk_id: usize,
        audio: Vec<f32>,
    ) -> Result<(), QueueError> {
        self.jobs
            .try_send(ChunkJob {
                session_id,
                chunk_id,
                audio,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => QueueError::Full,
                TrySendError::Disconnected(_) => QueueError::WorkerStopped,
            })
    }

    pub(super) fn try_recv(&self) -> Option<TranscriptionEvent> {
        self.events.try_recv().ok()
    }
}

fn run_worker(jobs: Receiver<ChunkJob>, events: Sender<TranscriptionEvent>, ui_wake: UiWake) {
    let status_events = events.clone();
    let status_wake = ui_wake.clone();
    let engine = AsrEngine::load_with_status(move |message| {
        let _ = status_events.send(TranscriptionEvent::EngineStatus(message));
        status_wake.request_repaint();
    });

    let engine = match engine {
        Ok(engine) => {
            let _ = events.send(TranscriptionEvent::EngineReady(Ok(engine.clone())));
            ui_wake.request_repaint();
            engine
        }
        Err(error) => {
            let _ = events.send(TranscriptionEvent::EngineReady(Err(format!("{error:#}"))));
            ui_wake.request_repaint();
            return;
        }
    };

    while let Ok(job) = jobs.recv() {
        let label = format!("session{}-chunk{}", job.session_id, job.chunk_id);
        let result = engine
            .transcribe_labeled(&job.audio, Some(&label))
            .map_err(|error| format!("{error:#}"));
        let _ = events.send(TranscriptionEvent::ChunkDone {
            session_id: job.session_id,
            chunk_id: job.chunk_id,
            result,
        });
        ui_wake.request_repaint();
    }
}
