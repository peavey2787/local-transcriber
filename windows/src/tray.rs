//! System tray icon + menu.

use anyhow::{Context, Result};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::hotkey::friendly_name;
use crate::util::make_mic_icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Settings,
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    hotkey_item: MenuItem,
    settings_id: muda::MenuId,
    quit_id: muda::MenuId,
}

fn hotkey_menu_text(hotkey: &str) -> String {
    if hotkey.trim().is_empty() {
        "No recording hotkey — open Settings…".into()
    } else {
        format!("{} to record", friendly_name(hotkey))
    }
}

impl Tray {
    pub fn new(hotkey: &str) -> Result<Self> {
        let rgba = make_mic_icon(64, [0x1B, 0xB9, 0xCE]);
        let (w, h) = (rgba.width(), rgba.height());
        let icon = Icon::from_rgba(rgba.into_raw(), w, h).context("tray icon from rgba")?;

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

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("local-stt — loading model…")
            .with_icon(icon)
            .build()
            .context("build tray icon")?;

        Ok(Self {
            icon: tray,
            hotkey_item,
            settings_id,
            quit_id,
        })
    }

    pub fn set_tooltip(&self, tip: &str) {
        let _ = self.icon.set_tooltip(Some(tip));
    }

    pub fn set_hotkey_hint(&self, hotkey: &str) {
        self.hotkey_item.set_text(hotkey_menu_text(hotkey));
    }

    pub fn poll_action(&self) -> Option<TrayAction> {
        let mut action = None;
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.settings_id {
                action = Some(TrayAction::Settings);
            } else if event.id == self.quit_id {
                action = Some(TrayAction::Quit);
            }
        }
        action
    }
}
