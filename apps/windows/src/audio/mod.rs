//! Windows microphone discovery adapter plus shared recorder lifecycle.

mod devices;

use anyhow::Result;
use std::sync::Arc;
use transcriber_core::facade::TranscriberCore;

pub(crate) use devices::input_device_options;
pub(crate) use transcriber_core::audio::{
    InputDeviceOption, InputDeviceSelection, Recorder,
};

pub(crate) fn create_recorder(
    preferred: Option<&InputDeviceSelection>,
) -> Result<Recorder> {
    TranscriberCore::create_recorder_for(Arc::new(devices::WindowsInputDeviceSource), preferred)
}
