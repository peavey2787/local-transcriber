//! Settings state, automatic persistence, and settings-window UI.

mod behavior;
mod device_discovery;
mod state;
mod ui;
mod window_state;

pub(super) use device_discovery::SettingsDeviceDiscovery;
pub(super) use state::SettingsState;
pub(super) use window_state::SettingsWindowState;
