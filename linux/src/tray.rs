//! System tray icon + menu.

use anyhow::{Context, Result};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::hotkey::friendly_name;
use crate::util::make_mic_icon;

const IDLE_COLOR: [u8; 3] = [0x1B, 0xB9, 0xCE];
const RECORDING_COLOR: [u8; 3] = [0xD7, 0x3F, 0x3F];
const BUSY_COLOR: [u8; 3] = [0xE6, 0x91, 0x38];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    Idle,
    Recording,
    Busy,
}

impl TrayStatus {
    fn color(self) -> [u8; 3] {
        match self {
            Self::Idle => IDLE_COLOR,
            Self::Recording => RECORDING_COLOR,
            Self::Busy => BUSY_COLOR,
        }
    }
}

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

fn status_icon(status: TrayStatus) -> Result<Icon> {
    let rgba = make_mic_icon(64, status.color());
    let (width, height) = (rgba.width(), rgba.height());
    Icon::from_rgba(rgba.into_raw(), width, height).context("tray icon from rgba")
}

pub(crate) fn install_legacy_backend_warning_filter() -> gtk::glib::LogHandlerId {
    gtk::glib::log_set_handler(
        Some("libayatana-appindicator"),
        gtk::glib::LogLevels::LEVEL_WARNING,
        false,
        false,
        |domain, level, message| {
            if message.contains("libayatana-appindicator is deprecated")
                && message.contains("libayatana-appindicator-glib")
            {
                return;
            }
            gtk::glib::log_default_handler(domain, level, Some(message));
        },
    )
}

impl Tray {
    pub fn new(hotkey: &str) -> Result<Self> {
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
            .with_icon(status_icon(TrayStatus::Busy)?)
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
