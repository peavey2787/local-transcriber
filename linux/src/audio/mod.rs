//! Microphone device discovery and recording.

mod backend_diagnostics;
mod capture;
mod device_labels;
mod device_metadata;
mod devices;
mod recorder;

pub(crate) use devices::{input_device_options, InputDeviceOption, InputDeviceSelection};
pub(crate) use recorder::Recorder;
