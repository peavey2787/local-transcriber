//! Windows storage adapter for the shared configuration schema.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use transcriber_core::config::{self as core_config, ConfigStorage};

use crate::platform;

pub(crate) use transcriber_core::config::Config;

struct WindowsConfigStorage;

impl ConfigStorage for WindowsConfigStorage {
    fn config_dir(&self) -> PathBuf {
        platform::app_data_dir()
    }

    fn prepare_directory(&self, path: &Path) -> Result<()> {
        fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))
    }

    fn replace_file(&self, temporary: &Path, destination: &Path) -> Result<()> {
        platform::replace_file(temporary, destination)
    }
}

pub(crate) fn models_dir() -> PathBuf {
    core_config::models_dir(&WindowsConfigStorage)
}

pub(crate) fn load() -> Config {
    core_config::load(&WindowsConfigStorage)
}

pub(crate) fn save(config: &Config) -> Result<()> {
    core_config::save(&WindowsConfigStorage, config)
}
