//! Idle-closed microphone recorder backed by CPAL.

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use super::capture::{on_input, CallbackGuard};
use super::{InputDeviceSelection, SAMPLE_RATE};

/// Device selected by a platform adapter.
pub struct SelectedInputDevice {
    pub device: Device,
    pub label: String,
}

/// Focused platform boundary for native microphone discovery.
pub trait InputDeviceSource: Send + Sync {
    fn select_input_device(
        &self,
        preferred: Option<&InputDeviceSelection>,
    ) -> Result<SelectedInputDevice>;
}

pub struct Recorder {
    source: Arc<dyn InputDeviceSource>,
    preferred_device: Option<InputDeviceSelection>,
    recording: Arc<AtomicBool>,
    buffer: Arc<Mutex<Vec<f32>>>,
    rms_milli: Arc<AtomicU32>,
    active_callbacks: Arc<AtomicUsize>,
    stream: Mutex<Option<Stream>>,
}

struct PreparedInput {
    device: Device,
    label: String,
    sample_format: SampleFormat,
    channels: u16,
    device_rate: u32,
    stream_config: StreamConfig,
}

impl Recorder {
    pub fn new(
        source: Arc<dyn InputDeviceSource>,
        preferred_device: Option<&InputDeviceSelection>,
    ) -> Result<Self> {
        let prepared = prepare_input(source.as_ref(), preferred_device)?;
        println!(
            "[local-stt] microphone configured and closed while idle ({}; {} Hz -> {SAMPLE_RATE} Hz, {} ch)",
            prepared.label, prepared.device_rate, prepared.channels
        );

        Ok(Self {
            source,
            preferred_device: preferred_device.cloned(),
            recording: Arc::new(AtomicBool::new(false)),
            buffer: Arc::new(Mutex::new(Vec::new())),
            rms_milli: Arc::new(AtomicU32::new(0)),
            active_callbacks: Arc::new(AtomicUsize::new(0)),
            stream: Mutex::new(None),
        })
    }

    pub fn start(&self) -> Result<()> {
        self.recording.store(false, Ordering::SeqCst);
        drop(self.stream.lock().take());
        self.buffer.lock().clear();
        self.rms_milli.store(0, Ordering::Relaxed);

        let prepared = prepare_input(self.source.as_ref(), self.preferred_device.as_ref())?;
        let stream = self.build_stream(&prepared).context("open microphone capture")?;
        self.recording.store(true, Ordering::SeqCst);
        if let Err(error) = stream.play() {
            self.recording.store(false, Ordering::SeqCst);
            self.rms_milli.store(0, Ordering::Relaxed);
            self.buffer.lock().clear();
            drop(stream);
            return Err(error).context("start microphone capture");
        }
        println!("[local-stt] microphone opened ({})", prepared.label);
        *self.stream.lock() = Some(stream);
        Ok(())
    }

    pub fn stop(&self) -> Vec<f32> {
        self.recording.store(false, Ordering::SeqCst);
        drop(self.stream.lock().take());

        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
        while self.active_callbacks.load(Ordering::Acquire) != 0
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let remaining = self.active_callbacks.load(Ordering::Acquire);
        if remaining != 0 {
            log::warn!("{remaining} microphone callback(s) still active after capture closed");
        }
        self.rms_milli.store(0, Ordering::Relaxed);
        std::mem::take(&mut *self.buffer.lock())
    }

    pub fn buffered_samples(&self) -> usize {
        self.buffer.lock().len()
    }

    pub fn take_prefix(&self, n: usize) -> Option<Vec<f32>> {
        let mut buffer = self.buffer.lock();
        if buffer.len() < n {
            return None;
        }
        Some(buffer.drain(..n).collect())
    }

    pub fn rms(&self) -> f32 {
        self.rms_milli.load(Ordering::Relaxed) as f32 / 1000.0
    }

    fn build_stream(&self, prepared: &PreparedInput) -> Result<Stream> {
        let channels = prepared.channels;
        let device_rate = prepared.device_rate;

        let stream = match prepared.sample_format {
            SampleFormat::F32 => {
                let recording = self.recording.clone();
                let buffer = self.buffer.clone();
                let rms = self.rms_milli.clone();
                let active_callbacks = self.active_callbacks.clone();
                prepared.device.build_input_stream(
                    &prepared.stream_config,
                    move |data: &[f32], _| {
                        let _callback = CallbackGuard::new(active_callbacks.as_ref());
                        on_input(data, channels, device_rate, &recording, &buffer, &rms);
                    },
                    log_stream_error,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let recording = self.recording.clone();
                let buffer = self.buffer.clone();
                let rms = self.rms_milli.clone();
                let active_callbacks = self.active_callbacks.clone();
                prepared.device.build_input_stream(
                    &prepared.stream_config,
                    move |data: &[i16], _| {
                        let _callback = CallbackGuard::new(active_callbacks.as_ref());
                        if !recording.load(Ordering::SeqCst) {
                            return;
                        }
                        let samples = data
                            .iter()
                            .map(|sample| *sample as f32 / 32768.0)
                            .collect::<Vec<_>>();
                        on_input(&samples, channels, device_rate, &recording, &buffer, &rms);
                    },
                    log_stream_error,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let recording = self.recording.clone();
                let buffer = self.buffer.clone();
                let rms = self.rms_milli.clone();
                let active_callbacks = self.active_callbacks.clone();
                prepared.device.build_input_stream(
                    &prepared.stream_config,
                    move |data: &[u16], _| {
                        let _callback = CallbackGuard::new(active_callbacks.as_ref());
                        if !recording.load(Ordering::SeqCst) {
                            return;
                        }
                        let samples = data
                            .iter()
                            .map(|sample| (*sample as f32 / 32768.0) - 1.0)
                            .collect::<Vec<_>>();
                        on_input(&samples, channels, device_rate, &recording, &buffer, &rms);
                    },
                    log_stream_error,
                    None,
                )?
            }
            other => bail!("unsupported sample format: {other:?}"),
        };
        Ok(stream)
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.recording.store(false, Ordering::SeqCst);
        self.rms_milli.store(0, Ordering::Relaxed);
        drop(self.stream.get_mut().take());
    }
}

fn prepare_input(
    source: &dyn InputDeviceSource,
    preferred_device: Option<&InputDeviceSelection>,
) -> Result<PreparedInput> {
    let selected = source.select_input_device(preferred_device)?;
    let configuration = selected
        .device
        .default_input_config()
        .context("read recording-device input configuration")?;
    Ok(PreparedInput {
        sample_format: configuration.sample_format(),
        channels: configuration.channels(),
        device_rate: configuration.sample_rate().0,
        stream_config: configuration.into(),
        device: selected.device,
        label: selected.label,
    })
}

fn log_stream_error(error: cpal::StreamError) {
    log::error!("audio stream error: {error}");
}
