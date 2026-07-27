//! Configurable global shortcut support for Linux X11 and supported Wayland
//! desktops.

use anyhow::{Context as AnyhowContext, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::Context as EguiContext;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::OnceLock;

/// Shared handle so hotkey events can wake the egui event loop.
pub type UiWake = Arc<Mutex<Option<EguiContext>>>;

static HOTKEY_TX: OnceLock<Sender<()>> = OnceLock::new();
static UI_WAKE: OnceLock<UiWake> = OnceLock::new();

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    registered: Option<HotKey>,
    rx: Receiver<()>,
}

impl Hotkeys {
    /// Creates the global-hotkey manager without making a shortcut conflict
    /// fatal to the whole application. The returned warning means the app is
    /// running with no recording shortcut until the user chooses another one.
    pub fn register(wake: UiWake, shortcut: &str) -> Result<(Self, Option<String>)> {
        let (tx, rx) = unbounded();
        let _ = HOTKEY_TX.set(tx);
        let _ = UI_WAKE.set(wake);

        GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
            if event.state != HotKeyState::Pressed {
                return;
            }
            if let Some(tx) = HOTKEY_TX.get() {
                let _ = tx.send(());
            }
            if let Some(wake) = UI_WAKE.get() {
                if let Some(ctx) = wake.lock().as_ref() {
                    ctx.request_repaint();
                }
            }
        }));

        let manager = GlobalHotKeyManager::new().context(
            "create global hotkey manager (on Linux, verify an X11 session or a desktop with the XDG GlobalShortcuts portal)",
        )?;

        let mut this = Self {
            manager,
            registered: None,
            rx,
        };

        let trimmed = shortcut.trim();
        if trimmed.is_empty() {
            return Ok((this, None));
        }

        let warning = match parse(trimmed) {
            Ok(requested) => match this.manager.register(requested) {
                Ok(()) => {
                    this.registered = Some(requested);
                    println!(
                        "[local-stt] hotkey registered: {}",
                        friendly_name(trimmed)
                    );
                    None
                }
                Err(error) => {
                    eprintln!(
                        "[local-stt] could not register hotkey {trimmed:?}: {error}"
                    );
                    Some(format!(
                        "The recording shortcut {} is already in use by another application or unavailable on this desktop.",
                        friendly_name(trimmed)
                    ))
                },
            },
            Err(error) => Some(error.to_string()),
        };

        Ok((this, warning))
    }

    /// Replaces the active shortcut. If registration fails, the previous
    /// shortcut stays dropped so the app never reports a key as active when it
    /// is not actually owned.
    pub fn rebind(&mut self, shortcut: &str) -> Result<()> {
        let replacement = parse(shortcut)?;
        if self.registered == Some(replacement) {
            return Ok(());
        }

        if let Some(previous) = self.registered {
            self.manager
                .unregister(previous)
                .context("unregister previous hotkey")?;
            self.registered = None;
        }

        if let Err(error) = self.manager.register(replacement) {
            anyhow::bail!(
                "could not register {shortcut:?}; the recording hotkey is now disabled: {error}"
            );
        }

        self.registered = Some(replacement);
        println!("[local-stt] hotkey changed to: {}", friendly_name(shortcut));
        Ok(())
    }


    /// Disables shortcut handling even if the desktop refuses to release the
    /// underlying registration. This keeps stale events from toggling audio.
    pub fn disable(&mut self) {
        if let Some(previous) = self.registered.take() {
            if let Err(error) = self.manager.unregister(previous) {
                eprintln!("[local-stt] could not release disabled hotkey: {error}");
            }
        }
        while self.rx.try_recv().is_ok() {}
    }

    pub fn is_bound(&self) -> bool {
        self.registered.is_some()
    }

    /// Returns true if the toggle hotkey was pressed since the previous poll.
    pub fn poll_toggle(&self) -> bool {
        let mut hit = false;
        while self.rx.try_recv().is_ok() {
            hit = true;
        }
        hit
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

pub fn friendly_name(shortcut: &str) -> String {
    let trimmed = shortcut.trim();
    if trimmed.is_empty() {
        return "No hotkey set".into();
    }
    if trimmed.eq_ignore_ascii_case("Backquote") || trimmed == "`" {
        return "Tilde / backquote key (` / ~)".into();
    }
    trimmed
        .replace("control", "Ctrl")
        .replace("CONTROL", "Ctrl")
        .replace("ctrl", "Ctrl")
        .replace("shift", "Shift")
        .replace("SHIFT", "Shift")
        .replace("super", "Super")
        .replace("SUPER", "Super")
        .replace("alt", "Alt")
        .replace("ALT", "Alt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_linux_key_parses() {
        validate("Backquote").unwrap();
    }

    #[test]
    fn modified_key_parses() {
        validate("ctrl+shift+Space").unwrap();
    }

    #[test]
    fn empty_hotkey_has_clear_label() {
        assert_eq!(friendly_name(""), "No hotkey set");
    }
}
