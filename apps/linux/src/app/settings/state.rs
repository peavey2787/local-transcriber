//! Linux settings-window state around the shared form model.

use anyhow::Result;
use std::ops::{Deref, DerefMut};

use crate::audio::input_device_options;
use crate::config::Config;
use transcriber_ui::settings_state::SettingsForm;

pub(super) use transcriber_ui::settings_state::SettingsChanges;

pub(in crate::app) struct SettingsState {
    pub(in crate::app) open: bool,
    pub(in crate::app) focus_pending: bool,
    form: SettingsForm,
}

impl SettingsState {
    pub(in crate::app) fn from_config(config: &Config) -> Self {
        Self {
            open: false,
            focus_pending: false,
            form: SettingsForm::from_config(config),
        }
    }

    pub(super) fn load_from_config(&mut self, config: &Config) {
        self.form.reload(config);
    }

    pub(super) fn refresh_input_devices(&mut self) -> Result<usize> {
        Ok(self.form.replace_input_devices(input_device_options()?))
    }
}

impl Deref for SettingsState {
    type Target = SettingsForm;

    fn deref(&self) -> &Self::Target {
        &self.form
    }
}

impl DerefMut for SettingsState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.form
    }
}
