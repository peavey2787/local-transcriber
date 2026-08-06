//! Shared transcription workers and recording-session state.

mod delivery;
mod session;
mod transcription;

pub use delivery::{
    failure_presentation, failure_summary, prepare_transcription_text, FailurePresentation,
};
pub use session::{
    LiveSession, RecordingPurpose, LIVE_CHUNK_SAMPLES, LIVE_CHUNK_SECONDS,
    MIN_FINAL_CHUNK_SAMPLES,
};
pub use transcription::{
    QueueError, TranscriptionEvent, TranscriptionJobId, TranscriptionWorker, UiWake,
};
