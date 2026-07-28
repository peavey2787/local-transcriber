//! Application facade.
//!
//! `LocalSttApp` coordinates focused subsystems for recording/transcription,
//! settings, overlay presentation, tray integration, and viewport placement.

mod controller;
mod recording;
mod result;
mod settings;
mod transcription;
mod viewport;

pub use controller::LocalSttApp;
