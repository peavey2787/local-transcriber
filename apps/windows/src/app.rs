//! Application facade.
//!
//! `LocalSttApp` coordinates focused subsystems for recording/transcription,
//! settings, overlay presentation, tray integration, and viewport placement.

mod commands;
mod controller;
mod lifecycle;
mod recording;
mod result;
mod settings;
mod transcription;
mod viewport;

pub(crate) use controller::LocalSttApp;
pub(crate) use viewport::{CONTROL_VIEWPORT_POSITION, CONTROL_VIEWPORT_SIZE};
