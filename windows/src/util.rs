//! Shared audio-processing helpers.

pub const SAMPLE_RATE: u32 = 16_000;

/// Drop leading/trailing near-silence so the encoder sees less audio.
pub fn trim_silence(audio: &[f32], sample_rate: u32, top_db: f32, pad_ms: u32) -> Vec<f32> {
    if audio.len() < (sample_rate as usize) / 4 {
        return audio.to_vec();
    }

    let frame = ((0.02 * sample_rate as f32) as usize).max(1);
    let n = audio.len() / frame;
    if n < 3 {
        return audio.to_vec();
    }

    let mut rms = Vec::with_capacity(n);
    let mut peak = 0.0f32;
    for i in 0..n {
        let slice = &audio[i * frame..(i + 1) * frame];
        let sum: f32 = slice.iter().map(|s| s * s).sum();
        let r = (sum / frame as f32).sqrt();
        peak = peak.max(r);
        rms.push(r);
    }
    if peak < 1e-6 {
        return audio.to_vec();
    }

    let mask: Vec<bool> = rms
        .iter()
        .map(|r| 20.0 * (r / peak + 1e-12).log10() > -top_db)
        .collect();
    if !mask.iter().any(|&m| m) {
        return audio.to_vec();
    }

    let first = mask.iter().position(|&m| m).unwrap_or(0);
    let last = mask.iter().rposition(|&m| m).unwrap_or(n - 1) + 1;
    let pad = ((pad_ms as f32) * sample_rate as f32 / 1000.0 / frame as f32) as usize;
    let first = first.saturating_sub(pad);
    let last = (last + pad).min(n);
    audio[first * frame..last * frame].to_vec()
}

/// Simple linear resampler to 16 kHz mono float32.
pub fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == 0 || input.is_empty() {
        return Vec::new();
    }
    if from_rate == to_rate {
        return input.to_vec();
    }
    let out_len = ((input.len() as u64) * to_rate as u64 / from_rate as u64) as usize;
    if out_len == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(out_len);
    let ratio = from_rate as f64 / to_rate as f64;
    for i in 0..out_len {
        let src = i as f64 * ratio;
        let i0 = src.floor() as usize;
        let i1 = (i0 + 1).min(input.len() - 1);
        let frac = (src - i0 as f64) as f32;
        out.push(input[i0] * (1.0 - frac) + input[i1] * frac);
    }
    out
}

pub fn cpu_threads() -> i32 {
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (n.saturating_sub(2)).max(2) as i32
}
