//! Shared shortcut capture and display formatting for egui controls.

use egui::Event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    Captured(String),
    Unsupported(String),
}

pub fn capture_shortcut<F>(event: &Event, validate: F) -> Option<CaptureOutcome>
where
    F: FnOnce(&str) -> anyhow::Result<()>,
{
    let Event::Key {
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


pub use transcriber_core::hotkey::friendly_name;
