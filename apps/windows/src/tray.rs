//! Windows system-tray integration.

use anyhow::{Context, Result};
use crossbeam_channel::{unbounded, Receiver};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use transcriber_ui::tray::{
    coalesce_actions, hotkey_menu_text, voice_hotkey_menu_text, TrayAction, TrayStatus,
    APP_ICON_SIZE,
};
use transcriber_ui::tray_icon::mic_icon_rgba_with_color;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::ui_wake::UiWake;

pub struct Tray {
    icon: TrayIcon,
    hotkey_item: MenuItem,
    voice_hotkey_item: MenuItem,
    actions: Receiver<TrayAction>,
}

fn status_icon(status: TrayStatus) -> Result<Icon> {
    Icon::from_rgba(
        mic_icon_rgba_with_color(APP_ICON_SIZE, status.rgba()),
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
