//! Microphone device discovery and recording.

mod capture;
mod devices;
mod recorder;

pub(crate) use devices::{input_device_options, InputDeviceOption, InputDeviceSelection};
pub(crate) use recorder::Recorder;
