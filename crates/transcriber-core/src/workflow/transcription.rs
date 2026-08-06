//! Bounded recognizer worker and transcription event routing.

use anyhow::Context;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TrySendError};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use crate::asr::AsrEngine;
use crate::facade::TranscriberCore;

const MAX_QUEUED_CHUNKS: usize = 8;

pub type UiWake = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptionJobId {
    pub session_id: u64,
    pub chunk_id: usize,
}

pub enum TranscriptionEvent {
    EngineStatus(String),
    EngineReady(std::result::Result<Arc<AsrEngine>, String>),
    ChunkDone {
        job: TranscriptionJobId,
        result: std::result::Result<String, String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueError {
    Full,
    WorkerStopped,
}

pub struct TranscriptionWorker {
    jobs: Sender<ChunkJob>,
    events: Receiver<TranscriptionEvent>,
    _thread: thread::JoinHandle<()>,
}

struct ChunkJob {
    id: TranscriptionJobId,
    audio: Vec<f32>,
}

impl TranscriptionWorker {
    pub fn spawn(models_dir: PathBuf, ui_wake: UiWake) -> anyhow::Result<Self> {
        let (event_tx, events) = unbounded();
        let (jobs, job_rx) = bounded::<ChunkJob>(MAX_QUEUED_CHUNKS);

        let worker = thread::Builder::new()
            .name("local-stt-recognizer".into())
            .spawn(move || run_worker(models_dir, job_rx, event_tx, ui_wake))
            .context("start recognizer worker")?;

        Ok(Self {
            jobs,
            events,
            _thread: worker,
        })
    }

    pub fn queue(
        &self,
        session_id: u64,
        chunk_id: usize,
        audio: Vec<f32>,
    ) -> Result<(), QueueError> {
        self.jobs
            .try_send(ChunkJob {
                id: TranscriptionJobId {
                    session_id,
                    chunk_id,
                },
                audio,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => QueueError::Full,
                TrySendError::Disconnected(_) => QueueError::WorkerStopped,
            })
    }

    pub fn try_recv(&self) -> Option<TranscriptionEvent> {
        self.events.try_recv().ok()
    }
}

fn run_worker(
    models_dir: PathBuf,
    jobs: Receiver<ChunkJob>,
    events: Sender<TranscriptionEvent>,
    ui_wake: UiWake,
) {
    let status_events = events.clone();
    let status_wake = ui_wake.clone();
    let engine = TranscriberCore::load_recognizer(&models_dir, move |message| {
        let _ = status_events.send(TranscriptionEvent::EngineStatus(message));
        status_wake();
    });

    let engine = match engine {
        Ok(engine) => {
            let _ = events.send(TranscriptionEvent::EngineReady(Ok(engine.clone())));
            ui_wake();
            engine
        }
        Err(error) => {
            let _ = events.send(TranscriptionEvent::EngineReady(Err(format!("{error:#}"))));
            ui_wake();
            return;
        }
    };

    while let Ok(job) = jobs.recv() {
        let label = format!("session{}-chunk{}", job.id.session_id, job.id.chunk_id);
        let result = engine
            .transcribe_labeled(&job.audio, Some(&label))
            .map_err(|error| format!("{error:#}"));
        let _ = events.send(TranscriptionEvent::ChunkDone {
            job: job.id,
            result,
        });
        ui_wake();
    }
}
