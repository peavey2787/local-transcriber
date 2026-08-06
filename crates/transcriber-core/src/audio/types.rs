use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Persisted microphone identity. `occurrence` disambiguates duplicate names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputDeviceSelection {
    pub name: String,
    #[serde(default)]
    pub occurrence: usize,
}

/// One entry shown by a platform's microphone picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceOption {
    pub selection: Option<InputDeviceSelection>,
    pub label: String,
}


/// Builds stable selections for native device names while leaving labels to the platform.
pub fn build_input_device_options<F>(
    names: &[String],
    mut label_for: F,
) -> Vec<InputDeviceOption>
where
    F: FnMut(usize, &str, usize, usize) -> String,
{
    let totals = names
        .iter()
        .fold(HashMap::<&str, usize>::new(), |mut map, name| {
            *map.entry(name.as_str()).or_default() += 1;
            map
        });
    let mut occurrences = HashMap::<&str, usize>::new();

    names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let occurrence = occurrences.entry(name.as_str()).or_default();
            let current = *occurrence;
            *occurrence += 1;
            InputDeviceOption {
                selection: Some(InputDeviceSelection {
                    name: name.clone(),
                    occurrence: current,
                }),
                label: label_for(index, name, current, totals[name.as_str()]),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_occurrences_are_stable_for_duplicate_names() {
        let names = vec!["Mic".to_string(), "Line In".to_string(), "Mic".to_string()];
        let options = build_input_device_options(&names, |_, name, occurrence, total| {
            if total > 1 {
                format!("{name} ({})", occurrence + 1)
            } else {
                name.to_string()
            }
        });

        assert_eq!(options[0].label, "Mic (1)");
        assert_eq!(options[2].label, "Mic (2)");
        assert_eq!(options[2].selection.as_ref().unwrap().occurrence, 1);
    }
}
