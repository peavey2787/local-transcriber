//! Human-readable labels for low-level audio device identifiers.

use std::collections::HashMap;

use super::device_metadata::{discover_sound_cards, SoundCardCatalog, SoundCardMetadata};

#[derive(Debug, Default)]
pub(super) struct DeviceLabeler {
    cards: SoundCardCatalog,
}

impl DeviceLabeler {
    pub(super) fn discover() -> Self {
        Self {
            cards: discover_sound_cards(),
        }
    }

    pub(super) fn system_default_label(&self, raw_name: Option<&str>) -> String {
        let Some(raw_name) = raw_name else {
            return "System default".to_string();
        };
        let Some(friendly) = self.friendly_name(raw_name) else {
            return "System default".to_string();
        };
        if friendly == "System default" {
            "System default".to_string()
        } else {
            format!("System default — {friendly}")
        }
    }

    pub(super) fn option_labels(&self, raw_names: &[String]) -> Vec<String> {
        let mut fallback_number = 0usize;
        let labels = raw_names
            .iter()
            .map(|raw_name| {
                self.friendly_name(raw_name).unwrap_or_else(|| {
                    fallback_number += 1;
                    format!("Device {fallback_number}")
                })
            })
            .collect::<Vec<_>>();
        disambiguate(labels)
    }

    pub(super) fn selected_device_label(&self, raw_name: &str) -> String {
        self.friendly_name(raw_name)
            .unwrap_or_else(|| "Selected recording device".to_string())
    }

    fn friendly_name(&self, raw_name: &str) -> Option<String> {
        let trimmed = raw_name.trim();
        match trimmed.to_ascii_lowercase().as_str() {
            "default" | "sysdefault" => return Some("System default".to_string()),
            "pulse" => return Some("PulseAudio".to_string()),
            "pipewire" => return Some("PipeWire".to_string()),
            "jack" => return Some("JACK".to_string()),
            _ => {}
        }

        if let Some(card) = self.sound_card_for_name(trimmed) {
            let device_number = parameter(trimmed, "DEV").and_then(|value| value.parse().ok());
            if let Some(capture_name) = device_number
                .and_then(|number| card.capture_names.get(&number))
                .filter(|name| useful_capture_name(name, &card.display_name))
            {
                return Some(format!("{} — {capture_name}", card.display_name));
            }
            if useful_card_name(&card.display_name) {
                return Some(card.display_name.clone());
            }
        }

        if looks_technical(trimmed) {
            None
        } else {
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
    }

    fn sound_card_for_name(&self, raw_name: &str) -> Option<&SoundCardMetadata> {
        if let Some(card_id) = parameter(raw_name, "CARD") {
            if let Some(card) = self.cards.by_id.get(&card_id.to_ascii_lowercase()) {
                return Some(card);
            }
        }

        numeric_card_number(raw_name).and_then(|number| self.cards.by_number.get(&number))
    }
}

pub(super) fn is_redundant_default_alias(raw_name: &str) -> bool {
    matches!(
        raw_name.trim().to_ascii_lowercase().as_str(),
        "default" | "sysdefault"
    )
}

fn parameter<'a>(raw_name: &'a str, key: &str) -> Option<&'a str> {
    raw_name
        .split(|character| character == ':' || character == ',')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| name.eq_ignore_ascii_case(key).then_some(value.trim()))
}

fn numeric_card_number(raw_name: &str) -> Option<u32> {
    let lower = raw_name.to_ascii_lowercase();
    if !(lower.starts_with("hw:") || lower.starts_with("plughw:")) {
        return None;
    }
    let coordinates = raw_name.split_once(':')?.1;
    let card = coordinates.split(',').next()?.trim();
    if card.contains('=') {
        None
    } else {
        card.parse().ok()
    }
}

fn looks_technical(raw_name: &str) -> bool {
    let lower = raw_name.to_ascii_lowercase();
    lower.contains("card=")
        || [
            "hw",
            "plughw",
            "front",
            "surround",
            "iec958",
            "spdif",
            "dmix",
            "dsnoop",
            "usbstream",
            "samplerate",
            "lavrate",
            "speexrate",
            "upmix",
            "vdownmix",
            "null",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

fn useful_card_name(card_name: &str) -> bool {
    !matches!(
        card_name.trim().to_ascii_lowercase().as_str(),
        "audio" | "generic" | "hd-audio generic" | "usb audio"
    )
}

fn useful_capture_name(capture_name: &str, card_name: &str) -> bool {
    let capture = capture_name.trim();
    !capture.is_empty()
        && !capture.eq_ignore_ascii_case(card_name)
        && !matches!(
            capture.to_ascii_lowercase().as_str(),
            "usb audio" | "capture"
        )
}

fn disambiguate(mut labels: Vec<String>) -> Vec<String> {
    let mut totals = HashMap::<String, usize>::new();
    for label in &labels {
        *totals.entry(label.to_ascii_lowercase()).or_default() += 1;
    }

    let mut occurrences = HashMap::<String, usize>::new();
    for label in &mut labels {
        let key = label.to_ascii_lowercase();
        if totals.get(&key).copied().unwrap_or_default() <= 1 {
            continue;
        }
        let occurrence = occurrences.entry(key).or_default();
        *occurrence += 1;
        label.push_str(&format!(" ({occurrence})"));
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labeler_with_card() -> DeviceLabeler {
        let card = SoundCardMetadata {
            display_name: "USB Wireless Headset".to_string(),
            capture_names: HashMap::new(),
        };
        DeviceLabeler {
            cards: SoundCardCatalog {
                by_id: HashMap::from([("wireless".to_string(), card.clone())]),
                by_number: HashMap::from([(3, card)]),
            },
        }
    }

    #[test]
    fn alsa_card_identifier_uses_friendly_card_name() {
        let labels = labeler_with_card()
            .option_labels(&["hw:CARD=Wireless,DEV=0".to_string()]);
        assert_eq!(labels, vec!["USB Wireless Headset".to_string()]);
    }

    #[test]
    fn numeric_alsa_card_identifier_uses_friendly_card_name() {
        let labels = labeler_with_card().option_labels(&["plughw:3,0".to_string()]);
        assert_eq!(labels, vec!["USB Wireless Headset".to_string()]);
    }

    #[test]
    fn unknown_technical_names_use_numbered_fallbacks() {
        let labels = DeviceLabeler::default().option_labels(&[
            "hw:CARD=Unknown,DEV=0".to_string(),
            "surround51:CARD=Unknown,DEV=0".to_string(),
        ]);
        assert_eq!(
            labels,
            vec!["Device 1".to_string(), "Device 2".to_string()]
        );
    }

    #[test]
    fn bare_alsa_plugin_names_use_numbered_fallbacks() {
        let labels = DeviceLabeler::default().option_labels(&["dmix".to_string()]);
        assert_eq!(labels, vec!["Device 1".to_string()]);
    }

    #[test]
    fn duplicate_labels_are_disambiguated() {
        let labels = DeviceLabeler::default()
            .option_labels(&["pulse".to_string(), "pulse".to_string()]);
        assert_eq!(
            labels,
            vec!["PulseAudio (1)".to_string(), "PulseAudio (2)".to_string()]
        );
    }

    #[test]
    fn system_default_does_not_repeat_default_alias() {
        assert_eq!(
            DeviceLabeler::default().system_default_label(Some("default")),
            "System default"
        );
    }
}
