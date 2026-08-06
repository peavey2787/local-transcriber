//! Configurable Windows global shortcuts plus live capture in Settings.

use anyhow::{Context as AnyhowContext, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::Event as EguiEvent;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::OnceLock;

use crate::ui_wake::UiWake;

static HOTKEY_TX: OnceLock<Sender<u32>> = OnceLock::new();
static UI_WAKE: OnceLock<UiWake> = OnceLock::new();

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyPresses {
    pub recording: bool,
    pub voice_command: bool,
}

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    recording: Option<HotKey>,
    voice_command: Option<HotKey>,
    rx: Receiver<u32>,
}

impl Hotkeys {
    /// Creates the global-hotkey manager without making a recording shortcut
    /// conflict fatal to the whole application.
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

        let manager =
            GlobalHotKeyManager::new().context("create the Windows global hotkey manager")?;
        let mut this = Self {
            manager,
            recording: None,
            voice_command: None,
            rx,
        };

        let trimmed = shortcut.trim();
        if trimmed.is_empty() {
            return Ok((this, None));
        }

        let warning = match parse(trimmed) {
            Ok(requested) => match this.manager.register(requested) {
                Ok(()) => {
                    this.recording = Some(requested);
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

    pub fn rebind(&mut self, shortcut: &str) -> Result<()> {
        let replacement = parse(shortcut)?;
        if self.recording == Some(replacement) {
            return Ok(());
        }
        if self.voice_command == Some(replacement) {
            anyhow::bail!("the recording and voice-command hotkeys must be different");
        }
        self.replace_recording(replacement, shortcut)
    }

    fn replace_recording(&mut self, replacement: HotKey, shortcut: &str) -> Result<()> {
        self.manager
            .register(replacement)
            .with_context(|| format!("could not register {shortcut:?}"))?;
        if let Some(previous) = self.recording.take() {
            if let Err(error) = self.manager.unregister(previous) {
                let _ = self.manager.unregister(replacement);
                self.recording = Some(previous);
                return Err(error).context("unregister previous recording hotkey");
            }
        }
        self.recording = Some(replacement);
        println!("[local-stt] recording hotkey changed to: {}", friendly_name(shortcut));
        Ok(())
    }

    pub fn configure_voice_commands(&mut self, enabled: bool, shortcut: &str) -> Result<()> {
        if !enabled {
            self.disable_voice_commands();
            return Ok(());
        }
        let replacement = parse(shortcut)?;
        if self.recording == Some(replacement) {
            anyhow::bail!("the voice-command hotkey must differ from the recording hotkey");
        }
        if self.voice_command == Some(replacement) {
            return Ok(());
        }

        self.manager
            .register(replacement)
            .with_context(|| format!("could not register voice-command hotkey {shortcut:?}"))?;
        if let Some(previous) = self.voice_command.take() {
            if let Err(error) = self.manager.unregister(previous) {
                let _ = self.manager.unregister(replacement);
                self.voice_command = Some(previous);
                return Err(error).context("unregister previous voice-command hotkey");
            }
        }
        self.voice_command = Some(replacement);
        println!("[local-stt] voice-command hotkey: {}", friendly_name(shortcut));
        Ok(())
    }

    pub fn disable_voice_commands(&mut self) {
        if let Some(previous) = self.voice_command.take() {
            if let Err(error) = self.manager.unregister(previous) {
                eprintln!("[local-stt] could not release disabled voice-command hotkey: {error}");
            }
        }
        self.drain_events();
    }

    pub fn is_bound(&self) -> bool {
        self.recording.is_some()
    }

    pub fn is_voice_commands_bound(&self) -> bool {
        self.voice_command.is_some()
    }

    pub fn poll(&self) -> HotkeyPresses {
        let mut presses = HotkeyPresses::default();
        while let Ok(id) = self.rx.try_recv() {
            if self.recording.is_some_and(|hotkey| hotkey.id() == id) {
                presses.recording = true;
            }
            if self.voice_command.is_some_and(|hotkey| hotkey.id() == id) {
                presses.voice_command = true;
            }
        }
        presses
    }

    fn drain_events(&self) {
        while self.rx.try_recv().is_ok() {}
    }
}

pub fn validate(shortcut: &str) -> Result<()> {
    let _ = parse(shortcut)?;
    Ok(())
}

pub fn same_shortcut(left: &str, right: &str) -> bool {
    match (parse(left), parse(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left.trim().eq_ignore_ascii_case(right.trim()),
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    Captured(String),
    Unsupported(String),
}

/// Converts one non-repeating egui key press and its current modifiers into a
/// global-hotkey shortcut. Non-key and key-release events are ignored.
pub fn capture_shortcut(event: &EguiEvent) -> Option<CaptureOutcome> {
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
    fn default_windows_key_parses() {
        validate("Backquote").unwrap();
    }

    #[test]
    fn modified_key_parses() {
        validate("control+shift+Space").unwrap();
    }

    #[test]
    fn shortcut_identity_detects_collisions() {
        assert!(same_shortcut("control+shift+KeyR", "control+shift+KeyR"));
        assert!(!same_shortcut("control+shift+KeyR", "control+shift+KeyT"));
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
        assert_eq!(
            normalize_egui_key_name("Backtick"),
            ("Backquote".into(), false)
        );
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
        assert_eq!(
            capture_shortcut(&event),
            Some(CaptureOutcome::Captured("control+shift+KeyR".into()))
        );
    }
}
