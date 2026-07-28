//! Idle-closed microphone recorder backed by CPAL.

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::audio::capture::{on_input, CallbackGuard};
use crate::audio::devices::select_input_device;
use crate::audio::InputDeviceSelection;
use crate::util::SAMPLE_RATE;

pub(crate) struct Recorder {
    recording: Arc<AtomicBool>,
    buffer: Arc<Mutex<Vec<f32>>>,
    rms_milli: Arc<AtomicU32>,
    active_callbacks: Arc<AtomicUsize>,
    device: Device,
    sample_format: SampleFormat,
    channels: u16,
    device_rate: u32,
    stream_config: StreamConfig,
    stream: Mutex<Option<Stream>>,
}

impl Recorder {
    pub(crate) fn new(preferred_device: Option<&InputDeviceSelection>) -> Result<Self> {
        let selected = select_input_device(preferred_device)?;
        let conf = selected
            .device
            .default_input_config()
            .context("read recording-device input configuration")?;

        let sample_format = conf.sample_format();
        let channels = conf.channels();
        let device_rate = conf.sample_rate().0;
        let stream_config: StreamConfig = conf.into();

        println!(
            "[local-stt] microphone ready but closed while idle ({}; {device_rate} Hz -> {SAMPLE_RATE} Hz, {channels} ch)",
            selected.label
        );

        Ok(Self {
            recording: Arc::new(AtomicBool::new(false)),
            buffer: Arc::new(Mutex::new(Vec::new())),
            rms_milli: Arc::new(AtomicU32::new(0)),
            active_callbacks: Arc::new(AtomicUsize::new(0)),
            device: selected.device,
            sample_format,
            channels,
            device_rate,
            stream_config,
            stream: Mutex::new(None),
        })
    }

    pub(crate) fn start(&self) -> Result<()> {
        self.recording.store(false, Ordering::SeqCst);
        drop(self.stream.lock().take());
        self.buffer.lock().clear();
        self.rms_milli.store(0, Ordering::Relaxed);

        let stream = self.build_stream().context("open microphone capture")?;
        self.recording.store(true, Ordering::SeqCst);
        if let Err(error) = stream.play() {
            self.recording.store(false, Ordering::SeqCst);
            self.rms_milli.store(0, Ordering::Relaxed);
            self.buffer.lock().clear();
            drop(stream);
            return Err(error).context("start microphone capture");
        }
        *self.stream.lock() = Some(stream);
        Ok(())
    }

    pub(crate) fn stop(&self) -> Vec<f32> {
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

    pub(crate) fn buffered_samples(&self) -> usize {
        self.buffer.lock().len()
    }

    pub(crate) fn take_prefix(&self, n: usize) -> Option<Vec<f32>> {
        let mut buffer = self.buffer.lock();
        if buffer.len() < n {
            return None;
        }
        Some(buffer.drain(..n).collect())
    }

    pub(crate) fn rms(&self) -> f32 {
        self.rms_milli.load(Ordering::Relaxed) as f32 / 1000.0
    }

    fn build_stream(&self) -> Result<Stream> {
        let channels = self.channels;
        let device_rate = self.device_rate;

        let stream = match self.sample_format {
            SampleFormat::F32 => {
                let recording = self.recording.clone();
                let buffer = self.buffer.clone();
                let rms = self.rms_milli.clone();
                let active_callbacks = self.active_callbacks.clone();
                self.device.build_input_stream(
                    &self.stream_config,
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
                self.device.build_input_stream(
                    &self.stream_config,
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
                self.device.build_input_stream(
                    &self.stream_config,
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

fn log_stream_error(error: cpal::StreamError) {
    log::error!("audio stream error: {error}");
}
