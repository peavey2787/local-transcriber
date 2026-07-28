//! Microphone capture via cpal (resampled to 16 kHz mono).

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::util::{resample_linear, SAMPLE_RATE};

pub struct Recorder {
    recording: Arc<AtomicBool>,
    buffer: Arc<Mutex<Vec<f32>>>,
    /// RMS * 1000 as an integer for cheap cross-thread reads.
    rms_milli: Arc<AtomicU32>,
    active_callbacks: Arc<AtomicUsize>,
    device: Device,
    sample_format: SampleFormat,
    channels: u16,
    device_rate: u32,
    stream_config: StreamConfig,
    /// `None` whenever recording is idle. Dropping the stream releases capture.
    stream: Mutex<Option<Stream>>,
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

        println!(
            "[local-stt] microphone ready but closed while idle (device {device_rate} Hz -> {SAMPLE_RATE} Hz, {channels} ch)"
        );

        Ok(Self {
            recording: Arc::new(AtomicBool::new(false)),
            buffer: Arc::new(Mutex::new(Vec::new())),
            rms_milli: Arc::new(AtomicU32::new(0)),
            active_callbacks: Arc::new(AtomicUsize::new(0)),
            device,
            sample_format,
            channels,
            device_rate,
            stream_config,
            stream: Mutex::new(None),
        })
    }

    pub fn start(&self) -> Result<()> {
        // Defensive cleanup in case a prior attempt ended unexpectedly.
        self.recording.store(false, Ordering::SeqCst);
        self.stream.lock().take();
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

    pub fn stop(&self) -> Vec<f32> {
        self.recording.store(false, Ordering::SeqCst);

        // Taking and dropping the CPAL stream closes active capture instead of
        // leaving a long-lived microphone stream behind while the tray app idles.
        let stream = self.stream.lock().take();
        drop(stream);

        // Wait for any callback that was already running when the stream was
        // dropped. This replaces the old fixed sleep with an observed handoff.
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

    /// Peel the first `n` samples if available (used for live chunked ASR).
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
                    move |error| log::error!("audio stream error: {error}"),
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
                        let samples: Vec<f32> = data
                            .iter()
                            .map(|sample| *sample as f32 / 32768.0)
                            .collect();
                        on_input(
                            &samples,
                            channels,
                            device_rate,
                            &recording,
                            &buffer,
                            &rms,
                        );
                    },
                    move |error| log::error!("audio stream error: {error}"),
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
                        let samples: Vec<f32> = data
                            .iter()
                            .map(|sample| (*sample as f32 / 32768.0) - 1.0)
                            .collect();
                        on_input(
                            &samples,
                            channels,
                            device_rate,
                            &recording,
                            &buffer,
                            &rms,
                        );
                    },
                    move |error| log::error!("audio stream error: {error}"),
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
        self.stream.get_mut().take();
    }
}

struct CallbackGuard<'a> {
    counter: &'a AtomicUsize,
}

impl<'a> CallbackGuard<'a> {
    fn new(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self { counter }
    }
}

impl Drop for CallbackGuard<'_> {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
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
    // Ignore callback races after stop. Idle periods have no stream at all, but
    // this also prevents conversion or buffering if one final callback is active.
    if !recording.load(Ordering::SeqCst) {
        return;
    }

    let mono: Vec<f32> = if channels <= 1 {
        data.to_vec()
    } else {
        data.chunks(channels as usize)
            .map(|channel_frame| channel_frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    if !mono.is_empty() {
        let mean_sq = mono.iter().map(|sample| sample * sample).sum::<f32>() / mono.len() as f32;
        rms_milli.store((mean_sq.sqrt() * 1000.0) as u32, Ordering::Relaxed);
    }

    let resampled = resample_linear(&mono, device_rate, SAMPLE_RATE);
    buffer.lock().extend_from_slice(&resampled);
}
