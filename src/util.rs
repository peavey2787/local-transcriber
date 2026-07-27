//! Shared helpers: silence trim, resampling, mic icon.

use image::{ImageBuffer, Rgba, RgbaImage};

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

/// Draw a simple mic tray icon (RGBA).
pub fn make_mic_icon(size: u32, color: [u8; 3]) -> RgbaImage {
    let mut img: RgbaImage = ImageBuffer::from_pixel(size, size, Rgba([0, 0, 0, 0]));
    let c = Rgba([color[0], color[1], color[2], 255]);
    let s = size as f32;

    // Mic body (rounded-ish rect)
    fill_ellipse(
        &mut img,
        (s * 0.50) as i32,
        (s * 0.30) as i32,
        (s * 0.18) as i32,
        (s * 0.26) as i32,
        c,
    );
    // Stem
    fill_rect(
        &mut img,
        (s * 0.46) as i32,
        (s * 0.54) as i32,
        (s * 0.08) as i32,
        (s * 0.22) as i32,
        c,
    );
    // Base
    fill_rect(
        &mut img,
        (s * 0.30) as i32,
        (s * 0.84) as i32,
        (s * 0.40) as i32,
        (s * 0.08) as i32,
        c,
    );
    // Arc stand (approx with thick points)
    let cx = (s * 0.50) as i32;
    let cy = (s * 0.52) as i32;
    let r = (s * 0.28) as i32;
    for deg in 10..170 {
        let rad = (deg as f32).to_radians();
        let x = cx + (r as f32 * rad.cos()) as i32;
        let y = cy + (r as f32 * rad.sin()) as i32;
        put_thick(&mut img, x, y, 2, c);
    }
    img
}

fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: Rgba<u8>) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    for yy in y..(y + h) {
        for xx in x..(x + w) {
            if xx >= 0 && yy >= 0 && xx < iw && yy < ih {
                img.put_pixel(xx as u32, yy as u32, c);
            }
        }
    }
}

fn fill_ellipse(img: &mut RgbaImage, cx: i32, cy: i32, rx: i32, ry: i32, c: Rgba<u8>) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    for yy in (cy - ry)..(cy + ry + 1) {
        for xx in (cx - rx)..(cx + rx + 1) {
            let dx = (xx - cx) as f32 / rx as f32;
            let dy = (yy - cy) as f32 / ry as f32;
            if dx * dx + dy * dy <= 1.0 && xx >= 0 && yy >= 0 && xx < iw && yy < ih {
                img.put_pixel(xx as u32, yy as u32, c);
            }
        }
    }
}

fn put_thick(img: &mut RgbaImage, x: i32, y: i32, r: i32, c: Rgba<u8>) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    for dy in -r..=r {
        for dx in -r..=r {
            let xx = x + dx;
            let yy = y + dy;
            if xx >= 0 && yy >= 0 && xx < iw && yy < ih {
                img.put_pixel(xx as u32, yy as u32, c);
            }
        }
    }
}
