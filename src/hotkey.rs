//! Configurable global shortcut support for Linux X11 and supported Wayland
//! desktops, plus live shortcut capture for the Settings window.

use anyhow::{Context as AnyhowContext, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{Context as EguiContext, Event as EguiEvent};
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
                    println!("[local-stt] hotkey registered: {}", friendly_name(trimmed));
                    None
                }
                Err(error) => {
                    eprintln!("[local-stt] could not register hotkey {trimmed:?}: {error}");
                    Some(format!(
                        "The recording shortcut {} is already in use by another application or unavailable on this desktop.",
                        friendly_name(trimmed)
                    ))
                }
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

/// Live key-combination capture used by Settings. Each capture completes on
/// one non-repeating main-key press and uses that event's current modifiers.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShortcutCapture;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    Captured(String),
    Unsupported(String),
}

impl ShortcutCapture {
    pub fn reset(&mut self) {}

    /// Feeds one egui keyboard event into the live shortcut recorder. Returns
    /// a result only after a non-modifier key is pressed.
    pub fn feed(&mut self, event: &EguiEvent) -> Option<CaptureOutcome> {
        let EguiEvent::Key {
            key,
            physical_key,
            pressed,
            repeat,
            modifiers,
            ..
        } = event
        else {
            return None;
        };

        if !*pressed || *repeat {
            return None;
        }

        let key_name = format!("{:?}", physical_key.as_ref().unwrap_or(key));
        let (main_key, implied_shift) = normalize_egui_key_name(&key_name);
        let mut pieces = Vec::new();
        if modifiers.ctrl {
            pieces.push("control".to_string());
        }
        if modifiers.alt {
            pieces.push("alt".to_string());
        }
        if modifiers.shift || implied_shift {
            pieces.push("shift".to_string());
        }
        pieces.push(main_key);
        let candidate = pieces.join("+");

        match validate(&candidate) {
            Ok(()) => Some(CaptureOutcome::Captured(candidate)),
            Err(error) => Some(CaptureOutcome::Unsupported(format!(
                "That key cannot be used as a global shortcut: {error}"
            ))),
        }
    }
}

/// Converts egui's key names to the names accepted by global-hotkey. The
/// physical key is used whenever eframe supplies one, so capture follows the
/// actual keyboard key rather than the active character layout.
fn normalize_egui_key_name(name: &str) -> (String, bool) {
    if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() {
        return (format!("Key{name}"), false);
    }
    if let Some(digit) = name.strip_prefix("Num") {
        if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() {
            return (format!("Digit{digit}"), false);
        }
    }

    match name {
        "Backtick" => ("Backquote".into(), false),
        "OpenBracket" => ("BracketLeft".into(), false),
        "CloseBracket" => ("BracketRight".into(), false),
        "Equals" => ("Equal".into(), false),
        // When no physical key is available, translate shifted logical symbols
        // back to their physical base key and preserve Shift explicitly.
        "Colon" => ("Semicolon".into(), true),
        "Questionmark" => ("Slash".into(), true),
        "Exclamationmark" => ("Digit1".into(), true),
        "Pipe" => ("Backslash".into(), true),
        "OpenCurlyBracket" => ("BracketLeft".into(), true),
        "CloseCurlyBracket" => ("BracketRight".into(), true),
        "Plus" => ("Equal".into(), true),
        _ => (name.to_string(), false),
    }
}

pub fn friendly_name(shortcut: &str) -> String {
    let trimmed = shortcut.trim();
    if trimmed.is_empty() {
        return "No hotkey set".into();
    }

    trimmed
        .split('+')
        .map(|part| match part.trim().to_ascii_lowercase().as_str() {
            "control" | "ctrl" => "Ctrl".to_string(),
            "shift" => "Shift".to_string(),
            "alt" | "option" => "Alt".to_string(),
            "super" | "command" | "cmd" => "Super".to_string(),
            _ => friendly_main_key(part.trim()),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn friendly_main_key(key: &str) -> String {
    if key.eq_ignore_ascii_case("Backquote") || key == "`" {
        return "` / ~".into();
    }
    if let Some(letter) = key.strip_prefix("Key") {
        if letter.len() == 1 && letter.as_bytes()[0].is_ascii_alphabetic() {
            return letter.to_ascii_uppercase();
        }
    }
    if let Some(digit) = key.strip_prefix("Digit") {
        if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() {
            return digit.to_string();
        }
    }
    key.to_string()
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
        validate("control+shift+Space").unwrap();
    }

    #[test]
    fn empty_hotkey_has_clear_label() {
        assert_eq!(friendly_name(""), "No hotkey set");
    }

    #[test]
    fn captured_names_are_human_readable() {
        assert_eq!(friendly_name("control+shift+KeyR"), "Ctrl + Shift + R");
        assert_eq!(friendly_name("Backquote"), "` / ~");
    }

    #[test]
    fn egui_names_are_normalized_for_global_hotkey() {
        assert_eq!(normalize_egui_key_name("A"), ("KeyA".into(), false));
        assert_eq!(normalize_egui_key_name("Num7"), ("Digit7".into(), false));
        assert_eq!(normalize_egui_key_name("Backtick"), ("Backquote".into(), false));
        assert_eq!(normalize_egui_key_name("Plus"), ("Equal".into(), true));
    }

    #[test]
    fn live_capture_uses_the_pressed_key_and_current_modifiers() {
        let event = EguiEvent::Key {
            key: eframe::egui::Key::R,
            physical_key: Some(eframe::egui::Key::R),
            pressed: true,
            repeat: false,
            modifiers: eframe::egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        };
        let mut capture = ShortcutCapture::default();
        assert_eq!(
            capture.feed(&event),
            Some(CaptureOutcome::Captured("control+shift+KeyR".into()))
        );
    }
}
