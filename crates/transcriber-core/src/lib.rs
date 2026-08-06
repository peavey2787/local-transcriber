//! Shared, platform-independent Local Transcriber behavior.
//!
//! Native applications provide focused adapters for device discovery, storage,
//! clipboard/input injection, notifications, hotkeys, and script execution.

pub mod asr;
pub mod audio;
pub mod commands;
pub mod config;
pub mod facade;
pub mod hotkey;
pub mod model;
pub mod sha256;
pub mod workflow;
