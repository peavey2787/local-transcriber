//! Platform-neutral shortcut identity and dual-hotkey binding state.

use anyhow::{Context, Result};
use crossbeam_channel::Receiver;

/// Native hotkey operations supplied by each platform application.
pub trait HotkeyBackend {
    type Hotkey: Copy + Eq;

    fn parse(&self, shortcut: &str) -> Result<Self::Hotkey>;
    fn register(&mut self, hotkey: Self::Hotkey) -> Result<()>;
    fn unregister(&mut self, hotkey: Self::Hotkey) -> Result<()>;
    fn id(hotkey: Self::Hotkey) -> u32;
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyPresses {
    pub recording: bool,
    pub voice_command: bool,
}

/// Shared registration and collision-handling policy for the two application
/// shortcuts. Native backends only parse/register/unregister concrete keys.
pub struct HotkeyBindings<B: HotkeyBackend> {
    backend: B,
    recording: Option<B::Hotkey>,
    voice_command: Option<B::Hotkey>,
    events: Receiver<u32>,
}

impl<B: HotkeyBackend> HotkeyBindings<B> {
    pub fn new(
        backend: B,
        events: Receiver<u32>,
        recording_shortcut: &str,
    ) -> Result<(Self, Option<String>)> {
        let mut bindings = Self {
            backend,
            recording: None,
            voice_command: None,
            events,
        };

        let trimmed = recording_shortcut.trim();
        if trimmed.is_empty() {
            return Ok((bindings, None));
        }

        let warning = match bindings.backend.parse(trimmed) {
            Ok(requested) => match bindings.backend.register(requested) {
                Ok(()) => {
                    bindings.recording = Some(requested);
                    println!(
                        "[local-stt] hotkey registered: {}",
                        friendly_name(trimmed)
                    );
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

        Ok((bindings, warning))
    }

    pub fn rebind(&mut self, shortcut: &str) -> Result<()> {
        let replacement = self.backend.parse(shortcut)?;
        if self.recording == Some(replacement) {
            return Ok(());
        }
        if self.voice_command == Some(replacement) {
            anyhow::bail!("the recording and voice-command hotkeys must be different");
        }

        self.backend
            .register(replacement)
            .with_context(|| format!("could not register {shortcut:?}"))?;
        if let Some(previous) = self.recording.take() {
            if let Err(error) = self.backend.unregister(previous) {
                let _ = self.backend.unregister(replacement);
                self.recording = Some(previous);
                return Err(error).context("unregister previous recording hotkey");
            }
        }
        self.recording = Some(replacement);
        println!(
            "[local-stt] recording hotkey changed to: {}",
            friendly_name(shortcut)
        );
        Ok(())
    }

    pub fn configure_voice_commands(&mut self, enabled: bool, shortcut: &str) -> Result<()> {
        if !enabled {
            self.disable_voice_commands();
            return Ok(());
        }

        let replacement = self.backend.parse(shortcut)?;
        if self.recording == Some(replacement) {
            anyhow::bail!("the voice-command hotkey must differ from the recording hotkey");
        }
        if self.voice_command == Some(replacement) {
            return Ok(());
        }

        self.backend
            .register(replacement)
            .with_context(|| format!("could not register voice-command hotkey {shortcut:?}"))?;
        if let Some(previous) = self.voice_command.take() {
            if let Err(error) = self.backend.unregister(previous) {
                let _ = self.backend.unregister(replacement);
                self.voice_command = Some(previous);
                return Err(error).context("unregister previous voice-command hotkey");
            }
        }
        self.voice_command = Some(replacement);
        println!(
            "[local-stt] voice-command hotkey: {}",
            friendly_name(shortcut)
        );
        Ok(())
    }

    pub fn disable_voice_commands(&mut self) {
        if let Some(previous) = self.voice_command.take() {
            if let Err(error) = self.backend.unregister(previous) {
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
        while let Ok(id) = self.events.try_recv() {
            if self
                .recording
                .is_some_and(|hotkey| B::id(hotkey) == id)
            {
                presses.recording = true;
            }
            if self
                .voice_command
                .is_some_and(|hotkey| B::id(hotkey) == id)
            {
                presses.voice_command = true;
            }
        }
        presses
    }

    fn drain_events(&self) {
        while self.events.try_recv().is_ok() {}
    }
}

/// Compares shortcut identity without depending on a native hotkey crate.
pub fn same_shortcut(left: &str, right: &str) -> bool {
    canonical_shortcut(left) == canonical_shortcut(right)
}

fn canonical_shortcut(shortcut: &str) -> String {
    let mut modifiers = [false; 4];
    let mut main = Vec::new();
    for part in shortcut.split('+').map(str::trim).filter(|part| !part.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "control" | "ctrl" => modifiers[0] = true,
            "alt" | "option" => modifiers[1] = true,
            "shift" => modifiers[2] = true,
            "super" | "command" | "cmd" => modifiers[3] = true,
            _ => main.push(part.to_ascii_lowercase()),
        }
    }

    let mut pieces = Vec::new();
    for (enabled, name) in modifiers.into_iter().zip(["control", "alt", "shift", "super"]) {
        if enabled {
            pieces.push(name.to_string());
        }
    }
    main.sort();
    pieces.extend(main);
    pieces.join("+")
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
    fn shortcut_identity_normalizes_modifier_order_and_aliases() {
        assert!(same_shortcut(
            "control+shift+KeyR",
            "SHIFT+ctrl+keyr"
        ));
        assert!(!same_shortcut(
            "control+shift+KeyR",
            "control+shift+KeyT"
        ));
    }

    #[test]
    fn shortcut_names_are_human_readable() {
        assert_eq!(friendly_name("control+shift+KeyR"), "Ctrl + Shift + R");
        assert_eq!(friendly_name("Backquote"), "` / ~");
    }
}
