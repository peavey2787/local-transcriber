//! System tray icon + menu.

use anyhow::{Context, Result};
use crossbeam_channel::{unbounded, Receiver};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::hotkey::friendly_name;
use crate::icon::{mic_icon_rgba, APP_ICON_SIZE};
use crate::ui_wake::UiWake;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Settings,
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    hotkey_item: MenuItem,
    actions: Receiver<TrayAction>,
}

fn hotkey_menu_text(hotkey: &str) -> String {
    if hotkey.trim().is_empty() {
        "No recording hotkey — open Settings…".into()
    } else {
        format!("{} to record", friendly_name(hotkey))
    }
}

impl Tray {
    pub fn new(hotkey: &str, ui_wake: UiWake) -> Result<Self> {
        let icon = Icon::from_rgba(mic_icon_rgba(APP_ICON_SIZE), APP_ICON_SIZE, APP_ICON_SIZE)
            .context("tray icon from rgba")?;

        let settings = MenuItem::new("Settings…", true, None);
        let settings_id = settings.id().clone();
        let quit = MenuItem::new("Quit", true, None);
        let quit_id = quit.id().clone();

        let menu = Menu::new();
        menu.append(&MenuItem::new("local-stt — Parakeet INT8", false, None))?;
        menu.append(&PredefinedMenuItem::separator())?;
        let hotkey_item = MenuItem::new(hotkey_menu_text(hotkey), false, None);
        menu.append(&hotkey_item)?;
        menu.append(&settings)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit)?;

        let (action_tx, actions) = unbounded();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = if event.id == settings_id {
                Some(TrayAction::Settings)
            } else if event.id == quit_id {
                Some(TrayAction::Quit)
            } else {
                None
            };
            if let Some(action) = action {
                if action_tx.send(action).is_ok() {
                    log::debug!("tray command queued: {action:?}");
                    ui_wake.request_root_repaint();
                }
            }
        }));

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("local-stt — loading model…")
            .with_icon(icon)
            .build()
            .context("build tray icon")?;

        Ok(Self {
            icon: tray,
            hotkey_item,
            actions,
        })
    }

    pub fn set_tooltip(&self, tip: &str) {
        let _ = self.icon.set_tooltip(Some(tip));
    }

    pub fn set_hotkey_hint(&self, hotkey: &str) {
        self.hotkey_item.set_text(hotkey_menu_text(hotkey));
    }

    pub fn poll_action(&self) -> Option<TrayAction> {
        coalesce_actions(self.actions.try_iter())
    }
}

fn coalesce_actions(actions: impl IntoIterator<Item = TrayAction>) -> Option<TrayAction> {
    let mut settings_requested = false;
    let mut quit_requested = false;
    for action in actions {
        match action {
            TrayAction::Quit => quit_requested = true,
            TrayAction::Settings => settings_requested = true,
        }
    }
    if quit_requested {
        Some(TrayAction::Quit)
    } else {
        settings_requested.then_some(TrayAction::Settings)
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
