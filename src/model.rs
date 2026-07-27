//! Download & extract the sherpa-onnx Parakeet TDT v3 INT8 model pack.

use anyhow::{bail, Context, Result};
use bzip2::read::BzDecoder;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tar::Archive;

use crate::config::models_dir;

pub const MODEL_DIR_NAME: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2";

#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub root: PathBuf,
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
}

impl ModelPaths {
    pub fn from_root(root: PathBuf) -> Self {
        Self {
            encoder: root.join("encoder.int8.onnx"),
            decoder: root.join("decoder.int8.onnx"),
            joiner: root.join("joiner.int8.onnx"),
            tokens: root.join("tokens.txt"),
            root,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.encoder.is_file()
            && self.decoder.is_file()
            && self.joiner.is_file()
            && self.tokens.is_file()
    }
}

pub fn model_root() -> PathBuf {
    models_dir().join(MODEL_DIR_NAME)
}

/// Ensure the Parakeet INT8 model is on disk; download + extract if missing.
pub fn ensure_parakeet_int8<F>(status: &F) -> Result<ModelPaths>
where
    F: Fn(String),
{
    status("Checking Parakeet model files…".into());
    let root = model_root();
    let paths = ModelPaths::from_root(root.clone());
    if paths.is_complete() {
        log::info!("model ready at {}", root.display());
        status("Model files found — loading Parakeet…".into());
        return Ok(paths);
    }

    fs::create_dir_all(models_dir()).context("create models dir")?;
    let archive_path = models_dir().join(format!("{MODEL_DIR_NAME}.tar.bz2"));

    if !archive_path.is_file() {
        log::info!("downloading Parakeet TDT v3 INT8 (~500 MB)...");
        status("Downloading Parakeet model — 0%".into());
        download_file(MODEL_URL, &archive_path, status)?;
        status("Model download complete".into());
    }

    log::info!("extracting {}", archive_path.display());
    status("Extracting Parakeet model…".into());
    extract_tar_bz2(&archive_path, &models_dir())?;

    let paths = ModelPaths::from_root(root);
    if !paths.is_complete() {
        bail!(
            "model files missing after extract under {}",
            paths.root.display()
        );
    }
    status("Model extracted — loading Parakeet…".into());
    Ok(paths)
}

fn download_file<F>(url: &str, dest: &Path, status: &F) -> Result<()>
where
    F: Fn(String),
{
    let resp = reqwest::blocking::get(url).with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        bail!("download failed: HTTP {}", resp.status());
    }
    let total = resp.content_length().unwrap_or(0);
    let mut reader = resp;
    let tmp = dest.with_extension("part");
    let mut file = File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;

    let mut buf = [0u8; 1024 * 256];
    let mut written: u64 = 0;
    let mut last_pct = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        written += n as u64;
        if total > 0 {
            let pct = written * 100 / total;
            if pct >= last_pct + 2 || pct == 100 {
                status(format!("Downloading Parakeet model — {pct}%"));
                last_pct = pct;
            }
        }
    }
    file.flush()?;
    drop(file);
    fs::rename(&tmp, dest).with_context(|| format!("rename to {}", dest.display()))?;
    Ok(())
}

fn extract_tar_bz2(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = File::open(archive_path).with_context(|| format!("open {}", archive_path.display()))?;
    let decoder = BzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(dest_dir)
        .with_context(|| format!("unpack into {}", dest_dir.display()))?;
    Ok(())
}
