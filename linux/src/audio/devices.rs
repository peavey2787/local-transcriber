//! Input-device discovery, stable-enough selection, and display labels.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host};
use serde::{Deserialize, Serialize};

use super::backend_diagnostics::during_device_enumeration;
use super::device_labels::{is_redundant_default_alias, DeviceLabeler};

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
    let default_name = during_device_enumeration(|| {
        host.default_input_device()
            .and_then(|device| device.name().ok())
    });
    let names = named_input_devices(&host)?
        .into_iter()
        .map(|(_, name)| name)
        .filter(|name| !is_redundant_default_alias(name))
        .collect::<Vec<_>>();
    let labeler = DeviceLabeler::discover();

    let mut options = Vec::with_capacity(names.len() + 1);
    options.push(InputDeviceOption {
        selection: None,
        label: labeler.system_default_label(default_name.as_deref()),
    });
    options.extend(options_for_names(&names, &labeler));
    Ok(options)
}

pub(super) fn select_input_device(
    preferred: Option<&InputDeviceSelection>,
) -> Result<SelectedInputDevice> {
    let host = cpal::default_host();
    let labeler = DeviceLabeler::discover();
    let Some(preferred) = preferred else {
        let (device, raw_name) = during_device_enumeration(|| {
            host.default_input_device()
                .map(|device| {
                    let raw_name = device.name().ok();
                    (device, raw_name)
                })
        })
        .context("no default input device")?;
        let label = labeler.system_default_label(raw_name.as_deref());
        return Ok(SelectedInputDevice { device, label });
    };

    let mut matching_occurrence = 0usize;
    for (device, name) in named_input_devices(&host)? {
        if name != preferred.name {
            continue;
        }
        if matching_occurrence == preferred.occurrence {
            let label = labeler.selected_device_label(&name);
            return Ok(SelectedInputDevice { device, label });
        }
        matching_occurrence += 1;
    }

    bail!("configured recording device is not available")
}

fn named_input_devices(host: &Host) -> Result<Vec<(Device, String)>> {
    during_device_enumeration(|| {
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
    })
}

fn options_for_names(names: &[String], labeler: &DeviceLabeler) -> Vec<InputDeviceOption> {
    let labels = labeler.option_labels(names);
    let mut occurrences = HashMap::<&str, usize>::new();
    names
        .iter()
        .zip(labels)
        .map(|(name, label)| {
            let occurrence = occurrences.entry(name.as_str()).or_default();
            let current = *occurrence;
            *occurrence += 1;
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
    fn duplicate_device_names_receive_distinct_selections() {
        let names = vec!["Mic".to_string(), "Line In".to_string(), "Mic".to_string()];
        let options = options_for_names(&names, &DeviceLabeler::default());

        assert_eq!(options[0].label, "Mic (1)");
        assert_eq!(options[2].label, "Mic (2)");
        assert_eq!(options[2].selection.as_ref().unwrap().occurrence, 1);
    }
}
