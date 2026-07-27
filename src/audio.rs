//! Microphone capture via cpal (resampled to 16 kHz mono).

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use crate::util::{resample_linear, SAMPLE_RATE};

pub struct Recorder {
    recording: Arc<AtomicBool>,
    buffer: Arc<Mutex<Vec<f32>>>,
    /// RMS * 1000 as integer for cheap cross-thread reads
    rms_milli: Arc<AtomicU32>,
    _stream: Stream,
}

impl Recorder {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no default input device")?;
        let conf = device
            .default_input_config()
            .context("default input config")?;

        let sample_format = conf.sample_format();
        let channels = conf.channels();
        let device_rate = conf.sample_rate().0;
        let stream_config: StreamConfig = conf.into();

        let recording = Arc::new(AtomicBool::new(false));
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let rms_milli = Arc::new(AtomicU32::new(0));

        let recording_c = recording.clone();
        let buffer_c = buffer.clone();
        let rms_c = rms_milli.clone();

        let err_fn = |e| log::error!("audio stream error: {e}");

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    on_input(data, channels, device_rate, &recording_c, &buffer_c, &rms_c);
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let f: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                    on_input(&f, channels, device_rate, &recording_c, &buffer_c, &rms_c);
                },
                err_fn,
                None,
            )?,
            SampleFormat::U16 => device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    let f: Vec<f32> = data
                        .iter()
                        .map(|s| (*s as f32 / 32768.0) - 1.0)
                        .collect();
                    on_input(&f, channels, device_rate, &recording_c, &buffer_c, &rms_c);
                },
                err_fn,
                None,
            )?,
            other => bail!("unsupported sample format: {other:?}"),
        };

        stream.play()?;
        println!(
            "[local-stt] mic ready (device {device_rate} Hz -> {SAMPLE_RATE} Hz, {channels} ch)"
        );

        Ok(Self {
            recording,
            buffer,
            rms_milli,
            _stream: stream,
        })
    }

    pub fn start(&self) {
        self.buffer.lock().clear();
        self.rms_milli.store(0, Ordering::Relaxed);
        self.recording.store(true, Ordering::SeqCst);
    }

    pub fn stop(&self) -> Vec<f32> {
        self.recording.store(false, Ordering::SeqCst);
        // small settle
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::mem::take(&mut *self.buffer.lock())
    }

    pub fn buffered_samples(&self) -> usize {
        self.buffer.lock().len()
    }

    /// Peel the first `n` samples if available (used for live chunked ASR).
    pub fn take_prefix(&self, n: usize) -> Option<Vec<f32>> {
        let mut buf = self.buffer.lock();
        if buf.len() < n {
            return None;
        }
        Some(buf.drain(..n).collect())
    }

    pub fn rms(&self) -> f32 {
        self.rms_milli.load(Ordering::Relaxed) as f32 / 1000.0
    }
}

fn on_input(
    data: &[f32],
    channels: u16,
    device_rate: u32,
    recording: &AtomicBool,
    buffer: &Mutex<Vec<f32>>,
    rms_milli: &AtomicU32,
) {
    // Always update RMS for the overlay wave
    let mono_preview: Vec<f32> = if channels <= 1 {
        data.to_vec()
    } else {
        data.chunks(channels as usize)
            .map(|c| c.iter().sum::<f32>() / channels as f32)
            .collect()
    };
    if !mono_preview.is_empty() {
        let mean_sq = mono_preview.iter().map(|s| s * s).sum::<f32>() / mono_preview.len() as f32;
        let rms = mean_sq.sqrt();
        rms_milli.store((rms * 1000.0) as u32, Ordering::Relaxed);
    }

    if !recording.load(Ordering::SeqCst) {
        return;
    }

    let mono = mono_preview;
    let resampled = resample_linear(&mono, device_rate, SAMPLE_RATE);
    buffer.lock().extend_from_slice(&resampled);
}
