//! Windows UI-wake adapter for the shared bounded transcription worker.

use std::sync::Arc;

use crate::ui_wake::UiWake as WindowsUiWake;

pub(super) use transcriber_core::workflow::{
    QueueError, TranscriptionEvent, TranscriptionWorker,
};

pub(super) fn spawn(ui_wake: WindowsUiWake) -> anyhow::Result<TranscriptionWorker> {
    let wake = Arc::new(move || ui_wake.request_repaint());
    TranscriptionWorker::spawn(crate::config::models_dir(), wake)
}
