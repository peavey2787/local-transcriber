//! System tray icon + menu.

use anyhow::{Context, Result};
use crossbeam_channel::{unbounded, Receiver};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::hotkey::friendly_name;
use crate::icon::{mic_icon_rgba_with_color, APP_ICON_SIZE, IDLE_ICON_COLOR};
use crate::ui_wake::UiWake;

const RECORDING_COLOR: [u8; 4] = [0xD7, 0x3F, 0x3F, 0xFF];
const BUSY_COLOR: [u8; 4] = [0xE6, 0x91, 0x38, 0xFF];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    Idle,
    Recording,
    Busy,
}

impl TrayStatus {
    fn color(self) -> [u8; 4] {
        match self {
            Self::Idle => IDLE_ICON_COLOR,
            Self::Recording => RECORDING_COLOR,
            Self::Busy => BUSY_COLOR,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Settings,
    VoiceCommands,
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    hotkey_item: MenuItem,
    voice_hotkey_item: MenuItem,
    actions: Receiver<TrayAction>,
}

fn hotkey_menu_text(hotkey: &str) -> String {
    if hotkey.trim().is_empty() {
        "No recording hotkey — open Settings…".into()
    } else {
        format!("{} to record", friendly_name(hotkey))
    }
}

fn voice_hotkey_menu_text(enabled: bool, hotkey: &str) -> String {
    if !enabled {
        "Voice commands disabled".into()
    } else if hotkey.trim().is_empty() {
        "Voice-command hotkey unavailable".into()
    } else {
        format!("{} for voice commands", friendly_name(hotkey))
    }
}

fn status_icon(status: TrayStatus) -> Result<Icon> {
    Icon::from_rgba(
        mic_icon_rgba_with_color(APP_ICON_SIZE, status.color()),
        APP_ICON_SIZE,
        APP_ICON_SIZE,
    )
    .context("tray icon from rgba")
}

impl Tray {
    pub fn new(
        hotkey: &str,
        voice_enabled: bool,
        voice_hotkey: &str,
        ui_wake: UiWake,
    ) -> Result<Self> {
        let settings = MenuItem::new("Settings…", true, None);
        let settings_id = settings.id().clone();
        let voice_commands = MenuItem::new("Voice Commands…", true, None);
        let voice_commands_id = voice_commands.id().clone();
        let quit = MenuItem::new("Quit", true, None);
        let quit_id = quit.id().clone();

        let menu = Menu::new();
        menu.append(&MenuItem::new("local-stt — Parakeet INT8", false, None))?;
        menu.append(&PredefinedMenuItem::separator())?;
        let hotkey_item = MenuItem::new(hotkey_menu_text(hotkey), false, None);
        menu.append(&hotkey_item)?;
        let voice_hotkey_item = MenuItem::new(
            voice_hotkey_menu_text(voice_enabled, voice_hotkey),
            false,
            None,
        );
        menu.append(&voice_hotkey_item)?;
        menu.append(&settings)?;
        menu.append(&voice_commands)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit)?;

        let (action_tx, actions) = unbounded();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = if event.id == settings_id {
                Some(TrayAction::Settings)
            } else if event.id == voice_commands_id {
                Some(TrayAction::VoiceCommands)
            } else if event.id == quit_id {
                Some(TrayAction::Quit)
            } else {
                None
            };
            if let Some(action) = action {
                if action_tx.send(action).is_ok() {
                    log::debug!("tray command queued: {action:?}");
                    ui_wake.request_repaint();
                }
            }
        }));

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("local-stt — loading model…")
            .with_icon(status_icon(TrayStatus::Busy)?)
            .build()
            .context("build tray icon")?;

        Ok(Self {
            icon: tray,
            hotkey_item,
            voice_hotkey_item,
            actions,
        })
    }

    pub fn set_tooltip(&self, tip: &str) {
        let _ = self.icon.set_tooltip(Some(tip));
    }

    pub fn set_status(&self, status: TrayStatus) {
        match status_icon(status) {
            Ok(icon) => {
                if let Err(error) = self.icon.set_icon(Some(icon)) {
                    log::warn!("could not update tray status icon: {error}");
                }
            }
            Err(error) => log::warn!("could not render tray status icon: {error:#}"),
        }
    }

    pub fn set_hotkey_hint(&self, hotkey: &str) {
        self.hotkey_item.set_text(hotkey_menu_text(hotkey));
    }

    pub fn set_voice_commands_hint(&self, enabled: bool, hotkey: &str) {
        self.voice_hotkey_item
            .set_text(voice_hotkey_menu_text(enabled, hotkey));
    }

    pub fn poll_action(&self) -> Option<TrayAction> {
        coalesce_actions(self.actions.try_iter())
    }
}

fn coalesce_actions(actions: impl IntoIterator<Item = TrayAction>) -> Option<TrayAction> {
    let mut requested_panel = None;
    let mut quit_requested = false;
    for action in actions {
        match action {
            TrayAction::Quit => quit_requested = true,
            TrayAction::Settings | TrayAction::VoiceCommands => requested_panel = Some(action),
        }
    }
    if quit_requested {
        Some(TrayAction::Quit)
    } else {
        requested_panel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_has_priority_over_any_queued_settings_commands() {
        assert_eq!(
            coalesce_actions([TrayAction::Settings, TrayAction::Quit, TrayAction::Settings]),
            Some(TrayAction::Quit)
        );
    }

    #[test]
    fn repeated_settings_commands_are_coalesced() {
        assert_eq!(
            coalesce_actions([TrayAction::Settings, TrayAction::Settings]),
            Some(TrayAction::Settings)
        );
        assert_eq!(coalesce_actions([]), None);
    }
}
