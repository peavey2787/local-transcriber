//! Linux UI-wake adapter for the shared bounded transcription worker.

use std::sync::Arc;

use crate::hotkey::UiWake as LinuxUiWake;

pub(super) use transcriber_core::workflow::{
    QueueError, TranscriptionEvent, TranscriptionWorker,
};

pub(super) fn spawn(ui_wake: LinuxUiWake) -> anyhow::Result<TranscriptionWorker> {
    let wake = Arc::new(move || {
        if let Some(context) = ui_wake.lock().as_ref() {
            context.request_repaint();
        }
    });
    TranscriptionWorker::spawn(crate::config::models_dir(), wake)
}
