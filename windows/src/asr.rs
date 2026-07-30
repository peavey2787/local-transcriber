//! Parakeet TDT v3 INT8 via sherpa-onnx.

use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};
use std::sync::Arc;
use std::time::Instant;

use crate::model::ensure_parakeet_int8;
use crate::util::{cpu_threads, trim_silence, SAMPLE_RATE};

pub struct AsrEngine {
    recognizer: Mutex<OfflineRecognizer>,
}

impl AsrEngine {
    pub fn load_with_status<F>(status: F) -> Result<Arc<Self>>
    where
        F: Fn(String),
    {
        let paths = ensure_parakeet_int8(&status)?;
        let threads = cpu_threads();
        status(format!(
            "Loading Parakeet TDT v3 INT8 on CPU ({threads} threads)…"
        ));
        println!(
            "[local-stt] loading Parakeet TDT v3 INT8 (sherpa-onnx, cpu, threads={threads})..."
        );

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.transducer = OfflineTransducerModelConfig {
            encoder: Some(paths.encoder.to_string_lossy().into_owned()),
            decoder: Some(paths.decoder.to_string_lossy().into_owned()),
            joiner: Some(paths.joiner.to_string_lossy().into_owned()),
        };
        config.model_config.tokens = Some(paths.tokens.to_string_lossy().into_owned());
        config.model_config.model_type = Some("nemo_transducer".into());
        config.model_config.provider = Some("cpu".into());
        config.model_config.num_threads = threads;
        config.model_config.debug = false;
        config.decoding_method = Some("greedy_search".into());

        let recognizer = OfflineRecognizer::create(&config)
            .context("SherpaOnnxCreateOfflineRecognizer failed")?;

        status("Warming up the speech model…".into());
        {
            let stream = recognizer.create_stream();
            let dummy = vec![0.0f32; (SAMPLE_RATE as usize) * 8 / 10];
            stream.accept_waveform(SAMPLE_RATE as i32, &dummy);
            recognizer.decode(&stream);
            let _ = stream.get_result();
        }

        println!("[local-stt] Parakeet INT8 ready");
        Ok(Arc::new(Self {
            recognizer: Mutex::new(recognizer),
        }))
    }

    pub fn label(&self) -> &'static str {
        "Parakeet TDT v3 - INT8 ready"
    }

    pub fn transcribe_labeled(&self, audio: &[f32], label: Option<&str>) -> Result<String> {
        let trimmed = trim_silence(audio, SAMPLE_RATE, 32.0, 120);
        let audio_s = trimmed.len() as f64 / SAMPLE_RATE as f64;
        if trimmed.len() < (SAMPLE_RATE as usize) * 3 / 10 {
            return Ok(String::new());
        }

        let t0 = Instant::now();
        let recognizer = self.recognizer.lock();
        let stream = recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, &trimmed);
        recognizer.decode(&stream);
        let text = match stream.get_result() {
            Some(r) => r.text.trim().to_string(),
            None => bail!("no recognition result"),
        };
        let dt = t0.elapsed().as_secs_f64();
        let rtf = if audio_s > 0.0 { dt / audio_s } else { 0.0 };
        let tag = label.unwrap_or("parakeet-int8");
        println!("[local-stt] {tag}: transcribed in {dt:.2}s (audio {audio_s:.1}s, RTF {rtf:.2}x)");
        Ok(text)
    }
}
