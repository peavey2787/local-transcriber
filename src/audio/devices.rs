//! Input-device discovery, stable-enough selection, and display labels.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InputDeviceSelection {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) occurrence: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InputDeviceOption {
    pub(crate) selection: Option<InputDeviceSelection>,
    pub(crate) label: String,
}

pub(super) struct SelectedInputDevice {
    pub(super) device: Device,
    pub(super) label: String,
}

pub(crate) fn input_device_options() -> Result<Vec<InputDeviceOption>> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|device| device.name().ok());
    let names = named_input_devices(&host)?
        .into_iter()
        .map(|(_, name)| name)
        .collect::<Vec<_>>();

    let mut options = Vec::with_capacity(names.len() + 1);
    options.push(InputDeviceOption {
        selection: None,
        label: default_name
            .map(|name| format!("System default — {name}"))
            .unwrap_or_else(|| "System default".to_string()),
    });
    options.extend(options_for_names(&names));
    Ok(options)
}

pub(super) fn select_input_device(
    preferred: Option<&InputDeviceSelection>,
) -> Result<SelectedInputDevice> {
    let host = cpal::default_host();
    let Some(preferred) = preferred else {
        let device = host
            .default_input_device()
            .context("no default input device")?;
        let label = device
            .name()
            .unwrap_or_else(|_| "System default input".to_string());
        return Ok(SelectedInputDevice { device, label });
    };

    let mut matching_occurrence = 0usize;
    for (device, name) in named_input_devices(&host)? {
        if name != preferred.name {
            continue;
        }
        if matching_occurrence == preferred.occurrence {
            let label = if preferred.occurrence == 0 {
                name
            } else {
                format!("{name} ({})", preferred.occurrence + 1)
            };
            return Ok(SelectedInputDevice { device, label });
        }
        matching_occurrence += 1;
    }

    bail!(
        "configured recording device '{}' is not available",
        preferred.name
    )
}

fn named_input_devices(host: &Host) -> Result<Vec<(Device, String)>> {
    let devices = host
        .input_devices()
        .context("enumerate recording devices")?;
    let mut named = Vec::new();
    for device in devices {
        match device.name() {
            Ok(name) => named.push((device, name)),
            Err(error) => log::warn!("could not read a recording-device name: {error}"),
        }
    }
    Ok(named)
}

fn options_for_names(names: &[String]) -> Vec<InputDeviceOption> {
    let mut totals = HashMap::<&str, usize>::new();
    for name in names {
        *totals.entry(name.as_str()).or_default() += 1;
    }

    let mut occurrences = HashMap::<&str, usize>::new();
    names
        .iter()
        .map(|name| {
            let occurrence = occurrences.entry(name.as_str()).or_default();
            let current = *occurrence;
            *occurrence += 1;
            let duplicate = totals
                .get(name.as_str())
                .copied()
                .unwrap_or_default()
                > 1;
            let label = if duplicate {
                format!("{name} ({})", current + 1)
            } else {
                name.clone()
            };
            InputDeviceOption {
                selection: Some(InputDeviceSelection {
                    name: name.clone(),
                    occurrence: current,
                }),
                label,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_device_names_receive_distinct_choices() {
        let names = vec!["Mic".to_string(), "Line In".to_string(), "Mic".to_string()];
        let options = options_for_names(&names);

        assert_eq!(options[0].label, "Mic (1)");
        assert_eq!(options[2].label, "Mic (2)");
        assert_eq!(options[2].selection.as_ref().unwrap().occurrence, 1);
    }
}
