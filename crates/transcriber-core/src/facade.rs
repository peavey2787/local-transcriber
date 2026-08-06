//! Narrow orchestration facade for the shared transcription use cases.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::asr::AsrEngine;
use crate::audio::{InputDeviceSelection, InputDeviceSource, Recorder};
use crate::commands::{matching_command, validate_command_list, ScriptRunner};
use crate::config::{Config, VoiceCommand};

/// Coordinates shared state without hiding pure helpers behind a god object.
pub struct TranscriberCore {
    config: Config,
}

impl TranscriberCore {
    pub fn initialize(config: Config) -> Self {
        Self {
            config: config.normalized(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn update_config(
        &mut self,
        config: Config,
        script_runner: &dyn ScriptRunner,
    ) -> Result<()> {
        if config.voice_commands_enabled {
            validate_command_list(&config.voice_commands, script_runner)?;
        }
        self.config = config.normalized();
        Ok(())
    }

    pub fn create_recorder(
        &self,
        source: Arc<dyn InputDeviceSource>,
    ) -> Result<Recorder> {
        Recorder::new(source, self.config.recording_device.as_ref())
    }

    pub fn create_recorder_for(
        source: Arc<dyn InputDeviceSource>,
        selection: Option<&InputDeviceSelection>,
    ) -> Result<Recorder> {
        Recorder::new(source, selection)
    }

    pub fn load_recognizer<F>(models_dir: &Path, status: F) -> Result<Arc<AsrEngine>>
    where
        F: Fn(String),
    {
        AsrEngine::load_with_status(models_dir, status)
    }

    pub fn match_voice_command(&self, text: &str) -> Option<VoiceCommand> {
        matching_command(&self.config.voice_commands, text)
    }
}
