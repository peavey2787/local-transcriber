//! Download, authenticate, and safely extract the Parakeet TDT model pack.

use anyhow::{bail, Context, Result};
use bzip2::read::BzDecoder;
use reqwest::redirect::Policy;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use tar::Archive;

use crate::config::models_dir;
use crate::sha256;

pub const MODEL_DIR_NAME: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
const MODEL_ARCHIVE_NAME: &str =
    "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2";
const MODEL_RECEIPT_NAME: &str = ".verified-source-sha256";
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2";
/// Published SHA-256 for the exact GitHub release asset above.
pub const MODEL_ARCHIVE_SHA256: &str =
    "5793d0fd397c5778d2cf2126994d58e9d56b1be7c04d13c7a15bb1b4eafb16bf";

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

/// Ensure the Parakeet INT8 model is on disk; download and extract if missing.
/// Every archive is authenticated before any archive entry is unpacked.
pub fn ensure_parakeet_int8<F>(status: &F) -> Result<ModelPaths>
where
    F: Fn(String),
{
    status("Checking Parakeet model files…".into());
    let root = model_root();
    let paths = ModelPaths::from_root(root.clone());
    if paths.is_complete() && model_receipt_matches(&root) {
        log::info!("authenticated model ready at {}", root.display());
        status("Verified model files found — loading Parakeet…".into());
        return Ok(paths);
    }
    if root.exists() {
        log::warn!(
            "removing model cache without a matching authentication receipt: {}",
            root.display()
        );
        status("Existing model cache was not authenticated — replacing it…".into());
        fs::remove_dir_all(&root)
            .with_context(|| format!("remove unauthenticated model cache {}", root.display()))?;
    }

    let parent = models_dir();
    fs::create_dir_all(&parent).context("create models directory")?;
    let archive_path = parent.join(MODEL_ARCHIVE_NAME);

    if archive_path.is_file() {
        status("Verifying cached model download…".into());
        if let Err(error) = sha256::verify_file(&archive_path, MODEL_ARCHIVE_SHA256) {
            log::warn!("discarding unauthenticated cached model archive: {error:#}");
            fs::remove_file(&archive_path).with_context(|| {
                format!("remove invalid model archive {}", archive_path.display())
            })?;
        }
    }

    if !archive_path.is_file() {
        log::info!("downloading authenticated Parakeet TDT model archive");
        status("Downloading Parakeet model — 0%".into());
        download_file(MODEL_URL, &archive_path, status)?;
        status("Verifying downloaded model archive…".into());
        if let Err(error) = sha256::verify_file(&archive_path, MODEL_ARCHIVE_SHA256) {
            let _ = fs::remove_file(&archive_path);
            return Err(error).context("reject downloaded Parakeet model archive");
        }
        status("Model download verified".into());
    }

    // Always authenticate a cached archive immediately before extraction too.
    sha256::verify_file(&archive_path, MODEL_ARCHIVE_SHA256)
        .context("authenticate Parakeet model archive before extraction")?;

    log::info!("extracting authenticated archive {}", archive_path.display());
    status("Extracting verified Parakeet model…".into());
    extract_tar_bz2_safely(&archive_path, &parent, &root)?;

    write_model_receipt(&root)?;
    let paths = ModelPaths::from_root(root);
    if !paths.is_complete() {
        bail!(
            "model files missing after verified extraction under {}",
            paths.root.display()
        );
    }

    // The authenticated archive is no longer needed after successful extraction.
    fs::remove_file(&archive_path)
        .with_context(|| format!("remove model archive {}", archive_path.display()))?;
    status("Model extracted — loading Parakeet…".into());
    Ok(paths)
}

fn model_receipt_matches(root: &Path) -> bool {
    fs::read_to_string(root.join(MODEL_RECEIPT_NAME))
        .map(|value| value.trim().eq_ignore_ascii_case(MODEL_ARCHIVE_SHA256))
        .unwrap_or(false)
}

fn write_model_receipt(root: &Path) -> Result<()> {
    let receipt = root.join(MODEL_RECEIPT_NAME);
    let temporary = root.join(format!("{MODEL_RECEIPT_NAME}.part"));
    {
        let mut file = File::create(&temporary)
            .with_context(|| format!("create model authentication receipt {}", temporary.display()))?;
        writeln!(file, "{MODEL_ARCHIVE_SHA256}")
            .context("write model authentication receipt")?;
        file.sync_all()
            .with_context(|| format!("flush model authentication receipt {}", temporary.display()))?;
    }
    fs::rename(&temporary, &receipt)
        .with_context(|| format!("activate model authentication receipt {}", receipt.display()))?;
    Ok(())
}

fn download_file<F>(url: &str, dest: &Path, status: &F) -> Result<()>
where
    F: Fn(String),
{
    let client = reqwest::blocking::Client::builder()
        .redirect(Policy::limited(5))
        .user_agent("local-stt-rs/model-downloader")
        .build()
        .context("build HTTPS model download client")?;
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?;
    if !response.status().is_success() {
        bail!("model download failed: HTTP {}", response.status());
    }

    let total = response.content_length().unwrap_or(0);
    let temporary = part_path(dest)?;
    if temporary.exists() {
        fs::remove_file(&temporary)
            .with_context(|| format!("remove stale partial download {}", temporary.display()))?;
    }

    let result = (|| -> Result<()> {
        let mut file = File::create(&temporary)
            .with_context(|| format!("create {}", temporary.display()))?;
        let mut buffer = [0u8; 256 * 1024];
        let mut written = 0u64;
        let mut last_percent = 0u64;
        loop {
            let count = response.read(&mut buffer).context("read model download")?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])
                .context("write model download")?;
            written += count as u64;
            if total > 0 {
                let percent = written.saturating_mul(100) / total;
                if percent >= last_percent + 2 || percent == 100 {
                    status(format!("Downloading Parakeet model — {percent}%"));
                    last_percent = percent;
                }
            }
        }
        file.sync_all()
            .with_context(|| format!("flush {}", temporary.display()))?;

        // Authenticate the temporary file before it receives the trusted name.
        sha256::verify_file(&temporary, MODEL_ARCHIVE_SHA256)
            .context("authenticate completed model download")?;
        fs::rename(&temporary, dest)
            .with_context(|| format!("rename verified download to {}", dest.display()))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn part_path(dest: &Path) -> Result<PathBuf> {
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .context("model archive path has no UTF-8 file name")?;
    Ok(dest.with_file_name(format!("{name}.part")))
}

fn extract_tar_bz2_safely(archive_path: &Path, parent: &Path, final_root: &Path) -> Result<()> {
    let temporary_root = parent.join(format!(
        ".{MODEL_DIR_NAME}.extracting-{}",
        std::process::id()
    ));
    if temporary_root.exists() {
        fs::remove_dir_all(&temporary_root).with_context(|| {
            format!("remove stale extraction directory {}", temporary_root.display())
        })?;
    }
    fs::create_dir_all(&temporary_root)
        .with_context(|| format!("create {}", temporary_root.display()))?;

    let extraction = (|| -> Result<()> {
        let file = File::open(archive_path)
            .with_context(|| format!("open {}", archive_path.display()))?;
        let decoder = BzDecoder::new(file);
        let mut archive = Archive::new(decoder);

        for entry in archive.entries().context("read model archive entries")? {
            let mut entry = entry.context("read model archive entry")?;
            let path = entry.path().context("read model archive path")?.into_owned();
            validate_archive_path(&path)?;

            let entry_type = entry.header().entry_type();
            if !(entry_type.is_file() || entry_type.is_dir()) {
                bail!(
                    "refusing non-file model archive entry {} ({entry_type:?})",
                    path.display()
                );
            }

            if !entry
                .unpack_in(&temporary_root)
                .with_context(|| format!("extract model entry {}", path.display()))?
            {
                bail!("model archive entry escaped extraction root: {}", path.display());
            }
        }

        let extracted_root = temporary_root.join(MODEL_DIR_NAME);
        let extracted_paths = ModelPaths::from_root(extracted_root.clone());
        if !extracted_paths.is_complete() {
            bail!(
                "verified archive did not contain the expected model files under {}",
                extracted_root.display()
            );
        }

        if final_root.exists() {
            fs::remove_dir_all(final_root).with_context(|| {
                format!("remove incomplete model directory {}", final_root.display())
            })?;
        }
        fs::rename(&extracted_root, final_root).with_context(|| {
            format!(
                "activate verified model directory {}",
                final_root.display()
            )
        })?;
        Ok(())
    })();

    let cleanup = fs::remove_dir_all(&temporary_root);
    if extraction.is_ok() {
        if let Err(error) = cleanup {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "could not remove extraction staging directory {}: {error}",
                    temporary_root.display()
                );
            }
        }
    } else {
        let _ = cleanup;
    }
    extraction
}

fn validate_archive_path(path: &Path) -> Result<()> {
    if path.is_absolute() {
        bail!("model archive contains an absolute path: {}", path.display());
    }

    let mut first_normal = None;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => {
                first_normal.get_or_insert(name);
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("model archive contains an unsafe path: {}", path.display());
            }
        }
    }

    let first = first_normal.context("model archive contains an empty path")?;
    if first != OsStr::new(MODEL_DIR_NAME) {
        bail!(
            "model archive contains an unexpected top-level path: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_path_policy_accepts_only_expected_root() {
        assert!(validate_archive_path(Path::new(MODEL_DIR_NAME)).is_ok());
        assert!(validate_archive_path(
            Path::new(MODEL_DIR_NAME).join("encoder.int8.onnx").as_path()
        )
        .is_ok());

        assert!(validate_archive_path(Path::new("../escape")).is_err());
        assert!(validate_archive_path(Path::new("/absolute/path")).is_err());
        assert!(validate_archive_path(Path::new("unexpected/file")).is_err());
    }

    #[test]
    fn partial_download_path_is_adjacent_to_destination() {
        let destination = Path::new("/tmp/model.tar.bz2");
        assert_eq!(
            part_path(destination).unwrap(),
            Path::new("/tmp/model.tar.bz2.part")
        );
    }
}
