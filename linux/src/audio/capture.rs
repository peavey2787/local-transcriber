//! Audio callback conversion, metering, and buffering.

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use crate::util::{resample_linear, SAMPLE_RATE};

pub(super) struct CallbackGuard<'a> {
    counter: &'a AtomicUsize,
}

impl<'a> CallbackGuard<'a> {
    pub(super) fn new(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self { counter }
    }
}

impl Drop for CallbackGuard<'_> {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) fn on_input(
    data: &[f32],
    channels: u16,
    device_rate: u32,
    recording: &AtomicBool,
    buffer: &Mutex<Vec<f32>>,
    rms_milli: &AtomicU32,
) {
    if !recording.load(Ordering::SeqCst) {
        return;
    }

    let mono: Vec<f32> = if channels <= 1 {
        data.to_vec()
    } else {
        data.chunks(channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    if !mono.is_empty() {
        let mean_sq = mono.iter().map(|sample| sample * sample).sum::<f32>() / mono.len() as f32;
        rms_milli.store((mean_sq.sqrt() * 1000.0) as u32, Ordering::Relaxed);
    }

    let resampled = resample_linear(&mono, device_rate, SAMPLE_RATE);
    buffer.lock().extend_from_slice(&resampled);
}
