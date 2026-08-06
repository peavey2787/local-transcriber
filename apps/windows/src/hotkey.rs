//! Windows global-hotkey backend and UI-wake integration.

use anyhow::{Context, Result};
use crossbeam_channel::{unbounded, Sender};
use eframe::egui::Event as EguiEvent;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::OnceLock;
use transcriber_core::hotkey::{HotkeyBackend, HotkeyBindings};

use crate::ui_wake::UiWake;

static HOTKEY_TX: OnceLock<Sender<u32>> = OnceLock::new();
static UI_WAKE: OnceLock<UiWake> = OnceLock::new();

struct WindowsHotkeyBackend {
    manager: GlobalHotKeyManager,
}

impl HotkeyBackend for WindowsHotkeyBackend {
    type Hotkey = HotKey;

    fn parse(&self, shortcut: &str) -> Result<HotKey> {
        parse(shortcut)
    }

    fn register(&mut self, hotkey: HotKey) -> Result<()> {
        self.manager
            .register(hotkey)
            .context("register the Windows global hotkey")
    }

    fn unregister(&mut self, hotkey: HotKey) -> Result<()> {
        self.manager
            .unregister(hotkey)
            .context("unregister the Windows global hotkey")
    }

    fn id(hotkey: HotKey) -> u32 {
        hotkey.id()
    }
}

pub struct Hotkeys {
    bindings: HotkeyBindings<WindowsHotkeyBackend>,
}

impl Hotkeys {
    pub fn register(wake: UiWake, shortcut: &str) -> Result<(Self, Option<String>)> {
        let (tx, rx) = unbounded();
        let _ = HOTKEY_TX.set(tx);
        let _ = UI_WAKE.set(wake);

        GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
            if event.state != HotKeyState::Pressed {
                return;
            }
            if let Some(tx) = HOTKEY_TX.get() {
                let _ = tx.send(event.id);
            }
            if let Some(wake) = UI_WAKE.get() {
                wake.request_repaint();
            }
        }));

        let manager = GlobalHotKeyManager::new().context("create the Windows global hotkey manager")?;
        let (bindings, warning) =
            HotkeyBindings::new(WindowsHotkeyBackend { manager }, rx, shortcut)?;
        Ok((Self { bindings }, warning))
    }

    pub fn rebind(&mut self, shortcut: &str) -> Result<()> {
        self.bindings.rebind(shortcut)
    }

    pub fn configure_voice_commands(&mut self, enabled: bool, shortcut: &str) -> Result<()> {
        self.bindings.configure_voice_commands(enabled, shortcut)
    }

    pub fn is_bound(&self) -> bool {
        self.bindings.is_bound()
    }

    pub fn is_voice_commands_bound(&self) -> bool {
        self.bindings.is_voice_commands_bound()
    }

    pub fn poll(&self) -> HotkeyPresses {
        self.bindings.poll()
    }
}

pub fn validate(shortcut: &str) -> Result<()> {
    let _ = parse(shortcut)?;
    Ok(())
}

fn parse(shortcut: &str) -> Result<HotKey> {
    let trimmed = shortcut.trim();
    if trimmed.is_empty() {
        anyhow::bail!("hotkey cannot be empty");
    }
    trimmed
        .parse::<HotKey>()
        .map_err(|error| anyhow::anyhow!("invalid hotkey {trimmed:?}: {error}"))
}

pub use transcriber_core::hotkey::{friendly_name, same_shortcut, HotkeyPresses};
pub use transcriber_ui::shortcut::CaptureOutcome;

pub fn capture_shortcut(event: &EguiEvent) -> Option<CaptureOutcome> {
    transcriber_ui::shortcut::capture_shortcut(event, validate)
}
