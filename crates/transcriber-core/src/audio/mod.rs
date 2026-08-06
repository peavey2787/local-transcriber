//! Audio signal processing, buffering, and recorder lifecycle.

mod capture;
mod recorder;
mod signal;
mod types;

pub use recorder::{InputDeviceSource, Recorder, SelectedInputDevice};
pub use signal::{cpu_threads, resample_linear, trim_silence, SAMPLE_RATE};
pub use types::{build_input_device_options, InputDeviceOption, InputDeviceSelection};
