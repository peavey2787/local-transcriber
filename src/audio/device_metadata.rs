//! Linux sound-card metadata used to label CPAL input devices.

use std::collections::HashMap;

#[derive(Debug, Default)]
pub(super) struct SoundCardCatalog {
    pub(super) by_id: HashMap<String, SoundCardMetadata>,
    pub(super) by_number: HashMap<u32, SoundCardMetadata>,
}

#[derive(Debug, Clone)]
pub(super) struct SoundCardMetadata {
    pub(super) display_name: String,
    pub(super) capture_names: HashMap<u32, String>,
}

pub(super) fn discover_sound_cards() -> SoundCardCatalog {
    #[cfg(target_os = "linux")]
    {
        linux::discover(std::path::Path::new("/proc/asound"))
    }

    #[cfg(not(target_os = "linux"))]
    {
        SoundCardCatalog::default()
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::path::Path;

    use super::{HashMap, SoundCardCatalog, SoundCardMetadata};

    pub(super) fn discover(root: &Path) -> SoundCardCatalog {
        let Ok(cards_text) = fs::read_to_string(root.join("cards")) else {
            return SoundCardCatalog::default();
        };

        let mut catalog = SoundCardCatalog::default();
        for line in cards_text.lines() {
            let Some((number, bracket_id, display_name)) = parse_card_header(line) else {
                continue;
            };
            let card_dir = root.join(format!("card{number}"));
            let file_id = fs::read_to_string(card_dir.join("id"))
                .ok()
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty());
            let card = SoundCardMetadata {
                display_name,
                capture_names: capture_names(&card_dir),
            };
            catalog.by_number.insert(number, card.clone());
            catalog
                .by_id
                .insert(bracket_id.to_ascii_lowercase(), card.clone());
            if let Some(file_id) = file_id {
                catalog.by_id.insert(file_id.to_ascii_lowercase(), card);
            }
        }
        catalog
    }

    fn parse_card_header(line: &str) -> Option<(u32, String, String)> {
        let trimmed = line.trim_start();
        let number_end = trimmed.find(|character: char| character.is_whitespace())?;
        let number = trimmed[..number_end].parse().ok()?;
        let bracket_start = trimmed.find('[')?;
        let bracket_end = trimmed[bracket_start + 1..].find(']')? + bracket_start + 1;
        let bracket_id = trimmed[bracket_start + 1..bracket_end].trim().to_string();
        let description = trimmed[bracket_end + 1..]
            .trim_start()
            .trim_start_matches(':')
            .trim();
        let display_name = description
            .split_once(" - ")
            .map(|(_, name)| name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or(bracket_id.as_str())
            .to_string();
        Some((number, bracket_id, display_name))
    }

    fn capture_names(card_dir: &Path) -> HashMap<u32, String> {
        let Ok(entries) = fs::read_dir(card_dir) else {
            return HashMap::new();
        };
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let file_name = entry.file_name();
                let file_name = file_name.to_str()?;
                let device_number = file_name
                    .strip_prefix("pcm")?
                    .strip_suffix('c')?
                    .parse::<u32>()
                    .ok()?;
                let info = fs::read_to_string(entry.path().join("info")).ok()?;
                let name = info
                    .lines()
                    .find_map(|line| line.trim().strip_prefix("name:"))?
                    .trim()
                    .to_string();
                Some((device_number, name))
            })
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_alsa_card_header() {
            let parsed = parse_card_header(
                " 3 [Wireless       ]: USB-Audio - USB Wireless Headset",
            );
            assert_eq!(
                parsed,
                Some((
                    3,
                    "Wireless".to_string(),
                    "USB Wireless Headset".to_string(),
                ))
            );
        }

        #[test]
        fn rejects_continuation_lines() {
            assert_eq!(
                parse_card_header("                      USB Wireless Headset at usb"),
                None
            );
        }
    }
}
