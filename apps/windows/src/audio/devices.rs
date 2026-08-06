//! Windows recording-device discovery and stable-enough selection.

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host};

use transcriber_core::audio::{
    build_input_device_options, InputDeviceOption, InputDeviceSelection, InputDeviceSource,
    SelectedInputDevice,
};

pub(super) struct WindowsInputDeviceSource;

impl InputDeviceSource for WindowsInputDeviceSource {
    fn select_input_device(
        &self,
        preferred: Option<&InputDeviceSelection>,
    ) -> Result<SelectedInputDevice> {
        select_input_device(preferred)
    }
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
        label: system_default_label(default_name.as_deref()),
    });
    options.extend(options_for_names(&names));
    Ok(options)
}

fn select_input_device(
    preferred: Option<&InputDeviceSelection>,
) -> Result<SelectedInputDevice> {
    let host = cpal::default_host();
    let Some(preferred) = preferred else {
        let device = host
            .default_input_device()
            .context("no default input device")?;
        let name = device.name().ok();
        return Ok(SelectedInputDevice {
            device,
            label: system_default_label(name.as_deref()),
        });
    };

    let devices = named_input_devices(&host)?;
    let matching_total = devices
        .iter()
        .filter(|(_, name)| name == &preferred.name)
        .count();
    let mut matching_occurrence = 0usize;
    for (device, name) in devices {
        if name != preferred.name {
            continue;
        }
        if matching_occurrence == preferred.occurrence {
            let label = disambiguated_label(&name, matching_occurrence, matching_total);
            return Ok(SelectedInputDevice { device, label });
        }
        matching_occurrence += 1;
    }

    bail!("configured recording device is not available")
}

fn named_input_devices(host: &Host) -> Result<Vec<(Device, String)>> {
    let devices = host
        .input_devices()
        .context("enumerate Windows recording devices")?;
    let mut named = Vec::new();
    for device in devices {
        match device.name() {
            Ok(name) => named.push((device, name)),
            Err(error) => log::warn!("could not read a recording-device name: {error}"),
        }
    }
    Ok(named)
}

fn system_default_label(raw_name: Option<&str>) -> String {
    match raw_name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => format!("System default — {name}"),
        None => "System default".to_string(),
    }
}

fn options_for_names(names: &[String]) -> Vec<InputDeviceOption> {
    build_input_device_options(names, |_, name, occurrence, total| {
        disambiguated_label(name, occurrence, total)
    })
}

fn disambiguated_label(name: &str, occurrence: usize, total: usize) -> String {
    if total > 1 {
        format!("{name} ({})", occurrence + 1)
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_device_names_receive_distinct_selections() {
        let names = vec!["Mic".to_string(), "Line In".to_string(), "Mic".to_string()];
        let options = options_for_names(&names);

        assert_eq!(options[0].label, "Mic (1)");
        assert_eq!(options[2].label, "Mic (2)");
        assert_eq!(options[2].selection.as_ref().unwrap().occurrence, 1);
    }

    #[test]
    fn unique_device_name_is_not_decorated() {
        let options = options_for_names(&["USB Microphone".to_string()]);
        assert_eq!(options[0].label, "USB Microphone");
    }

    #[test]
    fn default_device_includes_the_windows_name_when_available() {
        assert_eq!(
            system_default_label(Some("Microphone Array")),
            "System default — Microphone Array"
        );
        assert_eq!(system_default_label(None), "System default");
    }
}
