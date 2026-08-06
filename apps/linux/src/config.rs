//! Linux storage adapter for the shared configuration schema.

use anyhow::{Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use transcriber_core::config::{self as core_config, ConfigStorage};

pub(crate) use transcriber_core::config::Config;

struct LinuxConfigStorage;

impl ConfigStorage for LinuxConfigStorage {
    fn config_dir(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local-stt")
    }

    fn prepare_directory(&self, path: &Path) -> Result<()> {
        prepare_private_dir(path)
    }

    fn protect_temporary_file(&self, path: &Path) -> Result<()> {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("protect {}", path.display()))
    }

    fn replace_file(&self, temporary: &Path, destination: &Path) -> Result<()> {
        fs::rename(temporary, destination)
            .with_context(|| format!("activate {}", destination.display()))
    }
}

pub(crate) fn config_dir() -> PathBuf {
    LinuxConfigStorage.config_dir()
}

pub(crate) fn models_dir() -> PathBuf {
    core_config::models_dir(&LinuxConfigStorage)
}

pub(crate) fn load() -> Config {
    core_config::load(&LinuxConfigStorage)
}

pub(crate) fn save(config: &Config) -> Result<()> {
    core_config::save(&LinuxConfigStorage, config)
}

pub(crate) fn prepare_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("protect {}", path.display()))?;
    Ok(())
}
