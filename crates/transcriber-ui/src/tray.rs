//! Shared tray status, menu text, and action coalescing.

use transcriber_core::hotkey::friendly_name;

pub const APP_ICON_SIZE: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    Idle,
    Recording,
    Busy,
}

impl TrayStatus {
    pub fn rgba(self) -> [u8; 4] {
        match self {
            Self::Idle => [0x1B, 0xB9, 0xCE, 0xFF],
            Self::Recording => [0xD7, 0x3F, 0x3F, 0xFF],
            Self::Busy => [0xE6, 0x91, 0x38, 0xFF],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Settings,
    VoiceCommands,
    Quit,
}

pub fn hotkey_menu_text(hotkey: &str) -> String {
    if hotkey.trim().is_empty() {
        "No recording hotkey — open Settings…".into()
    } else {
        format!("{} to record", friendly_name(hotkey))
    }
}

pub fn voice_hotkey_menu_text(enabled: bool, hotkey: &str) -> String {
    if !enabled {
        "Voice commands disabled".into()
    } else if hotkey.trim().is_empty() {
        "Voice-command hotkey unavailable".into()
    } else {
        format!("{} for voice commands", friendly_name(hotkey))
    }
}

pub fn coalesce_actions(actions: impl IntoIterator<Item = TrayAction>) -> Option<TrayAction> {
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
    fn quit_has_priority_over_queued_panel_commands() {
        assert_eq!(
            coalesce_actions([
                TrayAction::Settings,
                TrayAction::Quit,
                TrayAction::VoiceCommands,
            ]),
            Some(TrayAction::Quit)
        );
    }

    #[test]
    fn repeated_panel_commands_are_coalesced() {
        assert_eq!(
            coalesce_actions([TrayAction::Settings, TrayAction::Settings]),
            Some(TrayAction::Settings)
        );
        assert_eq!(coalesce_actions([]), None);
    }
}
